//! Linear DNA / protein synthesis buffers (substitution-safe feature shifts).

use splicecraft_core::Feature;

use crate::optimize::{CodonMode, optimize};
use crate::table::UsageTable;

/// Hard cap matching upstream `SYNTH_MAX_BP`.
pub const SYNTH_MAX_BP: usize = 50_000;
/// Matching `SYNTH_MAX_AA`.
pub const SYNTH_MAX_AA: usize = SYNTH_MAX_BP / 3;

/// Linear DNA editor buffer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DnaBuffer {
    /// Sequence (IUPAC).
    pub seq: String,
    /// Cursor (0..=len).
    pub cursor: usize,
    /// Features in buffer coordinates.
    pub features: Vec<Feature>,
}

impl DnaBuffer {
    /// Insert `bases` at the cursor; features at/after the cursor shift right.
    pub fn insert(&mut self, bases: &str) {
        let cleaned: String = bases
            .chars()
            .filter(|c| splicecraft_bio::iupac::iupac_base_set(*c).is_some())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if cleaned.is_empty() {
            return;
        }
        let room = SYNTH_MAX_BP.saturating_sub(self.seq.len());
        let take = cleaned.chars().take(room).collect::<String>();
        if take.is_empty() {
            return;
        }
        let n = take.len();
        let at = self.cursor.min(self.seq.len());
        self.seq.insert_str(at, &take);
        for f in &mut self.features {
            if f.start >= at {
                f.start += n;
            }
            if f.end >= at {
                f.end += n;
            }
        }
        self.cursor = at + n;
        self.drop_empty_features();
    }

    /// Delete `[lo, hi)` and clip overlapping features.
    pub fn delete_range(&mut self, lo: usize, hi: usize) {
        let lo = lo.min(self.seq.len());
        let hi = hi.min(self.seq.len()).max(lo);
        if lo == hi {
            return;
        }
        let n = hi - lo;
        self.seq.replace_range(lo..hi, "");
        for f in &mut self.features {
            f.start = shift_delete(f.start, lo, n);
            f.end = shift_delete(f.end, lo, n);
        }
        self.drop_empty_features();
        self.cursor = lo;
    }

    fn drop_empty_features(&mut self) {
        self.features.retain(|f| f.start != f.end);
    }
}

fn shift_delete(pos: usize, lo: usize, n: usize) -> usize {
    if pos <= lo {
        pos
    } else if pos <= lo + n {
        lo
    } else {
        pos - n
    }
}

/// Protein composer: amino acids filled from the active codon table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProteinBuffer {
    /// Amino-acid sequence (`*` allowed).
    pub aa: String,
    /// Cursor in AA units.
    pub cursor: usize,
    /// Features in AA coordinates.
    pub features: Vec<Feature>,
}

impl ProteinBuffer {
    /// Insert amino acids (20 + stop). Ambiguity codes dropped.
    pub fn insert(&mut self, text: &str) {
        let cleaned: String = text
            .chars()
            .map(|c| c.to_ascii_uppercase())
            .filter(|c| "ACDEFGHIKLMNPQRSTVWY*".contains(*c))
            .collect();
        if cleaned.is_empty() {
            return;
        }
        let room = SYNTH_MAX_AA.saturating_sub(self.aa.len());
        let take: String = cleaned.chars().take(room).collect();
        if take.is_empty() {
            return;
        }
        let n = take.chars().count();
        let at = self.cursor.min(self.aa.chars().count());
        let byte = self
            .aa
            .char_indices()
            .nth(at)
            .map(|(i, _)| i)
            .unwrap_or(self.aa.len());
        self.aa.insert_str(byte, &take);
        for f in &mut self.features {
            if f.start >= at {
                f.start += n;
            }
            if f.end >= at {
                f.end += n;
            }
        }
        self.cursor = at + n;
        self.features.retain(|f| f.start != f.end);
    }

    /// Reverse-translate with the active table.
    pub fn to_dna(
        &self,
        table: &UsageTable,
        stops: i32,
        mode: CodonMode,
    ) -> Result<String, crate::error::CodonError> {
        optimize(&self.aa, table, stops, 1, mode)
    }
}

/// Most-frequent synonym per residue (TAA fallback for stop).
#[must_use]
pub fn codon_cache(raw: &UsageTable) -> std::collections::HashMap<char, String> {
    let (aa_map, _) = crate::table::build_aa_map(raw, 1);
    let mut out = std::collections::HashMap::new();
    for (aa, list) in aa_map {
        if let Some((codon, _)) = list.first() {
            out.insert(aa, codon.clone());
        }
    }
    out.entry('*').or_insert_with(|| "TAA".into());
    out
}
