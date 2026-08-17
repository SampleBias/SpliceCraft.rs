//! Persistent gel library. Writes go through `safe_save_json`. [INV-07]

use serde_json::Value;

use splicecraft_persist::{DataLayout, PersistError, load_gels, log_event, save_gels};

use crate::entry::{GelEntry, GelLaneJson, normalise_gel_entry};

/// In-memory gel snapshots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GelStore {
    /// Normalised entries.
    pub entries: Vec<GelEntry>,
}

impl GelStore {
    /// Empty library.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load from a layout (missing file → empty).
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let loaded = load_gels(layout);
        let mut entries = Vec::new();
        for v in &loaded.entries {
            if let Ok(raw) = serde_json::from_value::<GelEntry>(v.clone()) {
                entries.push(normalise_gel_entry(raw, false));
            }
        }
        Self { entries }
    }

    /// Persist through the chokepoint.
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        let values: Vec<Value> = self
            .entries
            .iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect();
        save_gels(layout, &values)?;
        log_event("gels.saved", &[("n", &self.entries.len().to_string())]);
        Ok(())
    }

    /// Insert or replace by id after normalisation.
    pub fn upsert(&mut self, entry: GelEntry) {
        let e = normalise_gel_entry(entry, false);
        if let Some(existing) = self.entries.iter_mut().find(|x| x.id == e.id) {
            *existing = e;
        } else {
            self.entries.push(e);
        }
    }
}

/// Encode a live lane list into a fresh gel snapshot.
#[must_use]
pub fn snapshot_gel(
    name: &str,
    agarose_pct: f64,
    lanes: &[crate::lanes::GelLane],
    notes: &str,
) -> GelEntry {
    let raw = GelEntry {
        id: String::new(),
        name: name.into(),
        notes: notes.into(),
        agarose_pct,
        lanes: lanes.iter().map(GelLaneJson::from_lane).collect(),
        created_at: String::new(),
        updated_at: String::new(),
        extra: serde_json::Map::new(),
    };
    normalise_gel_entry(raw, true)
}
