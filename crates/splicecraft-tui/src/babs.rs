//! BABS Ollama client. Loopback only — no cloud LLM providers. [INV-139]
//!
//! Public-host SSRF stays intact: only `127.0.0.1` / `localhost` / `::1` may
//! be contacted. HuggingFace / NCBI fetches still go through the hardened
//! public opener and the online setting.

use splicecraft_io::{HttpRequest, HttpResponse, HttpTransport};

/// Default Ollama listen address.
pub const DEFAULT_OLLAMA_HOST: &str = "http://127.0.0.1:11434";

/// BABS client errors.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum BabsError {
    /// Host is not loopback.
    #[error("refusing Ollama URL {url}: host {host} is not loopback")]
    NotLoopback { url: String, host: String },
    /// Transport / HTTP failure (no sequence content).
    #[error("{0}")]
    Transport(String),
}

/// Slash command parsed from a chat line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BabsCommand {
    /// `/help`
    Help,
    /// `/clear`
    Clear,
    /// `/export` transcript
    Export,
    /// `/model name`
    Model(String),
}

/// Resolve `$SPLICECRAFT_OLLAMA_HOST` / `$OLLAMA_HOST`, defaulting to loopback.
/// Malformed values degrade to [`DEFAULT_OLLAMA_HOST`].
#[must_use]
pub fn ollama_base() -> String {
    let raw = std::env::var("SPLICECRAFT_OLLAMA_HOST")
        .ok()
        .or_else(|| std::env::var("OLLAMA_HOST").ok());
    ollama_base_from(raw.as_deref())
}

/// Parse a raw host string the same way [`ollama_base`] does (tests).
#[must_use]
pub fn ollama_base_from(raw: Option<&str>) -> String {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return DEFAULT_OLLAMA_HOST.to_owned();
    };
    let with_scheme = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("http://{raw}")
    };
    match parse_http_url(&with_scheme) {
        Some((scheme, host, port)) => {
            let host = if host.contains(':') {
                format!("[{host}]")
            } else {
                host
            };
            format!("{scheme}://{host}:{port}")
        }
        None => DEFAULT_OLLAMA_HOST.to_owned(),
    }
}

/// Refuse any URL whose host is not loopback. No DNS.
pub fn assert_ollama_loopback(url: &str) -> Result<(), BabsError> {
    let host = url_host(url).ok_or_else(|| BabsError::NotLoopback {
        url: url.to_owned(),
        host: String::new(),
    })?;
    if is_loopback_host(&host) {
        Ok(())
    } else {
        Err(BabsError::NotLoopback {
            url: url.to_owned(),
            host,
        })
    }
}

/// POST `/api/chat` (non-streaming). The URL is asserted loopback first.
pub fn ollama_chat(
    transport: &impl HttpTransport,
    base: &str,
    model: &str,
    messages: &[(&str, &str)],
) -> Result<String, BabsError> {
    assert_ollama_loopback(base)?;
    let url = format!("{}/api/chat", base.trim_end_matches('/'));
    assert_ollama_loopback(&url)?;
    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|(role, content)| serde_json::json!({"role": role, "content": content}))
        .collect();
    let body = serde_json::json!({
        "model": model,
        "messages": msgs,
        "stream": false,
    });
    let resp = transport
        .execute(&HttpRequest {
            method: "POST".into(),
            url,
            body: body.to_string().into_bytes(),
            headers: vec![("Content-Type".into(), "application/json".into())],
        })
        .map_err(|e| BabsError::Transport(e.to_string()))?;
    parse_chat_response(&resp)
}

/// GET `/api/tags`.
pub fn ollama_list_models(
    transport: &impl HttpTransport,
    base: &str,
) -> Result<Vec<String>, BabsError> {
    assert_ollama_loopback(base)?;
    let url = format!("{}/api/tags", base.trim_end_matches('/'));
    assert_ollama_loopback(&url)?;
    let resp = transport
        .execute(&HttpRequest {
            method: "GET".into(),
            url,
            body: Vec::new(),
            headers: Vec::new(),
        })
        .map_err(|e| BabsError::Transport(e.to_string()))?;
    if resp.status >= 400 {
        return Err(BabsError::Transport(format!("HTTP {}", resp.status)));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&resp.body).map_err(|e| BabsError::Transport(e.to_string()))?;
    Ok(v.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default())
}

/// Strip `<think>…</think>` blocks from a model reply.
#[must_use]
pub fn strip_think(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start + 7..].find("</think>") {
            rest = &rest[start + 7 + end + 8..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

/// Parse a leading slash command.
#[must_use]
pub fn parse_command(line: &str) -> Option<BabsCommand> {
    let t = line.trim();
    if !t.starts_with('/') {
        return None;
    }
    let mut parts = t.splitn(2, char::is_whitespace);
    let cmd = parts.next()?.to_ascii_lowercase();
    let arg = parts.next().unwrap_or("").trim();
    match cmd.as_str() {
        "/help" => Some(BabsCommand::Help),
        "/clear" => Some(BabsCommand::Clear),
        "/export" => Some(BabsCommand::Export),
        "/model" if !arg.is_empty() => Some(BabsCommand::Model(arg.to_owned())),
        _ => None,
    }
}

/// Drop oldest turns until `chars` is under `budget`.
#[must_use]
pub fn trim_history(turns: &[(String, String)], budget: usize) -> Vec<(String, String)> {
    let mut out = turns.to_vec();
    while out.len() > 1 {
        let n: usize = out.iter().map(|(a, b)| a.len() + b.len()).sum();
        if n <= budget {
            break;
        }
        out.remove(0);
    }
    out
}

fn parse_chat_response(resp: &HttpResponse) -> Result<String, BabsError> {
    if resp.status >= 400 {
        return Err(BabsError::Transport(format!("HTTP {}", resp.status)));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&resp.body).map_err(|e| BabsError::Transport(e.to_string()))?;
    let text = v
        .pointer("/message/content")
        .or_else(|| v.get("response"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_owned();
    Ok(strip_think(&text))
}

fn parse_http_url(raw: &str) -> Option<(String, String, u16)> {
    let (scheme, rest) = raw.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let hostport = rest.split('/').next().unwrap_or("");
    if hostport.is_empty() {
        return None;
    }
    let (host, port) = if hostport.starts_with('[') {
        let end = hostport.find(']')?;
        let host = hostport[1..end].to_owned();
        let port = hostport[end + 1..]
            .strip_prefix(':')
            .and_then(|p| p.parse().ok())
            .unwrap_or(if scheme == "https" { 443 } else { 11434 });
        (host, port)
    } else if let Some((h, p)) = hostport.rsplit_once(':') {
        let port: u16 = p.parse().ok()?;
        (h.to_owned(), port)
    } else {
        (
            hostport.to_owned(),
            if scheme == "https" { 443 } else { 11434 },
        )
    };
    if host.is_empty() {
        return None;
    }
    Some((scheme.to_owned(), host, port))
}

fn url_host(url: &str) -> Option<String> {
    parse_http_url(url).map(|(_, host, _)| host.to_ascii_lowercase())
}

fn is_loopback_host(host: &str) -> bool {
    let h = host
        .trim()
        .trim_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if h == "localhost" || h == "::1" || h == "0:0:0:0:0:0:0:1" {
        return true;
    }
    if let Ok(ip) = h.parse::<std::net::Ipv4Addr>() {
        return ip.is_loopback();
    }
    if let Ok(ip) = h.parse::<std::net::Ipv6Addr>() {
        return ip.is_loopback();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_io::IoError;

    struct MockLoopback {
        body: Vec<u8>,
    }

    impl HttpTransport for MockLoopback {
        fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, IoError> {
            let host = url_host(&req.url).unwrap_or_default();
            assert!(
                is_loopback_host(&host),
                "mock must only see loopback, got {}",
                req.url
            );
            Ok(HttpResponse {
                status: 200,
                body: self.body.clone(),
            })
        }
    }

    #[test]
    fn ollama_base_defaults_and_malformed_degrade() {
        assert_eq!(ollama_base_from(None), DEFAULT_OLLAMA_HOST);
        assert_eq!(ollama_base_from(Some("")), DEFAULT_OLLAMA_HOST);
        assert_eq!(
            ollama_base_from(Some("127.0.0.1:11434")),
            "http://127.0.0.1:11434"
        );
        assert_eq!(ollama_base_from(Some("host:999999")), DEFAULT_OLLAMA_HOST);
        assert_eq!(ollama_base_from(Some("ftp://x")), DEFAULT_OLLAMA_HOST);
    }

    #[test]
    fn refuse_public_url() {
        for url in [
            "https://example.com/api/chat",
            "http://8.8.8.8:11434",
            "http://1.1.1.1:11434/api/tags",
            "https://ollama.com",
        ] {
            let err = assert_ollama_loopback(url).expect_err(url);
            assert!(
                matches!(err, BabsError::NotLoopback { .. }),
                "{url} → {err:?}"
            );
        }
        assert!(assert_ollama_loopback("http://127.0.0.1:11434").is_ok());
        assert!(assert_ollama_loopback("http://localhost:11434").is_ok());
        assert!(assert_ollama_loopback("http://[::1]:11434").is_ok());
    }

    #[test]
    fn chat_mocks_loopback_and_refuses_public() {
        let mock = MockLoopback {
            body: br#"{"message":{"content":"<think>no</think>hello"}}"#.to_vec(),
        };
        let reply =
            ollama_chat(&mock, "http://127.0.0.1:11434", "llama", &[("user", "hi")]).unwrap();
        assert_eq!(reply, "hello");
        let err =
            ollama_chat(&mock, "https://example.com", "llama", &[("user", "hi")]).unwrap_err();
        assert!(matches!(err, BabsError::NotLoopback { .. }));
    }

    #[test]
    fn slash_commands_and_trim() {
        assert_eq!(parse_command("/help"), Some(BabsCommand::Help));
        assert_eq!(parse_command("/clear"), Some(BabsCommand::Clear));
        assert_eq!(
            parse_command("/model llama3"),
            Some(BabsCommand::Model("llama3".into()))
        );
        assert_eq!(parse_command("hello"), None);
        let hist = vec![
            ("u".into(), "aaaa".into()),
            ("a".into(), "bbbb".into()),
            ("u".into(), "cccc".into()),
        ];
        let trimmed = trim_history(&hist, 10);
        assert!(trimmed.len() < hist.len() || trimmed.len() == 1);
    }
}
