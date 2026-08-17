//! Built-in Golden Braid L0 / MoClo Plant grammars and user-defined JSON.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use splicecraft_persist::{DataLayout, PersistError, load_grammars, save_grammars};

use crate::error::CloneError;

/// 4 nt pad for efficient terminal Type IIS cutting.
pub const GB_PAD: &str = "GCGC";
/// Esp3I recognition (GB L0).
pub const GB_L0_ENZYME_SITE: &str = "CGTCTC";
/// 1 nt between recognition and the overhang.
pub const GB_SPACER: &str = "A";
/// Esp3I isoschizomer name used by GB L0.
pub const GB_L0_ENZYME_NAME: &str = "Esp3I";

/// One position in a cloning grammar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrammarPosition {
    /// Display slot (`Pos 3-4`).
    pub name: String,
    /// Part type (`CDS`, `Promoter`, …).
    #[serde(rename = "type")]
    pub type_name: String,
    /// 5′ fusion overhang.
    pub oh5: String,
    /// 3′ fusion overhang.
    pub oh3: String,
    /// Palette colour name.
    #[serde(default)]
    pub color: String,
}

/// A Type IIS cloning grammar (built-in or user-defined).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grammar {
    /// Stable id (`gb_l0`, `moclo_plant`, …).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Current-level Type IIS enzyme.
    pub enzyme: String,
    /// Next-level enzyme (alternating cycle).
    #[serde(default)]
    pub level_up_enzyme: String,
    /// Recognition site of [`Self::enzyme`].
    pub site: String,
    /// Bases between site and overhang.
    pub spacer: String,
    /// 5′ pad on domestication primers.
    pub pad: String,
    /// Sites that must be absent from a domesticated part.
    #[serde(default)]
    pub forbidden_sites: BTreeMap<String, String>,
    /// Position table.
    pub positions: Vec<GrammarPosition>,
    /// Types that use ATG-fusion skip / codon repair (stage 09).
    #[serde(default)]
    pub coding_types: Vec<String>,
    /// User grammars are always editable.
    #[serde(default)]
    pub editable: bool,
}

impl Grammar {
    /// Look up a position by part type (exact, then case-insensitive).
    #[must_use]
    pub fn position_for_type(&self, part_type: &str) -> Option<&GrammarPosition> {
        self.positions
            .iter()
            .find(|p| p.type_name == part_type)
            .or_else(|| {
                let want = part_type.trim().to_ascii_lowercase();
                self.positions
                    .iter()
                    .find(|p| p.type_name.trim().eq_ignore_ascii_case(&want))
            })
    }

    /// TU boundary: first position's oh5 and last position's oh3.
    #[must_use]
    pub fn tu_overhangs(&self) -> (String, String) {
        match (self.positions.first(), self.positions.last()) {
            (Some(a), Some(b)) => (a.oh5.clone(), b.oh3.clone()),
            _ => (String::new(), String::new()),
        }
    }

    /// Coding part whose 5′ overhang embeds ATG → skip 3 bp on the forward bind.
    #[must_use]
    pub fn atg_offset(&self, oh5: &str, part_type: &str) -> usize {
        if !self.coding_types.iter().any(|t| t == part_type) {
            return 0;
        }
        let oh = oh5.to_ascii_uppercase();
        if oh.len() >= 4 && oh.ends_with("ATG") {
            3
        } else {
            0
        }
    }

    /// True when `part_type` is in this grammar's coding set.
    #[must_use]
    pub fn is_coding(&self, part_type: &str) -> bool {
        self.coding_types.iter().any(|t| t == part_type)
    }

    /// First of `type_names` that has both overhangs defined.
    #[must_use]
    pub fn position_overhangs(&self, type_names: &[&str]) -> Option<(String, String)> {
        for nm in type_names {
            if let Some(p) = self.position_for_type(nm)
                && !p.oh5.is_empty()
                && !p.oh3.is_empty()
            {
                return Some((p.oh5.clone(), p.oh3.clone()));
            }
        }
        None
    }
}

/// Golden Braid L0 (Esp3I) + MoClo Plant (BsaI).
#[must_use]
pub fn builtin_grammars() -> Vec<Grammar> {
    vec![gb_l0(), moclo_plant()]
}

/// Built-in Golden Braid L0.
#[must_use]
pub fn gb_l0() -> Grammar {
    Grammar {
        id: "gb_l0".into(),
        name: "Golden Braid L0".into(),
        enzyme: GB_L0_ENZYME_NAME.into(),
        level_up_enzyme: "BsaI".into(),
        site: GB_L0_ENZYME_SITE.into(),
        spacer: GB_SPACER.into(),
        pad: GB_PAD.into(),
        forbidden_sites: BTreeMap::from([
            ("BsaI".into(), "GGTCTC".into()),
            ("Esp3I".into(), "CGTCTC".into()),
        ]),
        positions: gb_positions(),
        coding_types: gb_coding_types(),
        editable: false,
    }
}

/// Built-in MoClo Plant (Weber 2011).
#[must_use]
pub fn moclo_plant() -> Grammar {
    Grammar {
        id: "moclo_plant".into(),
        name: "MoClo Plant (Weber 2011)".into(),
        enzyme: "BsaI".into(),
        level_up_enzyme: "BpiI".into(),
        site: "GGTCTC".into(),
        spacer: GB_SPACER.into(),
        pad: GB_PAD.into(),
        forbidden_sites: BTreeMap::from([
            ("BsaI".into(), "GGTCTC".into()),
            ("BpiI".into(), "GAAGAC".into()),
        ]),
        positions: vec![
            pos("Pos 1", "Promoter", "GGAG", "AATG", "green"),
            pos("Pos 2", "5' UTR", "AATG", "AGGT", "cyan"),
            pos("Pos 3", "CDS", "AGGT", "GCTT", "yellow"),
            pos("Pos 4", "C-tag", "GCTT", "GGTA", "magenta"),
            pos("Pos 5", "Terminator", "GGTA", "CGCT", "blue"),
        ],
        coding_types: vec!["CDS".into(), "C-tag".into()],
        editable: false,
    }
}

fn pos(name: &str, type_name: &str, oh5: &str, oh3: &str, color: &str) -> GrammarPosition {
    GrammarPosition {
        name: name.into(),
        type_name: type_name.into(),
        oh5: oh5.into(),
        oh3: oh3.into(),
        color: color.into(),
    }
}

fn gb_positions() -> Vec<GrammarPosition> {
    vec![
        pos("Pos 1", "Promoter", "GGAG", "AATG", "green"),
        pos("Pos 1a", "Promoter-only", "GGAG", "CCAT", "green"),
        pos("Pos 01-02", "Operator-A", "GGAG", "TCCC", "dark_green"),
        pos("Pos 02", "Operator-B", "TGAC", "TCCC", "dark_green"),
        pos("Pos 03-12", "Min Promoter", "TCCC", "AATG", "green"),
        pos("Pos 1b", "5' UTR", "CCAT", "AATG", "cyan"),
        pos("Pos 03-11", "Distal 5' UTR", "TCCC", "CCAT", "cyan"),
        pos("Pos 13", "Signal peptide", "AATG", "AGCC", "dark_orange"),
        pos("Pos 3-4", "CDS", "AATG", "GCTT", "yellow"),
        pos("Pos 3-4", "OPERON", "AATG", "GCTT", "yellow"),
        pos("Pos 3", "CDS-NS", "AATG", "TTCG", "dark_orange"),
        pos("Pos 4", "C-tag", "TTCG", "GCTT", "magenta"),
        pos("Pos 13-15", "CDS-NS (CT)", "AATG", "GCAG", "dark_orange"),
        pos("Pos 16", "CT-tag", "GCAG", "GCTT", "magenta"),
        pos("Pos 14-16", "CDS-after-SP", "AGCC", "GCTT", "yellow"),
        pos("Pos 5", "Terminator", "GCTT", "CGCT", "blue"),
        pos("Pos 17", "3' UTR", "GCTT", "GGTA", "blue"),
        pos("Pos 21", "Terminator-only", "GGTA", "CGCT", "blue"),
    ]
}

fn gb_coding_types() -> Vec<String> {
    [
        "CDS",
        "CDS-NS",
        "C-tag",
        "Signal peptide",
        "CDS-NS (CT)",
        "CT-tag",
        "CDS-after-SP",
        "OPERON",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

/// Built-ins plus user grammars from the persist chokepoint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrammarStore {
    /// User-defined (always editable).
    pub custom: Vec<Grammar>,
}

impl GrammarStore {
    /// Load custom grammars (missing file → empty).
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let custom = load_grammars(layout)
            .entries
            .iter()
            .filter_map(|v| serde_json::from_value::<Grammar>(v.clone()).ok())
            .map(|mut g| {
                g.editable = true;
                g
            })
            .collect();
        Self { custom }
    }

    /// Persist custom grammars through [`splicecraft_persist::safe_save_json`].
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        let entries: Result<Vec<Value>, _> = self
            .custom
            .iter()
            .map(|g| serde_json::to_value(g).map_err(PersistError::from))
            .collect();
        save_grammars(layout, &entries?)
    }

    /// Built-ins first; custom ids override a built-in of the same id.
    #[must_use]
    pub fn all(&self) -> Vec<Grammar> {
        let mut out = builtin_grammars();
        for g in &self.custom {
            if let Some(slot) = out.iter_mut().find(|b| b.id == g.id) {
                *slot = g.clone();
            } else {
                out.push(g.clone());
            }
        }
        out
    }

    /// Look up by id across built-ins + custom.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Grammar> {
        self.all().into_iter().find(|g| g.id == id)
    }

    /// Insert or replace a user grammar. Built-in ids are refused.
    pub fn upsert_custom(&mut self, mut grammar: Grammar) -> Result<(), CloneError> {
        if !grammar.editable && builtin_grammars().iter().any(|b| b.id == grammar.id) {
            return Err(CloneError::grammar(format!(
                "grammar '{}' is built-in and cannot be overwritten",
                grammar.id
            )));
        }
        grammar.editable = true;
        if let Some(slot) = self.custom.iter_mut().find(|g| g.id == grammar.id) {
            *slot = grammar;
        } else {
            self.custom.push(grammar);
        }
        Ok(())
    }
}
