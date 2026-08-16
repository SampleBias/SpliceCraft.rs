//! Thin domain `_load_X` / `_save_X` shapes. Caches land in later stages.

use serde_json::Value;

use crate::envelope::LoadResult;
use crate::error::PersistError;
use crate::load::safe_load_json;
use crate::paths::DataLayout;
use crate::save::safe_save_json;

/// Persist the live plasmid library through the chokepoint.
pub fn save_library(layout: &DataLayout, entries: &[Value]) -> Result<(), PersistError> {
    safe_save_json(&layout.library_file(), entries, "Plasmid library")
}

/// Load the live plasmid library (envelope or legacy list).
#[must_use]
pub fn load_library(layout: &DataLayout) -> LoadResult {
    safe_load_json(&layout.library_file(), "Plasmid library")
}

/// Persist named collections.
pub fn save_collections(layout: &DataLayout, entries: &[Value]) -> Result<(), PersistError> {
    safe_save_json(&layout.collections_file(), entries, "Plasmid collections")
}

/// Load named collections.
#[must_use]
pub fn load_collections(layout: &DataLayout) -> LoadResult {
    safe_load_json(&layout.collections_file(), "Plasmid collections")
}

/// Persist the active parts bin.
pub fn save_parts_bin(layout: &DataLayout, entries: &[Value]) -> Result<(), PersistError> {
    safe_save_json(&layout.parts_bin_file(), entries, "Parts bin")
}

/// Load the active parts bin.
#[must_use]
pub fn load_parts_bin(layout: &DataLayout) -> LoadResult {
    safe_load_json(&layout.parts_bin_file(), "Parts bin")
}

/// Persist the primer library.
pub fn save_primers(layout: &DataLayout, entries: &[Value]) -> Result<(), PersistError> {
    safe_save_json(&layout.primers_file(), entries, "Primers")
}

/// Load the primer library.
#[must_use]
pub fn load_primers(layout: &DataLayout) -> LoadResult {
    safe_load_json(&layout.primers_file(), "Primers")
}

/// Persist reusable feature snippets.
pub fn save_features(layout: &DataLayout, entries: &[Value]) -> Result<(), PersistError> {
    safe_save_json(&layout.features_file(), entries, "Features")
}

/// Load reusable feature snippets.
#[must_use]
pub fn load_features(layout: &DataLayout) -> LoadResult {
    safe_load_json(&layout.features_file(), "Features")
}
