//! Fragment end chemistry, digest, ligation, and reverse-complement.

use splicecraft_bio::{enzyme, enzyme_cuts, rc};
use splicecraft_core::Feature;

use crate::error::CloneError;

/// One sticky / blunt / linear edge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FragEnd {
    /// Top-strand-canonical overhang (empty for blunt / linear).
    pub overhang_seq: String,
    /// `5'`, `3'`, `blunt`, or `linear`.
    pub kind: String,
    /// Enzyme that left this edge (empty for a molecule terminus).
    pub enzyme: String,
}

impl FragEnd {
    /// Uncut linear terminus.
    #[must_use]
    pub fn linear() -> Self {
        Self {
            overhang_seq: String::new(),
            kind: "linear".into(),
            enzyme: String::new(),
        }
    }
}

/// A feature in fragment-local coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FragFeature {
    /// Inclusive start (0-based).
    pub start: usize,
    /// Exclusive end.
    pub end: usize,
    /// `1` / `-1` / `0`.
    pub strand: i8,
    /// Display label.
    pub label: String,
    /// GenBank-style type.
    pub kind: String,
    /// Split tag (`head` / `tail` / `whole` / `mid`) from a cut inside the gene.
    pub split: Option<String>,
    /// Optional note (disruption).
    pub note: String,
}

impl FragFeature {
    /// Lift a record feature into fragment-local coords (caller shifts).
    #[must_use]
    pub fn from_core(feat: &Feature) -> Self {
        Self {
            start: feat.start,
            end: feat.end,
            strand: feat.strand,
            label: feat.label.clone(),
            kind: feat.kind.clone(),
            split: None,
            note: feat.qualifiers.get("note").cloned().unwrap_or_default(),
        }
    }

    /// Record feature in the same coordinates.
    #[must_use]
    pub fn to_core(&self) -> Feature {
        let mut f = Feature::new(
            self.kind.clone(),
            self.start,
            self.end,
            self.strand,
            self.label.clone(),
        );
        if !self.note.is_empty() {
            f.qualifiers.insert("note".into(), self.note.clone());
        }
        f
    }
}

/// Linear digest piece with sticky ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    /// Top-strand body (overhang inclusion follows the digest convention).
    pub top_seq: String,
    /// 5′ edge.
    pub left: FragEnd,
    /// 3′ edge.
    pub right: FragEnd,
    /// Features in `top_seq`-local coordinates.
    pub features: Vec<FragFeature>,
    /// Source name (insert / vector / part).
    pub source_label: String,
}

/// Same kind + matching overhang. A `linear` edge never ligates.
#[must_use]
pub fn ends_compatible(a: &FragEnd, b: &FragEnd) -> bool {
    if a.kind == "linear" || b.kind == "linear" {
        return false;
    }
    a.kind == b.kind && a.overhang_seq == b.overhang_seq
}

/// Ligate `a.right` to `b.left`. Features from `b` shift by `len(a.top_seq)`.
#[must_use]
pub fn ligate_fragments(a: &Fragment, b: &Fragment) -> Option<Fragment> {
    if !ends_compatible(&a.right, &b.left) {
        return None;
    }
    let shift = a.top_seq.len();
    let mut features = a.features.clone();
    for f in &b.features {
        let mut g = f.clone();
        g.start += shift;
        g.end += shift;
        features.push(g);
    }
    let source_label = if a.source_label.is_empty() && b.source_label.is_empty() {
        String::new()
    } else {
        format!("{}+{}", a.source_label, b.source_label)
    };
    Some(Fragment {
        top_seq: format!("{}{}", a.top_seq, b.top_seq),
        left: a.left.clone(),
        right: b.right.clone(),
        features,
        source_label,
    })
}

/// Close a linear fragment by ligating its own ends.
#[must_use]
pub fn close_circular(frag: &Fragment) -> Option<ClosedProduct> {
    if !ends_compatible(&frag.right, &frag.left) {
        return None;
    }
    Some(ClosedProduct {
        top_seq: frag.top_seq.clone(),
        features: frag.features.clone(),
        source_label: frag.source_label.clone(),
        circular: true,
    })
}

/// Closed ligation product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedProduct {
    /// Circle top strand, linearised at the join.
    pub top_seq: String,
    /// Features in product coordinates.
    pub features: Vec<FragFeature>,
    /// Joined source labels.
    pub source_label: String,
    /// Always `true` for a successful close.
    pub circular: bool,
}

/// Reverse-complement a fragment, swapping ends. Synthetic stamps use naive RC.
#[must_use]
pub fn rc_fragment(frag: &Fragment) -> Fragment {
    let n = frag.top_seq.len();
    let top = &frag.top_seq;
    let left_oh = frag.left.overhang_seq.as_str();
    let right_oh = frag.right.overhang_seq.as_str();
    let can_check_left = frag.left.kind == "5'" && !left_oh.is_empty();
    let can_check_right = frag.right.kind == "3'" && !right_oh.is_empty();
    let excise_match_left = can_check_left
        && top
            .get(..left_oh.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(left_oh));
    let excise_match_right = can_check_right
        && n >= right_oh.len()
        && top[n - right_oh.len()..].eq_ignore_ascii_case(right_oh);
    let synth_match_left = can_check_left && !excise_match_left;
    let synth_match_right = can_check_right && !excise_match_right;
    let is_excise = !(synth_match_left || synth_match_right);

    let (new_top, left_strip, new_left_extra_len) = if !is_excise {
        (rc(top), 0, 0)
    } else {
        let left_strip = if excise_match_left { left_oh.len() } else { 0 };
        let right_strip = if excise_match_right {
            right_oh.len()
        } else {
            0
        };
        let core = if right_strip == 0 {
            &top[left_strip..]
        } else {
            &top[left_strip..n - right_strip]
        };
        let new_left_extra = if frag.right.kind == "5'" && !right_oh.is_empty() && right_strip == 0
        {
            rc(right_oh)
        } else {
            String::new()
        };
        let new_right_extra = if frag.left.kind == "3'" && !left_oh.is_empty() && left_strip == 0 {
            rc(left_oh)
        } else {
            String::new()
        };
        let extra_len = new_left_extra.len();
        (
            format!("{}{}{}", new_left_extra, rc(core), new_right_extra),
            left_strip,
            extra_len,
        )
    };
    let new_n = new_top.len();
    let features = frag
        .features
        .iter()
        .map(|f| {
            let new_start_raw =
                (n as i32 - f.end as i32) - left_strip as i32 + new_left_extra_len as i32;
            let new_end_raw =
                (n as i32 - f.start as i32) - left_strip as i32 + new_left_extra_len as i32;
            FragFeature {
                start: new_start_raw.clamp(0, new_n as i32) as usize,
                end: new_end_raw.clamp(0, new_n as i32) as usize,
                strand: -f.strand,
                label: f.label.clone(),
                kind: f.kind.clone(),
                split: f.split.clone(),
                note: f.note.clone(),
            }
        })
        .collect();
    Fragment {
        top_seq: new_top,
        left: FragEnd {
            overhang_seq: if right_oh.is_empty() {
                String::new()
            } else {
                rc(right_oh)
            },
            kind: frag.right.kind.clone(),
            enzyme: frag.right.enzyme.clone(),
        },
        right: FragEnd {
            overhang_seq: if left_oh.is_empty() {
                String::new()
            } else {
                rc(left_oh)
            },
            kind: frag.left.kind.clone(),
            enzyme: frag.left.enzyme.clone(),
        },
        features,
        source_label: frag.source_label.clone(),
    }
}

/// Stamp canonical sticky ends. Type IIS is refused (overhang is not in the site).
pub fn make_synthetic_fragment(
    seq: &str,
    enz_left: &str,
    enz_right: &str,
    source_label: &str,
    features: Vec<FragFeature>,
) -> Result<Fragment, CloneError> {
    let (site_l, fwd_l, rev_l) =
        enzyme(enz_left).ok_or_else(|| CloneError::UnknownEnzyme(enz_left.into()))?;
    let (site_r, fwd_r, rev_r) =
        enzyme(enz_right).ok_or_else(|| CloneError::UnknownEnzyme(enz_right.into()))?;
    let site_l_u = site_l.to_ascii_uppercase();
    let site_r_u = site_r.to_ascii_uppercase();
    let (lo_l, hi_l) = (fwd_l.min(rev_l), fwd_l.max(rev_l));
    let (lo_r, hi_r) = (fwd_r.min(rev_r), fwd_r.max(rev_r));
    if hi_l > site_l_u.len() as i32 || lo_l < 0 {
        return Err(CloneError::TypeIisSynthetic(enz_left.into()));
    }
    if hi_r > site_r_u.len() as i32 || lo_r < 0 {
        return Err(CloneError::TypeIisSynthetic(enz_right.into()));
    }
    Ok(Fragment {
        top_seq: seq.to_ascii_uppercase(),
        left: FragEnd {
            overhang_seq: site_l_u[lo_l as usize..hi_l as usize].to_owned(),
            kind: cut_kind(fwd_l, rev_l),
            enzyme: enz_left.into(),
        },
        right: FragEnd {
            overhang_seq: site_r_u[lo_r as usize..hi_r as usize].to_owned(),
            kind: cut_kind(fwd_r, rev_r),
            enzyme: enz_right.into(),
        },
        features,
        source_label: source_label.into(),
    })
}

/// True when the enzyme cuts outside its recognition site.
#[must_use]
pub fn enzyme_is_type_iis(name: &str) -> bool {
    let Some((site, fwd, rev)) = enzyme(name) else {
        return false;
    };
    let lo = fwd.min(rev);
    let hi = fwd.max(rev);
    hi > site.len() as i32 || lo < 0
}

fn cut_kind(fwd: i32, rev: i32) -> String {
    if fwd == rev {
        "blunt".into()
    } else if fwd < rev {
        "5'".into()
    } else {
        "3'".into()
    }
}

/// Digest into fragments that carry left/right chemistry.
#[must_use]
pub fn digest_to_fragments(
    seq: &str,
    enzyme_names: &[&str],
    circular: bool,
    features: &[Feature],
    source_label: &str,
) -> Vec<Fragment> {
    let cuts = enzyme_cuts(seq, enzyme_names, circular);
    fragments_from_enzyme_cuts(seq, &cuts, circular, features, source_label)
}

/// Two-cut circular excise, or an error naming the cut count.
pub fn excise_fragment_pair(
    seq: &str,
    enzyme_names: &[&str],
    circular: bool,
    features: &[Feature],
    source_label: &str,
) -> Result<Vec<Fragment>, CloneError> {
    let cuts = enzyme_cuts(seq, enzyme_names, circular);
    let n_cuts = cuts.len();
    if n_cuts == 0 {
        return Err(CloneError::digest(format!(
            "no cut sites found for {}",
            if enzyme_names.is_empty() {
                "(none)".into()
            } else {
                enzyme_names.join(", ")
            }
        )));
    }
    let fragments = fragments_from_enzyme_cuts(seq, &cuts, circular, features, source_label);
    if circular && n_cuts < 2 {
        return Err(CloneError::digest(format!(
            "need ≥2 cuts to excise an insert; got {n_cuts} on a circular plasmid"
        )));
    }
    if circular && n_cuts > 2 {
        return Err(CloneError::digest(format!(
            "got {n_cuts} cut sites; need exactly 2 for unambiguous excise"
        )));
    }
    Ok(fragments)
}

fn fragments_from_enzyme_cuts(
    seq: &str,
    cuts: &[splicecraft_bio::EnzymeCut],
    circular: bool,
    features: &[Feature],
    source_label: &str,
) -> Vec<Fragment> {
    let n = seq.len();
    if n == 0 {
        return Vec::new();
    }
    if cuts.is_empty() {
        return vec![Fragment {
            top_seq: seq.to_owned(),
            left: FragEnd::linear(),
            right: FragEnd::linear(),
            features: features.iter().map(FragFeature::from_core).collect(),
            source_label: source_label.into(),
        }];
    }
    let mut cuts: Vec<_> = cuts.to_vec();
    let unique_tops: std::collections::BTreeSet<usize> = cuts.iter().map(|c| c.top).collect();
    if unique_tops.len() != cuts.len() {
        cuts.sort_by_key(|c| c.top);
        let mut seen = std::collections::HashSet::new();
        cuts.retain(|c| seen.insert(c.top));
    }
    let cut_tops: Vec<usize> = cuts.iter().map(|c| c.top).collect();
    let slots = split_features_at_cuts(features, n, &cut_tops, circular);
    if circular {
        return cuts
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let nxt = &cuts[(i + 1) % cuts.len()];
                let (a, b) = (c.top, nxt.top);
                let top_seq = if a < b {
                    seq[a..b].to_owned()
                } else {
                    format!("{}{}", &seq[a..], &seq[..b])
                };
                let frag_len = top_seq.len();
                let local = slot_features_circular(
                    slots.get(&i).map(Vec::as_slice).unwrap_or(&[]),
                    a,
                    b,
                    n,
                    frag_len,
                );
                Fragment {
                    top_seq,
                    left: end_from_cut(c),
                    right: end_from_cut(nxt),
                    features: local,
                    source_label: source_label.into(),
                }
            })
            .collect();
    }
    let mut bounds = vec![0];
    bounds.extend(cut_tops);
    bounds.push(n);
    let mut out = Vec::new();
    for i in 0..bounds.len() - 1 {
        let (a, b) = (bounds[i], bounds[i + 1]);
        let top_seq = seq[a..b].to_owned();
        let local = slots
            .get(&i)
            .into_iter()
            .flatten()
            .map(|f| {
                let mut g = f.clone();
                g.start = g.start.saturating_sub(a).min(top_seq.len());
                g.end = g.end.saturating_sub(a).min(top_seq.len());
                g
            })
            .collect();
        let left = if i == 0 {
            FragEnd::linear()
        } else {
            end_from_cut(&cuts[i - 1])
        };
        let right = if i == bounds.len() - 2 {
            FragEnd::linear()
        } else {
            end_from_cut(&cuts[i])
        };
        out.push(Fragment {
            top_seq,
            left,
            right,
            features: local,
            source_label: source_label.into(),
        });
    }
    out
}

fn end_from_cut(c: &splicecraft_bio::EnzymeCut) -> FragEnd {
    FragEnd {
        overhang_seq: c.overhang_seq.clone(),
        kind: c.kind.clone(),
        enzyme: c.enzyme.clone(),
    }
}

fn slot_features_circular(
    feats: &[FragFeature],
    a: usize,
    b: usize,
    n: usize,
    frag_len: usize,
) -> Vec<FragFeature> {
    let mut local = Vec::new();
    for f in feats {
        let (new_s, new_e) = if a < b {
            (f.start.saturating_sub(a), f.end.saturating_sub(a))
        } else {
            let new_s = (f.start + n - a) % n;
            let new_s = if new_s >= frag_len { 0 } else { new_s };
            let new_e = if f.end == f.start {
                new_s
            } else {
                let e = ((f.end + n - a - 1) % n) + 1;
                if e > frag_len { frag_len } else { e }
            };
            (new_s, new_e)
        };
        let new_s = new_s.min(frag_len);
        let new_e = new_e.min(frag_len);
        if new_s > new_e {
            local.push(FragFeature {
                start: new_s,
                end: frag_len,
                ..f.clone()
            });
            local.push(FragFeature {
                start: 0,
                end: new_e,
                ..f.clone()
            });
        } else {
            local.push(FragFeature {
                start: new_s,
                end: new_e,
                ..f.clone()
            });
        }
    }
    local
}

fn split_features_at_cuts(
    features: &[Feature],
    n: usize,
    cut_tops: &[usize],
    circular: bool,
) -> std::collections::HashMap<usize, Vec<FragFeature>> {
    let mut out: std::collections::HashMap<usize, Vec<FragFeature>> =
        std::collections::HashMap::new();
    if features.is_empty() {
        return out;
    }
    if cut_tops.is_empty() {
        out.insert(0, features.iter().map(FragFeature::from_core).collect());
        return out;
    }
    let mut expanded = Vec::new();
    for f in features {
        if circular && f.end < f.start && f.end <= n && f.start <= n {
            let mut tail = FragFeature::from_core(f);
            tail.end = n;
            expanded.push(tail);
            let mut head = FragFeature::from_core(f);
            head.start = 0;
            expanded.push(head);
        } else {
            expanded.push(FragFeature::from_core(f));
        }
    }
    let n_cuts = cut_tops.len();
    let spans: Vec<Vec<(usize, usize)>> = if circular {
        (0..n_cuts)
            .map(|i| {
                let a = cut_tops[i];
                let b = cut_tops[(i + 1) % n_cuts];
                if a < b {
                    vec![(a, b)]
                } else {
                    vec![(a, n), (0, b)]
                }
            })
            .collect()
    } else {
        let mut bounds = vec![0];
        bounds.extend(cut_tops.iter().copied());
        bounds.push(n);
        bounds.windows(2).map(|w| vec![(w[0], w[1])]).collect()
    };
    for f in expanded {
        if f.end == f.start {
            let slot = slot_for(f.start, cut_tops, circular, n_cuts);
            out.entry(slot).or_default().push(f);
            continue;
        }
        let cut_inside = cut_tops.iter().any(|&c| f.start < c && c < f.end);
        for (i, ivs) in spans.iter().enumerate() {
            if !ivs.iter().any(|&(lo, hi)| f.start.max(lo) < f.end.min(hi)) {
                continue;
            }
            let starts_here = ivs.iter().any(|&(lo, hi)| lo <= f.start && f.start < hi);
            let ends_here = f.end > 0 && ivs.iter().any(|&(lo, hi)| lo < f.end && f.end <= hi);
            let mut piece = f.clone();
            piece.split = if starts_here && ends_here {
                if cut_inside {
                    Some("whole".into())
                } else {
                    None
                }
            } else if starts_here {
                Some("head".into())
            } else if ends_here {
                Some("tail".into())
            } else {
                Some("mid".into())
            };
            out.entry(i).or_default().push(piece);
        }
    }
    out
}

fn slot_for(bp: usize, cut_tops: &[usize], circular: bool, n_cuts: usize) -> usize {
    if circular {
        for i in 0..n_cuts {
            let a = cut_tops[i];
            let b = cut_tops[(i + 1) % n_cuts];
            if a < b {
                if a <= bp && bp < b {
                    return i;
                }
            } else if bp >= a || bp < b {
                return i;
            }
        }
        return 0;
    }
    for (i, &c) in cut_tops.iter().enumerate() {
        if bp < c {
            return i;
        }
    }
    n_cuts
}

/// Mark halves split by a cloning cut as disrupted (label only).
pub fn label_disrupted_split_features(features: &mut [FragFeature], enzymes: &[&str]) {
    let enz: Vec<&str> = {
        let mut v: Vec<&str> = enzymes.iter().copied().filter(|e| !e.is_empty()).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let where_ = if enz.is_empty() {
        String::new()
    } else {
        format!(" ({} cut site inside it)", enz.join("/"))
    };
    for f in features {
        let Some(split) = f.split.as_deref() else {
            continue;
        };
        if !matches!(split, "head" | "tail" | "whole" | "mid") {
            continue;
        }
        if !f.label.contains("(disrupted)") {
            if f.label.is_empty() {
                f.label = format!(
                    "{} (disrupted)",
                    if f.kind.is_empty() {
                        "feature"
                    } else {
                        &f.kind
                    }
                );
            } else {
                f.label = format!("{} (disrupted)", f.label);
            }
        }
        let tag = format!("Disrupted by the cloning insertion{where_}.");
        if f.note.is_empty() {
            f.note = tag;
        } else if !f.note.contains("Disrupted by the cloning insertion") {
            f.note = format!("{}; {tag}", f.note);
        }
    }
}
