//! NEB-scale enzyme catalog plus the stage-01 sacred subset.
//!
//! `data/neb_enzymes.json` is transcribed recognition/cut data from the
//! upstream `_NEB_ENZYMES` table (Binomica-Labs/SpliceCraft
//! `splicecraft_dataaccess.py` on `master`). It is data, not Python.

use std::sync::OnceLock;

use serde::Deserialize;

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

/// One NEB catalog row (static after first parse).
#[derive(Clone, Debug, Deserialize)]
pub struct NebEnzyme {
    /// Display / lookup name (`EcoRI`).
    pub name: String,
    /// IUPAC recognition site.
    pub site: String,
    /// Forward cut offset from site start.
    pub fwd_cut: i32,
    /// Reverse cut offset from site start.
    pub rev_cut: i32,
}

/// User-defined enzyme merged into scans via [`crate::ScanOptions::extra_enzymes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomEnzyme {
    /// Unique name.
    pub name: String,
    /// IUPAC recognition site.
    pub site: String,
    /// Forward cut offset.
    pub fwd_cut: i32,
    /// Reverse cut offset.
    pub rev_cut: i32,
}

/// Parsed NEB catalog (201 entries). Lives for the process.
#[must_use]
pub fn neb_enzymes() -> &'static [NebEnzyme] {
    static CATALOG: OnceLock<Vec<NebEnzyme>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../data/neb_enzymes.json"))
            .expect("neb_enzymes.json must parse")
    })
}

/// Look up a built-in catalog entry (NEB, including HF aliases).
#[must_use]
pub fn enzyme(name: &str) -> Option<EnzymeSpec> {
    neb_enzymes()
        .iter()
        .find(|e| e.name == name)
        .map(|e| (e.site.as_str(), e.fwd_cut, e.rev_cut))
}

/// Look up NEB first, then `custom` (custom wins on a name clash).
#[must_use]
pub fn enzyme_lookup<'a>(name: &str, custom: &'a [CustomEnzyme]) -> Option<(&'a str, i32, i32)> {
    if let Some(c) = custom.iter().find(|e| e.name == name) {
        return Some((c.site.as_str(), c.fwd_cut, c.rev_cut));
    }
    enzyme(name)
}

/// Combined catalog: custom entries override a matching NEB name.
#[must_use]
pub fn all_enzymes(custom: &[CustomEnzyme]) -> Vec<(String, String, i32, i32)> {
    let mut out: Vec<(String, String, i32, i32)> = neb_enzymes()
        .iter()
        .map(|e| (e.name.clone(), e.site.clone(), e.fwd_cut, e.rev_cut))
        .collect();
    for c in custom {
        if let Some(slot) = out.iter_mut().find(|(n, _, _, _)| n == &c.name) {
            *slot = (c.name.clone(), c.site.clone(), c.fwd_cut, c.rev_cut);
        } else {
            out.push((c.name.clone(), c.site.clone(), c.fwd_cut, c.rev_cut));
        }
    }
    out
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
        _ => {
            const PALETTE: &[&str] = &[
                "red", "magenta", "blue", "cyan", "green", "yellow", "orange", "pink", "white",
                "gray",
            ];
            let h = name
                .bytes()
                .fold(0u64, |a, b| a.wrapping_mul(16_777_619) ^ u64::from(b));
            PALETTE[h as usize % PALETTE.len()]
        }
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
    fn neb_catalog_is_neb_scale() {
        assert!(
            neb_enzymes().len() >= 200,
            "expected 200+ NEB enzymes, got {}",
            neb_enzymes().len()
        );
    }

    #[test]
    fn common_enzymes_present() {
        for must in [
            "EcoRI", "BamHI", "HindIII", "BsaI", "BsmBI", "BbsI", "Esp3I",
        ] {
            assert!(enzyme(must).is_some(), "{must}");
        }
    }

    #[test]
    fn stage01_subset_matches_neb() {
        for (name, spec) in STAGE01_ENZYMES {
            assert_eq!(enzyme(name), Some(*spec), "{name}");
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
    fn custom_overrides_and_extends() {
        let extra = [CustomEnzyme {
            name: "TestUnique1".into(),
            site: "AAATTT".into(),
            fwd_cut: 3,
            rev_cut: 3,
        }];
        let combined = all_enzymes(&extra);
        assert!(
            combined
                .iter()
                .any(|(n, s, f, r)| { n == "TestUnique1" && s == "AAATTT" && *f == 3 && *r == 3 })
        );
        assert!(combined.iter().any(|(n, _, _, _)| n == "EcoRI"));
        let (site, fwd, rev) = enzyme_lookup("TestUnique1", &extra).unwrap();
        assert_eq!((site, fwd, rev), ("AAATTT", 3, 3));
    }

    #[test]
    fn superscript_and_decorated_label() {
        assert_eq!(superscript_int(2), "²");
        assert_eq!(superscript_int(12), "¹²");
        assert_eq!(feat_decorated_label("EcoRI", Some(2)), "EcoRI²");
        assert_eq!(feat_decorated_label("EcoRI", None), "EcoRI");
    }

    #[test]
    fn stage01_colors_stay_named() {
        assert_eq!(enzyme_color("EcoRI"), "red");
        assert_eq!(enzyme_color("BsaI"), "yellow");
        assert!(!enzyme_color("NotI").is_empty());
    }
}
