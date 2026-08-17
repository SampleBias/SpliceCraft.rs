//! Frequency-matching and max-CAI reverse translation.

use splicecraft_bio::{codon_aa_table, stop_codons};

use crate::error::CodonError;
use crate::table::{UsageTable, allocate, build_aa_map};

/// Codon-selection strategy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodonMode {
    /// Match host synonymous frequencies (default).
    #[default]
    Frequency,
    /// Every residue gets its most-frequent synonym.
    MaxCai,
}

impl CodonMode {
    /// Parse `"frequency"` / `"max_cai"`.
    pub fn parse(s: &str) -> Result<Self, CodonError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "frequency" | "" => Ok(Self::Frequency),
            "max_cai" => Ok(Self::MaxCai),
            other => Err(CodonError::UnknownMode(other.to_owned())),
        }
    }

    /// Wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Frequency => "frequency",
            Self::MaxCai => "max_cai",
        }
    }
}

/// Codon-optimize `protein` against `raw`. Trailing `*` run overrides `stops`.
pub fn optimize(
    protein: &str,
    raw: &UsageTable,
    stops: i32,
    transl_table: i32,
    mode: CodonMode,
) -> Result<String, CodonError> {
    let (aa_codons, codon_frac) = build_aa_map(raw, transl_table);
    let mut body = protein.to_owned();
    let mut n_trailing = 0usize;
    while body.ends_with('*') {
        body.pop();
        n_trailing += 1;
    }
    if body.contains('*') {
        return Err(CodonError::InternalStop);
    }
    let n_stops = if n_trailing > 0 {
        n_trailing
    } else {
        stops.max(0) as usize
    };

    let body_u: Vec<char> = body.chars().map(|c| c.to_ascii_uppercase()).collect();
    let mut positions: Vec<(char, Vec<usize>)> = Vec::new();
    for (i, aa) in body_u.iter().enumerate() {
        if let Some((_, idxs)) = positions.iter_mut().find(|(a, _)| a == aa) {
            idxs.push(i);
        } else {
            positions.push((*aa, vec![i]));
        }
    }
    let mut codon_at = vec![String::new(); body_u.len()];
    for (aa, idxs) in positions {
        let Some(codons_for_aa) = aa_codons.get(&aa) else {
            return Err(CodonError::NoCodons(aa));
        };
        if codons_for_aa.is_empty() {
            return Err(CodonError::NoCodons(aa));
        }
        let chosen = match mode {
            CodonMode::MaxCai => vec![codons_for_aa[0].0.clone(); idxs.len()],
            CodonMode::Frequency => allocate(codons_for_aa, idxs.len()),
        };
        for (pos, codon) in idxs.into_iter().zip(chosen) {
            codon_at[pos] = codon;
        }
    }

    let code_stops = stop_codons(transl_table);
    let single_stop = if code_stops.contains(&"TAA") {
        "TAA"
    } else {
        code_stops.first().copied().unwrap_or("TAA")
    };
    let tail = if n_stops <= 1 {
        single_stop.repeat(n_stops)
    } else {
        let mut stop_codons_frac: Vec<(String, f64)> = raw
            .iter()
            .filter(|(c, _, _)| codon_aa_table(c, transl_table) == '*')
            .map(|(c, _, _)| (c.to_owned(), codon_frac.get(c).copied().unwrap_or(0.0)))
            .collect();
        stop_codons_frac.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if stop_codons_frac.is_empty() {
            stop_codons_frac.push((single_stop.to_owned(), 1.0));
        }
        allocate(&stop_codons_frac, n_stops).concat()
    };
    Ok(codon_at.concat() + &tail)
}
