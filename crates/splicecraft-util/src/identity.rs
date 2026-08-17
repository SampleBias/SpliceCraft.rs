//! Honest identity-percentage formatting. A sub-100% value never reads as 100%.

/// Format an alignment identity so a true sub-100% value never renders as `"100%"`.
///
/// Escalates decimal places (from `decimals` up to 4) until the shown number is
/// strictly `< 100`. A genuine `>= 100` value is the clean `"100%"`. Values so
/// close to 100 that even 4 dp round up become `"<100%"`.
#[must_use]
pub fn format_identity_pct(pct: Option<f64>, decimals: i32) -> String {
    let Some(v) = pct else {
        return "—".into();
    };
    if !v.is_finite() {
        return "—".into();
    }
    if v >= 100.0 {
        return "100%".into();
    }
    let d = decimals.max(0) as usize;
    for places in d..=4 {
        let s = format!("{v:.places$}");
        if s.parse::<f64>().ok().is_some_and(|shown| shown < 100.0) {
            return format!("{s}%");
        }
    }
    "<100%".into()
}

/// Colour tier for an identity cell. Strict `>= 100.0` is the only light-blue.
#[must_use]
pub fn identity_pct_color(pct: Option<f64>) -> &'static str {
    let Some(v) = pct else {
        return "white";
    };
    if !v.is_finite() {
        return "white";
    }
    if v >= 100.0 {
        "bright_cyan"
    } else if v >= 90.0 {
        "green"
    } else if v >= 80.0 {
        "yellow"
    } else if v >= 51.0 {
        "dark_orange"
    } else if v >= 11.0 {
        "red"
    } else {
        "grey50"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_100_is_clean() {
        assert_eq!(format_identity_pct(Some(100.0), 1), "100%");
        assert_eq!(
            format_identity_pct(Some(100.0 * 18094.0 / 18094.0), 1),
            "100%"
        );
    }

    #[test]
    fn one_bp_off_in_18kb_does_not_round_to_100() {
        let v = 100.0 * 18093.0 / 18094.0;
        let out = format_identity_pct(Some(v), 1);
        assert_ne!(out, "100%");
        assert_ne!(out, "100.0%");
        assert!(out.ends_with('%'));
        let n: f64 = out.trim_end_matches('%').parse().unwrap();
        assert!(n < 100.0, "{out}");
    }

    #[test]
    fn normal_value_keeps_one_decimal() {
        assert_eq!(format_identity_pct(Some(99.5), 1), "99.5%");
        assert_eq!(format_identity_pct(Some(52.1), 1), "52.1%");
        assert_eq!(format_identity_pct(Some(0.0), 1), "0.0%");
    }

    #[test]
    fn escalates_when_one_dp_rounds_up() {
        assert_eq!(format_identity_pct(Some(99.994), 1), "99.99%");
    }

    #[test]
    fn pathologically_close_uses_lt_marker() {
        assert_eq!(format_identity_pct(Some(99.999999), 1), "<100%");
    }

    #[test]
    fn above_100_clamps_to_clean_100() {
        assert_eq!(format_identity_pct(Some(100.5), 1), "100%");
    }

    #[test]
    fn none_is_dash() {
        assert_eq!(format_identity_pct(None, 1), "—");
        assert_eq!(format_identity_pct(Some(f64::NAN), 1), "—");
    }

    #[test]
    fn decimals_zero_still_avoids_false_100() {
        assert_eq!(format_identity_pct(Some(99.0), 0), "99%");
        assert_eq!(format_identity_pct(Some(99.6), 0), "99.6%");
    }

    #[test]
    fn color_and_number_never_disagree() {
        for v in [99.99447, 99.994, 99.9, 90.0, 51.0, 11.0, 0.5] {
            let num = format_identity_pct(Some(v), 1);
            let color = identity_pct_color(Some(v));
            if num == "100%" {
                assert_eq!(color, "bright_cyan", "{v} {num}");
            } else {
                assert_ne!(color, "bright_cyan", "{v} {num}");
            }
        }
        assert_eq!(identity_pct_color(Some(100.0)), "bright_cyan");
        assert_eq!(identity_pct_color(None), "white");
    }
}
