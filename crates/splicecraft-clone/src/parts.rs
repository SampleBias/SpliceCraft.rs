//! Parts Bin per-grammar and classify-by-digest.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use splicecraft_core::Feature;
use splicecraft_persist::{DataLayout, PersistError, load_parts_bin, save_parts_bin};

use crate::error::CloneError;
use crate::fragment::excise_fragment_pair;
use crate::grammar::{Grammar, GrammarStore};
use crate::synfrag::L0Part;

/// One filed part. `sequence` is the body between overhangs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartRecord {
    /// Display name.
    pub name: String,
    /// Grammar part type.
    #[serde(rename = "type", default)]
    pub type_name: String,
    /// Position label.
    #[serde(default)]
    pub position: String,
    /// 5′ fusion overhang.
    #[serde(default)]
    pub oh5: String,
    /// 3′ fusion overhang.
    #[serde(default)]
    pub oh3: String,
    /// Body only — overhangs live in [`Self::oh5`] / [`Self::oh3`].
    #[serde(default)]
    pub sequence: String,
    /// Grammar id.
    #[serde(default)]
    pub grammar: String,
    /// 0 = L0 part, 1 = TU.
    #[serde(default)]
    pub level: u8,
    /// Type IIS enzyme that released it.
    #[serde(default)]
    pub enzyme: String,
    /// Two-tier nesting.
    #[serde(default)]
    pub nested: bool,
}

impl PartRecord {
    /// From a successful L0-from-syn-frag filing.
    #[must_use]
    pub fn from_l0(part: &L0Part) -> Self {
        Self {
            name: part.name.clone(),
            type_name: part.type_name.clone(),
            position: part.position.clone(),
            oh5: part.oh5.clone(),
            oh3: part.oh3.clone(),
            sequence: part.sequence.clone(),
            grammar: part.grammar.clone(),
            level: part.level,
            enzyme: part.enzyme.clone(),
            nested: part.nested,
        }
    }
}

/// In-memory parts bin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartsBinStore {
    /// Filed parts.
    pub parts: Vec<PartRecord>,
}

impl PartsBinStore {
    /// Load from the persist chokepoint.
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let parts = load_parts_bin(layout)
            .entries
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        Self { parts }
    }

    /// Persist through [`splicecraft_persist::safe_save_json`].
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        let entries: Result<Vec<Value>, _> = self
            .parts
            .iter()
            .map(|p| serde_json::to_value(p).map_err(PersistError::from))
            .collect();
        save_parts_bin(layout, &entries?)
    }

    /// File a part. Refuses an empty body (cannot assemble).
    pub fn file(&mut self, part: PartRecord) -> Result<(), CloneError> {
        if part.sequence.is_empty() {
            return Err(CloneError::assembly(
                "refusing to file a part with no insert body — it cannot assemble",
            ));
        }
        if part.oh5.len() != 4 || part.oh3.len() != 4 {
            return Err(CloneError::assembly(
                "refusing to file a part without a 4-nt overhang pair",
            ));
        }
        self.parts.push(part);
        Ok(())
    }

    /// Parts that belong to `grammar_id`.
    #[must_use]
    pub fn for_grammar<'a>(&'a self, grammar_id: &str) -> Vec<&'a PartRecord> {
        self.parts
            .iter()
            .filter(|p| p.grammar == grammar_id)
            .collect()
    }
}

/// Classifier hit from a circular plasmid digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedPart {
    /// Grammar id.
    pub grammar_id: String,
    /// Grammar display name.
    pub grammar_name: String,
    /// 0 = L0 position, 1 = TU boundary.
    pub level: u8,
    /// Position label.
    pub position: String,
    /// Part type (empty for a TU).
    pub type_name: String,
    /// Released insert overhangs.
    pub oh5: String,
    /// See [`Self::oh5`].
    pub oh3: String,
    /// Enzyme that released the pair.
    pub release_enzyme: String,
}

/// Identify grammar + position from the released overhang pair only.
#[must_use]
pub fn classify_part_from_plasmid(
    seq: &str,
    circular: bool,
    features: &[Feature],
    grammars: &GrammarStore,
) -> Option<ClassifiedPart> {
    if seq.is_empty() || !circular {
        return None;
    }
    for g in grammars.all() {
        let enzymes = unique_enzymes(&g);
        for enzyme in enzymes {
            let Ok(frags) = excise_fragment_pair(seq, &[enzyme.as_str()], true, features, &g.name)
            else {
                continue;
            };
            if frags.len() != 2 {
                continue;
            }
            for insert in &frags {
                let oh5 = insert.left.overhang_seq.to_ascii_uppercase();
                let oh3 = insert.right.overhang_seq.to_ascii_uppercase();
                if oh5.is_empty() || oh3.is_empty() {
                    continue;
                }
                if let Some(pos) = g
                    .positions
                    .iter()
                    .find(|p| p.oh5.eq_ignore_ascii_case(&oh5) && p.oh3.eq_ignore_ascii_case(&oh3))
                {
                    return Some(ClassifiedPart {
                        grammar_id: g.id.clone(),
                        grammar_name: g.name.clone(),
                        level: 0,
                        position: pos.name.clone(),
                        type_name: pos.type_name.clone(),
                        oh5,
                        oh3,
                        release_enzyme: enzyme,
                    });
                }
                let (tu5, tu3) = g.tu_overhangs();
                if tu5.eq_ignore_ascii_case(&oh5) && tu3.eq_ignore_ascii_case(&oh3) {
                    return Some(ClassifiedPart {
                        grammar_id: g.id.clone(),
                        grammar_name: g.name.clone(),
                        level: 1,
                        position: "TU".into(),
                        type_name: String::new(),
                        oh5,
                        oh3,
                        release_enzyme: enzyme,
                    });
                }
            }
        }
    }
    None
}

fn unique_enzymes(g: &Grammar) -> Vec<String> {
    let mut v = vec![g.enzyme.clone()];
    if !g.level_up_enzyme.is_empty() && g.level_up_enzyme != g.enzyme {
        v.push(g.level_up_enzyme.clone());
    }
    v.retain(|e| !e.is_empty());
    v
}
