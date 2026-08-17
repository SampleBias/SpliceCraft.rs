//! Primer records: Designed → Ordered → Validated.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use splicecraft_persist::{DataLayout, PersistError, load_primers, save_primers};

/// Lifecycle status stored on each primer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimerStatus {
    /// Fresh from a designer.
    #[default]
    Designed,
    /// Submitted to a vendor.
    Ordered,
    /// Wet-lab confirmed.
    Validated,
}

impl PrimerStatus {
    /// Designed → Ordered → Validated → Designed.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Designed => Self::Ordered,
            Self::Ordered => Self::Validated,
            Self::Validated => Self::Designed,
        }
    }
}

impl std::fmt::Display for PrimerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Designed => write!(f, "Designed"),
            Self::Ordered => write!(f, "Ordered"),
            Self::Validated => write!(f, "Validated"),
        }
    }
}

/// One oligo in `primers.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PrimerRecord {
    /// Display name (spaces preserved).
    pub name: String,
    /// Oligo, 5′→3′.
    pub sequence: String,
    /// Optional Tm (°C).
    #[serde(default)]
    pub tm: Option<f64>,
    /// Lifecycle.
    #[serde(default)]
    pub status: PrimerStatus,
    /// `generic` / `cloning` / `detection` / `imported` / …
    #[serde(default)]
    pub primer_type: String,
    /// Optional IDT scale override.
    #[serde(default)]
    pub scale: Option<String>,
    /// Optional IDT purification override.
    #[serde(default)]
    pub purification: Option<String>,
}

/// In-memory primer library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrimerStore {
    /// Live list.
    pub primers: Vec<PrimerRecord>,
}

impl PrimerStore {
    /// Load from a layout (missing file → empty).
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let loaded = load_primers(layout);
        let primers = loaded
            .entries
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        Self { primers }
    }

    /// Persist through the chokepoint. Tests must sandbox `XDG_DATA_HOME`.
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        let entries: Result<Vec<Value>, _> =
            self.primers.iter().map(serde_json::to_value).collect();
        save_primers(layout, &entries?)
    }

    /// Append a designed oligo.
    pub fn add(&mut self, rec: PrimerRecord) {
        self.primers.push(rec);
    }

    /// Cycle status on `idx`.
    pub fn cycle_status(&mut self, idx: usize) {
        if let Some(p) = self.primers.get_mut(idx) {
            p.status = p.status.cycle();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_persist::{DataLayout, authorize_writes_for_sandbox};

    #[test]
    fn status_cycles() {
        assert_eq!(PrimerStatus::Designed.cycle(), PrimerStatus::Ordered);
        assert_eq!(PrimerStatus::Ordered.cycle(), PrimerStatus::Validated);
        assert_eq!(PrimerStatus::Validated.cycle(), PrimerStatus::Designed);
    }

    #[test]
    fn primer_library_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        let mut store = PrimerStore::load(&layout);
        store.add(PrimerRecord {
            name: "existing primer 1".into(),
            sequence: "ACGT".into(),
            status: PrimerStatus::Designed,
            ..PrimerRecord::default()
        });
        store.persist(&layout).unwrap();
        let again = PrimerStore::load(&layout);
        assert_eq!(again.primers.len(), 1);
        assert_eq!(again.primers[0].name, "existing primer 1");
        assert_eq!(again.primers[0].status, PrimerStatus::Designed);
    }
}
