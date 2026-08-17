//! Handler registry. `/tools` is built from this map.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::session::AgentSession;

/// Names that must never be registered (master wipe / whole-library delete).
pub const FORBIDDEN_ENDPOINT_NAMES: &[&str] = &[
    "wipe",
    "master-delete",
    "master_delete",
    "master-wipe",
    "wipe-library",
    "wipe-all",
    "delete-all",
    "delete-library",
    "clear-library",
    "factory-reset",
];

/// Preferred HTTP method advertised in `/tools`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointMethod {
    /// Read. GET or POST.
    Get,
    /// Mutation. POST only.
    Post,
}

impl EndpointMethod {
    /// `GET` or `POST`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// One registered endpoint.
pub struct Endpoint {
    /// URL path leaf (`list-library`).
    pub name: &'static str,
    /// Advertised method.
    pub method: EndpointMethod,
    /// Requires bearer + dirty-guard + persist authorisation on disk writes.
    pub write: bool,
    /// One-line summary.
    pub doc: &'static str,
    /// Full request-body documentation.
    pub doc_full: &'static str,
    /// JSON Schema for the request object.
    pub schema: Value,
    /// In-process handler (same path HTTP uses).
    pub handler: fn(&mut AgentSession, &Value) -> crate::dispatch::AgentResponse,
}

/// Name → endpoint.
pub struct Registry {
    endpoints: BTreeMap<&'static str, Endpoint>,
}

impl Registry {
    /// Empty map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            endpoints: BTreeMap::new(),
        }
    }

    /// Insert. Panics if the name is a forbidden wipe endpoint.
    pub fn register(&mut self, ep: Endpoint) {
        assert!(
            !is_forbidden_name(ep.name),
            "refusing to register forbidden agent endpoint {}",
            ep.name
        );
        self.endpoints.insert(ep.name, ep);
    }

    /// Lookup.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Endpoint> {
        self.endpoints.get(name)
    }

    /// Sorted names.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.endpoints.keys().copied().collect()
    }

    /// All endpoints in name order.
    pub fn iter(&self) -> impl Iterator<Item = &Endpoint> {
        self.endpoints.values()
    }

    /// Bare `/tools` body (no `data` envelope).
    #[must_use]
    pub fn tools_document(&self) -> Value {
        let endpoints: Vec<Value> = self
            .iter()
            .map(|ep| {
                serde_json::json!({
                    "name": ep.name,
                    "method": ep.method.as_str(),
                    "write": ep.write,
                    "doc": ep.doc,
                    "doc_full": ep.doc_full,
                    "schema": ep.schema,
                })
            })
            .collect();
        serde_json::json!({ "endpoints": endpoints })
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// True for master-wipe / whole-library-delete names.
#[must_use]
pub fn is_forbidden_name(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    FORBIDDEN_ENDPOINT_NAMES
        .iter()
        .any(|f| n == *f || n.contains(f))
}

/// Process-wide builtin registry.
#[must_use]
pub fn builtin() -> &'static Registry {
    use std::sync::OnceLock;
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(crate::handlers::register_builtin)
}
