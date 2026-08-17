//! Named enzyme collections, custom enzymes, and the active-collection pointer.
//!
//! Every write goes through [`crate::safe_save_json`]. [INV-07]

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{
    load_custom_enzymes, load_enzyme_active, load_enzyme_collections, save_custom_enzymes,
    save_enzyme_active, save_enzyme_collections,
};
use crate::error::PersistError;
use crate::event::log_event;
use crate::paths::DataLayout;

/// A named subset of enzyme names that scopes restriction scans.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnzymeCollection {
    /// Unique display name.
    pub name: String,
    /// Enzyme names (NEB and/or custom).
    #[serde(default)]
    pub enzymes: Vec<String>,
}

/// User-defined cutter stored beside the NEB catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomEnzymeRecord {
    /// Unique name.
    pub name: String,
    /// IUPAC recognition site.
    pub site: String,
    /// Forward cut offset.
    pub fwd_cut: i32,
    /// Reverse cut offset.
    pub rev_cut: i32,
    /// Optional type label (`II_blunt`, …).
    #[serde(default, rename = "type")]
    pub kind: String,
    /// Optional fridge / vendor note.
    #[serde(default)]
    pub supplier: String,
}

/// In-memory enzyme collections + custom catalog + active pointer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnzymeStore {
    /// Named collections.
    pub collections: Vec<EnzymeCollection>,
    /// Extra catalog entries.
    pub custom: Vec<CustomEnzymeRecord>,
    /// Active collection name, or `None` for the full catalog.
    pub active: Option<String>,
}

impl EnzymeStore {
    /// Load from a layout (missing files → empty).
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let collections = decode_list(&load_enzyme_collections(layout).entries);
        let custom = decode_list(&load_custom_enzymes(layout).entries);
        let active = load_enzyme_active(layout)
            .entries
            .first()
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        Self {
            collections,
            custom,
            active,
        }
    }

    /// Persist all three files through the chokepoint.
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        save_enzyme_collections(layout, &encode_list(&self.collections)?)?;
        save_custom_enzymes(layout, &encode_list(&self.custom)?)?;
        let active = match &self.active {
            Some(name) if !name.is_empty() => vec![serde_json::json!({"name": name})],
            _ => Vec::new(),
        };
        save_enzyme_active(layout, &active)?;
        log_event(
            "enzymes.saved",
            &[
                ("collections", &self.collections.len().to_string()),
                ("custom", &self.custom.len().to_string()),
            ],
        );
        Ok(())
    }

    /// Look up a collection by name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&EnzymeCollection> {
        self.collections.iter().find(|c| c.name == name)
    }

    /// Names the active collection allows, or `None` to scan the full catalog.
    #[must_use]
    pub fn allowed_enzymes(&self) -> Option<Vec<String>> {
        let name = self.active.as_deref()?;
        self.find(name).map(|c| c.enzymes.clone())
    }

    /// Custom enzymes as bio-layer extras (name/site/cuts only).
    #[must_use]
    pub fn custom_for_scan(&self) -> Vec<(String, String, i32, i32)> {
        self.custom
            .iter()
            .map(|e| (e.name.clone(), e.site.clone(), e.fwd_cut, e.rev_cut))
            .collect()
    }
}

fn decode_list<T: for<'de> Deserialize<'de>>(values: &[Value]) -> Vec<T> {
    values
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect()
}

fn encode_list<T: Serialize>(entries: &[T]) -> Result<Vec<Value>, PersistError> {
    entries
        .iter()
        .map(|e| serde_json::to_value(e).map_err(PersistError::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::authorize_writes_for_sandbox;
    use crate::paths::DataLayout;

    fn sandbox() -> (tempfile::TempDir, DataLayout) {
        let tmp = tempfile::tempdir().expect("tempdir");
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        (tmp, layout)
    }

    #[test]
    fn enzyme_collections_round_trip() {
        let (_tmp, layout) = sandbox();
        let mut store = EnzymeStore::load(&layout);
        store.collections.push(EnzymeCollection {
            name: "Common cloners".into(),
            enzymes: vec!["EcoRI".into(), "BamHI".into()],
        });
        store.active = Some("Common cloners".into());
        store.persist(&layout).unwrap();
        let again = EnzymeStore::load(&layout);
        assert_eq!(again.collections.len(), 1);
        assert_eq!(again.collections[0].name, "Common cloners");
        assert_eq!(again.collections[0].enzymes, ["EcoRI", "BamHI"]);
        assert_eq!(again.active.as_deref(), Some("Common cloners"));
        assert_eq!(
            again.allowed_enzymes().as_deref(),
            Some(["EcoRI".to_string(), "BamHI".to_string()].as_slice())
        );
    }

    #[test]
    fn custom_enzymes_round_trip() {
        let (_tmp, layout) = sandbox();
        let mut store = EnzymeStore::load(&layout);
        store.custom.push(CustomEnzymeRecord {
            name: "MyEnzI".into(),
            site: "GGTACC".into(),
            fwd_cut: 1,
            rev_cut: 5,
            kind: "II_5overhang".into(),
            supplier: "Lab fridge".into(),
        });
        store.persist(&layout).unwrap();
        let again = EnzymeStore::load(&layout);
        assert_eq!(again.custom.len(), 1);
        assert_eq!(again.custom[0].name, "MyEnzI");
        assert_eq!(again.custom[0].supplier, "Lab fridge");
        assert!(again.find("nope").is_none());
    }

    #[test]
    fn missing_active_means_full_catalog() {
        let store = EnzymeStore::default();
        assert!(store.allowed_enzymes().is_none());
        assert!(store.active.is_none());
    }
}
