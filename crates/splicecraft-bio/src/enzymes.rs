//! Stage-01 enzyme catalog (palindromes + Type IIS). Full NEB list is stage 07.

/// `(site, fwd_cut, rev_cut)` — BioPython convention `(fst5, size + fst3)`.
pub type EnzymeSpec = (&'static str, i32, i32);

/// Built-in enzymes needed for sacred-scan tests and Type IIS cuts.
pub static STAGE01_ENZYMES: &[(&str, EnzymeSpec)] = &[
    ("EcoRI", ("GAATTC", 1, 5)),
    ("BamHI", ("GGATCC", 1, 5)),
    ("HindIII", ("AAGCTT", 1, 5)),
    ("BstEII", ("GGTNACC", 1, 6)),
    ("HaeIII", ("GGCC", 2, 2)),
    ("BsaI", ("GGTCTC", 7, 11)),
    ("Esp3I", ("CGTCTC", 7, 11)),
    ("BsmBI", ("CGTCTC", 7, 11)),
    ("BbsI", ("GAAGAC", 8, 12)),
];

/// Look up a catalog entry.
#[must_use]
pub fn enzyme(name: &str) -> Option<EnzymeSpec> {
    STAGE01_ENZYMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, spec)| *spec)
}

/// Stable overlay color per enzyme (wrap-piece matching uses color).
#[must_use]
pub fn enzyme_color(name: &str) -> &'static str {
    match name {
        "EcoRI" => "red",
        "BamHI" => "magenta",
        "HindIII" => "blue",
        "BstEII" => "cyan",
        "HaeIII" => "green",
        "BsaI" => "yellow",
        "Esp3I" => "orange",
        "BsmBI" => "pink",
        "BbsI" => "white",
        _ => "gray",
    }
}

/// Unicode superscript digits (`12` → `¹²`).
#[must_use]
pub fn superscript_int(n: i32) -> String {
    const MAP: &[char] = &['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    n.to_string()
        .chars()
        .map(|c| c.to_digit(10).map(|d| MAP[d as usize]).unwrap_or(c))
        .collect()
}

/// Render-only label: `EcoRI²` when `cut_count > 1`. Does not mutate the key.
#[must_use]
pub fn feat_decorated_label(label: &str, cut_count: Option<u32>) -> String {
    match cut_count {
        Some(n) if n > 1 => format!("{label}{}", superscript_int(n as i32)),
        _ => label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_enzymes_present() {
        for must in [
            "EcoRI", "BamHI", "HindIII", "BsaI", "BsmBI", "BbsI", "Esp3I",
        ] {
            assert!(enzyme(must).is_some(), "{must}");
        }
    }

    #[test]
    fn ecori_and_bsai_canonical() {
        let (site, fwd, _rev) = enzyme("EcoRI").unwrap();
        assert_eq!(site, "GAATTC");
        assert_eq!(fwd, 1);
        let (site, fwd, _rev) = enzyme("BsaI").unwrap();
        assert_eq!(site, "GGTCTC");
        assert!(fwd > site.len() as i32);
    }

    #[test]
    fn superscript_and_decorated_label() {
        assert_eq!(superscript_int(2), "²");
        assert_eq!(superscript_int(12), "¹²");
        assert_eq!(feat_decorated_label("EcoRI", Some(2)), "EcoRI²");
        assert_eq!(feat_decorated_label("EcoRI", None), "EcoRI");
    }
}
