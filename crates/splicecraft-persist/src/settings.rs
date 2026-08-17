//! App preferences as `{key, value}` rows through the JSON chokepoint.
//!
//! Upstream `_save_settings` stores `[{key, value}, …]`. Online search stays
//! off until `allow_online_search` is ticked.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::domain::{load_settings, save_settings};
use crate::error::PersistError;
use crate::paths::DataLayout;

/// Agent / BLAST URL-API gate. Default false — sequences are never uploaded
/// until a human ticks this.
pub const SETTING_ALLOW_ONLINE_SEARCH: &str = "allow_online_search";
/// Name-only online lookups (stage 14/15). Separate from sequence upload.
pub const SETTING_ALLOW_ONLINE_LOOKUPS: &str = "allow_online_lookups";

/// Load settings as a key → JSON value map.
#[must_use]
pub fn load_settings_map(layout: &DataLayout) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    for entry in load_settings(layout).entries {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let Some(key) = obj.get("key").and_then(Value::as_str) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        let value = obj.get("value").cloned().unwrap_or(Value::Null);
        map.insert(key.to_owned(), value);
    }
    map
}

/// Persist a settings map as `{key, value}` rows.
pub fn save_settings_map(
    layout: &DataLayout,
    map: &BTreeMap<String, Value>,
) -> Result<(), PersistError> {
    let entries: Vec<Value> = map
        .iter()
        .map(|(k, v)| json!({"key": k, "value": v}))
        .collect();
    save_settings(layout, &entries)
}

/// Boolean setting with a default when missing or mistyped.
#[must_use]
pub fn setting_bool(map: &BTreeMap<String, Value>, key: &str, default: bool) -> bool {
    match map.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => {
            matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
        }
        _ => default,
    }
}

/// Write one boolean key (RMW through the chokepoint).
pub fn set_setting_bool(layout: &DataLayout, key: &str, value: bool) -> Result<(), PersistError> {
    let mut map = load_settings_map(layout);
    map.insert(key.to_owned(), Value::Bool(value));
    save_settings_map(layout, &map)
}

/// `allow_online_search` — off unless explicitly ticked.
#[must_use]
pub fn allow_online_search(layout: &DataLayout) -> bool {
    setting_bool(
        &load_settings_map(layout),
        SETTING_ALLOW_ONLINE_SEARCH,
        false,
    )
}

/// `allow_online_lookups` — name-only lookups; off unless ticked.
#[must_use]
pub fn allow_online_lookups(layout: &DataLayout) -> bool {
    setting_bool(
        &load_settings_map(layout),
        SETTING_ALLOW_ONLINE_LOOKUPS,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::authorize_writes_for_sandbox;

    fn sandbox() -> (tempfile::TempDir, DataLayout) {
        let tmp = tempfile::tempdir().expect("tempdir");
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        (tmp, layout)
    }

    #[test]
    fn online_search_defaults_off_and_roundtrips() {
        let (tmp, layout) = sandbox();
        assert!(layout.root.starts_with(tmp.path()));
        assert!(!allow_online_search(&layout));
        set_setting_bool(&layout, SETTING_ALLOW_ONLINE_SEARCH, true).unwrap();
        assert!(allow_online_search(&layout));
        set_setting_bool(&layout, SETTING_ALLOW_ONLINE_SEARCH, false).unwrap();
        assert!(!allow_online_search(&layout));
    }
}
