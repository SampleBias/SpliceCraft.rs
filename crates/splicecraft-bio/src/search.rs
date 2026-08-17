//! Subsequence search and circular slices used by primer-check.

use crate::iupac::{BioError, iupac_compatible, rc};

/// One Hamming hit in forward coordinates ([INV-02]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubseqHit {
    /// Inclusive start on the top strand.
    pub start: usize,
    /// Exclusive end; may exceed `seq.len()` when a circular hit wraps.
    pub end: usize,
    /// `1` forward, `-1` reverse (query RC sits on the top strand).
    pub strand: i8,
    /// Substitution count (no indels).
    pub mismatches: usize,
}

/// Uppercase, `U`→`T`, strip whitespace / digits / FASTA headers.
/// Foreign characters error so callers can surface the offending base.
pub fn normalize_dna_for_align(raw: &str) -> Result<String, BioError> {
    let mut out = String::with_capacity(raw.len());
    let mut in_header = false;
    let mut bad = Vec::new();
    for c in raw.chars() {
        if c == '>' {
            in_header = true;
            continue;
        }
        if in_header {
            if c == '\n' {
                in_header = false;
            }
            continue;
        }
        if c.is_ascii_whitespace() || c.is_ascii_digit() {
            continue;
        }
        let u = match c.to_ascii_uppercase() {
            'U' => 'T',
            other => other,
        };
        if crate::iupac::iupac_base_set(u).is_none() {
            bad.push(u);
        } else {
            out.push(u);
        }
    }
    if !bad.is_empty() {
        let shown: String = bad
            .iter()
            .take(6)
            .map(|c| format!("{c:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(BioError::NonIupac {
            site: raw.to_owned(),
            bad: shown,
        });
    }
    Ok(out)
}

/// `length` bases starting at `start` (mod `total`), wrapping as needed.
#[must_use]
pub fn circ_slice(seq: &str, start: i64, length: usize, total: usize) -> String {
    if total == 0 || length == 0 || seq.is_empty() {
        return String::new();
    }
    let mut pos = start.rem_euclid(total as i64) as usize;
    let mut out = String::with_capacity(length);
    let mut need = length;
    while need > 0 {
        let take = (total - pos).min(need);
        let end = pos + take;
        if end > seq.len() {
            break;
        }
        out.push_str(&seq[pos..end]);
        need -= take;
        pos = 0;
        if take == 0 {
            break;
        }
    }
    out
}

/// Find every position where `query` occurs in `seq` within `max_mismatches`
/// substitutions (Hamming — no indels). IUPAC-aware.
pub fn search_subsequence(
    seq: &str,
    query: &str,
    max_mismatches: i32,
    circular: bool,
    both_strands: bool,
) -> Result<Vec<SubseqHit>, BioError> {
    if max_mismatches < 0 {
        return Ok(Vec::new());
    }
    let s = normalize_dna_for_align(seq)?;
    let q = normalize_dna_for_align(query)?;
    if s.len() != seq.len() {
        return Err(BioError::NonIupac {
            site: seq.to_owned(),
            bad: "haystack length changed under normalisation".into(),
        });
    }
    let n = s.len();
    let m = q.len();
    if m == 0 || n == 0 || m > n {
        return Ok(Vec::new());
    }
    let k = (max_mismatches as usize).min(m);
    let mut hits = Vec::new();
    scan_one(&s, &q, k, circular, 1, &mut hits);
    if both_strands {
        let rcq = rc(&q);
        if rcq != q {
            scan_one(&s, &rcq, k, circular, -1, &mut hits);
        }
    }
    let mut seen: std::collections::HashMap<(usize, usize), SubseqHit> =
        std::collections::HashMap::new();
    for h in hits {
        let key = (h.start, h.end);
        match seen.get(&key) {
            None => {
                seen.insert(key, h);
            }
            Some(prev) if h.mismatches < prev.mismatches => {
                seen.insert(key, h);
            }
            Some(prev) if h.mismatches == prev.mismatches && prev.strand < 0 && h.strand > 0 => {
                seen.insert(key, h);
            }
            _ => {}
        }
    }
    let mut out: Vec<SubseqHit> = seen.into_values().collect();
    out.sort_by_key(|h| (h.start, if h.strand > 0 { 0 } else { 1 }, h.mismatches));
    Ok(out)
}

fn scan_one(
    s: &str,
    pattern: &str,
    k: usize,
    circular: bool,
    strand: i8,
    out: &mut Vec<SubseqHit>,
) {
    let n = s.len();
    let m = pattern.len();
    let scan = if circular && m > 1 {
        let mut t = s.to_owned();
        t.push_str(&s[..m.saturating_sub(1).min(n)]);
        t
    } else {
        s.to_owned()
    };
    let last = if circular {
        n.saturating_sub(1)
    } else {
        n.saturating_sub(m)
    };
    let pb: Vec<char> = pattern.chars().collect();
    let sb: Vec<char> = scan.chars().collect();
    if sb.len() < m {
        return;
    }
    for p in 0..=last {
        if p + m > sb.len() {
            break;
        }
        let mut mm = 0;
        let mut ok = true;
        for i in 0..m {
            if !iupac_compatible(pb[i], sb[p + i]) {
                mm += 1;
                if mm > k {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            out.push(SubseqHit {
                start: p,
                end: p + m,
                strand,
                mismatches: mm,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circ_slice_wrap_and_linear() {
        assert_eq!(circ_slice("ABCDEFGH", 2, 3, 8), "CDE");
        assert_eq!(circ_slice("ABCDEFGH", 6, 4, 8), "GHAB");
        assert_eq!(circ_slice("ABCDEFGH", -2, 4, 8), "GHAB");
        assert_eq!(circ_slice("", 0, 4, 8), "");
        assert_eq!(circ_slice("ABC", 0, 0, 3), "");
    }

    #[test]
    fn search_exact_both_strands() {
        let hits = search_subsequence("TTTGAATTCAAA", "GAATTC", 0, false, true).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start, 3);
        assert_eq!(hits[0].strand, 1);
    }

    #[test]
    fn normalize_rejects_foreign() {
        assert!(normalize_dna_for_align("ACGTZZZ").is_err());
    }
}
