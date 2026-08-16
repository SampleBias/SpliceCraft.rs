//! Sequence records and wrap-capable features.
//!
//! Coordinates are **0-based half-open**. A wrap feature is encoded as
//! `end < start` (tail `[start, total)` + head `[0, end)`). Convert to
//! 1-based GenBank at the I/O boundary (stage 03).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::circular::{bp_in, feat_len};

/// One linear interval of a (possibly compound) feature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeaturePart {
    /// Inclusive start (0-based).
    pub start: usize,
    /// Exclusive end.
    pub end: usize,
    /// `1` forward, `-1` reverse, `0` unknown.
    pub strand: i8,
}

/// An annotated interval on a [`Record`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feature {
    /// GenBank-style type (`CDS`, `misc_feature`, `resite`, …).
    pub kind: String,
    /// Start (0-based). For a wrap feature this is the tail start.
    pub start: usize,
    /// Exclusive end. `end < start` means the feature wraps the origin.
    pub end: usize,
    /// `1` forward, `-1` reverse, `0` unknown.
    pub strand: i8,
    /// Display / lookup label.
    pub label: String,
    /// Extra qualifiers (`/note`, `/gene`, …).
    pub qualifiers: BTreeMap<String, String>,
    /// Explicit compound parts. Empty means "use wrap encoding on start/end".
    pub parts: Vec<FeaturePart>,
}

impl Feature {
    /// Build a simple (possibly wrapping) feature.
    #[must_use]
    pub fn new(
        kind: impl Into<String>,
        start: usize,
        end: usize,
        strand: i8,
        label: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            start,
            end,
            strand,
            label: label.into(),
            qualifiers: BTreeMap::new(),
            parts: Vec::new(),
        }
    }

    /// Whether this feature uses wrap encoding (`end < start`).
    #[must_use]
    pub fn is_wrap(&self) -> bool {
        self.end < self.start
    }

    /// Wrap-aware length on a molecule of `total` bp. [INV-08]
    #[must_use]
    pub fn len_on(&self, total: usize) -> usize {
        feat_len(self.start, self.end, total)
    }

    /// Membership test. [INV-08] related wrap encoding.
    #[must_use]
    pub fn contains_bp(&self, bp: usize) -> bool {
        bp_in(bp, self.start, self.end)
    }

    /// Parts to shift during an edit. Wrap features become tail + head.
    #[must_use]
    pub fn effective_parts(&self, total: usize) -> Vec<FeaturePart> {
        if !self.parts.is_empty() {
            return self.parts.clone();
        }
        if self.end < self.start && total > 0 {
            return vec![
                FeaturePart {
                    start: 0,
                    end: self.end,
                    strand: self.strand,
                },
                FeaturePart {
                    start: self.start,
                    end: total,
                    strand: self.strand,
                },
            ];
        }
        vec![FeaturePart {
            start: self.start,
            end: self.end,
            strand: self.strand,
        }]
    }
}

/// An annotated DNA (or RNA) molecule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Display name.
    pub name: String,
    /// Stable id (often the same as `name`).
    pub id: String,
    /// Sequence bases. Callers should keep this uppercase.
    pub sequence: String,
    /// Circular plasmid vs linear fragment.
    pub circular: bool,
    /// Annotations in record coordinates.
    pub features: Vec<Feature>,
    /// `DNA`, `RNA`, …
    pub molecule_type: String,
}

impl Record {
    /// Empty named record.
    #[must_use]
    pub fn new(name: impl Into<String>, sequence: impl Into<String>, circular: bool) -> Self {
        let name = name.into();
        Self {
            id: name.clone(),
            name,
            sequence: sequence.into(),
            circular,
            features: Vec::new(),
            molecule_type: "DNA".into(),
        }
    }

    /// Sequence length in bp.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sequence.len()
    }

    /// Whether the sequence is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sequence.is_empty()
    }
}
