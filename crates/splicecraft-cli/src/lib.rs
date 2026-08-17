//! Command-line sidecar for the SpliceCraft.rs agent API.
//!
//! `call` hits the same registry the localhost server dispatches.

#![cfg_attr(not(test), forbid(unsafe_code))]

pub use splicecraft_agent as agent;

use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use splicecraft_persist::{PYTHON_XDG_DATA_DIR_LEAF, XDG_DATA_DIR_LEAF, path_has_python_leaf};

/// Stage that implements the real agent-CLI surface.
pub const IMPLEMENTATION_STAGE: u8 = 14;

/// Default agent host (loopback only).
pub const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
/// Token file cap (upstream `_CLI_TOKEN_FILE_MAX_BYTES`).
pub const TOKEN_FILE_MAX_BYTES: u64 = 1024;
/// Response cap (upstream `_CLI_RESPONSE_MAX_BYTES`).
pub const RESPONSE_MAX_BYTES: usize = 50 * 1024 * 1024;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// `splicecraft-cli` arguments.
#[derive(Debug, Parser)]
#[command(
    name = "splicecraft-cli",
    version,
    about = "SpliceCraft.rs agent CLI sidecar"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Sidecar subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print the CLI version.
    Version,
    /// List every registered endpoint (`GET /tools`).
    Tools {
        /// Emit raw JSON instead of the table.
        #[arg(long)]
        json: bool,
    },
    /// Session snapshot (`GET /status`).
    Status,
    /// Call any agent endpoint by name (generic passthrough).
    Call {
        /// Endpoint name (see `tools`), e.g. `list-library`.
        endpoint: String,
        /// HTTP method. Default: POST when `--json` is given, else GET
        /// (auto-upgraded to POST on a 405 that names `allow: POST`).
        #[arg(long)]
        method: Option<String>,
        /// Request body as a JSON object.
        #[arg(long)]
        json: Option<String>,
    },
}

/// Sidecar errors.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Catch-all.
    #[error("{0}")]
    Message(String),
    /// Agent HTTP error (JSON already printed by [`run`] for `call`).
    #[error("agent HTTP {code}")]
    Agent {
        /// Status.
        code: u16,
        /// Body.
        body: Value,
    },
}

impl CliError {
    /// Process exit code.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Agent { .. } => 1,
            Self::Message(_) => 1,
        }
    }
}

/// Result of [`execute_call`].
#[derive(Clone, Debug)]
pub struct CallResult {
    /// HTTP status.
    pub http_code: u16,
    /// JSON body.
    pub body: Value,
}

/// Run a parsed CLI invocation.
pub fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Some(Commands::Version) | None => {
            println!("splicecraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Commands::Tools { json }) => {
            let result = execute_call("tools", "GET", None, false)?;
            if json {
                println!("{}", pretty(&result.body));
            } else {
                let Some(eps) = result.body.get("endpoints").and_then(Value::as_array) else {
                    println!("{}", pretty(&result.body));
                    return Ok(());
                };
                for ep in eps {
                    let flag = if ep.get("write").and_then(Value::as_bool).unwrap_or(false) {
                        "WRITE"
                    } else {
                        "READ "
                    };
                    let name = ep.get("name").and_then(Value::as_str).unwrap_or("?");
                    let doc = ep.get("doc").and_then(Value::as_str).unwrap_or("");
                    println!("  {flag}  {name:24}  {doc}");
                }
            }
            Ok(())
        }
        Some(Commands::Status) => {
            let result = execute_call("status", "GET", None, false)?;
            println!("{}", pretty(&result.body));
            Ok(())
        }
        Some(Commands::Call {
            endpoint,
            method,
            json: json_body,
        }) => {
            let payload = match json_body {
                Some(raw) => {
                    let v: Value = serde_json::from_str(&raw)
                        .map_err(|e| CliError::Message(format!("--json is not valid JSON: {e}")))?;
                    if !v.is_object() {
                        return Err(CliError::Message("--json must be a JSON object".into()));
                    }
                    Some(v)
                }
                None => None,
            };
            let explicit = method.is_some();
            let method = method
                .unwrap_or_else(|| {
                    if payload.is_some() {
                        "POST".into()
                    } else {
                        "GET".into()
                    }
                })
                .to_ascii_uppercase();
            match execute_call(&endpoint, &method, payload.as_ref(), !explicit) {
                Ok(result) => {
                    println!("{}", pretty(&result.body));
                    Ok(())
                }
                Err(CliError::Agent { code, mut body }) => {
                    if let Value::Object(map) = &mut body {
                        map.insert("http_code".into(), json!(code));
                    }
                    println!("{}", pretty(&body));
                    Err(CliError::Agent { code, body })
                }
                Err(e) => Err(e),
            }
        }
    }
}

/// HTTP call against the running agent (same registry the server dispatches).
pub fn execute_call(
    endpoint: &str,
    method: &str,
    payload: Option<&Value>,
    allow_upgrade: bool,
) -> Result<CallResult, CliError> {
    match request(endpoint, method, payload) {
        Ok(result) => Ok(result),
        Err(CliError::Agent { code: 405, body })
            if allow_upgrade && method.eq_ignore_ascii_case("GET") =>
        {
            let allow = body.get("allow").and_then(Value::as_str).unwrap_or("");
            if allow.eq_ignore_ascii_case("POST") {
                return request(endpoint, "POST", payload.or(Some(&json!({}))));
            }
            Err(CliError::Agent { code: 405, body })
        }
        Err(e) => Err(e),
    }
}

fn request(endpoint: &str, method: &str, payload: Option<&Value>) -> Result<CallResult, CliError> {
    let (host, port, token) = if endpoint == "tools" || endpoint == "healthz" {
        match read_session() {
            Ok(s) => s,
            Err(_) => (DEFAULT_HOST, agent::DEFAULT_PORT, String::new()),
        }
    } else {
        read_session()?
    };
    if !host.is_loopback() {
        return Err(CliError::Message(
            "refusing to connect to a non-loopback agent host".into(),
        ));
    }
    let body = payload.map(|v| serde_json::to_vec(v).unwrap_or_else(|_| b"{}".to_vec()));
    let path = format!("/{endpoint}");
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nConnection: close\r\n"
    );
    if !token.is_empty() {
        req.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(bytes) = &body {
        req.push_str("Content-Type: application/json\r\n");
        req.push_str(&format!("Content-Length: {}\r\n", bytes.len()));
    }
    req.push_str("\r\n");
    let mut stream = TcpStream::connect((host, port))
        .map_err(|e| CliError::Message(format!("could not connect to {host}:{port}: {e}")))?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream
        .write_all(req.as_bytes())
        .map_err(|e| CliError::Message(e.to_string()))?;
    if let Some(bytes) = &body {
        stream
            .write_all(bytes)
            .map_err(|e| CliError::Message(e.to_string()))?;
    }
    let raw = read_capped(&mut stream, RESPONSE_MAX_BYTES)?;
    let (status, json_body) = parse_http_json(&raw)?;
    if status >= 400 {
        return Err(CliError::Agent {
            code: status,
            body: json_body,
        });
    }
    Ok(CallResult {
        http_code: status,
        body: json_body,
    })
}

fn read_capped(stream: &mut TcpStream, cap: usize) -> Result<Vec<u8>, CliError> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        let n = stream
            .read(&mut tmp)
            .map_err(|e| CliError::Message(e.to_string()))?;
        if n == 0 {
            break;
        }
        if buf.len() + n > cap {
            return Err(CliError::Message(
                "refusing to read agent response over the 50 MB cap".into(),
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    Ok(buf)
}

fn parse_http_json(raw: &[u8]) -> Result<(u16, Value), CliError> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .or_else(|| text.split_once("\n\n"))
        .ok_or_else(|| CliError::Message("malformed HTTP response".into()))?;
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| CliError::Message("malformed HTTP status".into()))?;
    let body = decode_body(head, body.as_bytes())?;
    let json = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| CliError::Message("agent response was not JSON".into()))?
    };
    Ok((status, json))
}

fn decode_body(head: &str, rest: &[u8]) -> Result<Vec<u8>, CliError> {
    let chunked = head.lines().any(|l| {
        l.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    });
    if chunked {
        return decode_chunked(rest);
    }
    Ok(rest.to_vec())
}

fn decode_chunked(data: &[u8]) -> Result<Vec<u8>, CliError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let nl = data[i..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| CliError::Message("malformed chunked body".into()))?;
        let size_line = std::str::from_utf8(&data[i..i + nl]).unwrap_or("0");
        let size = usize::from_str_radix(size_line.trim(), 16)
            .map_err(|_| CliError::Message("malformed chunk size".into()))?;
        i += nl + 2;
        if size == 0 {
            break;
        }
        if i + size > data.len() {
            return Err(CliError::Message("truncated chunked body".into()));
        }
        out.extend_from_slice(&data[i..i + size]);
        i += size;
        if i + 2 <= data.len() {
            i += 2;
        }
    }
    Ok(out)
}

/// Resolve `(host, port, token)` from the running session's token file.
pub fn read_session() -> Result<(IpAddr, u16, String), CliError> {
    let path = token_file_path()?;
    read_token_file(&path)
}

/// `$SPLICECRAFT_DATA_DIR/agent_token` or `$XDG_DATA_HOME/splicecraft-rs/agent_token`.
pub fn token_file_path() -> Result<PathBuf, CliError> {
    if let Ok(dir) = std::env::var("SPLICECRAFT_DATA_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            let p = PathBuf::from(dir);
            if path_has_python_leaf(&p)
                || p.file_name().and_then(|s| s.to_str()) == Some(PYTHON_XDG_DATA_DIR_LEAF)
            {
                return Err(CliError::Message(
                    "refusing the Python data-dir leaf splicecraft; use splicecraft-rs".into(),
                ));
            }
            return Ok(p.join(agent::AGENT_TOKEN_FILE_NAME));
        }
    }
    let xdg = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("."))
        });
    Ok(xdg
        .join(XDG_DATA_DIR_LEAF)
        .join(agent::AGENT_TOKEN_FILE_NAME))
}

fn read_token_file(path: &Path) -> Result<(IpAddr, u16, String), CliError> {
    let meta = fs::symlink_metadata(path).map_err(|_| {
        CliError::Message(format!(
            "No SpliceCraft.rs session found.\n  Expected token file: {}\n  Start with: splicecraft --agent",
            path.display()
        ))
    })?;
    if meta.file_type().is_symlink() {
        return Err(CliError::Message(format!(
            "Refusing to read symlinked token file {}.",
            path.display()
        )));
    }
    if meta.len() > TOKEN_FILE_MAX_BYTES {
        return Err(CliError::Message(format!(
            "Refusing to read oversized token file {} ({} bytes).",
            path.display(),
            meta.len()
        )));
    }
    let text = fs::read_to_string(path)
        .map_err(|e| CliError::Message(format!("could not read {}: {e}", path.display())))?;
    let mut lines = text.lines();
    let port_line = lines.next().unwrap_or("").trim();
    let token = lines.next().unwrap_or("").trim().to_owned();
    if port_line.is_empty() || token.is_empty() {
        return Err(CliError::Message(format!(
            "Malformed token file at {} (expected `port\\ntoken`).",
            path.display()
        )));
    }
    let port: u16 = port_line.parse().map_err(|_| {
        CliError::Message(format!(
            "Malformed port in {}: {port_line:?}",
            path.display()
        ))
    })?;
    Ok((DEFAULT_HOST, port, token))
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use splicecraft_agent::{ServerConfig, start_server};
    use splicecraft_core::Record;
    use splicecraft_io::record_to_library_entry;
    use splicecraft_persist::{self as persist, DataLayout, LibraryStore};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn sandbox() -> (tempfile::TempDir, DataLayout) {
        let tmp = tempfile::tempdir().expect("tempdir");
        persist::authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        let rec = Record::new("pCLI", format!("ATG{}TAA", "AAA".repeat(40)), true);
        let entry = record_to_library_entry(&rec).expect("gb");
        let mut store = LibraryStore::load(&layout);
        store.keep(entry, None);
        store.persist(&layout).expect("persist");
        persist::revoke_thread_writes();
        (tmp, layout)
    }

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-cli");
        assert_eq!(IMPLEMENTATION_STAGE, 14);
    }

    #[test]
    fn version_command_succeeds() {
        run(Cli {
            command: Some(Commands::Version),
        })
        .expect("version");
    }

    #[test]
    fn call_targets_the_same_registry() {
        let names = agent::builtin().names();
        assert!(names.contains(&"list-library"));
        assert!(names.contains(&"find-orfs"));
        assert!(!names.iter().any(|n| agent::is_forbidden_name(n)));
    }

    #[test]
    fn token_path_uses_rust_leaf_never_python() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let prev_xdg = std::env::var_os("XDG_DATA_HOME");
        let prev_sc = std::env::var_os("SPLICECRAFT_DATA_DIR");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", tmp.path());
            std::env::remove_var("SPLICECRAFT_DATA_DIR");
        }
        let path = token_file_path().expect("path");
        assert!(
            path.ends_with(format!("{XDG_DATA_DIR_LEAF}/agent_token"))
                || path
                    .components()
                    .any(|c| c.as_os_str() == XDG_DATA_DIR_LEAF)
        );
        assert!(!path_has_python_leaf(&path));
        unsafe {
            std::env::set_var(
                "SPLICECRAFT_DATA_DIR",
                tmp.path().join(PYTHON_XDG_DATA_DIR_LEAF),
            );
        }
        assert!(token_file_path().is_err());
        unsafe {
            match prev_xdg {
                Some(v) => std::env::set_var("XDG_DATA_HOME", v),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match prev_sc {
                Some(v) => std::env::set_var("SPLICECRAFT_DATA_DIR", v),
                None => std::env::remove_var("SPLICECRAFT_DATA_DIR"),
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::await_holding_lock)]
    async fn call_hits_the_same_registry_over_http() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (_tmp, layout) = sandbox();
        let mut session = agent::AgentSession::load(layout.clone());
        session.headless = true;
        let handle = start_server(
            session,
            ServerConfig {
                port: 0,
                headless: true,
                write_token_file: true,
                authorize_process_writes: false,
            },
        )
        .await
        .expect("bind");
        assert!(handle.addr.ip().is_loopback());
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let prev_sc = std::env::var_os("SPLICECRAFT_DATA_DIR");
        unsafe {
            std::env::set_var("SPLICECRAFT_DATA_DIR", &layout.root);
        }
        let result = execute_call("list-library", "GET", None, true).expect("call");
        assert_eq!(result.http_code, 200, "{}", result.body);
        assert_eq!(result.body["library"][0]["name"], "pCLI");

        let via_dispatch = {
            let mut s = agent::AgentSession::load(layout);
            agent::dispatch(agent::builtin(), &mut s, "list-library", &json!({}))
        };
        assert_eq!(
            result.body["library"][0]["name"],
            via_dispatch.body["library"][0]["name"]
        );

        unsafe {
            match prev_sc {
                Some(v) => std::env::set_var("SPLICECRAFT_DATA_DIR", v),
                None => std::env::remove_var("SPLICECRAFT_DATA_DIR"),
            }
        }
        handle.shutdown().await;
    }
}
