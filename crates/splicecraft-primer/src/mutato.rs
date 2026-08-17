//! Site-directed mutagenesis: SOE 4-primer and near-end 2-primer paths.

use regex::Regex;
use splicecraft_bio::{codon_aa, rc};
use splicecraft_codon::{UsageTable, build_aa_map};

use crate::error::PrimerError;
use crate::tm::wallace_tm;

/// BsaI-AATG tail on the constant FWD outer.
pub const MUT_BSAI_FWD_TAIL: &str = "CCCCGGTCTCAAATG";
/// BsaI-AACG tail on the constant REV outer.
pub const MUT_BSAI_REV_TAIL: &str = "CCCCGGTCTCAAACG";
/// Below this fragment length, fold the mutation into a modified outer.
pub const MUT_MIN_SOE_FRAG: usize = 60;
const MUT_OUTER_MAX_LEN: usize = 45;
const MUT_OUTER_ANCHOR: usize = 8;

/// One scored oligo.
#[derive(Clone, Debug, PartialEq)]
pub struct MutOligo {
    /// Annealing region.
    pub anneal: String,
    /// Full oligo (tail + anneal).
    pub full: String,
    /// Wallace Tm of the anneal.
    pub tm_anneal: f64,
    /// GC%.
    pub gc: f64,
    /// Lower is better.
    pub score: f64,
    /// Optional edge-case label.
    pub label: String,
}

/// Constant outer pair for a CDS.
#[derive(Clone, Debug, PartialEq)]
pub struct OuterPrimers {
    /// Forward outer.
    pub fwd: MutOligo,
    /// Reverse outer.
    pub rev: MutOligo,
    /// B3 overhang after digest.
    pub b3_overhang: String,
    /// B5 overhang name (revcomp of insert-side AACG).
    pub b5_overhang: String,
    /// FWD anneal start on the CDS (always 3).
    pub fwd_anneal_start: usize,
}

/// One inner-pair candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct InnerCandidate {
    /// Mutagenic forward (contains mutant codon).
    pub fwd: String,
    /// Exact reverse-complement of `fwd`.
    pub rev: String,
    /// Wallace Tm.
    pub tm: f64,
    /// GC%.
    pub gc: f64,
    /// Length.
    pub length: usize,
    /// Score (lower better).
    pub score: f64,
    /// Start of the window on the mutant CDS.
    pub lo: usize,
    /// Rank 1..=5.
    pub rank: usize,
}

/// Near-end 2-primer shortcut.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeCase {
    /// Mutation near CDS start.
    pub near_start: bool,
    /// Mutation near CDS end.
    pub near_end: bool,
    /// Fragment A length.
    pub frag_a: usize,
    /// Fragment B length.
    pub frag_b: usize,
    /// Folded outer that carries the mutation.
    pub modified_outer: MutOligo,
}

/// Inner mutagenic design.
#[derive(Clone, Debug, PartialEq)]
pub struct InnerDesign {
    /// `V40F`.
    pub mutation: String,
    /// 1-based nucleotide start of the codon.
    pub nt_position: usize,
    /// Wild-type codon.
    pub wt_codon: String,
    /// Chosen mutant codon.
    pub mut_codon: String,
    /// Number of base changes.
    pub nt_changes: usize,
    /// Ranked candidates (best first).
    pub candidates: Vec<InnerCandidate>,
    /// 2-primer shortcut when the primer actually spans the mutation.
    pub edge_case: Option<EdgeCase>,
}

/// Parse `W140F` / `w140*` → (wt, 1-based pos, mut).
pub fn mut_parse(s: &str) -> Result<(char, usize, char), PrimerError> {
    let re = Regex::new(r"^([A-Za-z\*])(\d+)([A-Za-z\*])$").expect("static");
    let cap = re.captures(s.trim()).ok_or_else(|| {
        PrimerError::Design(format!(
            "Cannot parse '{s}'. Use format: [WT][pos][MUT], e.g. W140F"
        ))
    })?;
    let wt = cap[1].to_ascii_uppercase().chars().next().unwrap();
    let pos: usize = cap[2].parse().unwrap_or(0);
    let mut_aa = cap[3].to_ascii_uppercase().chars().next().unwrap();
    Ok((wt, pos, mut_aa))
}

/// Translate until the first stop (stop itself is omitted).
#[must_use]
pub fn mut_translate(dna: &str) -> String {
    let mut aa = String::new();
    let bytes = dna.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        let c = dna[i..i + 3].to_ascii_uppercase();
        if matches!(c.as_str(), "TAA" | "TAG" | "TGA") {
            break;
        }
        aa.push(codon_aa(&c));
        i += 3;
    }
    aa
}

fn gc_pct(seq: &str) -> f64 {
    if seq.is_empty() {
        return 0.0;
    }
    let gc = seq
        .bytes()
        .filter(|b| matches!(b, b'G' | b'C' | b'g' | b'c'))
        .count();
    gc as f64 / seq.len() as f64 * 100.0
}

fn ends_gc(seq: &str) -> bool {
    seq.as_bytes()
        .last()
        .is_some_and(|b| matches!(b, b'G' | b'C' | b'g' | b'c'))
}

fn score_outer(anneal: &str, target_tm: f64) -> f64 {
    let t = wallace_tm(anneal);
    let gc = gc_pct(anneal);
    abs_diff(t, target_tm) * 2.0
        + if ends_gc(anneal) { 0.0 } else { 4.0 }
        + abs_diff(gc, 50.0) * 0.1
}

fn abs_diff(a: f64, b: f64) -> f64 {
    (a - b).abs()
}

fn design_fwd_anneal(dna: &str) -> Option<MutOligo> {
    let body = dna.get(3..)?;
    let mut best: Option<MutOligo> = None;
    for length in 18..28 {
        if body.len() < length {
            continue;
        }
        let anneal = &body[..length];
        let s = score_outer(anneal, 60.0);
        if best.as_ref().is_none_or(|b| s < b.score) {
            best = Some(MutOligo {
                anneal: anneal.to_owned(),
                full: format!("{MUT_BSAI_FWD_TAIL}{anneal}"),
                tm_anneal: wallace_tm(anneal),
                gc: gc_pct(anneal),
                score: s,
                label: String::new(),
            });
        }
    }
    best
}

fn design_rev_anneal(dna: &str) -> Option<MutOligo> {
    let end_rc = rc(dna);
    let mut best: Option<MutOligo> = None;
    for length in 18..28 {
        if end_rc.len() < length {
            continue;
        }
        let anneal = &end_rc[..length];
        let s = score_outer(anneal, 60.0);
        if best.as_ref().is_none_or(|b| s < b.score) {
            best = Some(MutOligo {
                anneal: anneal.to_owned(),
                full: format!("{MUT_BSAI_REV_TAIL}{anneal}"),
                tm_anneal: wallace_tm(anneal),
                gc: gc_pct(anneal),
                score: s,
                label: String::new(),
            });
        }
    }
    best
}

/// Constant FWD/REV outer primers with BsaI-AATG / BsaI-AACG tails.
pub fn design_outer(dna: &str) -> Result<OuterPrimers, PrimerError> {
    let fwd = design_fwd_anneal(dna).ok_or_else(|| {
        PrimerError::Design("CDS is too short to design outer primers (need ≥ 21 nt).".into())
    })?;
    let rev = design_rev_anneal(dna).ok_or_else(|| {
        PrimerError::Design("CDS is too short to design outer primers (need ≥ 21 nt).".into())
    })?;
    Ok(OuterPrimers {
        fwd,
        rev,
        b3_overhang: "AATG".into(),
        b5_overhang: "CGTT".into(),
        fwd_anneal_start: 3,
    })
}

fn design_modified_outer(dna_mut: &str, near_start: bool, nt_start: usize) -> Option<MutOligo> {
    let mut_end = nt_start + 3;
    if mut_end > dna_mut.len() {
        return None;
    }
    if near_start {
        let anneal_start = 3;
        if nt_start < anneal_start {
            return None;
        }
        let end_hi = dna_mut.len().min(anneal_start + MUT_OUTER_MAX_LEN);
        let mut best: Option<MutOligo> = None;
        for end in (mut_end + MUT_OUTER_ANCHOR)..=end_hi {
            let anneal = &dna_mut[anneal_start..end];
            if anneal.len() < 18 {
                continue;
            }
            let s = score_outer(anneal, 60.0);
            if best.as_ref().is_none_or(|b| s < b.score) {
                best = Some(MutOligo {
                    anneal: anneal.to_owned(),
                    full: format!("{MUT_BSAI_FWD_TAIL}{anneal}"),
                    tm_anneal: wallace_tm(anneal),
                    gc: gc_pct(anneal),
                    score: s,
                    label: "modified_FWD_outer".into(),
                });
            }
        }
        return best;
    }
    let seq_len = dna_mut.len();
    let lo_start = seq_len.saturating_sub(MUT_OUTER_MAX_LEN);
    let hi_start = nt_start.saturating_sub(MUT_OUTER_ANCHOR);
    let mut best: Option<MutOligo> = None;
    if lo_start > hi_start {
        return None;
    }
    for start in lo_start..=hi_start {
        let tail = &dna_mut[start..];
        if tail.len() < 18 {
            continue;
        }
        let anneal = rc(tail);
        let s = score_outer(&anneal, 60.0);
        if best.as_ref().is_none_or(|b| s < b.score) {
            best = Some(MutOligo {
                anneal: anneal.clone(),
                full: format!("{MUT_BSAI_REV_TAIL}{anneal}"),
                tm_anneal: wallace_tm(&anneal),
                gc: gc_pct(&anneal),
                score: s,
                label: "modified_REV_outer".into(),
            });
        }
    }
    best
}

/// Inner mutagenic pair. FWD carries the mutant codon; REV = rc(FWD).
pub fn design_inner(
    dna: &str,
    mut_pos_1: usize,
    mut_aa: char,
    wt_aa: char,
    codon_table: Option<&UsageTable>,
) -> Result<InnerDesign, PrimerError> {
    let idx = mut_pos_1.saturating_sub(1);
    let nt_start = idx * 3;
    if nt_start + 3 > dna.len() {
        return Err(PrimerError::Design(format!(
            "Position {mut_pos_1} is past the end of the CDS."
        )));
    }
    let wt_codon = dna[nt_start..nt_start + 3].to_ascii_uppercase();
    let wt_actual = codon_aa(&wt_codon);
    if wt_actual != wt_aa {
        return Err(PrimerError::Design(format!(
            "Position {mut_pos_1}: mutation says WT='{wt_aa}' but DNA codon '{wt_codon}' encodes '{wt_actual}'."
        )));
    }
    let mut_codon = if mut_aa == '*' {
        if let Some(table) = codon_table {
            table
                .iter()
                .filter(|(_, aa, _)| *aa == '*')
                .max_by_key(|(_, _, n)| *n)
                .map(|(c, _, _)| c.to_owned())
                .unwrap_or_else(|| "TAA".into())
        } else {
            "TAA".into()
        }
    } else {
        let aa_map = if let Some(table) = codon_table {
            build_aa_map(table, 1).0
        } else {
            build_aa_map(&mut_k12(), 1).0
        };
        aa_map
            .get(&mut_aa)
            .and_then(|list| {
                list.iter()
                    .find(|(c, _)| *c != wt_codon)
                    .map(|(c, _)| c.clone())
            })
            .ok_or_else(|| {
                PrimerError::Design(format!(
                    "No alternative codon available for '{mut_aa}' in the selected codon table"
                ))
            })?
    };
    let mut_dna = format!("{}{}{}", &dna[..nt_start], mut_codon, &dna[nt_start + 3..]);
    let seq_len = mut_dna.len();
    let mut candidates = Vec::new();
    for left_ext in 5..28 {
        for right_ext in 5..28 {
            let lo = nt_start.saturating_sub(left_ext);
            let hi = (nt_start + 3 + right_ext).min(seq_len);
            let fwd = &mut_dna[lo..hi];
            if fwd.len() < 15 || fwd.len() > 58 {
                continue;
            }
            let t = wallace_tm(fwd);
            let gc = gc_pct(fwd);
            if !(55.0..=75.0).contains(&t) || !(35.0..=68.0).contains(&gc) {
                continue;
            }
            let mut score = abs_diff(t, 60.0) * 2.0
                + if ends_gc(fwd) { 0.0 } else { 4.0 }
                + abs_diff(gc, 50.0) * 0.1;
            if abs_diff(t, 60.0) <= 1.0 {
                score -= fwd.len() as f64 * 0.15;
            }
            candidates.push(InnerCandidate {
                fwd: fwd.to_owned(),
                rev: rc(fwd),
                tm: t,
                gc,
                length: fwd.len(),
                score,
                lo,
                rank: 0,
            });
        }
    }
    if candidates.is_empty() {
        return Err(PrimerError::Design(format!(
            "No valid inner primers found for {wt_aa}{mut_pos_1}{mut_aa}. Mutation may be too close to sequence ends."
        )));
    }
    candidates.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen = std::collections::HashSet::new();
    let mut ranked = Vec::new();
    for c in candidates {
        if seen.insert(c.fwd.clone()) {
            ranked.push(c);
        }
        if ranked.len() == 5 {
            break;
        }
    }
    for (i, c) in ranked.iter_mut().enumerate() {
        c.rank = i + 1;
    }
    let best_lo = ranked[0].lo;
    let best_hi = best_lo + ranked[0].length;
    let fwd_anneal_start = 3;
    let frag_a = best_hi.saturating_sub(fwd_anneal_start);
    let frag_b = seq_len.saturating_sub(best_lo);
    let near_start = frag_a < MUT_MIN_SOE_FRAG;
    let near_end = frag_b < MUT_MIN_SOE_FRAG;
    let edge_case = if near_start || near_end {
        design_modified_outer(&mut_dna, near_start, nt_start).map(|modified_outer| EdgeCase {
            near_start,
            near_end,
            frag_a,
            frag_b,
            modified_outer,
        })
    } else {
        None
    };
    let nt_changes = wt_codon
        .bytes()
        .zip(mut_codon.bytes())
        .filter(|(a, b)| a != b)
        .count();
    Ok(InnerDesign {
        mutation: format!("{wt_aa}{mut_pos_1}{mut_aa}"),
        nt_position: nt_start + 1,
        wt_codon,
        mut_codon,
        nt_changes,
        candidates: ranked,
        edge_case,
    })
}

fn mut_k12() -> UsageTable {
    splicecraft_codon::builtin_k12()
}

/// CDS DNA in biological 5′→3′ orientation (wrap + reverse-strand).
#[must_use]
pub fn extract_cds(full_seq: &str, start: usize, end: usize, strand: i8) -> String {
    let sub = if end < start {
        format!("{}{}", &full_seq[start..], &full_seq[..end])
    } else {
        full_seq.get(start..end).unwrap_or("").to_owned()
    };
    let mut sub = sub.to_ascii_uppercase();
    if strand < 0 {
        sub = rc(&sub);
    }
    sub
}

/// Full Mutato plan: outers + inner (or modified-outer shortcut).
pub fn design_mutagenesis(
    cds: &str,
    mutation: &str,
    codon_table: Option<&UsageTable>,
) -> Result<(OuterPrimers, InnerDesign), PrimerError> {
    let (wt, pos, mut_aa) = mut_parse(mutation)?;
    let inner = design_inner(cds, pos, mut_aa, wt, codon_table)?;
    let outer = design_outer(cds)?;
    Ok((outer, inner))
}
