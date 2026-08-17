//! Named plasmid collections, keep, feature snippets, and collision policy.
//!
//! Every write goes through [`crate::safe_save_json`]. [INV-07]

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use splicecraft_util::{natural_sort_key, sanitize_filename, sanitize_plasmid_name};

use crate::domain::{
    load_collections, load_features, load_library, save_collections, save_features, save_library,
};
use crate::error::PersistError;
use crate::event::log_event;
use crate::paths::DataLayout;

/// Upstream `_DEFAULT_COLLECTION_NAME`.
pub const DEFAULT_COLLECTION_NAME: &str = "Main Collection";

/// One plasmid in `library.json` / a collection's `plasmids` list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryEntry {
    /// Stable id (often a sanitised name).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Sequence length in bp (never log the bases).
    pub size: usize,
    /// GenBank text. Empty is allowed for fixture-only tests.
    #[serde(default)]
    pub gb_text: String,
    /// Provenance stamp (`plasmidsaurus:<run>:<sample>`). Never logged as sequence.
    #[serde(default)]
    pub source: String,
    /// Compact sequencing badges (no aligned strings — those stay in the TUI session).
    #[serde(default)]
    pub alignments: Vec<AlignmentBadge>,
    /// CommercialSaaS / construction history XML. Counts only in logs; never DNA.
    #[serde(default)]
    pub history_xml: String,
}

/// Library-column sequencing status. Counts only; no DNA payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlignmentBadge {
    /// Read / zip-member label.
    #[serde(default)]
    pub label: String,
    /// `verified` / `near` / `partial` / `divergent`.
    #[serde(default)]
    pub status: String,
    /// Honest identity display (`99.6%`, never a false `100%`).
    #[serde(default)]
    pub identity: String,
    /// Mismatched columns.
    #[serde(default)]
    pub n_mismatches: i64,
    /// Indel events (gap runs), not gapped bp.
    #[serde(default)]
    pub n_indels: i64,
    /// First variant in target coordinates, for jump-to-sequence.
    pub first_variant_bp: Option<usize>,
}

/// A named plasmid collection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    /// Unique display name.
    pub name: String,
    /// Optional prose.
    #[serde(default)]
    pub description: String,
    /// Plasmids in this collection.
    #[serde(default)]
    pub plasmids: Vec<LibraryEntry>,
}

/// Reusable feature snippet (`features.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureSnippet {
    /// Display name.
    pub name: String,
    /// GenBank-style type (`CDS`, `promoter`, …).
    pub feature_type: String,
    /// Snippet DNA (may be empty for a marker-only entry).
    #[serde(default)]
    pub sequence: String,
    /// `1` / `-1` / `0` (arrowless).
    #[serde(default = "default_strand")]
    pub strand: i8,
    /// Optional note.
    #[serde(default)]
    pub description: String,
    /// Optional `#RRGGBB`.
    #[serde(default)]
    pub color: Option<String>,
    /// Extra qualifiers.
    #[serde(default)]
    pub qualifiers: BTreeMap<String, Vec<String>>,
}

fn default_strand() -> i8 {
    1
}

/// How a candidate name relates to the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionClass {
    /// Name is free.
    New,
    /// Same name and same content.
    ExactCopy,
    /// Same name, different content.
    NameClash,
}

/// User (or test) decision. Overwrite is never implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionChoice {
    /// Leave the original; drop the new item.
    Skip,
    /// Keep both; rename the new item with ` COPY`.
    Copy,
    /// Replace the original with the new item.
    Overwrite,
    /// Abort the whole keep.
    Cancel,
}

/// Outcome of [`LibraryStore::keep`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeepOutcome {
    /// Library mutated (caller should [`LibraryStore::persist`]).
    Applied {
        /// Name that landed (may be a COPY suffix).
        name: String,
    },
    /// Caller must ask; nothing was written.
    NeedsChoice {
        /// Exact copy vs different content.
        class: CollisionClass,
        /// Existing display name.
        existing_name: String,
    },
    /// User cancelled.
    Cancelled,
}

/// In-memory collections + the live (active) plasmid list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryStore {
    /// All named collections.
    pub collections: Vec<Collection>,
    /// Active collection name.
    pub active: String,
    /// Live library (mirrors the active collection).
    pub plasmids: Vec<LibraryEntry>,
}

impl Default for LibraryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryStore {
    /// Empty Main Collection.
    #[must_use]
    pub fn new() -> Self {
        Self {
            collections: vec![Collection {
                name: DEFAULT_COLLECTION_NAME.into(),
                description: String::new(),
                plasmids: Vec::new(),
            }],
            active: DEFAULT_COLLECTION_NAME.into(),
            plasmids: Vec::new(),
        }
    }

    /// Load collections + library from `layout` (legacy lists accepted).
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let collections = decode_list::<Collection>(&load_collections(layout).entries);
        let plasmids = decode_list::<LibraryEntry>(&load_library(layout).entries);
        let mut store = if collections.is_empty() {
            let mut s = Self::new();
            s.plasmids = plasmids;
            s.sync_active_plasmids();
            s
        } else {
            let active = collections
                .iter()
                .find(|c| c.name == DEFAULT_COLLECTION_NAME)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| collections[0].name.clone());
            Self {
                collections,
                active,
                plasmids,
            }
        };
        if store.plasmids.is_empty()
            && let Some(col) = store.collections.iter().find(|c| c.name == store.active)
        {
            store.plasmids = col.plasmids.clone();
        }
        store.sort_plasmids();
        store
    }

    /// Write library.json + collections.json through the chokepoint.
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        let lib = encode_list(&self.plasmids)?;
        let cols = encode_list(&self.collections)?;
        save_library(layout, &lib)?;
        save_collections(layout, &cols)?;
        log_event(
            "library.saved",
            &[
                ("collection", &self.active),
                ("n", &self.plasmids.len().to_string()),
            ],
        );
        Ok(())
    }

    /// Keep `entry` into the active collection. Asks when a name collides.
    pub fn keep(
        &mut self,
        mut entry: LibraryEntry,
        choice: Option<CollisionChoice>,
    ) -> KeepOutcome {
        entry.name = sanitize_plasmid_name(&entry.name, "plasmid", 256);
        if entry.id.is_empty() {
            entry.id = sanitize_filename(&entry.name);
        }
        let class = classify_entry(&entry, &self.plasmids);
        match class {
            CollisionClass::New => {
                self.plasmids.push(entry.clone());
                self.sort_plasmids();
                self.sync_active_plasmids();
                KeepOutcome::Applied { name: entry.name }
            }
            CollisionClass::ExactCopy | CollisionClass::NameClash => match choice {
                None => KeepOutcome::NeedsChoice {
                    class,
                    existing_name: entry.name,
                },
                Some(CollisionChoice::Cancel) => KeepOutcome::Cancelled,
                Some(CollisionChoice::Skip) => KeepOutcome::Applied { name: entry.name },
                Some(CollisionChoice::Copy) => {
                    let taken: HashSet<String> =
                        self.plasmids.iter().map(|e| e.name.clone()).collect();
                    let ids: HashSet<String> = self.plasmids.iter().map(|e| e.id.clone()).collect();
                    entry.name = unique_copy_name(&entry.name, &taken);
                    entry.id = unique_id(&entry.id, &ids);
                    let name = entry.name.clone();
                    self.plasmids.push(entry);
                    self.sort_plasmids();
                    self.sync_active_plasmids();
                    KeepOutcome::Applied { name }
                }
                Some(CollisionChoice::Overwrite) => {
                    if let Some(slot) = self.plasmids.iter_mut().find(|e| e.name == entry.name) {
                        *slot = entry.clone();
                    } else {
                        self.plasmids.push(entry.clone());
                    }
                    self.sort_plasmids();
                    self.sync_active_plasmids();
                    KeepOutcome::Applied { name: entry.name }
                }
            },
        }
    }

    /// Plasmidsaurus / sequencing import: never clobber an existing row.
    ///
    /// Name collisions become a unique copy (`COPY` suffix). Exact sequence
    /// copies are skipped. The `source` stamp (`plasmidsaurus:…`) is left intact.
    pub fn import_without_overwrite(&mut self, entry: LibraryEntry) -> KeepOutcome {
        match classify_entry(&entry, &self.plasmids) {
            CollisionClass::ExactCopy => KeepOutcome::Applied { name: entry.name },
            CollisionClass::New => self.keep(entry, None),
            CollisionClass::NameClash => self.keep(entry, Some(CollisionChoice::Copy)),
        }
    }

    /// Remove the plasmid at `index` (session undo lives in the TUI).
    pub fn remove_at(&mut self, index: usize) -> Option<LibraryEntry> {
        if index >= self.plasmids.len() {
            return None;
        }
        let entry = self.plasmids.remove(index);
        self.sync_active_plasmids();
        Some(entry)
    }

    /// Put a previously removed plasmid back (does not re-sort until persist).
    pub fn restore_at(&mut self, index: usize, entry: LibraryEntry) {
        let i = index.min(self.plasmids.len());
        self.plasmids.insert(i, entry);
        self.sort_plasmids();
        self.sync_active_plasmids();
    }

    /// Natural-sort the live list (`pBin2` before `pBin10`).
    pub fn sort_plasmids(&mut self) {
        sort_entries_natural(&mut self.plasmids);
    }

    fn sync_active_plasmids(&mut self) {
        if let Some(col) = self.collections.iter_mut().find(|c| c.name == self.active) {
            col.plasmids = self.plasmids.clone();
        } else {
            self.collections.push(Collection {
                name: self.active.clone(),
                description: String::new(),
                plasmids: self.plasmids.clone(),
            });
        }
    }
}

/// Classify `new` against `existing` by display name + `gb_text`.
#[must_use]
pub fn classify_entry(new: &LibraryEntry, existing: &[LibraryEntry]) -> CollisionClass {
    classify_name_content(
        &new.name,
        &new.gb_text,
        existing
            .iter()
            .map(|e| (e.name.as_str(), e.gb_text.as_str())),
    )
}

/// Generic name/content classifier (plasmids, features, …).
pub fn classify_name_content<'a, I>(name: &str, content: &str, existing: I) -> CollisionClass
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    if name.is_empty() {
        return CollisionClass::New;
    }
    let mut saw_name = false;
    for (n, c) in existing {
        if n != name {
            continue;
        }
        saw_name = true;
        if c == content {
            return CollisionClass::ExactCopy;
        }
    }
    if saw_name {
        CollisionClass::NameClash
    } else {
        CollisionClass::New
    }
}

/// `{base} COPY`, `{base} COPY 2`, … until unique.
#[must_use]
pub fn unique_copy_name(base: &str, existing_names: &HashSet<String>) -> String {
    if !existing_names.contains(base) {
        return base.to_owned();
    }
    let candidate = format!("{base} COPY");
    if !existing_names.contains(&candidate) {
        return candidate;
    }
    for n in 2..10_000 {
        let candidate = format!("{base} COPY {n}");
        if !existing_names.contains(&candidate) {
            return candidate;
        }
    }
    format!("{base} COPY extra")
}

fn unique_id(base: &str, existing: &HashSet<String>) -> String {
    if !existing.contains(base) {
        return base.to_owned();
    }
    for n in 2..10_000 {
        let candidate = format!("{base}_{n}");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    format!("{base}_x")
}

/// Sort library rows with [`natural_sort_key`].
pub fn sort_entries_natural(entries: &mut [LibraryEntry]) {
    entries.sort_by_key(|a| natural_sort_key(&a.name));
}

/// Case-insensitive export stem collision (`pUC19.gb` vs `puc19.GB`).
#[must_use]
pub fn unique_export_stem(base: &str, taken_lower: &mut HashSet<String>) -> String {
    let stem = sanitize_filename(base);
    let mut candidate = stem.clone();
    let mut n = 2;
    while taken_lower.contains(&candidate.to_ascii_lowercase()) {
        candidate = format!("{stem}_{n}");
        n += 1;
    }
    taken_lower.insert(candidate.to_ascii_lowercase());
    candidate
}

/// Load feature snippets (non-dicts dropped).
#[must_use]
pub fn feature_library(layout: &DataLayout) -> Vec<FeatureSnippet> {
    decode_list(&load_features(layout).entries)
}

/// Insert or update a snippet. Collisions need an explicit [`CollisionChoice`].
pub fn upsert_feature(
    layout: &DataLayout,
    mut snippet: FeatureSnippet,
    choice: Option<CollisionChoice>,
) -> Result<KeepOutcome, PersistError> {
    snippet.name = sanitize_plasmid_name(&snippet.name, "feature", 256);
    if snippet.feature_type.eq_ignore_ascii_case("source") {
        return Err(PersistError::Commit(
            "feature library refuses a second source feature".into(),
        ));
    }
    let mut entries = feature_library(layout);
    let class = classify_name_content(
        &snippet.name,
        &snippet.sequence,
        entries
            .iter()
            .map(|e| (e.name.as_str(), e.sequence.as_str())),
    );
    let outcome = match class {
        CollisionClass::New => {
            let name = snippet.name.clone();
            entries.push(snippet);
            KeepOutcome::Applied { name }
        }
        CollisionClass::ExactCopy | CollisionClass::NameClash => match choice {
            None => {
                return Ok(KeepOutcome::NeedsChoice {
                    class,
                    existing_name: snippet.name,
                });
            }
            Some(CollisionChoice::Cancel) => KeepOutcome::Cancelled,
            Some(CollisionChoice::Skip) => KeepOutcome::Applied { name: snippet.name },
            Some(CollisionChoice::Copy) => {
                let taken: HashSet<String> = entries.iter().map(|e| e.name.clone()).collect();
                snippet.name = unique_copy_name(&snippet.name, &taken);
                let name = snippet.name.clone();
                entries.push(snippet);
                KeepOutcome::Applied { name }
            }
            Some(CollisionChoice::Overwrite) => {
                if let Some(slot) = entries.iter_mut().find(|e| e.name == snippet.name) {
                    *slot = snippet.clone();
                } else {
                    entries.push(snippet.clone());
                }
                KeepOutcome::Applied { name: snippet.name }
            }
        },
    };
    if matches!(outcome, KeepOutcome::Applied { .. }) {
        save_features(layout, &encode_list(&entries)?)?;
        log_event("features.saved", &[("n", &entries.len().to_string())]);
    }
    Ok(outcome)
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
    use crate::auth::{authorize_writes_for_sandbox, revoke_thread_writes};
    use crate::paths::DataLayout;

    fn sandbox() -> (tempfile::TempDir, DataLayout) {
        let tmp = tempfile::tempdir().expect("tempdir");
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        (tmp, layout)
    }

    fn entry(name: &str, gb: &str) -> LibraryEntry {
        LibraryEntry {
            id: name.into(),
            name: name.into(),
            size: gb.len(),
            gb_text: gb.into(),
            source: String::new(),
            alignments: Vec::new(),
            history_xml: String::new(),
        }
    }

    #[test]
    fn keep_then_reload_sees_entry() {
        let (_tmp, layout) = sandbox();
        let mut store = LibraryStore::load(&layout);
        let out = store.keep(entry("pKeep", "LOCUS pKeep"), None);
        assert!(matches!(out, KeepOutcome::Applied { ref name } if name == "pKeep"));
        store.persist(&layout).unwrap();
        let again = LibraryStore::load(&layout);
        assert!(
            again.plasmids.iter().any(|e| e.name == "pKeep"),
            "{:?}",
            again.plasmids
        );
        assert_eq!(again.active, DEFAULT_COLLECTION_NAME);
        assert!(
            again
                .collections
                .iter()
                .any(|c| c.name == DEFAULT_COLLECTION_NAME
                    && c.plasmids.iter().any(|e| e.name == "pKeep"))
        );
    }

    #[test]
    fn import_without_overwrite_never_clobbers() {
        let mut store = LibraryStore::new();
        assert!(matches!(
            store.keep(entry("pA", "LOCUS a"), None),
            KeepOutcome::Applied { .. }
        ));
        let clash = store.import_without_overwrite(entry("pA", "LOCUS different"));
        assert!(matches!(clash, KeepOutcome::Applied { .. }));
        assert_eq!(store.plasmids.len(), 2);
        let original = store
            .plasmids
            .iter()
            .find(|e| e.name == "pA")
            .expect("original");
        assert_eq!(original.gb_text, "LOCUS a");
        assert!(store.plasmids.iter().any(|e| e.name.contains("COPY")));
        let n = store.plasmids.len();
        store.import_without_overwrite(entry("pA", "LOCUS a"));
        assert_eq!(store.plasmids.len(), n);
    }

    #[test]
    fn remove_at_then_restore_at_round_trips() {
        let mut store = LibraryStore::new();
        store.keep(entry("pA", "LOCUS pA"), None);
        store.keep(entry("pB", "LOCUS pB"), None);
        let idx = store.plasmids.iter().position(|e| e.name == "pA").unwrap();
        let removed = store.remove_at(idx).expect("pA");
        assert_eq!(removed.name, "pA");
        assert!(!store.plasmids.iter().any(|e| e.name == "pA"));
        store.restore_at(idx, removed);
        assert!(store.plasmids.iter().any(|e| e.name == "pA"));
    }

    #[test]
    fn collision_copy_does_not_drop_original() {
        let mut store = LibraryStore::new();
        assert!(matches!(
            store.keep(entry("pUC19", "ORIGINAL"), None),
            KeepOutcome::Applied { .. }
        ));
        let ask = store.keep(entry("pUC19", "DIFFERENT"), None);
        assert!(matches!(
            ask,
            KeepOutcome::NeedsChoice {
                class: CollisionClass::NameClash,
                ..
            }
        ));
        let copied = store.keep(entry("pUC19", "DIFFERENT"), Some(CollisionChoice::Copy));
        assert!(matches!(copied, KeepOutcome::Applied { ref name } if name == "pUC19 COPY"));
        let names: Vec<_> = store.plasmids.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"pUC19"), "{names:?}");
        assert!(names.contains(&"pUC19 COPY"), "{names:?}");
        assert_eq!(
            store
                .plasmids
                .iter()
                .find(|e| e.name == "pUC19")
                .unwrap()
                .gb_text,
            "ORIGINAL"
        );
    }

    #[test]
    fn exact_copy_skip_leaves_one() {
        let mut store = LibraryStore::new();
        store.keep(entry("pUC19", "SAME"), None);
        let ask = store.keep(entry("pUC19", "SAME"), None);
        assert!(matches!(
            ask,
            KeepOutcome::NeedsChoice {
                class: CollisionClass::ExactCopy,
                ..
            }
        ));
        store.keep(entry("pUC19", "SAME"), Some(CollisionChoice::Skip));
        assert_eq!(store.plasmids.len(), 1);
    }

    #[test]
    fn natural_sort_pbin2_before_pbin10() {
        let mut entries = vec![
            entry("pBin10", "a"),
            entry("pBin2", "b"),
            entry("pBin1", "c"),
        ];
        sort_entries_natural(&mut entries);
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["pBin1", "pBin2", "pBin10"]);
    }

    #[test]
    fn unique_copy_increments_past_existing_copy() {
        let taken = HashSet::from(["FOO".into(), "FOO COPY".into()]);
        assert_eq!(unique_copy_name("FOO", &taken), "FOO COPY 2");
    }

    #[test]
    fn export_stem_is_case_insensitive() {
        let mut taken = HashSet::new();
        assert_eq!(unique_export_stem("pUC19", &mut taken), "pUC19");
        assert_eq!(unique_export_stem("puc19", &mut taken), "puc19_2");
    }

    #[test]
    fn unauthorized_persist_fails() {
        let (_tmp, layout) = sandbox();
        revoke_thread_writes();
        let store = LibraryStore::new();
        let err = store.persist(&layout).expect_err("gated");
        assert!(matches!(err, PersistError::Unauthorized { .. }));
    }

    #[test]
    fn feature_upsert_copy_keeps_original() {
        let (_tmp, layout) = sandbox();
        let snip = FeatureSnippet {
            name: "lacZ".into(),
            feature_type: "CDS".into(),
            sequence: "ATG".into(),
            strand: 1,
            description: String::new(),
            color: Some("#FF6347".into()),
            qualifiers: BTreeMap::new(),
        };
        upsert_feature(&layout, snip.clone(), None).unwrap();
        let mut other = snip.clone();
        other.sequence = "ATGCCC".into();
        let ask = upsert_feature(&layout, other.clone(), None).unwrap();
        assert!(matches!(ask, KeepOutcome::NeedsChoice { .. }));
        upsert_feature(&layout, other, Some(CollisionChoice::Copy)).unwrap();
        let loaded = feature_library(&layout);
        assert_eq!(loaded.len(), 2);
        assert!(
            loaded
                .iter()
                .any(|e| e.name == "lacZ" && e.sequence == "ATG")
        );
        assert!(loaded.iter().any(|e| e.name == "lacZ COPY"));
        assert_eq!(
            loaded
                .iter()
                .find(|e| e.name == "lacZ")
                .unwrap()
                .color
                .as_deref(),
            Some("#FF6347")
        );
    }

    #[test]
    fn feature_refuses_source_kind() {
        let (_tmp, layout) = sandbox();
        let err = upsert_feature(
            &layout,
            FeatureSnippet {
                name: "src".into(),
                feature_type: "source".into(),
                sequence: "A".into(),
                strand: 0,
                description: String::new(),
                color: None,
                qualifiers: BTreeMap::new(),
            },
            None,
        )
        .expect_err("source");
        assert!(err.to_string().contains("source"));
    }
}
