//! 3′-anchored binding sites and post-rotation re-derivation.

use splicecraft_bio::{circ_slice, normalize_dna_for_align, rc, search_subsequence};

use crate::error::PrimerError;

/// Exact 3′-anchor required for a primer-check binding call.
pub const PRIMER_CHECK_SEED_LEN: usize = 12;
/// Per-template binding-site cap (repeat guard).
pub const PRIMER_CHECK_MAX_SITES: usize = 200;
/// Shortest 3′ suffix considered when re-deriving a map feature.
pub const PRIMER_REBIND_MIN: usize = 12;

/// One 3′-anchored footprint on the top strand.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingSite {
    /// `1` forward, `-1` reverse.
    pub strand: i8,
    /// 0-based footprint start on the top strand.
    pub foot_start: usize,
    /// Primer length.
    pub length: usize,
    /// Full-primer identity 0..100.
    pub ident_pct: f64,
    /// Mismatches over the full primer.
    pub mismatches: usize,
}

/// Longest 3′-anchored match of `primer` on `template`.
///
/// Returns `(pos_start, pos_end)` in top-strand coordinates — half-open,
/// with `pos_end < pos_start` when the binding wraps the origin.
#[must_use]
pub fn rederive_primer_binding(
    primer_seq: &str,
    strand: i8,
    template: &str,
    hint_start: usize,
    circular: bool,
) -> Option<(usize, usize)> {
    let seq = primer_seq.to_ascii_uppercase();
    let template = template.to_ascii_uppercase();
    let total = template.len();
    if seq.is_empty() || template.is_empty() || total == 0 {
        return None;
    }
    let aug = if circular {
        let tail = total.min(seq.len()).saturating_sub(1);
        let mut s = template.clone();
        s.push_str(&template[..tail]);
        s
    } else {
        template.clone()
    };
    let rc_seq = if strand < 0 { rc(&seq) } else { String::new() };
    let max_l = seq.len().min(total);
    let hint = if total == 0 { 0 } else { hint_start % total };
    for len in (PRIMER_REBIND_MIN..=max_l).rev() {
        let target = if strand >= 0 {
            seq[seq.len() - len..].to_owned()
        } else {
            rc_seq[..len].to_owned()
        };
        let mut starts = Vec::new();
        let mut i = 0;
        while let Some(pos) = aug[i..].find(&target) {
            let abs = i + pos;
            if abs < total {
                starts.push(abs);
            }
            i = abs + 1;
            if i >= aug.len() {
                break;
            }
        }
        if starts.is_empty() {
            continue;
        }
        let chosen = *starts
            .iter()
            .min_by_key(|p| {
                let d = p.abs_diff(hint);
                d.min(total - d)
            })
            .expect("non-empty");
        let mut pos_end = chosen + len;
        if pos_end > total {
            pos_end -= total;
        }
        return Some((chosen, pos_end));
    }
    None
}

/// 3′-anchored binding sites of `primer` on top-strand `top`.
pub fn primer_binding_sites(
    primer: &str,
    top: &str,
    circular: bool,
    seed_len: usize,
    min_identity_pct: f64,
) -> Result<Vec<BindingSite>, PrimerError> {
    let p = normalize_dna_for_align(primer)?;
    let top_u = top.to_ascii_uppercase();
    let total = top_u.len();
    let len = p.len();
    if len == 0 || total == 0 || len > total {
        return Ok(Vec::new());
    }
    let seed = seed_len.clamp(1, len);
    let anchor = &p[len - seed..];
    let hits = search_subsequence(&top_u, anchor, 0, circular, true)?;
    let mut sites = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for h in hits {
        let (foot_start, s_strand) = if h.strand > 0 {
            let start = h.end as i64 - len as i64;
            (start, 1i8)
        } else {
            (h.start as i64, -1)
        };
        let window = if circular {
            circ_slice(&top_u, foot_start, len, total)
        } else if foot_start < 0 || foot_start as usize + len > total {
            continue;
        } else {
            top_u[foot_start as usize..foot_start as usize + len].to_owned()
        };
        if window.len() != len {
            continue;
        }
        let oriented = if s_strand == 1 { window } else { rc(&window) };
        let mut mm = 0;
        for (a, b) in p.chars().zip(oriented.chars()) {
            if !splicecraft_bio::iupac_compatible(a, b) {
                mm += 1;
            }
        }
        let ident = 100.0 * (len - mm) as f64 / len as f64;
        if ident < min_identity_pct {
            continue;
        }
        let canon = if total == 0 {
            0
        } else {
            foot_start.rem_euclid(total as i64) as usize
        };
        if !seen.insert((canon, s_strand)) {
            continue;
        }
        sites.push(BindingSite {
            strand: s_strand,
            foot_start: canon,
            length: len,
            ident_pct: ident,
            mismatches: mm,
        });
        if sites.len() >= PRIMER_CHECK_MAX_SITES {
            break;
        }
    }
    sites.sort_by(|a, b| {
        b.ident_pct
            .partial_cmp(&a.ident_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.foot_start.cmp(&b.foot_start))
    });
    Ok(sites)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> String {
        let mut rng = 1u64;
        (0..600)
            .map(|_| {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                b"ACGT"[(rng >> 33) as usize % 4] as char
            })
            .collect()
    }

    #[test]
    fn three_prime_identity() {
        let t = template();
        let fwd = t[100..122].to_owned();
        let sites = primer_binding_sites(&fwd, &t, false, PRIMER_CHECK_SEED_LEN, 0.0).unwrap();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].strand, 1);
        assert_eq!(sites[0].foot_start, 100);
        assert!((sites[0].ident_pct - 100.0).abs() < 1e-9);

        let tailed = format!("GGGGGCCCCC{fwd}");
        let sites = primer_binding_sites(&tailed, &t, false, PRIMER_CHECK_SEED_LEN, 0.0).unwrap();
        assert!(!sites.is_empty(), "5′ tail must still bind via the 3′ seed");
        assert!(sites[0].ident_pct > 0.0 && sites[0].ident_pct < 100.0);
        assert!(sites[0].mismatches > 0);

        let mut flipped: Vec<char> = fwd.chars().collect();
        let last = flipped.len() - 1;
        flipped[last] = match flipped[last] {
            'A' => 'C',
            'C' => 'A',
            'G' => 'T',
            _ => 'G',
        };
        let bad: String = flipped.into_iter().collect();
        let sites = primer_binding_sites(&bad, &t, false, PRIMER_CHECK_SEED_LEN, 0.0).unwrap();
        assert!(sites.is_empty(), "3′ mismatch must not bind");
    }

    #[test]
    fn reverse_and_wrap() {
        let t = template();
        let region = t[400..422].to_owned();
        let rev = rc(&region);
        let sites = primer_binding_sites(&rev, &t, false, PRIMER_CHECK_SEED_LEN, 0.0).unwrap();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].strand, -1);
        assert_eq!(sites[0].foot_start, 400);

        let primer = format!("{}{}", &t[590..], &t[..12]);
        let circ = primer_binding_sites(&primer, &t, true, PRIMER_CHECK_SEED_LEN, 0.0).unwrap();
        assert!(circ.iter().any(|s| s.foot_start == 590 && s.strand == 1));
        let lin = primer_binding_sites(&primer, &t, false, PRIMER_CHECK_SEED_LEN, 0.0).unwrap();
        assert!(lin.iter().all(|s| s.foot_start != 590));
    }

    #[test]
    fn foreign_char_errors() {
        let t = template();
        assert!(primer_binding_sites("ACGTZZZACGTACGT", &t, false, 12, 0.0).is_err());
    }

    #[test]
    fn rederive_after_rotation_matches_anneal() {
        let motif = "ACGTACGTACGTACGTACGTAC";
        let seq = format!("{}{motif}{}", "C".repeat(100), "G".repeat(100));
        let bind = rederive_primer_binding(motif, 1, &seq, 100, true).unwrap();
        assert_eq!(bind, (100, 122));
        let k = 50;
        let rotated = format!("{}{}", &seq[k..], &seq[..k]);
        let bind2 = rederive_primer_binding(motif, 1, &rotated, 100 - k, true).unwrap();
        assert_eq!(bind2, (50, 72));
    }
}
