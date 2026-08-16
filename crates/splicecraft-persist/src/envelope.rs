//! Schema envelope `{"_schema_version": N, "entries": [...]}` and legacy lists.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

/// Current on-disk schema stamp.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;

/// Load cap for the user's own library JSON (1 GiB).
pub const SAFE_LOAD_JSON_MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// Full `json.loads` + entry-count read-back below this size.
pub const SAVE_READBACK_FULL_PARSE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Newest timestamped backups retained per file.
pub const BACKUP_RETENTION_COUNT: usize = 10;

/// Never byte-prune below this many newest timestamped backups.
pub const BACKUP_MIN_KEEP: usize = 2;

/// Aggregate byte cap across timestamped backups of one file.
pub const BACKUP_TOTAL_SIZE_CAP_BYTES: u64 = 1024 * 1024 * 1024;

/// Lost-entries files retained.
pub const LOST_ENTRIES_RETENTION_COUNT: usize = 5;

static OBSERVED_SCHEMA: LazyLock<Mutex<HashMap<String, i64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Outcome of [`crate::safe_load_json`].
#[derive(Clone, Debug, PartialEq)]
pub struct LoadResult {
    /// Extracted entries (empty on missing / unrecoverable corrupt).
    pub entries: Vec<Value>,
    /// Soft warning (future schema, restored from `.bak`, …).
    pub warning: Option<String>,
}

/// Pull `entries` from an envelope or a legacy bare list.
///
/// Returns `(None, warning)` on an unknown shape so the loader can fall
/// through to `.bak`.
#[must_use]
pub fn extract_entries(raw: &Value, label: &str) -> (Option<Vec<Value>>, Option<String>) {
    match raw {
        Value::Array(items) => (Some(items.clone()), None),
        Value::Object(map) => {
            let Some(Value::Array(items)) = map.get("entries") else {
                return (
                    None,
                    Some(format!("{label}: unexpected JSON shape (object)")),
                );
            };
            let version = map.get("_schema_version").and_then(Value::as_i64);
            if let Some(v) = version
                && v > CURRENT_SCHEMA_VERSION
            {
                return (
                    Some(items.clone()),
                    Some(format!(
                        "{label} was written by a newer SpliceCraft \
                             (schema v{v} > v{CURRENT_SCHEMA_VERSION}) — some \
                             fields may be lost on save."
                    )),
                );
            }
            (Some(items.clone()), None)
        }
        other => (
            None,
            Some(format!(
                "{label}: unexpected JSON shape ({})",
                type_name(other)
            )),
        ),
    }
}

pub(crate) fn record_observed_schema(path: &Path, raw: &Value) {
    let Some(v) = raw.get("_schema_version").and_then(Value::as_i64) else {
        return;
    };
    if v > CURRENT_SCHEMA_VERSION
        && let Ok(mut map) = OBSERVED_SCHEMA.lock()
    {
        map.insert(path.to_string_lossy().into_owned(), v);
    }
}

pub(crate) fn schema_version_for_save(path: &Path, explicit: Option<i64>) -> i64 {
    if let Some(v) = explicit {
        return v;
    }
    let observed = OBSERVED_SCHEMA
        .lock()
        .ok()
        .and_then(|m| m.get(&path.to_string_lossy().into_owned()).copied())
        .unwrap_or(0);
    CURRENT_SCHEMA_VERSION.max(observed)
}

pub(crate) fn wrap_envelope(entries: &[Value], schema_version: i64) -> Value {
    serde_json::json!({
        "_schema_version": schema_version,
        "entries": entries,
    })
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
