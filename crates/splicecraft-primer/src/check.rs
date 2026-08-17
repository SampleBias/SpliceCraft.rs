//! Primer-check confidence badges and in-silico PCR pairing.

use crate::binding::BindingSite;

/// Default max amplicon (upstream `_PCR_DEFAULT_MAX_AMPLICON`).
pub const PCR_DEFAULT_MAX_AMPLICON: usize = 10_000;
/// Hard cap on a single product length.
pub const PCR_AMPLICON_HARD_CAP: usize = 50_000;
/// Max products returned from one pairing pass.
pub const PCR_MAX_AMPLICONS: usize = 200;

/// Identity → (glyph, colour) for the primer-check table.
#[must_use]
pub fn primer_check_confidence(pct: Option<f64>) -> (&'static str, &'static str) {
    let Some(v) = pct else {
        return ("?", "white");
    };
    if v >= 99.999 {
        ("✓", "bright_cyan")
    } else if v >= 90.0 {
        ("✓", "green")
    } else if v >= 75.0 {
        ("⚠", "yellow")
    } else if v >= 60.0 {
        ("~", "dark_orange")
    } else {
        ("✗", "red")
    }
}

/// One predicted amplicon from a forward × reverse site pair.
#[derive(Clone, Debug, PartialEq)]
pub struct Amplicon {
    /// 5′ on the top strand.
    pub start: usize,
    /// Product length including both primers.
    pub length: usize,
    /// True when the product crosses the origin.
    pub wraps: bool,
    /// Forward-site identity.
    pub fwd_ident: f64,
    /// Reverse-site identity.
    pub rev_ident: f64,
    /// `min(fwd, rev)` identity.
    pub certainty: f64,
    /// Which input list played forward (`0` = sites_a).
    pub fwd_primer: usize,
    /// Which input list played reverse.
    pub rev_primer: usize,
    /// Reverse-primer 3′ end (canonical).
    pub rev_3p: usize,
}

/// Pair forward + reverse binding sites into amplicons.
#[must_use]
pub fn insilico_pcr_amplicons(
    sites_a: &[BindingSite],
    sites_b: &[BindingSite],
    total: usize,
    circular: bool,
    max_amplicon: usize,
) -> Vec<Amplicon> {
    if total == 0 {
        return Vec::new();
    }
    let mut fwd = Vec::new();
    let mut rev = Vec::new();
    for (idx, lst) in [(0usize, sites_a), (1, sites_b)] {
        for s in lst {
            if s.strand == 1 {
                fwd.push((s, idx));
            } else if s.strand == -1 {
                rev.push((s, idx));
            }
        }
    }
    let max_amp = max_amplicon.clamp(1, PCR_AMPLICON_HARD_CAP);
    let mut amps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (fs, fidx) in &fwd {
        let f_left = fs.foot_start;
        for (rs, ridx) in &rev {
            let r_left = rs.foot_start;
            let lr = rs.length;
            let length = if circular {
                ((r_left + total - f_left) % total) + lr
            } else {
                if r_left < f_left || r_left + lr > total {
                    continue;
                }
                (r_left + lr) - f_left
            };
            let lo = fs.length + lr;
            if length < lo || length > max_amp {
                continue;
            }
            let start = f_left % total;
            let end_canon = (f_left + length) % total;
            if !seen.insert((start, end_canon, length)) {
                continue;
            }
            amps.push(Amplicon {
                start,
                length,
                wraps: circular && (f_left + length) > total,
                fwd_ident: fs.ident_pct,
                rev_ident: rs.ident_pct,
                certainty: fs.ident_pct.min(rs.ident_pct),
                fwd_primer: *fidx,
                rev_primer: *ridx,
                rev_3p: r_left,
            });
            if amps.len() >= PCR_MAX_AMPLICONS {
                break;
            }
        }
        if amps.len() >= PCR_MAX_AMPLICONS {
            break;
        }
    }
    amps.sort_by(|a, b| {
        b.certainty
            .partial_cmp(&a.certainty)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.length.cmp(&b.length))
            .then_with(|| a.start.cmp(&b.start))
    });
    amps
}

/// Hits of one oligo on a named template (library primer-check).
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateHits {
    /// Plasmid / record name (never the sequence).
    pub name: String,
    /// Binding sites.
    pub sites: Vec<BindingSite>,
}

/// Scan `primer` against each `(name, seq, circular)` template.
pub fn check_primer_on_library(
    primer: &str,
    templates: &[(String, String, bool)],
) -> Result<Vec<TemplateHits>, crate::error::PrimerError> {
    let mut out = Vec::new();
    for (name, seq, circular) in templates {
        let sites = crate::binding::primer_binding_sites(
            primer,
            seq,
            *circular,
            crate::binding::PRIMER_CHECK_SEED_LEN,
            0.0,
        )?;
        if !sites.is_empty() {
            out.push(TemplateHits {
                name: name.clone(),
                sites,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{PRIMER_CHECK_SEED_LEN, primer_binding_sites};
    use splicecraft_bio::rc;

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
    fn confidence_tiers() {
        assert_eq!(primer_check_confidence(Some(100.0)), ("✓", "bright_cyan"));
        assert_eq!(primer_check_confidence(Some(90.0)), ("✓", "green"));
        assert_eq!(primer_check_confidence(Some(75.0)), ("⚠", "yellow"));
        assert_eq!(primer_check_confidence(Some(60.0)), ("~", "dark_orange"));
        assert_eq!(primer_check_confidence(Some(10.0)), ("✗", "red"));
        assert_eq!(primer_check_confidence(None), ("?", "white"));
    }

    #[test]
    fn two_oligos_yield_amplicon_length() {
        let t = template();
        let fwd = t[100..122].to_owned();
        let rev = rc(&t[400..422]);
        let s1 = primer_binding_sites(&fwd, &t, false, PRIMER_CHECK_SEED_LEN, 0.0).unwrap();
        let s2 = primer_binding_sites(&rev, &t, false, PRIMER_CHECK_SEED_LEN, 0.0).unwrap();
        let amps = insilico_pcr_amplicons(&s1, &s2, t.len(), false, 20_000);
        assert!(!amps.is_empty());
        assert_eq!(amps[0].start, 100);
        assert_eq!(amps[0].length, 322);
    }
}
