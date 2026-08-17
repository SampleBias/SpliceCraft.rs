//! Construction history node on a product. Full History UI is stage 12.

use serde::{Deserialize, Serialize};

/// One construction step. Parent names only — never sequence bases.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryNode {
    /// `ligateFwd`, `ligateRev`, `gibson`, `goldenGate`, `l0FromSynFrag`.
    pub operation: String,
    /// Product display name.
    pub name: String,
    /// Product length (bp).
    pub seq_len: usize,
    /// Product topology.
    pub circular: bool,
    /// Parent record / part names (no DNA).
    pub parents: Vec<String>,
    /// Human note (enzyme pair, grammar id, …).
    pub note: String,
}

impl HistoryNode {
    /// Build a node. `parents` must not contain sequence.
    #[must_use]
    pub fn new(
        operation: impl Into<String>,
        name: impl Into<String>,
        seq_len: usize,
        circular: bool,
        parents: Vec<String>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            operation: operation.into(),
            name: name.into(),
            seq_len,
            circular,
            parents,
            note: note.into(),
        }
    }

    /// One-line comment stamped onto the product record (no bases).
    #[must_use]
    pub fn comment_line(&self) -> String {
        let parents = if self.parents.is_empty() {
            String::new()
        } else {
            format!(" parents={}", self.parents.join(","))
        };
        format!(
            "history: {} {} {}bp{}{}",
            self.operation,
            if self.circular { "circular" } else { "linear" },
            self.seq_len,
            parents,
            if self.note.is_empty() {
                String::new()
            } else {
                format!(" ({})", self.note)
            }
        )
    }
}
