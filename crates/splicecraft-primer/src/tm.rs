//! Wallace / 2+4 Tm (MIT default). Do not take the GPL `primer3` crate.

/// Hard cap on the total synthesised oligo (tail + binding).
pub const PRIMER_MAX_OLIGO_LEN: usize = 50;

/// Wallace Tm: `2*AT + 4*GC + 3*other` (IUPAC midpoint). Empty → `None`.
///
/// This is the upstream fallback when primer3-py is missing. Golden numbers
/// live in `tests/data/wallace_tm.json` (numbers, not Python).
#[must_use]
pub fn primer_tm(seq: &str) -> Option<f64> {
    let s = seq.trim().to_ascii_uppercase();
    if s.is_empty() {
        return None;
    }
    Some(wallace_tm(&s))
}

/// Same estimator used by [`pick_binding_region`].
#[must_use]
pub fn wallace_tm(seq: &str) -> f64 {
    let u = seq.to_ascii_uppercase();
    let gc = u.chars().filter(|c| *c == 'G' || *c == 'C').count();
    let at = u.chars().filter(|c| *c == 'A' || *c == 'T').count();
    let other = u.len() - gc - at;
    (2 * at + 4 * gc + 3 * other) as f64
}

/// Largest binding length that keeps `tail_len + binding` within the oligo cap.
#[must_use]
pub fn binding_max_len(tail_len: usize, min_len: usize) -> usize {
    let min_len = min_len.max(1);
    let cap = PRIMER_MAX_OLIGO_LEN.saturating_sub(tail_len);
    cap.max(min_len)
}

/// Prefix of `seq` (length `min_len..=max_len`) whose Tm is closest to `target_tm`.
#[must_use]
pub fn pick_binding_region(
    seq: &str,
    target_tm: f64,
    min_len: usize,
    max_len: usize,
) -> (String, f64) {
    let min_len = min_len.max(1);
    let best_init = seq.chars().take(min_len).collect::<String>();
    let mut best_seq = best_init.clone();
    let mut best_tm = if best_init.is_empty() {
        0.0
    } else {
        wallace_tm(&best_init)
    };
    let mut best_diff = f64::INFINITY;
    let upper = max_len.min(seq.len());
    if min_len <= upper {
        for n in min_len..=upper {
            let candidate: String = seq.chars().take(n).collect();
            let tm = wallace_tm(&candidate);
            let diff = (tm - target_tm).abs();
            if diff < best_diff {
                best_seq = candidate;
                best_tm = tm;
                best_diff = diff;
            }
        }
    }
    (best_seq, best_tm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tm_is_none() {
        assert_eq!(primer_tm(""), None);
        assert_eq!(primer_tm("   "), None);
    }

    #[test]
    fn at_rich_below_n_rich_below_gc_rich() {
        let at = primer_tm("AAAAAAAAAA").unwrap();
        let n = primer_tm("NNNNNNNNNN").unwrap();
        let gc = primer_tm("GCGCGCGCGC").unwrap();
        assert!(at < n && n < gc, "AT={at} N={n} GC={gc}");
    }
}
