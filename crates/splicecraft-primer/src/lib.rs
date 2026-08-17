//! Primer Tm, designers, primer-check, and IDT CSV.
//!
//! MIT path only: Wallace / 2+4 Tm. Do not take the GPL `primer3` crate as a
//! default dependency. See `docs/stages/07-enzymes-primers.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_codon as codon;
pub use splicecraft_core as core;
pub use splicecraft_persist as persist;

mod binding;
mod check;
mod csv;
mod design;
mod error;
mod library;
mod mutato;
mod scrub;
mod tm;

pub use binding::{
    BindingSite, PRIMER_CHECK_MAX_SITES, PRIMER_CHECK_SEED_LEN, PRIMER_REBIND_MIN,
    primer_binding_sites, rederive_primer_binding,
};
pub use check::{
    Amplicon, PCR_AMPLICON_HARD_CAP, PCR_DEFAULT_MAX_AMPLICON, PCR_MAX_AMPLICONS, TemplateHits,
    check_primer_on_library, insilico_pcr_amplicons, primer_check_confidence,
};
pub use csv::{
    IDT_DEFAULT_PURIFICATION, IDT_DEFAULT_SCALE, OrderFormat, export_idt_csv, export_primers_csv,
};
pub use design::{
    CloningPrimers, DetectionPrimers, GenericPrimers, design_cloning_primers,
    design_cloning_primers_raw, design_detection_primers, design_generic_primers,
    design_golden_braid_primers,
};
pub use error::PrimerError;
pub use library::{PrimerRecord, PrimerStatus, PrimerStore};
pub use mutato::{
    EdgeCase, InnerCandidate, InnerDesign, MUT_BSAI_FWD_TAIL, MUT_BSAI_REV_TAIL, MUT_MIN_SOE_FRAG,
    MutOligo, OuterPrimers, design_inner, design_mutagenesis, design_outer, extract_cds, mut_parse,
    mut_translate,
};
pub use scrub::{
    QcPrimers, SCRUB_DEFAULT_ENZYMES, SCRUB_PRIMER_FOOTPRINT, ScrubEdit, ScrubPlan, ScrubSite,
    circ_extract, cluster_edits, cluster_span, qc_primers, qc_verify, resolve_sites, scrub_design,
};
pub use tm::{PRIMER_MAX_OLIGO_LEN, binding_max_len, pick_binding_region, primer_tm, wallace_tm};

/// Stage that implements this crate's real designers.
pub const IMPLEMENTATION_STAGE: u8 = 9;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-primer");
    }

    #[test]
    fn default_features_remain_mit_without_primer3() {
        let primer_manifest = include_str!("../Cargo.toml");
        assert!(
            !primer_manifest.contains("primer3"),
            "splicecraft-primer must not depend on the GPL primer3 crate"
        );
        let workspace = include_str!("../../../Cargo.toml");
        assert!(
            !workspace.contains("primer3"),
            "workspace default deps must not include primer3"
        );
    }

    #[test]
    fn wallace_golden_file() {
        let raw = include_str!("../tests/data/wallace_tm.json");
        let rows: Vec<serde_json::Value> = serde_json::from_str(raw).expect("golden json");
        assert!(!rows.is_empty());
        for row in rows {
            let seq = row["seq"].as_str().unwrap_or("");
            let expected = row["tm"].as_f64();
            assert_eq!(primer_tm(seq), expected, "seq={seq:?}");
        }
    }

    const CDS_LONG: &str = concat!(
        "ATG",
        "GCTGAAGTTCAGGATAACCTGGCGAAAGTTCAGGAAGCGGTTGATACCCTGAAACGTGGT",
        "CTGGAAGCGGCGAAAGCGACCCTGGAAAAAGCGGGTGAAGATATCGCGAAAGCGGTTGAT",
        "GGTAAACGTAAAGGCGATCTGGAAAAACTGGCGGAAGCGCTGCAGAAAGTTGAAGCGGAT",
        "ATCGCGAAAGCGGTTGATGGTAAACGTAAAGGCGATCTGGAAAAACTGGCGGAAGCGCTG",
        "TAA",
    );

    #[test]
    fn mut_parse_goldens() {
        assert_eq!(mut_parse("W140F").unwrap(), ('W', 140, 'F'));
        assert_eq!(mut_parse("w140f").unwrap(), ('W', 140, 'F'));
        assert_eq!(mut_parse("W140*").unwrap(), ('W', 140, '*'));
        assert!(mut_parse("nope").is_err());
    }

    #[test]
    fn soe_mid_cds_carries_mismatch() {
        let inner = design_inner(CDS_LONG, 40, 'F', 'V', None).unwrap();
        assert_eq!(inner.mutation, "V40F");
        assert_eq!(inner.wt_codon, "GTT");
        assert_ne!(inner.mut_codon, "GTT");
        assert!(inner.edge_case.is_none());
        let best = &inner.candidates[0];
        assert_eq!(best.rev, bio::rc(&best.fwd));
        assert!(
            best.fwd.contains(&inner.mut_codon),
            "inner FWD must carry the mutant codon"
        );
    }

    #[test]
    fn near_end_uses_two_primer_path() {
        let start = design_inner(CDS_LONG, 3, 'F', 'E', None).unwrap();
        let ec = start.edge_case.expect("E3F should fold into FWD outer");
        assert!(ec.near_start);
        assert_eq!(ec.modified_outer.label, "modified_FWD_outer");
        assert!(ec.modified_outer.full.starts_with("CCCCGGTCTCAAATG"));
        let end = design_inner(CDS_LONG, 78, 'F', 'A', None).unwrap();
        let ec = end.edge_case.expect("A78F should fold into REV outer");
        assert!(ec.near_end);
        assert_eq!(ec.modified_outer.label, "modified_REV_outer");
        assert!(ec.modified_outer.full.starts_with("CCCCGGTCTCAAACG"));
    }

    #[test]
    fn outer_tails_are_bsai_overhangs() {
        let outer = design_outer(CDS_LONG).unwrap();
        assert!(outer.fwd.full.starts_with(MUT_BSAI_FWD_TAIL));
        assert!(outer.rev.full.starts_with(MUT_BSAI_REV_TAIL));
        assert_eq!(outer.fwd_anneal_start, 3);
        assert_eq!(outer.b3_overhang, "AATG");
    }

    #[test]
    fn scrub_does_not_spawn_bsai_while_killing_esp3i() {
        let seq = "AAAAGGTCTCAAAACGTCTCAAAAGGGGTT";
        let enzymes = ["BsaI", "Esp3I", "BbsI"];
        let plan = scrub_design(seq, &[], Some(&enzymes), true, None, &[]);
        assert!(plan.ok);
        assert_eq!(plan.sites_removed.len(), 2);
        let aug = format!(
            "{}{}",
            plan.cured_seq,
            &plan.cured_seq[..5.min(plan.cured_seq.len())]
        );
        for site in ["GGTCTC", "GAGACC", "CGTCTC", "GAGACG", "GAAGAC", "GTCTTC"] {
            assert!(
                !aug.contains(site),
                "residual {site} after scrub of {seq} -> {}",
                plan.cured_seq
            );
        }
    }

    #[test]
    fn codon_frequency_tiebreak_prefers_gga() {
        let cds = "ATGGGTCTCAAAGGGCCCTTTGACTAA";
        let feats = [core::Feature::new("CDS", 0, 27, 1, "orf")];
        let mut raw = codon::UsageTable::new();
        raw.insert("GGA", 'G', 90);
        raw.insert("GGC", 'G', 5);
        raw.insert("GGG", 'G', 5);
        raw.insert("GGT", 'G', 0);
        for c in ["CTA", "CTG", "CTC", "CTT", "TTA", "TTG"] {
            raw.insert(c, 'L', 1);
        }
        let plan = scrub_design(cds, &feats, Some(&["BsaI"]), true, Some(&raw), &[]);
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(plan.edits[0].pos, 5);
        assert_eq!(plan.edits[0].to, 'A');
        assert_eq!(&plan.cured_seq[3..6], "GGA");
        let plan2 = scrub_design(cds, &feats, Some(&["BsaI"]), true, None, &[]);
        assert_eq!(plan2.edits[0].to, 'C');
    }
}
