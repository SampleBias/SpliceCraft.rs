//! IUPAC, reverse-complement, restriction scanning, digest, and translation.
//!
//! Sacred invariants 1–6 and 8–9 (scan / rc / wrap). See `docs/invariants.md`.

#![forbid(unsafe_code)]

pub use splicecraft_core as core;
pub use splicecraft_util as util;

pub mod digest;
pub mod enzymes;
pub mod iupac;
pub mod scan;
pub mod translate;

pub use digest::{EnzymeCut, digest_with_enzymes, enzyme_cuts, fragments_from_cuts};
pub use enzymes::{
    EnzymeSpec, STAGE01_ENZYMES, enzyme, enzyme_color, feat_decorated_label, superscript_int,
};
pub use iupac::{BioError, iupac_pattern, pattern_cache_clear, pattern_cache_contains, rc};
pub use scan::{
    HitKind, RestrictionHit, ScanOptions, scan_restriction_sites, scan_restriction_sites_default,
};
pub use translate::{CODON_TABLE, codon_aa, codon_table_for, translate_cds};

/// Stage that implements this crate's real biology.
pub const IMPLEMENTATION_STAGE: u8 = 1;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-bio");
    }

    #[test]
    fn layer_below_is_wired() {
        assert_eq!(core::crate_name(), "splicecraft-core");
        assert_eq!(util::crate_name(), "splicecraft-util");
    }
}

#[cfg(test)]
mod scan_invariants {
    use super::*;

    fn resites<'a>(feats: &'a [RestrictionHit], enzyme: Option<&str>) -> Vec<&'a RestrictionHit> {
        feats
            .iter()
            .filter(|f| f.is_resite())
            .filter(|f| enzyme.is_none_or(|e| f.label == e))
            .collect()
    }

    fn recuts<'a>(feats: &'a [RestrictionHit], enzyme: Option<&str>) -> Vec<&'a RestrictionHit> {
        feats
            .iter()
            .filter(|f| f.is_recut())
            .filter(|f| enzyme.is_none_or(|e| f.label == e))
            .collect()
    }

    fn scan(seq: &str, min_len: usize, unique: bool, circular: bool) -> Vec<RestrictionHit> {
        scan_restriction_sites(
            seq,
            &ScanOptions {
                min_recognition_len: min_len,
                unique_only: unique,
                circular,
                allowed_enzymes: None,
            },
        )
    }

    fn scan_allowed(seq: &str, names: &[&str], circular: bool) -> Vec<RestrictionHit> {
        scan_restriction_sites(
            seq,
            &ScanOptions {
                min_recognition_len: 6,
                unique_only: false,
                circular,
                allowed_enzymes: Some(names.iter().map(|s| (*s).to_owned()).collect()),
            },
        )
    }

    #[test]
    fn inv01_ecori_single_site_not_double_counted() {
        let seq = format!("AAA{}AAA", "GAATTC");
        let feats = scan(&seq, 6, true, true);
        let eco = resites(&feats, Some("EcoRI"));
        assert_eq!(eco.len(), 1);
        assert_eq!(eco[0].start, 3);
        assert_eq!(eco[0].end, 9);
        assert_eq!(eco[0].strand, 1);
    }

    #[test]
    fn inv01_palindrome_one_recut() {
        let seq = format!("AAA{}AAA", "GAATTC");
        let feats = scan(&seq, 6, true, true);
        let cuts = recuts(&feats, Some("EcoRI"));
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].start, 4);
    }

    #[test]
    fn inv01_three_ecori_sites() {
        let seq = format!("AA{}AAA{}AAA{}AA", "GAATTC", "GAATTC", "GAATTC");
        let feats = scan(&seq, 6, false, true);
        assert_eq!(resites(&feats, Some("EcoRI")).len(), 3);
    }

    #[test]
    fn inv01_cut_count_badge() {
        let two = format!(
            "{}{}{}{}{}",
            "ACGT".repeat(5),
            "GAATTC",
            "ACGT".repeat(12),
            "GAATTC",
            "ACGT".repeat(5)
        );
        let f2 = scan_allowed(&two, &["EcoRI"], false);
        let res2: Vec<_> = f2
            .iter()
            .filter(|f| f.is_resite() && f.label == "EcoRI")
            .collect();
        assert!(!res2.is_empty());
        assert!(res2.iter().all(|f| f.cut_count == Some(2)));
        assert_eq!(
            feat_decorated_label(&res2[0].label, res2[0].cut_count),
            "EcoRI²"
        );
        let one = format!("{}{}{}", "ACGT".repeat(5), "GAATTC", "ACGT".repeat(17));
        let f1 = scan_allowed(&one, &["EcoRI"], false);
        let res1: Vec<_> = f1
            .iter()
            .filter(|f| f.is_resite() && f.label == "EcoRI")
            .collect();
        assert!(res1.iter().all(|f| f.cut_count.is_none()));
        assert_eq!(
            feat_decorated_label(&res1[0].label, res1[0].cut_count),
            "EcoRI"
        );
    }

    #[test]
    fn inv02_bsai_forward_and_reverse_coords() {
        let seq = format!("{}{}{}", "AAAAAA", "GGTCTC", "NNNNNNNNNN");
        let feats = scan(&seq, 6, true, true);
        let sites = resites(&feats, Some("BsaI"));
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].strand, 1);
        assert_eq!(sites[0].start, 6);
        assert_eq!(sites[0].end, 12);
        let cuts = recuts(&feats, Some("BsaI"));
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].start, 13);

        let seq = format!("{}{}{}", "AAAAAAAAA", "GAGACC", "AAAAAAAAA");
        let feats = scan(&seq, 6, true, true);
        let sites = resites(&feats, Some("BsaI"));
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].strand, -1);
        assert_eq!(sites[0].start, 9);
        assert_eq!(sites[0].end, 15);

        let seq = format!("{}{}{}", "AA", "GAGACC", "AAAAAAAAAAAAAAAAAAAA");
        let feats = scan(&seq, 6, true, true);
        let r = resites(&feats, Some("BsaI"))[0];
        assert_eq!(r.strand, -1);
        assert_eq!(r.start, 2);
        assert_ne!(r.start, seq.len() - 2 - 6);
    }

    #[test]
    fn inv02_bsai_reverse_recut_off_by_one() {
        let seq = format!("{}{}{}", "A".repeat(10), "GAGACC", "A".repeat(10));
        let feats = scan(&seq, 6, false, false);
        let cuts: Vec<_> = feats
            .iter()
            .filter(|f| f.label == "BsaI" && f.is_recut())
            .collect();
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].strand, -1);
        assert_eq!(cuts[0].start, 9);
        let sites: Vec<_> = feats
            .iter()
            .filter(|f| f.label == "BsaI" && f.is_resite())
            .collect();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].ext_cut_bp, Some(5));
    }

    #[test]
    fn unique_only_and_min_len_filters() {
        let seq = format!("AA{}AAA{}AA", "GAATTC", "GAATTC");
        let unique = scan(&seq, 6, true, true);
        let all = scan(&seq, 6, false, true);
        assert!(resites(&unique, Some("EcoRI")).is_empty());
        assert_eq!(resites(&all, Some("EcoRI")).len(), 2);

        let seq = format!("AAA{}AAA", "GGCC");
        let feats6 = scan(&seq, 6, false, true);
        assert!(resites(&feats6, Some("HaeIII")).is_empty());
        let feats4 = scan(&seq, 4, false, true);
        assert!(!resites(&feats4, Some("HaeIII")).is_empty());
    }

    #[test]
    fn empty_and_short_sequences() {
        assert!(scan("", 6, true, true).is_empty());
        assert!(scan("AAAA", 6, true, true).is_empty());
        for short in ["", "A", "GAATT", "GAATTC", "GAATTCA"] {
            let _ = scan(short, 6, false, true);
        }
    }

    #[test]
    fn degenerate_bsteii_all_n() {
        let seq = format!(
            "AA{}AA{}AA{}AA{}AA",
            "GGTAACC", "GGTCACC", "GGTGACC", "GGTTACC"
        );
        let feats = scan(&seq, 6, false, true);
        assert_eq!(resites(&feats, Some("BstEII")).len(), 4);
    }

    #[test]
    fn inv06_circular_wrap_ecori() {
        let seq = format!("{}{}{}", "TTC", "ACGTACGTACGTACGTACGT", "GAA");
        let feats = scan(&seq, 6, true, true);
        let eco = resites(&feats, Some("EcoRI"));
        assert_eq!(eco.len(), 1);
        assert_eq!(eco[0].start, seq.len() - 3);
        assert_eq!(eco[0].end, seq.len());
        let heads: Vec<_> = feats
            .iter()
            .filter(|f| f.is_resite() && f.color == eco[0].color && f.start == 0)
            .collect();
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].end, 3);
        let cuts = recuts(&feats, Some("EcoRI"));
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].start, seq.len() - 2);

        let linear = scan(&seq, 6, true, false);
        assert!(resites(&linear, Some("EcoRI")).is_empty());
    }

    #[test]
    fn inv06_wrap_type_iis_ext_and_rec_bounds() {
        let n = 30;
        let seq = format!("{}{}{}", "CTC", "X".repeat(n - 6), "GGT");
        let feats = scan(&seq, 6, true, true);
        let sites: Vec<_> = feats
            .iter()
            .filter(|f| f.label == "BsaI" && f.is_resite())
            .collect();
        assert_eq!(sites.len(), 1);
        let tail = sites[0];
        assert_eq!(tail.start, 27);
        assert_eq!(tail.end, 30);
        assert_eq!(tail.ext_cut_bp, Some(4));
        assert_eq!(tail.rec_start, Some(27));
        assert_eq!(tail.rec_end, Some(3));
        assert!(tail.rec_end.unwrap() < tail.rec_start.unwrap());
        let heads: Vec<_> = feats
            .iter()
            .filter(|f| {
                f.is_resite() && f.label.is_empty() && f.start == 0 && f.color == tail.color
            })
            .collect();
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].ext_cut_bp, Some(4));
        assert_eq!(heads[0].rec_start, Some(27));
        assert_eq!(heads[0].rec_end, Some(3));
    }

    #[test]
    fn inv06_non_wrap_rec_bounds_equal_span() {
        let seq = format!("{}{}{}", "AAAA", "GGTCTC", "A".repeat(20));
        let feats = scan(&seq, 6, true, true);
        let r = resites(&feats, Some("BsaI"))[0];
        assert_eq!(r.rec_start, Some(r.start));
        assert_eq!(r.rec_end, Some(r.end));
        assert!(r.rec_end.unwrap() >= r.rec_start.unwrap());
    }

    #[test]
    fn inv06_wrap_reverse_bsai() {
        let n = 30;
        let seq = format!("{}{}{}", "ACC", "X".repeat(n - 6), "GAG");
        let feats = scan(&seq, 6, true, true);
        let sites = resites(&feats, Some("BsaI"));
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].strand, -1);
        assert_eq!(sites[0].start, 27);
        assert_eq!(sites[0].end, 30);
    }

    #[test]
    fn both_strands_emit_one_recut_each() {
        let seq = format!("{}AAAA{}{}", "GGTCTC", "GAGACC", "A".repeat(13));
        let feats = scan(&seq, 6, false, false);
        let cuts: Vec<_> = feats
            .iter()
            .filter(|f| f.label == "BsaI" && f.is_recut())
            .collect();
        assert_eq!(cuts.iter().filter(|c| c.strand == 1).count(), 1);
        assert_eq!(cuts.iter().filter(|c| c.strand == -1).count(), 1);
    }

    #[test]
    fn wrap_plus_linear_fails_unique() {
        let seq = format!("{}{}{}{}", "TTC", "ACGT", "GAATTC", "ACGTACGTGAA");
        let unique = scan(&seq, 6, true, true);
        let all = scan(&seq, 6, false, true);
        assert!(resites(&unique, Some("EcoRI")).is_empty());
        assert_eq!(resites(&all, Some("EcoRI")).len(), 2);
    }

    #[test]
    fn no_duplicate_at_wrap_boundary() {
        let seq = format!("{}{}", "AAAAAAAA", "GAATTC");
        let feats = scan(&seq, 6, true, true);
        let eco = resites(&feats, Some("EcoRI"));
        assert_eq!(eco.len(), 1);
        assert_eq!(eco[0].start, 8);
        assert_eq!(eco[0].end, 14);
    }

    #[test]
    fn linear_in_body_still_found() {
        let seq = format!("AAA{}AAAA", "GAATTC");
        let feats = scan(&seq, 6, true, false);
        let eco = resites(&feats, Some("EcoRI"));
        assert_eq!(eco.len(), 1);
        assert_eq!(eco[0].start, 3);
        assert_eq!(eco[0].end, 9);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    const IUPAC: &[u8] = b"ACGTRYWSMKBDHVN";

    proptest! {
        #[test]
        fn inv03_rc_is_involutive(s in prop::collection::vec(prop::sample::select(IUPAC), 0..80)) {
            let seq = String::from_utf8(s).unwrap();
            prop_assert_eq!(rc(&rc(&seq)), seq);
        }

        #[test]
        fn inv01_planted_ecori_counted_once(prefix in 0_usize..20, suffix in 0_usize..20) {
            let seq = format!("{}GAATTC{}", "C".repeat(prefix), "C".repeat(suffix));
            let feats = scan_restriction_sites(
                &seq,
                &ScanOptions {
                    min_recognition_len: 6,
                    unique_only: true,
                    circular: false,
                    allowed_enzymes: Some(vec!["EcoRI".into()]),
                },
            );
            let labeled = feats.iter().filter(|h| h.is_resite() && h.label == "EcoRI").count();
            prop_assert_eq!(labeled, 1);
        }
    }
}
