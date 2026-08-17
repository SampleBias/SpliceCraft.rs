//! Six-frame ORF finder. Wrap-aware; length is never `(end - start)`.
//!
//! Port of upstream `_find_orfs` (`splicecraft_seqanalysis.py`). On a circle,
//! a wrap ORF is reported with `end < start` ([INV-08]). A full-lap ORF
//! (`nt_len >= n`) cannot be expressed as a start/end pair — read
//! [`Orf::length_aa`] / [`Orf::nt_len`], not the span.

use std::collections::HashSet;

use crate::iupac::rc;
use crate::translate::codon_aa;

/// Default minimum coded residues (stop excluded). `30` ⇒ ≥ 93 bp with stop.
pub const ORF_DEFAULT_MIN_AA: usize = 30;

/// One six-frame hit. Coordinates are 0-based half-open on the forward strand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Orf {
    /// Start (0-based). Wrap / full-lap may have `end < start`.
    pub start: usize,
    /// Exclusive end. Do **not** derive length from this pair.
    pub end: usize,
    /// `1` forward, `-1` reverse.
    pub strand: i8,
    /// Coded residues excluding the stop codon.
    pub length_aa: usize,
    /// Coding length in bases **including** the stop codon.
    pub nt_len: usize,
    /// True when `nt_len >=` molecule length (span is pinned, not authoritative).
    pub exceeds_one_lap: bool,
    /// Translated AA including a trailing `*` when a stop was found.
    pub aa_seq: String,
}

/// Six-frame ORF scan. Sorted by [`Orf::length_aa`] descending.
///
/// `include_alt_starts` adds GTG / TTG; default is ATG only.
#[must_use]
pub fn find_orfs(seq: &str, circular: bool, min_aa: usize, include_alt_starts: bool) -> Vec<Orf> {
    let n = seq.len();
    if n < 6 {
        return Vec::new();
    }
    let seq_u = seq.to_ascii_uppercase();
    let mut starts = HashSet::from(["ATG"]);
    if include_alt_starts {
        starts.insert("GTG");
        starts.insert("TTG");
    }

    let mut orfs = Vec::new();
    for strand in [1_i8, -1] {
        let owned_rc;
        let scan_base: &str = if strand == 1 {
            &seq_u
        } else {
            owned_rc = rc(&seq_u);
            &owned_rc
        };
        let scan_seq = if circular {
            let mut s = String::with_capacity(scan_base.len() * 2);
            s.push_str(scan_base);
            s.push_str(scan_base);
            s
        } else {
            scan_base.to_owned()
        };
        let scan_n = scan_seq.len();

        for frame in 0..3 {
            let mut current_start: isize = -1;
            let mut i = frame;
            while i + 3 <= scan_n {
                let codon = &scan_seq[i..i + 3];
                if current_start < 0 {
                    if starts.contains(codon) {
                        current_start = i as isize;
                    }
                } else if is_stop(codon) {
                    let start = current_start as usize;
                    let aa_len = (i - start) / 3;
                    if (circular && start >= n) || aa_len < min_aa {
                        current_start = -1;
                        i += 3;
                        continue;
                    }
                    let nt_seq = &scan_seq[start..i + 3];
                    let aa_seq: String = nt_seq
                        .as_bytes()
                        .chunks_exact(3)
                        .map(|c| codon_aa(std::str::from_utf8(c).unwrap_or("NNN")))
                        .collect();
                    let (o_s, mut o_e) = if strand == 1 {
                        let mut e = i + 3;
                        if circular && e > n {
                            e -= n;
                        }
                        (start, e)
                    } else {
                        let p_rc = start;
                        let e_rc = i + 3;
                        if circular {
                            (
                                mod_n(n as isize - e_rc as isize, n),
                                mod_n(n as isize - p_rc as isize, n),
                            )
                        } else {
                            (n - e_rc, n - p_rc)
                        }
                    };
                    let nt_len = nt_seq.len();
                    let over_lap = circular && nt_len >= n;
                    if over_lap {
                        o_e = (o_s + n - 1) % n;
                    }
                    orfs.push(Orf {
                        start: o_s,
                        end: o_e,
                        strand,
                        length_aa: aa_len,
                        nt_len,
                        exceeds_one_lap: over_lap,
                        aa_seq,
                    });
                    current_start = -1;
                }
                i += 3;
            }
        }
    }

    orfs.sort_by_key(|o| std::cmp::Reverse(o.length_aa));
    let mut seen = HashSet::new();
    orfs.retain(|o| seen.insert((o.start, o.end, o.strand)));
    orfs
}

fn is_stop(codon: &str) -> bool {
    matches!(codon, "TAA" | "TAG" | "TGA")
}

fn mod_n(x: isize, n: usize) -> usize {
    x.rem_euclid(n as isize) as usize
}

/// Fingerprint used to drop search/ORF results after the canvas moved on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordFingerprint {
    /// Display / locus name (never sequence).
    pub name: String,
    /// Molecule length in bp.
    pub len: usize,
    /// Opaque hash of the sequence (not logged as DNA).
    pub seq_hash: u64,
}

/// Hash name + length + bases. The hash is not a sequence dump.
#[must_use]
pub fn record_fingerprint(name: &str, sequence: &str) -> RecordFingerprint {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    sequence.hash(&mut h);
    RecordFingerprint {
        name: name.to_owned(),
        len: sequence.len(),
        seq_hash: h.finish(),
    }
}

/// True when the loaded record is no longer the one the search was started on.
#[must_use]
pub fn results_are_stale(submitted: &RecordFingerprint, name: &str, sequence: &str) -> bool {
    &record_fingerprint(name, sequence) != submitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_core::feat_len;

    /// ATG at the tail, 29 AAA + TAA after the origin → wrap, 30 AA.
    fn wrap_fixture() -> String {
        format!("{}{}{}{}", "AAA".repeat(29), "TAA", "CCC".repeat(5), "ATG")
    }

    #[test]
    fn wrap_orf_lists_wrap_and_exact_aa_length() {
        let seq = wrap_fixture();
        let n = seq.len();
        let orfs = find_orfs(&seq, true, 30, false);
        let hit = orfs
            .iter()
            .find(|o| o.strand == 1 && o.end < o.start && !o.exceeds_one_lap)
            .expect("wrap ORF");
        assert_eq!(hit.length_aa, 30);
        assert_eq!(hit.nt_len, 93);
        assert_eq!(feat_len(hit.start, hit.end, n), 93);
        let coded: String = hit.aa_seq.chars().filter(|c| *c != '*').collect();
        assert_eq!(coded.len(), hit.length_aa);
        assert_eq!(coded, format!("M{}", "K".repeat(29)));
        let linear = find_orfs(&seq, false, 30, false);
        assert!(
            linear.iter().all(|o| !(o.strand == 1 && o.end < o.start)),
            "{linear:?}"
        );
    }

    #[test]
    fn full_lap_orf_does_not_use_span_as_length() {
        let seq = format!("ATG{}TAA", "AAA".repeat(31));
        assert_eq!(seq.len(), 99);
        let orfs = find_orfs(&seq, true, 10, false);
        let hit = orfs
            .iter()
            .find(|o| o.exceeds_one_lap && o.strand == 1)
            .expect("full-lap ORF");
        assert_eq!(hit.length_aa, 32);
        assert_eq!(hit.nt_len, 99);
        assert!(hit.nt_len >= seq.len());
        let span = if hit.end >= hit.start {
            hit.end - hit.start
        } else {
            feat_len(hit.start, hit.end, seq.len())
        };
        assert_ne!(
            span, hit.nt_len,
            "start/end pair must not be treated as coding length"
        );
        assert_eq!(
            hit.aa_seq.chars().filter(|c| *c != '*').count(),
            hit.length_aa
        );
    }

    #[test]
    fn atg_only_by_default_gtg_opt_in() {
        let seq = format!("GTG{}TAA", "AAA".repeat(40));
        let atg_only = find_orfs(&seq, false, 30, false);
        assert!(
            atg_only.iter().all(|o| !o.aa_seq.starts_with('V')),
            "{atg_only:?}"
        );
        let alt = find_orfs(&seq, false, 30, true);
        assert!(
            alt.iter()
                .any(|o| o.length_aa == 41 && o.aa_seq.starts_with('V')),
            "{alt:?}"
        );
    }

    #[test]
    fn min_aa_filters_short_orfs() {
        let seq = format!("ATG{}TAA", "AAA".repeat(10));
        assert!(find_orfs(&seq, false, 30, false).is_empty());
        assert!(!find_orfs(&seq, false, 5, false).is_empty());
    }

    #[test]
    fn stale_fingerprint_detects_canvas_move() {
        let a = record_fingerprint("pA", "ATGCATGC");
        assert!(!results_are_stale(&a, "pA", "ATGCATGC"));
        assert!(results_are_stale(&a, "pA", "GGGGGGGG"));
        assert!(results_are_stale(&a, "pB", "ATGCATGC"));
    }
}
