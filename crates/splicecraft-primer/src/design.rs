//! Generic / cloning / detection / Golden Braid designers (MIT, no primer3).

use splicecraft_bio::{CustomEnzyme, enzyme_lookup, rc};
use splicecraft_core::slice_circular;

use crate::error::PrimerError;
use crate::tm::{binding_max_len, pick_binding_region};

const MIN_BIND: usize = 18;
const DEFAULT_PAD: &str = "GCGC";

fn is_iupac_site(site: &str) -> bool {
    !site.is_empty()
        && site
            .chars()
            .all(|c| splicecraft_bio::iupac::iupac_base_set(c).is_some())
}

/// No-tail primers at the start / RC(end) of `[start, end)`.
#[derive(Clone, Debug, PartialEq)]
pub struct GenericPrimers {
    /// Forward oligo (binding only).
    pub fwd_seq: String,
    /// Reverse oligo (binding only).
    pub rev_seq: String,
    /// Wallace Tm, 1 dp.
    pub fwd_tm: f64,
    /// Wallace Tm, 1 dp.
    pub rev_tm: f64,
    /// Forward footprint `(start, end)` on the template.
    pub fwd_pos: (usize, usize),
    /// Reverse footprint `(start, end)` on the template.
    pub rev_pos: (usize, usize),
}

/// Cloning primers with pad + RE-site tails.
#[derive(Clone, Debug, PartialEq)]
pub struct CloningPrimers {
    /// Full forward oligo.
    pub fwd_full: String,
    /// Full reverse oligo.
    pub rev_full: String,
    /// Forward annealing region.
    pub fwd_binding: String,
    /// Reverse annealing region.
    pub rev_binding: String,
    /// Binding Tm, 1 dp.
    pub fwd_tm: f64,
    /// Binding Tm, 1 dp.
    pub rev_tm: f64,
    /// 5′ enzyme name.
    pub re_5prime: String,
    /// 3′ enzyme name.
    pub re_3prime: String,
    /// 5′ recognition site.
    pub site_5: String,
    /// 3′ recognition site.
    pub site_3: String,
    /// Insert that was primed.
    pub insert_seq: String,
    /// Forward footprint.
    pub fwd_pos: (usize, usize),
    /// Reverse footprint.
    pub rev_pos: (usize, usize),
}

/// Diagnostic pair inside an included region.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectionPrimers {
    /// Forward oligo.
    pub fwd_seq: String,
    /// Reverse oligo.
    pub rev_seq: String,
    /// Tm, 1 dp.
    pub fwd_tm: f64,
    /// Tm, 1 dp.
    pub rev_tm: f64,
    /// Forward footprint.
    pub fwd_pos: (usize, usize),
    /// Reverse footprint.
    pub rev_pos: (usize, usize),
    /// Amplicon length (fwd 5′ → rev 3′ inclusive of primers).
    pub product_size: usize,
}

fn map_pos(origin: usize, offset: usize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        (origin + offset) % total
    }
}

/// Design simple binding primers (no tails).
pub fn design_generic_primers(
    template_seq: &str,
    start: usize,
    end: usize,
    target_tm: f64,
) -> Result<GenericPrimers, PrimerError> {
    let seq = template_seq.to_ascii_uppercase();
    let total = seq.len();
    let insert = slice_circular(&seq, start, end);
    let wraps = end < start;
    if insert.len() < MIN_BIND {
        return Err(PrimerError::RegionTooShort);
    }
    let max_len = binding_max_len(0, MIN_BIND);
    let (fwd_bind, fwd_tm) = pick_binding_region(&insert, target_tm, MIN_BIND, max_len);
    let (rev_bind, rev_tm) = pick_binding_region(&rc(&insert), target_tm, MIN_BIND, max_len);
    let (fwd_pos, rev_pos) = if wraps {
        (
            (start, map_pos(start, fwd_bind.len(), total)),
            (map_pos(end + total - rev_bind.len(), 0, total), end),
        )
    } else {
        ((start, start + fwd_bind.len()), (end - rev_bind.len(), end))
    };
    Ok(GenericPrimers {
        fwd_seq: fwd_bind,
        rev_seq: rev_bind,
        fwd_tm: round1(fwd_tm),
        rev_tm: round1(rev_tm),
        fwd_pos,
        rev_pos,
    })
}

#[allow(clippy::too_many_arguments)]
/// Design cloning primers from raw recognition sites.
pub fn design_cloning_primers_raw(
    template_seq: &str,
    start: usize,
    end: usize,
    site_5: &str,
    site_3: &str,
    name_5: &str,
    name_3: &str,
    target_tm: f64,
    padding: &str,
) -> Result<CloningPrimers, PrimerError> {
    let site_5 = site_5.to_ascii_uppercase();
    let site_3 = site_3.to_ascii_uppercase();
    if !is_iupac_site(&site_5) || !is_iupac_site(&site_3) {
        return Err(PrimerError::InvalidSite);
    }
    let seq = template_seq.to_ascii_uppercase();
    let total = seq.len();
    let insert = slice_circular(&seq, start, end);
    let wraps = end < start;
    if insert.len() < MIN_BIND {
        return Err(PrimerError::RegionTooShort);
    }
    let pad = if padding.is_empty() {
        DEFAULT_PAD
    } else {
        padding
    };
    let fwd_max = binding_max_len(pad.len() + site_5.len(), MIN_BIND);
    let rev_max = binding_max_len(pad.len() + site_3.len(), MIN_BIND);
    let (fwd_bind, fwd_tm) = pick_binding_region(&insert, target_tm, MIN_BIND, fwd_max);
    let (rev_bind, rev_tm) = pick_binding_region(&rc(&insert), target_tm, MIN_BIND, rev_max);
    let fwd_full = format!("{pad}{site_5}{fwd_bind}");
    let rev_full = format!("{pad}{}{rev_bind}", rc(&site_3));
    let (fwd_pos, rev_pos) = if wraps {
        (
            (start, map_pos(start, fwd_bind.len(), total)),
            (map_pos(end + total - rev_bind.len(), 0, total), end),
        )
    } else {
        ((start, start + fwd_bind.len()), (end - rev_bind.len(), end))
    };
    Ok(CloningPrimers {
        fwd_full,
        rev_full,
        fwd_binding: fwd_bind,
        rev_binding: rev_bind,
        fwd_tm: round1(fwd_tm),
        rev_tm: round1(rev_tm),
        re_5prime: name_5.to_owned(),
        re_3prime: name_3.to_owned(),
        site_5,
        site_3,
        insert_seq: insert,
        fwd_pos,
        rev_pos,
    })
}

#[allow(clippy::too_many_arguments)]
/// Design cloning primers from enzyme names (NEB ∪ `custom`).
pub fn design_cloning_primers(
    template_seq: &str,
    start: usize,
    end: usize,
    re_5prime: &str,
    re_3prime: &str,
    target_tm: f64,
    padding: &str,
    custom: &[CustomEnzyme],
) -> Result<CloningPrimers, PrimerError> {
    let (site_5, _, _) = enzyme_lookup(re_5prime, custom)
        .ok_or_else(|| PrimerError::UnknownEnzyme(re_5prime.into()))?;
    let (site_3, _, _) = enzyme_lookup(re_3prime, custom)
        .ok_or_else(|| PrimerError::UnknownEnzyme(re_3prime.into()))?;
    design_cloning_primers_raw(
        template_seq,
        start,
        end,
        site_5,
        site_3,
        re_5prime,
        re_3prime,
        target_tm,
        padding,
    )
}

/// Golden Braid L0 tails: BsaI / BsaI cloning primers. Grammar overhangs land in stage 08.
pub fn design_golden_braid_primers(
    template_seq: &str,
    start: usize,
    end: usize,
    target_tm: f64,
    custom: &[CustomEnzyme],
) -> Result<CloningPrimers, PrimerError> {
    design_cloning_primers(
        template_seq,
        start,
        end,
        "BsaI",
        "BsaI",
        target_tm,
        DEFAULT_PAD,
        custom,
    )
}

/// Diagnostic primers that bind *inside* `[target_start, target_end)`.
pub fn design_detection_primers(
    template_seq: &str,
    target_start: usize,
    target_end: usize,
    product_min: usize,
    product_max: usize,
    target_tm: f64,
) -> Result<DetectionPrimers, PrimerError> {
    let seq = template_seq.to_ascii_uppercase();
    let total = seq.len();
    let region = slice_circular(&seq, target_start, target_end);
    let region_len = region.len();
    if region_len < 1 {
        return Err(PrimerError::EmptyRegion);
    }
    if region_len < product_min {
        return Err(PrimerError::RegionShorter {
            len: region_len,
            min: product_min,
        });
    }
    if product_max < MIN_BIND * 2 {
        return Err(PrimerError::NoPair);
    }
    let max_bind = binding_max_len(0, MIN_BIND).min(36);
    let mut best: Option<DetectionPrimers> = None;
    let mut best_score = f64::INFINITY;
    let lo = product_min.max(MIN_BIND * 2);
    let hi = product_max.min(region_len);
    if lo > hi {
        return Err(PrimerError::NoPair);
    }
    let mid = (lo + hi) / 2;
    let mut products = vec![mid, lo, hi];
    if hi > lo + 1 {
        products.push((lo + hi) / 2);
    }
    products.sort_unstable();
    products.dedup();
    for product in products {
        if product > region_len {
            continue;
        }
        let max_off = region_len - product;
        let offsets = unique_offsets(max_off);
        for offset in offsets {
            let window = &region[offset..offset + product];
            let (fwd, fwd_tm) = pick_binding_region(window, target_tm, MIN_BIND, max_bind);
            let (rev, rev_tm) = pick_binding_region(&rc(window), target_tm, MIN_BIND, max_bind);
            if fwd.len() < MIN_BIND || rev.len() < MIN_BIND {
                continue;
            }
            let score = (fwd_tm - target_tm).abs() + (rev_tm - target_tm).abs();
            let fwd_start = map_pos(target_start, offset, total);
            let fwd_end = map_pos(target_start, offset + fwd.len(), total);
            let rev_start = map_pos(target_start, offset + product - rev.len(), total);
            let rev_end = map_pos(target_start, offset + product, total);
            let cand = DetectionPrimers {
                fwd_seq: fwd,
                rev_seq: rev,
                fwd_tm: round1(fwd_tm),
                rev_tm: round1(rev_tm),
                fwd_pos: (fwd_start, fwd_end),
                rev_pos: (rev_start, rev_end),
                product_size: product,
            };
            if score < best_score {
                best_score = score;
                best = Some(cand);
            }
        }
    }
    best.ok_or(PrimerError::NoPair)
}

fn unique_offsets(max_off: usize) -> Vec<usize> {
    let mut v = vec![0, max_off / 2, max_off];
    v.sort_unstable();
    v.dedup();
    v
}

fn round1(tm: f64) -> f64 {
    (tm * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq_3k() -> String {
        let mut rng = 0xBEEFu64;
        (0..3000)
            .map(|_| {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                b"ACGT"[(rng >> 33) as usize % 4] as char
            })
            .collect()
    }

    #[test]
    fn generic_and_cloning_have_expected_tails() {
        let s = seq_3k();
        let g = design_generic_primers(&s, 200, 800, 60.0).unwrap();
        assert!(g.fwd_seq.len() >= MIN_BIND);
        assert!(g.rev_seq.len() >= MIN_BIND);
        let c = design_cloning_primers(&s, 200, 800, "EcoRI", "BamHI", 60.0, "GCGC", &[]).unwrap();
        assert!(c.fwd_full.starts_with("GCGC"));
        assert!(c.rev_full.starts_with("GCGC"));
        assert!(c.fwd_full.contains("GAATTC"));
        assert!(c.rev_full.contains("GGATCC"));
        let gb = design_golden_braid_primers(&s, 200, 800, 60.0, &[]).unwrap();
        assert!(gb.fwd_full.contains("GGTCTC"));
        assert!(gb.rev_full.contains("GAGACC"));
    }

    #[test]
    fn detection_inside_region() {
        let s = seq_3k();
        let d = design_detection_primers(&s, 200, 1200, 450, 550, 60.0).unwrap();
        assert!(d.fwd_pos.0 >= 200);
        assert!(d.fwd_pos.1 <= 1200 || d.fwd_pos.1 < d.fwd_pos.0);
        assert!(d.rev_pos.0 >= 200 || d.rev_pos.1 < d.rev_pos.0);
        assert!((450..=550).contains(&d.product_size));
        assert!(d.fwd_tm > 55.0 && d.fwd_tm < 80.0);
    }

    #[test]
    fn detection_errors() {
        let s = seq_3k();
        assert!(matches!(
            design_detection_primers(&s, 500, 500, 450, 550, 60.0),
            Err(PrimerError::EmptyRegion)
        ));
        assert!(matches!(
            design_detection_primers(&s, 500, 600, 450, 550, 60.0),
            Err(PrimerError::RegionShorter { .. })
        ));
        assert!(design_detection_primers(&s, 100, 800, 10, 20, 60.0).is_err());
    }

    #[test]
    fn unknown_enzyme_errors() {
        let s = seq_3k();
        assert!(matches!(
            design_cloning_primers(&s, 200, 800, "NoSuchI", "BamHI", 60.0, "GCGC", &[]),
            Err(PrimerError::UnknownEnzyme(_))
        ));
    }
}
