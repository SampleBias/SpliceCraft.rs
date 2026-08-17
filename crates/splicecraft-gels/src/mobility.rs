//! Helling–Goodman–Boyer agarose mobility (Sambrook 3e Table 5-1).

/// Fraction of the lane reserved beyond each resolution-window edge.
pub const GEL_EDGE_BAND: f64 = 0.03;

/// Configured agarose % → (bp_min, bp_max) resolution window.
pub const AGAROSE_RANGES: &[(f64, u32, u32)] = &[
    (0.5, 1000, 30_000),
    (0.7, 800, 12_000),
    (0.8, 800, 12_000),
    (1.0, 500, 10_000),
    (1.2, 400, 7_000),
    (1.5, 200, 4_000),
    (2.0, 100, 2_000),
    (2.5, 100, 1_500),
    (3.0, 50, 1_000),
    (4.0, 25, 500),
];

/// DNA conformation that shifts effective MW.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DnaForm {
    /// Linear dsDNA.
    #[default]
    Linear,
    /// Covalently closed supercoiled.
    Supercoiled,
    /// Open-circle / nicked.
    Nicked,
    /// Synonym for [`Self::Nicked`].
    Relaxed,
}

impl DnaForm {
    /// Parse a lane-form label. Unknown → linear.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "supercoiled" => Self::Supercoiled,
            "nicked" => Self::Nicked,
            "relaxed" => Self::Relaxed,
            _ => Self::Linear,
        }
    }

    /// Textbook midline MW multiplier (Lewis & Slater 1986).
    #[must_use]
    pub fn factor(self) -> f64 {
        match self {
            Self::Linear => 1.0,
            Self::Supercoiled => 0.7,
            Self::Nicked | Self::Relaxed => 1.4,
        }
    }

    /// Canonical wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Supercoiled => "supercoiled",
            Self::Nicked => "nicked",
            Self::Relaxed => "relaxed",
        }
    }
}

/// Snap `pct` to the nearest configured agarose percentage.
#[must_use]
pub fn snap_agarose(pct: f64) -> f64 {
    AGAROSE_RANGES
        .iter()
        .min_by(|a, b| {
            (a.0 - pct)
                .abs()
                .partial_cmp(&(b.0 - pct).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|r| r.0)
        .unwrap_or(1.0)
}

/// Relative mobility in `[0, 1]`: 0 = well, 1 = dye front.
///
/// In-window: linear in `-log10(eff_bp)`, compressed into
/// `[GEL_EDGE_BAND, 1 - GEL_EDGE_BAND]`. Out-of-window: damped
/// extrapolation that keeps size order without collapsing onto one row.
#[must_use]
pub fn agarose_mobility(bp: i64, gel_pct: f64, form: DnaForm) -> f64 {
    if bp <= 0 {
        return 1.0;
    }
    let eff = ((bp as f64) * form.factor()).round().max(1.0);
    let snapped = snap_agarose(gel_pct);
    let Some((_, bp_min, bp_max)) = AGAROSE_RANGES.iter().find(|r| (r.0 - snapped).abs() < 1e-9)
    else {
        return 1.0;
    };
    let log_lo = (*bp_min as f64).log10();
    let log_hi = (*bp_max as f64).log10();
    let log_x = eff.log10();
    let raw = (log_hi - log_x) / (log_hi - log_lo);
    if (0.0..=1.0).contains(&raw) {
        return GEL_EDGE_BAND + (1.0 - 2.0 * GEL_EDGE_BAND) * raw;
    }
    if raw > 1.0 {
        let excess = raw - 1.0;
        let damped = 1.0 - 0.5_f64.powf(excess);
        return (1.0 - GEL_EDGE_BAND) + GEL_EDGE_BAND * damped;
    }
    let deficit = -raw;
    let damped = 1.0 - 0.5_f64.powf(deficit);
    GEL_EDGE_BAND - GEL_EDGE_BAND * damped
}
