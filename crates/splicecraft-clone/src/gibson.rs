//! N-fragment Gibson: longest exact overlap, wrap junction, homology arms.

use splicecraft_bio::rc;

use crate::error::CloneError;
use crate::fragment::FragFeature;

/// Default minimum homology (Gibson's commonly cited floor).
pub const GIBSON_MIN_OVERLAP_BP: usize = 15;
/// Cap on the suffix/prefix probe.
pub const GIBSON_MAX_OVERLAP_BP: usize = 200;

/// One linear Gibson input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GibsonFragment {
    /// Lane label.
    pub name: String,
    /// Top-strand DNA 5′→3′.
    pub sequence: String,
    /// Features in fragment-local coordinates.
    pub features: Vec<FragFeature>,
    /// Length of an auto-designed 5′ arm (idempotent strip).
    pub arm5_len: usize,
}

impl GibsonFragment {
    /// Named sequence, no features.
    #[must_use]
    pub fn new(name: impl Into<String>, sequence: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sequence: sequence.into(),
            features: Vec::new(),
            arm5_len: 0,
        }
    }

    /// From a record (sequence only; features copied linearly).
    #[must_use]
    pub fn from_record(rec: &splicecraft_core::Record) -> Self {
        Self {
            name: rec.name.clone(),
            sequence: rec.sequence.clone(),
            features: rec.features.iter().map(FragFeature::from_core).collect(),
            arm5_len: 0,
        }
    }
}

/// One detected junction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GibsonOverlap {
    /// 1-based junction index.
    pub junction: usize,
    /// Upstream fragment name.
    pub from: String,
    /// Downstream fragment name.
    pub to: String,
    /// Longest exact overlap (0 if none ≥ min).
    pub length: usize,
    /// Overlap bases (empty when length is 0).
    pub seq: String,
    /// `length >= min_overlap`.
    pub ok: bool,
    /// Last junction of a circular assembly.
    pub is_wrap: bool,
    /// Reverse-orientation hint (empty when ok).
    pub rc_hint: String,
}

/// Gibson simulator result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GibsonResult {
    /// All junctions met min-overlap and body-length checks.
    pub success: bool,
    /// Product top strand (overlaps appear once).
    pub product_seq: String,
    /// Requested topology.
    pub circular: bool,
    /// Features in product coordinates (wrap halves re-merged when adjacent).
    pub features: Vec<FragFeature>,
    /// One entry per junction (including failed ones).
    pub overlaps: Vec<GibsonOverlap>,
    /// Hard failures.
    pub errors: Vec<String>,
    /// Soft notes (short fragments).
    pub warnings: Vec<String>,
}

/// Longest exact suffix(a)/prefix(b) overlap in `[min, max]`. Returns 0 if none.
#[must_use]
pub fn gibson_overlap_len(
    a_seq: &str,
    b_seq: &str,
    min_overlap: usize,
    max_overlap: usize,
) -> usize {
    let min_overlap = min_overlap.max(1);
    let a = a_seq.to_ascii_uppercase();
    let b = b_seq.to_ascii_uppercase();
    let full_match_safe = a != b;
    let mut max_check = max_overlap.min(a.len()).min(b.len());
    if !full_match_safe {
        max_check = max_check.min(a.len().saturating_sub(1));
    }
    if max_check < min_overlap {
        return 0;
    }
    for k in (min_overlap..=max_check).rev() {
        if a[a.len() - k..] == b[..k] {
            return k;
        }
    }
    0
}

/// Rotate a circular sequence so `at` becomes index 0.
#[must_use]
pub fn linearize_at(seq: &str, at: usize) -> String {
    let n = seq.len();
    if n == 0 {
        return String::new();
    }
    let at = at % n;
    format!("{}{}", &seq[at..], &seq[..at])
}

/// Simulate Gibson of N linear fragments.
#[must_use]
pub fn simulate_gibson_assembly(
    fragments: &[GibsonFragment],
    min_overlap: usize,
    circular: bool,
) -> GibsonResult {
    let min_overlap = if min_overlap == 0 {
        GIBSON_MIN_OVERLAP_BP
    } else {
        min_overlap
    };
    let Some(norm) = normalize(fragments) else {
        return fail(circular, vec!["No fragments supplied.".into()], Vec::new());
    };
    for f in &norm {
        if f.sequence.is_empty() {
            return fail(
                circular,
                vec![format!("Fragment '{}' has no sequence.", f.name)],
                Vec::new(),
            );
        }
    }
    if norm.len() == 1 && !circular {
        return GibsonResult {
            success: true,
            product_seq: norm[0].sequence.clone(),
            circular: false,
            features: norm[0].features.clone(),
            overlaps: Vec::new(),
            errors: Vec::new(),
            warnings: vec![
                "Single linear fragment — no Gibson junctions to validate. \
                 Product is the fragment as supplied."
                    .into(),
            ],
        };
    }
    let (overlaps, overlap_lens, junction_errors) = detect_overlaps(&norm, min_overlap, circular);
    if !junction_errors.is_empty() {
        return fail(circular, junction_errors, overlaps);
    }
    let body_errors = validate_body_lengths(&norm, &overlap_lens, circular);
    if !body_errors.is_empty() {
        return fail(circular, body_errors, overlaps);
    }
    let warnings = short_fragment_warnings(&norm, min_overlap);
    let (product_seq, offsets) = build_product(&norm, &overlap_lens, circular);
    let product_len = product_seq.len();
    let shifted = shift_features(&norm, &overlap_lens, &offsets, product_len, circular);
    GibsonResult {
        success: true,
        product_seq,
        circular,
        features: shifted,
        overlaps,
        errors: Vec::new(),
        warnings,
    }
}

/// Append 5′ homology arms so every junction reaches `min_overlap`. Idempotent.
pub fn design_homology_arms(
    lane: &mut [GibsonFragment],
    min_overlap: usize,
    circular: bool,
) -> Result<(usize, usize, Vec<String>), CloneError> {
    if lane.len() < 2 {
        return Err(CloneError::assembly(
            "Add at least two fragments before designing overlaps.",
        ));
    }
    let min_oh = min_overlap.max(1);
    for f in lane.iter_mut() {
        let prev = f.arm5_len;
        if prev > 0 {
            if prev <= f.sequence.len() {
                f.sequence = f.sequence[prev..].to_owned();
            }
            for ft in &mut f.features {
                ft.start = ft.start.saturating_sub(prev);
                ft.end = ft.end.saturating_sub(prev);
            }
            f.arm5_len = 0;
        }
    }
    let n = lane.len();
    let mut junctions: Vec<(usize, usize)> = (1..n).map(|i| (i - 1, i)).collect();
    if circular {
        junctions.push((n - 1, 0));
    }
    let mut armed = 0usize;
    let mut already = 0usize;
    let mut skipped = Vec::new();
    for (up_i, down_i) in junctions {
        let up_seq = lane[up_i].sequence.clone();
        let down_seq = lane[down_i].sequence.clone();
        if gibson_overlap_len(&up_seq, &down_seq, min_oh, GIBSON_MAX_OVERLAP_BP) >= min_oh {
            already += 1;
            continue;
        }
        if up_seq.len() < min_oh {
            skipped.push(lane[down_i].name.clone());
            continue;
        }
        let arm = up_seq[up_seq.len() - min_oh..].to_owned();
        let arm_len = arm.len();
        lane[down_i].sequence = format!("{arm}{down_seq}");
        lane[down_i].arm5_len = arm_len;
        for ft in &mut lane[down_i].features {
            ft.start += arm_len;
            ft.end += arm_len;
        }
        armed += 1;
    }
    Ok((armed, already, skipped))
}

fn fail(circular: bool, errors: Vec<String>, overlaps: Vec<GibsonOverlap>) -> GibsonResult {
    GibsonResult {
        success: false,
        product_seq: String::new(),
        circular,
        features: Vec::new(),
        overlaps,
        errors,
        warnings: Vec::new(),
    }
}

fn normalize(fragments: &[GibsonFragment]) -> Option<Vec<GibsonFragment>> {
    if fragments.is_empty() {
        return None;
    }
    Some(
        fragments
            .iter()
            .map(|f| {
                let cleaned: String = f
                    .sequence
                    .chars()
                    .map(|c| c.to_ascii_uppercase())
                    .map(|c| if c == 'U' { 'T' } else { c })
                    .filter(|c| !c.is_whitespace())
                    .collect();
                GibsonFragment {
                    name: if f.name.is_empty() {
                        "?".into()
                    } else {
                        f.name.clone()
                    },
                    sequence: cleaned,
                    features: f.features.clone(),
                    arm5_len: f.arm5_len,
                }
            })
            .collect(),
    )
}

fn detect_overlaps(
    norm: &[GibsonFragment],
    min_overlap: usize,
    circular: bool,
) -> (Vec<GibsonOverlap>, Vec<usize>, Vec<String>) {
    let n = norm.len();
    let n_junctions = if circular { n } else { n.saturating_sub(1) };
    let mut overlaps = Vec::new();
    let mut lens = Vec::new();
    let mut errors = Vec::new();
    for i in 0..n_junctions {
        let a = &norm[i];
        let b = &norm[(i + 1) % n];
        let k = gibson_overlap_len(&a.sequence, &b.sequence, min_overlap, GIBSON_MAX_OVERLAP_BP);
        let ok = k >= min_overlap;
        let rc_hint = if ok {
            String::new()
        } else {
            rc_hint(a, b, min_overlap)
        };
        if !ok {
            errors.push(format!(
                "Junction {} ('{}' → '{}'): no overlap ≥ {min_overlap} bp.{rc_hint}",
                i + 1,
                a.name,
                b.name
            ));
        }
        overlaps.push(GibsonOverlap {
            junction: i + 1,
            from: a.name.clone(),
            to: b.name.clone(),
            length: k,
            seq: if k == 0 {
                String::new()
            } else {
                a.sequence[a.sequence.len() - k..].to_owned()
            },
            ok,
            is_wrap: circular && i + 1 == n,
            rc_hint,
        });
        lens.push(k);
    }
    (overlaps, lens, errors)
}

fn rc_hint(a: &GibsonFragment, b: &GibsonFragment, min_overlap: usize) -> String {
    let probe_min = min_overlap.min(10);
    let k_b_rc = gibson_overlap_len(
        &a.sequence,
        &rc(&b.sequence),
        probe_min,
        GIBSON_MAX_OVERLAP_BP,
    );
    let k_a_rc = gibson_overlap_len(
        &rc(&a.sequence),
        &b.sequence,
        probe_min,
        GIBSON_MAX_OVERLAP_BP,
    );
    if k_b_rc >= probe_min && k_b_rc >= k_a_rc {
        format!(
            " — but reverse-complement of '{}' yields a {k_b_rc} bp overlap; did you mean to flip '{}'?",
            b.name, b.name
        )
    } else if k_a_rc >= probe_min {
        format!(
            " — but reverse-complement of '{}' yields a {k_a_rc} bp overlap; did you mean to flip '{}'?",
            a.name, a.name
        )
    } else {
        String::new()
    }
}

fn validate_body_lengths(
    norm: &[GibsonFragment],
    overlap_lens: &[usize],
    circular: bool,
) -> Vec<String> {
    let n = norm.len();
    let mut errors = Vec::new();
    for i in 1..n {
        let oh_lead = overlap_lens[i - 1];
        let frag_len = norm[i].sequence.len();
        if oh_lead >= frag_len {
            errors.push(format!(
                "Fragment '{}' is consumed by its leading {oh_lead} bp overlap (fragment is {frag_len} bp). \
                 Use a longer fragment or shorter overlap.",
                norm[i].name
            ));
        }
    }
    if circular && errors.is_empty() {
        if n == 1 {
            let wrap_oh = overlap_lens[0];
            let frag_len = norm[0].sequence.len();
            if wrap_oh >= frag_len {
                errors.push(format!(
                    "Fragment '{}' is fully consumed by its self-circularisation overlap ({wrap_oh} ≥ {frag_len} bp).",
                    norm[0].name
                ));
            }
        } else {
            let last_lead = overlap_lens[n - 2];
            let last_trail = overlap_lens[n - 1];
            let last_len = norm[n - 1].sequence.len();
            if last_lead + last_trail >= last_len {
                errors.push(format!(
                    "Fragment '{}' is fully consumed by its homology arms ({last_lead} + {last_trail} ≥ {last_len} bp). \
                     Pick a longer fragment or shorter overlaps.",
                    norm[n - 1].name
                ));
            }
            let first_lead = overlap_lens[n - 1];
            let first_trail = overlap_lens[0];
            let first_len = norm[0].sequence.len();
            if first_lead + first_trail >= first_len {
                errors.push(format!(
                    "Fragment '{}' is fully consumed by its homology arms ({first_lead} + {first_trail} ≥ {first_len} bp). \
                     Pick a longer fragment or shorter overlaps.",
                    norm[0].name
                ));
            }
        }
    }
    errors
}

fn short_fragment_warnings(norm: &[GibsonFragment], min_overlap: usize) -> Vec<String> {
    norm.iter()
        .filter(|f| {
            let n = f.sequence.len();
            n > 0 && n < 3 * min_overlap
        })
        .map(|f| {
            format!(
                "Fragment '{}' is short ({} bp) relative to the {min_overlap} bp homology arms — \
                 assembly may be hard to confirm by gel.",
                f.name,
                f.sequence.len()
            )
        })
        .collect()
}

fn build_product(
    norm: &[GibsonFragment],
    overlap_lens: &[usize],
    circular: bool,
) -> (String, Vec<i32>) {
    let n = norm.len();
    let mut seq_parts = vec![norm[0].sequence.clone()];
    let mut offsets = vec![0i32];
    let mut cursor = norm[0].sequence.len() as i32;
    for i in 1..n {
        let oh_lead = overlap_lens[i - 1];
        let frag_seq = &norm[i].sequence;
        let body = if oh_lead <= frag_seq.len() {
            frag_seq[oh_lead..].to_owned()
        } else {
            String::new()
        };
        seq_parts.push(body.clone());
        offsets.push(cursor - oh_lead as i32);
        cursor += body.len() as i32;
    }
    if circular {
        let wrap_oh = overlap_lens[n - 1];
        if wrap_oh > 0 {
            if n == 1 {
                let s = &mut seq_parts[0];
                if wrap_oh <= s.len() {
                    s.truncate(s.len() - wrap_oh);
                }
            } else {
                let s = seq_parts.last_mut().expect("n>1");
                if wrap_oh <= s.len() {
                    s.truncate(s.len() - wrap_oh);
                }
            }
        }
    }
    (seq_parts.concat(), offsets)
}

fn shift_features(
    norm: &[GibsonFragment],
    overlap_lens: &[usize],
    offsets: &[i32],
    product_len: usize,
    circular: bool,
) -> Vec<FragFeature> {
    let mut shifted = Vec::new();
    for (i, f_dict) in norm.iter().enumerate() {
        let offset = offsets[i];
        let oh_lead = if i > 0 { overlap_lens[i - 1] } else { 0 };
        for feat in &f_dict.features {
            if feat.kind == "source" || feat.end <= feat.start {
                continue;
            }
            if i > 0 && feat.end <= oh_lead {
                continue;
            }
            let mut new_s = offset + feat.start as i32;
            let mut new_e = offset + feat.end as i32;
            let span = new_e - new_s;
            if circular && product_len > 0 {
                let ms = new_s.rem_euclid(product_len as i32) as usize;
                let me_raw = new_e.rem_euclid(product_len as i32) as usize;
                let me = if me_raw == 0 && span > 0 {
                    product_len
                } else {
                    me_raw
                };
                if span as usize >= product_len {
                    new_s = 0;
                    new_e = product_len as i32;
                } else {
                    new_s = ms as i32;
                    new_e = me as i32;
                }
            } else {
                if new_s < 0 {
                    continue;
                }
                if new_e > product_len as i32 {
                    new_e = product_len as i32;
                }
                if new_e <= new_s {
                    continue;
                }
            }
            shifted.push(FragFeature {
                start: new_s as usize,
                end: new_e as usize,
                strand: feat.strand,
                label: feat.label.clone(),
                kind: feat.kind.clone(),
                split: feat.split.clone(),
                note: feat.note.clone(),
            });
        }
    }
    shifted
}
