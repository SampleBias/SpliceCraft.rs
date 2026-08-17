//! Axum server bound to `127.0.0.1` only. Host allowlist + bearer token.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{
    HeaderMap, HeaderValue, Method, StatusCode, Uri,
    header::{AUTHORIZATION, CONTENT_TYPE, HOST},
};
use axum::response::Response;
use axum::routing::{any, get};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::dispatch::{dispatch, requires_post};
use crate::error::AgentError;
use crate::handlers;
use crate::registry::builtin;
use crate::session::AgentSession;
use crate::token::{generate_token, token_path, write_token_file};
use splicecraft_persist as persist;

/// Loopback only. Never `0.0.0.0`.
pub const BIND_HOST: Ipv4Addr = Ipv4Addr::LOCALHOST;
/// Upstream `_AGENT_API_PORT_DEFAULT`.
pub const DEFAULT_PORT: u16 = 6701;

/// How to start the server.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// `0` asks the OS for an ephemeral port (tests).
    pub port: u16,
    /// Surfaced by `/healthz`.
    pub headless: bool,
    /// Write `<data-dir>/agent_token`.
    pub write_token_file: bool,
    /// Process-wide persist opt-in (production). Tests leave this false.
    pub authorize_process_writes: bool,
}

/// Running server.
pub struct ServerHandle {
    /// Bound address (always loopback).
    pub addr: SocketAddr,
    /// Bearer token.
    pub token: String,
    /// Token file path (may not exist if `write_token_file` was false).
    pub token_path: std::path::PathBuf,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<Result<(), std::io::Error>>>,
}

impl ServerHandle {
    /// Stop the server and remove the token file.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
        let _ = std::fs::remove_file(&self.token_path);
    }
}

#[derive(Clone)]
struct AppState {
    session: Arc<Mutex<AgentSession>>,
    token: String,
    headless: bool,
}

/// Bind `127.0.0.1` and serve.
pub async fn start_server(
    mut session: AgentSession,
    cfg: ServerConfig,
) -> Result<ServerHandle, AgentError> {
    if cfg.authorize_process_writes {
        persist::authorize_writes("splicecraft-agent");
    }
    session.headless = cfg.headless;
    let token = generate_token()?;
    let token_file = token_path(&session.layout);
    let addr = SocketAddr::from((BIND_HOST, cfg.port));
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    if !bound.ip().is_loopback() {
        return Err(AgentError::Message(format!(
            "refusing to serve on non-loopback {}",
            bound.ip()
        )));
    }
    if cfg.write_token_file {
        write_token_file(&token_file, bound.port(), &token)?;
    }
    let state = AppState {
        session: Arc::new(Mutex::new(session)),
        token: token.clone(),
        headless: cfg.headless,
    };
    let app = router(state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });
    Ok(ServerHandle {
        addr: bound,
        token,
        token_path: token_file,
        shutdown: Some(shutdown_tx),
        join: Some(join),
    })
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz_http))
        .route("/tools", get(tools_http).post(tools_http))
        .route("/", get(tools_http))
        .route("/{name}", any(dispatch_http))
        .with_state(state)
}

async fn healthz_http(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(resp) = reject_host(&headers) {
        return resp;
    }
    let mut session = state.session.lock().unwrap_or_else(|e| e.into_inner());
    session.headless = state.headless;
    json_response(200, &handlers::healthz(&mut session, &json!({})).body)
}

async fn tools_http(State(_state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(resp) = reject_host(&headers) {
        return resp;
    }
    json_response(200, &builtin().tools_document())
}

async fn dispatch_http(
    State(state): State<AppState>,
    Path(name): Path<String>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(resp) = reject_host(&headers) {
        return resp;
    }
    if !check_token(&headers, &state.token) {
        return json_response(401, &json!({"error": "missing or invalid bearer token"}));
    }
    if method == Method::GET {
        if requires_post(builtin(), &name) {
            return method_not_allowed(&name);
        }
        let payload = query_from_uri(&uri);
        return run_dispatch(&state, &name, &payload);
    }
    if method != Method::POST {
        return json_response(
            405,
            &json!({"error": format!("method {method} not allowed"), "allow": "GET, POST"}),
        );
    }
    let payload = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(map)) => Value::Object(map),
            Ok(Value::Null) => json!({}),
            Ok(_) => return json_response(400, &json!({"error": "JSON body must be an object"})),
            Err(_) => return json_response(400, &json!({"error": "malformed JSON body"})),
        }
    };
    run_dispatch(&state, &name, &payload)
}

fn query_from_uri(uri: &Uri) -> Value {
    let mut map = HashMap::new();
    if let Some(q) = uri.query() {
        for pair in q.split('&') {
            if pair.is_empty() {
                continue;
            }
            if let Some((k, v)) = pair.split_once('=') {
                map.insert(k.to_owned(), v.to_owned());
            } else {
                map.insert(pair.to_owned(), String::new());
            }
        }
    }
    query_to_json(&map)
}

fn run_dispatch(state: &AppState, name: &str, payload: &Value) -> Response {
    let mut session = state.session.lock().unwrap_or_else(|e| e.into_inner());
    let resp = dispatch(builtin(), &mut session, name, payload);
    if name != "healthz" && name != "tools" && (200..300).contains(&resp.status) {
        persist::log_event(
            "agent.ok",
            &[("endpoint", name), ("status", &resp.status.to_string())],
        );
    }
    json_response(resp.status, &resp.body)
}

fn query_to_json(query: &HashMap<String, String>) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in query {
        if v == "true" {
            map.insert(k.clone(), Value::Bool(true));
        } else if v == "false" {
            map.insert(k.clone(), Value::Bool(false));
        } else if let Ok(n) = v.parse::<i64>() {
            map.insert(k.clone(), json!(n));
        } else {
            map.insert(k.clone(), Value::String(v.clone()));
        }
    }
    Value::Object(map)
}

fn check_token(headers: &HeaderMap, expected: &str) -> bool {
    let Some(val) = headers.get(AUTHORIZATION) else {
        return false;
    };
    let Ok(s) = val.to_str() else {
        return false;
    };
    let provided = s
        .strip_prefix("Bearer ")
        .or_else(|| s.strip_prefix("bearer "))
        .unwrap_or(s);
    provided == expected
}

fn reject_host(headers: &HeaderMap) -> Option<Response> {
    let val = headers.get(HOST)?;
    let Ok(host) = val.to_str() else {
        return Some(json_response(403, &json!({"error": "forbidden host"})));
    };
    if !host_is_loopback(host) {
        return Some(json_response(403, &json!({"error": "forbidden host"})));
    }
    None
}

/// DNS-rebinding defence: Host must be loopback when present.
#[must_use]
pub fn host_is_loopback(host: &str) -> bool {
    let h = host.trim();
    let hostname = if let Some(rest) = h.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        match h.rsplit_once(':') {
            Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) => name,
            _ => h,
        }
    };
    matches!(
        hostname.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "localhost" | "::1"
    )
}

fn method_not_allowed(name: &str) -> Response {
    let body = json!({
        "error": format!("endpoint {name:?} requires POST (mutation)"),
        "allow": "POST",
    });
    let mut resp = json_response(405, &body);
    resp.headers_mut()
        .insert(axum::http::header::ALLOW, HeaderValue::from_static("POST"));
    resp
}

fn json_response(status: u16, body: &Value) -> Response {
    let bytes = serde_json::to_vec(body).unwrap_or_else(|_| b"{\"error\":\"serialize\"}".to_vec());
    let mut resp = Response::new(Body::from(bytes));
    *resp.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    resp.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    resp
}

/// Headless blocking entry (`splicecraft --headless`).
pub fn run_headless(port: u16) -> Result<(), AgentError> {
    persist::authorize_writes("splicecraft-agent");
    let layout = persist::DataLayout::resolve()?;
    let mut session = AgentSession::load(layout);
    session.headless = true;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let handle = start_server(
            session,
            ServerConfig {
                port,
                headless: true,
                write_token_file: true,
                authorize_process_writes: false,
            },
        )
        .await?;
        println!(
            "SpliceCraft agent API ready on http://127.0.0.1:{}/ (token file: {})",
            handle.addr.port(),
            handle.token_path.display()
        );
        tokio::signal::ctrl_c().await?;
        handle.shutdown().await;
        Ok(())
    })
}

/// Background thread for `splicecraft --agent` beside the TUI.
pub fn spawn_background(port: u16, headless: bool) -> Result<BackgroundAgent, AgentError> {
    persist::authorize_writes("splicecraft-agent");
    let layout = persist::DataLayout::resolve()?;
    let mut session = AgentSession::load(layout);
    session.headless = headless;
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let join = std::thread::Builder::new()
        .name("splicecraft-agent-api".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            rt.block_on(async move {
                match start_server(
                    session,
                    ServerConfig {
                        port,
                        headless,
                        write_token_file: true,
                        authorize_process_writes: false,
                    },
                )
                .await
                {
                    Ok(handle) => {
                        let _ = ready_tx.send(Ok((
                            handle.addr,
                            handle.token.clone(),
                            handle.token_path.clone(),
                        )));
                        std::future::pending::<()>().await;
                        drop(handle);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                    }
                }
            });
        })?;
    match ready_rx.recv() {
        Ok(Ok((addr, token, token_path))) => Ok(BackgroundAgent {
            addr,
            token,
            token_path,
            _join: join,
        }),
        Ok(Err(e)) => Err(AgentError::Message(e)),
        Err(_) => Err(AgentError::Message("agent thread died before bind".into())),
    }
}

/// Handle for a background agent thread.
pub struct BackgroundAgent {
    /// Bound address.
    pub addr: SocketAddr,
    /// Bearer token.
    pub token: String,
    /// Token file.
    pub token_path: std::path::PathBuf,
    _join: std::thread::JoinHandle<()>,
}
