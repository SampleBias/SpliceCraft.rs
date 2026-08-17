//! CAI and GC metrics.

use splicecraft_bio::codon_aa_table;

use crate::table::{UsageTable, build_aa_map};

/// Codon Adaptation Index (geometric mean of freq / peak synonym).
#[must_use]
pub fn cai(dna: &str, raw: &UsageTable, transl_table: i32) -> f64 {
    let (aa_codons, codon_frac) = build_aa_map(raw, transl_table);
    let mut w = Vec::new();
    let bytes = dna.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        let codon = dna[i..i + 3].to_ascii_uppercase();
        let aa = codon_aa_table(&codon, transl_table);
        if raw.get(&codon).is_none() || aa == '*' {
            i += 3;
            continue;
        }
        let peak = aa_codons
            .get(&aa)
            .and_then(|v| v.first())
            .map(|(_, f)| *f)
            .unwrap_or(0.0);
        if peak > 0.0 {
            w.push(codon_frac.get(&codon).copied().unwrap_or(0.0) / peak);
        }
        i += 3;
    }
    if w.is_empty() {
        return 0.0;
    }
    let sum: f64 = w.iter().map(|v| v.max(1e-10).ln()).sum();
    (sum / w.len() as f64).exp()
}

/// GC%. Empty → 0.
#[must_use]
pub fn gc_pct(dna: &str) -> f64 {
    if dna.is_empty() {
        return 0.0;
    }
    let gc = dna
        .bytes()
        .filter(|b| matches!(b, b'G' | b'C' | b'g' | b'c'))
        .count();
    gc as f64 / dna.len() as f64 * 100.0
}

/// GC% at the third codon position over complete codons.
#[must_use]
pub fn gc3(dna: &str) -> f64 {
    let usable = (dna.len() / 3) * 3;
    if usable < 3 {
        return 0.0;
    }
    let third: Vec<u8> = dna.as_bytes()[2..usable]
        .iter()
        .step_by(3)
        .copied()
        .collect();
    if third.is_empty() {
        return 0.0;
    }
    let gc = third
        .iter()
        .filter(|b| matches!(b, b'G' | b'C' | b'g' | b'c'))
        .count();
    gc as f64 / third.len() as f64 * 100.0
}
