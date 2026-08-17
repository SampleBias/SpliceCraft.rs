//! In-process dispatch — the same registry HTTP and the CLI exercise.

use serde_json::{Value, json};

use crate::paths::scrub_path;
use crate::registry::{EndpointMethod, Registry};
use crate::session::AgentSession;
use splicecraft_persist::{self as persist, PersistError};

/// Handler result.
#[derive(Clone, Debug)]
pub struct AgentResponse {
    /// HTTP status.
    pub status: u16,
    /// JSON body.
    pub body: Value,
}

impl AgentResponse {
    /// 200 with `body`.
    #[must_use]
    pub fn ok(body: Value) -> Self {
        Self { status: 200, body }
    }

    /// Error object `{"error": msg}`.
    #[must_use]
    pub fn err(status: u16, msg: impl Into<String>) -> Self {
        Self {
            status,
            body: json!({ "error": msg.into() }),
        }
    }
}

/// Dirty-guard: unsaved canvas + no `force` → 409.
///
/// `force` must be in the JSON body. A query `?force=1` is not honoured
/// (writes are POST-only and the dispatcher reads the body).
pub fn dirty_guard(session: &AgentSession, payload: &Value) -> Option<AgentResponse> {
    if session.dirty && !payload_force(payload) {
        return Some(AgentResponse {
            status: 409,
            body: json!({
                "error": "unsaved changes — pass {\"force\": true} to override",
                "dirty": true,
            }),
        });
    }
    None
}

/// `{"force": true}` in a JSON object.
#[must_use]
pub fn payload_force(payload: &Value) -> bool {
    payload
        .as_object()
        .and_then(|m| m.get("force"))
        .is_some_and(truthy)
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::String(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Value::Number(n) => n.as_i64() == Some(1),
        _ => false,
    }
}

/// Persist-chokepoint pre-check. Disk writes still go through `safe_save_json`.
pub fn require_persist_writes() -> Result<(), AgentResponse> {
    if persist::writes_authorized() {
        Ok(())
    } else {
        Err(AgentResponse::err(
            403,
            "data-dir writes are not authorised in this process",
        ))
    }
}

/// Map a persist failure to 403 / 500. Paths are scrubbed.
pub fn persist_save_error(label: &str, err: PersistError) -> AgentResponse {
    match err {
        PersistError::Unauthorized { .. } | PersistError::UnauthorizedDelete { .. } => {
            AgentResponse::err(403, "data-dir writes are not authorised in this process")
        }
        other => AgentResponse {
            status: 500,
            body: json!({
                "error": format!(
                    "save failed for {label}: {}",
                    scrub_path(&other.to_string())
                ),
            }),
        },
    }
}

/// Envelope authenticated 2xx with `ok` + `data` (healthz/tools stay bare).
#[must_use]
pub fn data_envelope(name: &str, mut response: AgentResponse) -> AgentResponse {
    if name == "healthz" || name == "tools" {
        return response;
    }
    if response.status != 200 {
        return response;
    }
    if let Value::Object(map) = &mut response.body {
        if !map.contains_key("ok") {
            map.insert("ok".into(), json!(true));
        }
        if !map.contains_key("data") {
            let data = Value::Object(map.clone());
            map.insert("data".into(), data);
        }
    }
    response
}

/// Run `name` against `session`. HTTP auth is applied *outside* this function.
pub fn dispatch(
    registry: &Registry,
    session: &mut AgentSession,
    name: &str,
    payload: &Value,
) -> AgentResponse {
    if name == "tools" {
        return AgentResponse::ok(registry.tools_document());
    }
    let Some(ep) = registry.get(name) else {
        return AgentResponse {
            status: 404,
            body: json!({
                "error": format!("unknown endpoint {name:?}"),
                "endpoints": registry.names(),
            }),
        };
    };
    if ep.write
        && let Some(guard) = dirty_guard(session, payload)
    {
        return guard;
    }
    let response = (ep.handler)(session, payload);
    data_envelope(name, response)
}

/// Whether `name` requires POST.
#[must_use]
pub fn requires_post(registry: &Registry, name: &str) -> bool {
    registry
        .get(name)
        .is_some_and(|ep| ep.write || ep.method == EndpointMethod::Post)
}
