//! Exact-match in-silico PCR (`_simulate_pcr`). Wrap-aware on circular templates.

use splicecraft_bio::rc;
use splicecraft_core::{Feature, Record};
use splicecraft_primer::primer_tm;

use crate::error::GelError;

/// Primers shorter than this cannot anneal.
pub const PCR_MIN_PRIMER_LEN: usize = 10;
/// Absurdly long primer = user error.
pub const PCR_MAX_PRIMER_LEN: usize = 80;
/// Function / agent default ceiling (bp).
pub const PCR_DEFAULT_MAX_AMPLICON: usize = 20_000;
/// UI starting point for the max-amplicon box.
pub const PCR_UI_DEFAULT_MAX_AMPLICON: usize = 500;
/// Safety cap regardless of UI input.
pub const PCR_AMPLICON_HARD_CAP: usize = 100_000;
/// Cap on result count (mispriming runaway).
pub const PCR_MAX_AMPLICONS: usize = 50;
/// Skip chromosome-scale templates.
pub const PCR_MAX_TEMPLATE_BP: usize = 5_000_000;
/// Per-side exact-match hit cap (O(N²) defence).
pub const PCR_MAX_PRIMER_HITS: usize = 5_000;
/// Longest 3′ suffix considered for cloning-primer flaps.
pub const PCR_PARTIAL_MIN_BINDING: usize = 15;

/// One predicted PCR product.
#[derive(Clone, Debug, PartialEq)]
pub struct PcrAmplicon {
    /// 5′ on the top strand (0-based).
    pub start: usize,
    /// 3′ exclusive on the top strand (`== n` means end-at-origin).
    pub end: usize,
    /// Product length including both primers.
    pub length: usize,
    /// Crosses the origin (circular only).
    pub wraps: bool,
    /// Forward primer as supplied.
    pub fwd_seq: String,
    /// Reverse primer as supplied.
    pub rev_seq: String,
    /// Full top-strand product.
    pub amplicon_seq: String,
    /// GC % 0..100.
    pub gc_pct: f64,
    /// Full-primer Tm.
    pub fwd_tm: Option<f64>,
    /// Full-primer Tm.
    pub rev_tm: Option<f64>,
    /// Binding-region Tm (flap excluded).
    pub fwd_binding_tm: Option<f64>,
    /// Binding-region Tm (flap excluded).
    pub rev_binding_tm: Option<f64>,
    /// Annealing length of the forward primer.
    pub fwd_binding_len: usize,
    /// Annealing length of the reverse primer.
    pub rev_binding_len: usize,
    /// True when 5′ flaps were required to bind.
    pub partial_binding: bool,
    /// `fwd == rc(rev)` — many pairings are self-hits.
    pub palindromic_pair: bool,
}

/// Linear DNA record with `primer_bind` features at both ends.
#[must_use]
pub fn amplicon_to_record(amp: &PcrAmplicon) -> Record {
    let name = format!("amp_{}bp", amp.length);
    let mut rec = Record::new(&name, &amp.amplicon_seq, false);
    rec.molecule_type = "DNA".into();
    let fwd_len = amp.fwd_seq.len().min(amp.amplicon_seq.len());
    rec.features
        .push(Feature::new("primer_bind", 0, fwd_len, 1, "fwd"));
    let rev_len = amp.rev_seq.len().min(amp.amplicon_seq.len());
    let rev_start = amp.amplicon_seq.len().saturating_sub(rev_len);
    rec.features.push(Feature::new(
        "primer_bind",
        rev_start,
        amp.amplicon_seq.len(),
        -1,
        "rev",
    ));
    rec
}

/// Find every legal amplicon for `(fwd, rev)` on `template`.
///
/// Binding is exact-match (plus a 3′-anchored partial fallback for 5′
/// cloning flaps). IUPAC primers error so the UI can distinguish
/// "filtered" from "no bind".
pub fn simulate_pcr(
    template_seq: &str,
    fwd_primer: &str,
    rev_primer: &str,
    circular: bool,
    max_amplicon: i64,
) -> Result<Vec<PcrAmplicon>, GelError> {
    let seq = template_seq.to_ascii_uppercase().replace('U', "T");
    let fwd = fwd_primer
        .to_ascii_uppercase()
        .replace('U', "T")
        .trim()
        .to_owned();
    let rev = rev_primer
        .to_ascii_uppercase()
        .replace('U', "T")
        .trim()
        .to_owned();
    let n = seq.len();
    if n == 0 || fwd.is_empty() || rev.is_empty() {
        return Ok(Vec::new());
    }
    if n > PCR_MAX_TEMPLATE_BP {
        return Ok(Vec::new());
    }
    if fwd.len() < PCR_MIN_PRIMER_LEN || rev.len() < PCR_MIN_PRIMER_LEN {
        return Ok(Vec::new());
    }
    if fwd.len() > PCR_MAX_PRIMER_LEN || rev.len() > PCR_MAX_PRIMER_LEN {
        return Ok(Vec::new());
    }
    if let Some(bad) = non_acgt(&fwd) {
        return Err(GelError::Pcr(format!(
            "PCR simulator does not support IUPAC degenerate / ambiguity characters in primers. Use only A/C/G/T ({bad} primer rejected)."
        )));
    }
    if let Some(bad) = non_acgt(&rev) {
        return Err(GelError::Pcr(format!(
            "PCR simulator does not support IUPAC degenerate / ambiguity characters in primers. Use only A/C/G/T ({bad} primer rejected)."
        )));
    }

    let max_amp = max_amplicon.clamp(1, PCR_AMPLICON_HARD_CAP as i64) as usize;
    let min_amp = fwd.len() + rev.len();
    if max_amp < min_amp {
        return Ok(Vec::new());
    }

    let rev_rc = rc(&rev);
    let palindromic_pair = fwd == rev_rc;

    let search_seq = if circular {
        let extend_by = max_amp;
        if extend_by <= n {
            format!("{}{}", seq, &seq[..extend_by])
        } else {
            let full = extend_by / n;
            let rem = extend_by % n;
            let mut s = seq.repeat(full + 1);
            s.push_str(&seq[..rem]);
            s
        }
    } else {
        seq.clone()
    };

    let mut fwd_hits = exact_match_positions(&search_seq, &fwd);
    let mut rev_rc_hits = exact_match_positions(&search_seq, &rev_rc);
    let mut used_partial = false;
    let mut fwd_binding_len = fwd.len();
    let mut rev_binding_len = rev_rc.len();

    if fwd_hits.is_empty() && rev_rc_hits.is_empty() {
        let partial_fwd = partial_3p_binding_positions(&search_seq, &fwd, PCR_PARTIAL_MIN_BINDING);
        let mut partial_rev: Vec<(usize, usize)> = Vec::new();
        let min_k = 14.max(PCR_MIN_PRIMER_LEN);
        if rev_rc.len() >= min_k {
            for k in (min_k..=rev_rc.len()).rev() {
                let prefix = &rev_rc[..k];
                let hits = exact_match_positions(&search_seq, prefix);
                if !hits.is_empty() {
                    partial_rev = hits.into_iter().map(|p| (p, k)).collect();
                    break;
                }
            }
        }
        if !partial_fwd.is_empty() && !partial_rev.is_empty() {
            used_partial = true;
            fwd_hits = partial_fwd.iter().map(|(p, _)| *p).collect();
            rev_rc_hits = partial_rev.iter().map(|(p, _)| *p).collect();
            fwd_binding_len = partial_fwd[0].1;
            rev_binding_len = partial_rev[0].1;
        }
    }
    if fwd_hits.is_empty() || rev_rc_hits.is_empty() {
        return Ok(Vec::new());
    }
    if fwd_hits.len() > PCR_MAX_PRIMER_HITS || rev_rc_hits.len() > PCR_MAX_PRIMER_HITS {
        return Ok(Vec::new());
    }
    if circular {
        fwd_hits.retain(|p| *p < n);
    }

    let mut amplicons = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for fp in &fwd_hits {
        for rp in &rev_rc_hits {
            if *rp < *fp {
                continue;
            }
            let template_span_end = rp + rev_binding_len;
            let body_len = rp.saturating_sub(fp + fwd_binding_len);
            let length = fwd.len() + body_len + rev_rc.len();
            if length < min_amp || length > max_amp {
                continue;
            }
            if !circular && template_span_end > n {
                continue;
            }
            let wraps = circular && template_span_end > n;
            let start_t = *fp;
            let mut end_t = if circular {
                template_span_end % n
            } else {
                template_span_end
            };
            if circular && end_t == 0 && template_span_end == n {
                end_t = n;
            }
            if !seen.insert((start_t, end_t)) {
                continue;
            }
            let amplicon_seq = if used_partial {
                let body_start = fp + fwd_binding_len;
                let body = if circular {
                    if body_start >= n {
                        search_seq[body_start..*rp].to_owned()
                    } else if *rp >= n {
                        format!("{}{}", &seq[body_start..], &seq[..rp - n])
                    } else {
                        seq[body_start..*rp].to_owned()
                    }
                } else {
                    seq[body_start..*rp].to_owned()
                };
                format!("{fwd}{body}{rev_rc}")
            } else if circular && wraps {
                format!("{}{}", &seq[*fp..], &seq[..(rp + rev_rc.len()) - n])
            } else if circular {
                search_seq[*fp..fp + length].to_owned()
            } else {
                seq[*fp..fp + length].to_owned()
            };
            let gc = amplicon_seq
                .chars()
                .filter(|c| *c == 'G' || *c == 'C')
                .count() as f64
                / amplicon_seq.len().max(1) as f64
                * 100.0;
            let fwd_binding_seq = &fwd[fwd.len() - fwd_binding_len..];
            let rev_binding_seq = &rev[rev.len() - rev_binding_len..];
            amplicons.push(PcrAmplicon {
                start: start_t,
                end: end_t,
                length,
                wraps,
                fwd_seq: fwd.clone(),
                rev_seq: rev.clone(),
                amplicon_seq,
                gc_pct: gc,
                fwd_tm: primer_tm(&fwd),
                rev_tm: primer_tm(&rev),
                fwd_binding_tm: primer_tm(fwd_binding_seq),
                rev_binding_tm: primer_tm(rev_binding_seq),
                fwd_binding_len,
                rev_binding_len,
                partial_binding: used_partial,
                palindromic_pair,
            });
            if amplicons.len() >= PCR_MAX_AMPLICONS {
                amplicons.sort_by(|a, b| b.length.cmp(&a.length).then(a.start.cmp(&b.start)));
                return Ok(amplicons);
            }
        }
    }
    amplicons.sort_by(|a, b| b.length.cmp(&a.length).then(a.start.cmp(&b.start)));
    Ok(amplicons)
}

fn non_acgt(s: &str) -> Option<&'static str> {
    if s.chars().any(|c| !matches!(c, 'A' | 'C' | 'G' | 'T')) {
        Some("IUPAC")
    } else {
        None
    }
}

fn exact_match_positions(text: &str, pattern: &str) -> Vec<usize> {
    if pattern.is_empty() || text.is_empty() || pattern.len() > text.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(idx) = text[start..].find(pattern) {
        let abs = start + idx;
        out.push(abs);
        start = abs + 1;
        if start > text.len() {
            break;
        }
    }
    out
}

fn partial_3p_binding_positions(
    text: &str,
    primer: &str,
    min_binding: usize,
) -> Vec<(usize, usize)> {
    if text.is_empty() || primer.is_empty() || min_binding < 1 || min_binding > primer.len() {
        return Vec::new();
    }
    for k in (min_binding..=primer.len()).rev() {
        let suffix = &primer[primer.len() - k..];
        let hits = exact_match_positions(text, suffix);
        if !hits.is_empty() {
            return hits.into_iter().map(|p| (p, k)).collect();
        }
    }
    Vec::new()
}
