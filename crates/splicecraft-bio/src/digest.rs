//! Enzyme cut list and fragment split helpers.

use std::collections::BTreeMap;

use crate::enzymes::enzyme;
use crate::iupac::{iter_match_starts, iupac_pattern, rc};

/// One phosphodiester cut (top/bot in 0-based forward coordinates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnzymeCut {
    /// Top-strand cut.
    pub top: usize,
    /// Bottom-strand cut.
    pub bot: usize,
    /// `blunt`, `5'`, or `3'`.
    pub kind: String,
    /// Top-strand bases between the two cuts.
    pub overhang_seq: String,
    /// Enzyme name (isoschizomers joined with `/`).
    pub enzyme: String,
}

/// All cuts from `enzyme_names` on `seq`, sorted by top then name.
pub fn enzyme_cuts(seq: &str, enzyme_names: &[&str], circular: bool) -> Vec<EnzymeCut> {
    let n = seq.len();
    if n == 0 || enzyme_names.is_empty() {
        return Vec::new();
    }
    let seq_u = seq.to_ascii_uppercase();
    let mut out: BTreeMap<(usize, usize), EnzymeCut> = BTreeMap::new();
    let max_site_len = enzyme_names
        .iter()
        .filter_map(|e| enzyme(e).map(|(site, _, _)| site.len()))
        .max()
        .unwrap_or(0);
    let scan_seq = if circular && n > 0 && max_site_len > 1 {
        let mut s = seq_u.clone();
        s.push_str(&seq_u[..max_site_len.saturating_sub(1).min(n)]);
        s
    } else {
        seq_u.clone()
    };

    for ename in enzyme_names {
        let Some((site, fwd_cut, rev_cut)) = enzyme(ename) else {
            continue;
        };
        let site_u = site.to_ascii_uppercase();
        let site_len = site_u.len();
        let Ok(pat) = iupac_pattern(&site_u) else {
            continue;
        };
        let rc_site = rc(&site_u);
        let is_pal = rc_site == site_u;

        let mut emit = |top_raw: i32, bot_raw: i32| {
            let top_bp = (top_raw.rem_euclid(n as i32)) as usize;
            let bot_bp = (bot_raw.rem_euclid(n as i32)) as usize;
            let overhang_len = (top_raw - bot_raw).unsigned_abs() as usize;
            let oh_start = if top_raw <= bot_raw { top_bp } else { bot_bp };
            let oh_end = if n == 0 {
                0
            } else {
                (oh_start + overhang_len) % n
            };
            let overhang = if overhang_len == 0 {
                String::new()
            } else if oh_end > oh_start {
                seq_u[oh_start..oh_end].to_owned()
            } else {
                format!("{}{}", &seq_u[oh_start..], &seq_u[..oh_end])
            };
            let kind = if top_raw == bot_raw {
                "blunt"
            } else if top_raw < bot_raw {
                "5'"
            } else {
                "3'"
            };
            let key = (top_bp, bot_bp);
            if let Some(prev) = out.get_mut(&key) {
                let names: Vec<&str> = prev.enzyme.split('/').collect();
                if !names.contains(ename) {
                    prev.enzyme = format!("{}/{}", prev.enzyme, ename);
                }
                return;
            }
            out.insert(
                key,
                EnzymeCut {
                    top: top_bp,
                    bot: bot_bp,
                    kind: kind.into(),
                    overhang_seq: overhang,
                    enzyme: (*ename).to_owned(),
                },
            );
        };

        for p in iter_match_starts(&pat, &scan_seq) {
            if p >= n {
                continue;
            }
            if !circular
                && ((p as i32 + fwd_cut) < 0
                    || (p as i32 + fwd_cut) > n as i32
                    || (p as i32 + rev_cut) < 0
                    || (p as i32 + rev_cut) > n as i32)
            {
                continue;
            }
            emit(p as i32 + fwd_cut, p as i32 + rev_cut);
        }
        if !is_pal {
            let Ok(rc_pat) = iupac_pattern(&rc_site) else {
                continue;
            };
            for p in iter_match_starts(&rc_pat, &scan_seq) {
                if p >= n {
                    continue;
                }
                let rev_top = p as i32 + site_len as i32 - rev_cut;
                let rev_bot = p as i32 + site_len as i32 - fwd_cut;
                if !circular
                    && (rev_top < 0 || rev_top > n as i32 || rev_bot < 0 || rev_bot > n as i32)
                {
                    continue;
                }
                emit(rev_top, rev_bot);
            }
        }
    }

    let mut v: Vec<_> = out.into_values().collect();
    v.sort_by(|a, b| a.top.cmp(&b.top).then(a.enzyme.cmp(&b.enzyme)));
    v
}

/// Digest `seq` with `enzyme_names` into top-strand fragments.
pub fn digest_with_enzymes(seq: &str, enzyme_names: &[&str], circular: bool) -> Vec<String> {
    let cuts = enzyme_cuts(seq, enzyme_names, circular);
    fragments_from_cuts(seq, &cuts, circular)
}

/// Slice `seq` at cut tops into contiguous top-strand pieces.
pub fn fragments_from_cuts(seq: &str, cuts: &[EnzymeCut], circular: bool) -> Vec<String> {
    let n = seq.len();
    if n == 0 {
        return Vec::new();
    }
    if cuts.is_empty() {
        return vec![seq.to_owned()];
    }
    let mut tops: Vec<usize> = cuts.iter().map(|c| c.top).collect();
    tops.sort_unstable();
    tops.dedup();
    if circular {
        let mut frags = Vec::new();
        for i in 0..tops.len() {
            let a = tops[i];
            let b = tops[(i + 1) % tops.len()];
            if a < b {
                frags.push(seq[a..b].to_owned());
            } else {
                frags.push(format!("{}{}", &seq[a..], &seq[..b]));
            }
        }
        frags
    } else {
        let mut bounds = vec![0];
        bounds.extend(tops);
        bounds.push(n);
        bounds
            .windows(2)
            .map(|w| seq[w[0]..w[1]].to_owned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecori_cut_on_synthetic_template() {
        let padding = "AAAAA";
        let seq = format!("{padding}GAATTC{padding}");
        let hits = enzyme_cuts(&seq, &["EcoRI"], false);
        let tops: Vec<usize> = hits.iter().map(|h| h.top).collect();
        // pad(5) + G^AATTC → top cut at 6
        assert!(tops.contains(&6), "{tops:?}");
        assert_eq!(hits[0].kind, "5'");
        assert_eq!(hits[0].overhang_seq, "AATT");
    }

    #[test]
    fn bsai_type_iis_top_cut() {
        let padding = "AAAAAAAAAA";
        let seq = format!("{padding}GGTCTC{padding}");
        let hits = enzyme_cuts(&seq, &["BsaI"], false);
        let tops: Vec<usize> = hits.iter().map(|h| h.top).collect();
        // pad(10) + fwd_cut 7 → 17
        assert!(tops.contains(&17), "{tops:?}");
    }

    #[test]
    fn digest_circular_single_cut_one_fragment() {
        let seq = format!("AAA{}AAA", "GAATTC");
        let frags = digest_with_enzymes(&seq, &["EcoRI"], true);
        assert_eq!(frags.len(), 1);
        assert_eq!(frags[0].len(), seq.len());
    }
}
