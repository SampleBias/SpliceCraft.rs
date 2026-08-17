//! Traditional, Gibson, Golden Braid, and MoClo construction simulation.
//!
//! Stage 08. Digest + ligate is the product ([INV-127]). See
//! `docs/stages/08-cloning.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_codon as codon;
pub use splicecraft_core as core;
pub use splicecraft_persist as persist;
pub use splicecraft_primer as primer;

mod domesticator;
mod error;
mod fragment;
mod gibson;
mod goldengate;
mod grammar;
mod history;
mod parts;
mod product;
mod synfrag;
mod traditional;

pub use domesticator::{
    DomesticationPrimers, design_gb_primers, find_forbidden_hits, fuse_overhang_body,
    simulate_primed_amplicon,
};
pub use error::CloneError;
pub use fragment::{
    ClosedProduct, FragEnd, FragFeature, Fragment, close_circular, digest_to_fragments,
    ends_compatible, enzyme_is_type_iis, excise_fragment_pair, label_disrupted_split_features,
    ligate_fragments, make_synthetic_fragment, rc_fragment,
};
pub use gibson::{
    GIBSON_MAX_OVERLAP_BP, GIBSON_MIN_OVERLAP_BP, GibsonFragment, GibsonOverlap, GibsonResult,
    design_homology_arms, gibson_overlap_len, linearize_at, simulate_gibson_assembly,
};
pub use goldengate::{
    GoldenGateResult, assemble_parts, gg_greedy_chain, gg_released_bodies, simulate_golden_gate,
};
pub use grammar::{
    GB_L0_ENZYME_NAME, GB_L0_ENZYME_SITE, GB_PAD, GB_SPACER, Grammar, GrammarPosition,
    GrammarStore, builtin_grammars, gb_l0, moclo_plant,
};
pub use history::HistoryNode;
pub use parts::{ClassifiedPart, PartRecord, PartsBinStore, classify_part_from_plasmid};
pub use product::{carry_parent_features, product_record, stamp_history};
pub use synfrag::{
    ClosedClone, L0Part, SynFragment, build_synthesis_l0_fragment,
    clone_syn_fragment_into_entry_vector, l0_part_from_syn_fragment, pupd2_backbone_stub,
    released_insert_from_fragment, stub_entry_vector,
};
pub use traditional::{
    OrientationProduct, TraditionalResult, simulate_traditional_cloning, traditional_closed,
};

/// Stage that implements this crate's real assemblers.
pub const IMPLEMENTATION_STAGE: u8 = 8;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_persist::{DataLayout, authorize_writes_for_sandbox};

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-clone");
    }

    #[test]
    fn layer_below_is_wired() {
        assert_eq!(bio::crate_name(), "splicecraft-bio");
        assert_eq!(primer::crate_name(), "splicecraft-primer");
        assert_eq!(persist::crate_name(), "splicecraft-persist");
    }

    #[test]
    fn ligate_two_sticky_fragments_closes_expected_circle() {
        let a = make_synthetic_fragment("AAA", "EcoRV", "EcoRI", "a", Vec::new()).unwrap();
        let b = make_synthetic_fragment("CCC", "EcoRI", "EcoRV", "b", Vec::new()).unwrap();
        let merged = ligate_fragments(&a, &b).expect("EcoRI/EcoRI should ligate");
        assert_eq!(merged.top_seq, "AAACCC");
        assert_eq!(merged.left, a.left);
        assert_eq!(merged.right, b.right);
        let closed = close_circular(&merged).expect("EcoRV/EcoRV circle");
        assert!(closed.circular);
        assert_eq!(closed.top_seq, "AAACCC");
        let rec = product_record(
            "pLigate",
            &closed.top_seq,
            true,
            &closed.features,
            &HistoryNode::new(
                "ligateFwd",
                "pLigate",
                6,
                true,
                vec!["a".into(), "b".into()],
                "EcoRI",
            ),
        );
        assert_eq!(rec.sequence, "AAACCC");
        assert!(rec.circular);
        assert!(rec.comments.iter().any(|c| c.contains("ligateFwd")));
        assert!(
            !rec.comments
                .iter()
                .any(|c| c.contains("AAA") || c.contains("CCC"))
        );
    }

    #[test]
    fn incompatible_kinds_and_linear_edges_do_not_ligate() {
        let a = make_synthetic_fragment("AAA", "EcoRV", "EcoRI", "", Vec::new()).unwrap();
        let fake = Fragment {
            top_seq: "CCC".into(),
            left: FragEnd {
                overhang_seq: "AATT".into(),
                kind: "3'".into(),
                enzyme: "fake".into(),
            },
            right: FragEnd::linear(),
            features: Vec::new(),
            source_label: String::new(),
        };
        assert!(ligate_fragments(&a, &fake).is_none());
        let linear = Fragment {
            top_seq: "CCC".into(),
            left: FragEnd::linear(),
            right: FragEnd::linear(),
            features: Vec::new(),
            source_label: String::new(),
        };
        assert!(ligate_fragments(&a, &linear).is_none());
        let mismatch = make_synthetic_fragment("AAA", "EcoRI", "BamHI", "", Vec::new()).unwrap();
        assert!(close_circular(&mismatch).is_none());
    }

    #[test]
    fn type_iis_synthetic_stamp_is_refused() {
        let err = make_synthetic_fragment("AAA", "BsaI", "EcoRI", "", Vec::new()).unwrap_err();
        assert!(matches!(err, CloneError::TypeIisSynthetic(_)));
        assert!(enzyme_is_type_iis("BsaI"));
        assert!(enzyme_is_type_iis("Esp3I"));
        assert!(!enzyme_is_type_iis("EcoRI"));
    }

    #[test]
    fn traditional_directional_only_forward() {
        let insert = make_synthetic_fragment(
            "GAGCATGAAACGGCCAAGTAA",
            "EcoRI",
            "BamHI",
            "insert",
            Vec::new(),
        )
        .unwrap();
        let vector = Fragment {
            top_seq: "TGGCCCC".repeat(10),
            left: FragEnd {
                overhang_seq: "GATC".into(),
                kind: "5'".into(),
                enzyme: "BamHI".into(),
            },
            right: FragEnd {
                overhang_seq: "AATT".into(),
                kind: "5'".into(),
                enzyme: "EcoRI".into(),
            },
            features: Vec::new(),
            source_label: "vector".into(),
        };
        let result = simulate_traditional_cloning(&insert, &vector);
        assert!(result.forward.compatible);
        assert!(!result.reverse.compatible);
        assert!(result.warnings.iter().any(|w| w.contains("Directional")));
        assert!(result.errors.is_empty());
        assert!(traditional_closed(&insert, &vector, false).is_some());
        assert!(traditional_closed(&insert, &vector, true).is_none());
    }

    #[test]
    fn traditional_neither_orientation_errors() {
        let insert = make_synthetic_fragment("AAA", "EcoRI", "EcoRI", "", Vec::new()).unwrap();
        let vector = Fragment {
            top_seq: "CCC".into(),
            left: FragEnd {
                overhang_seq: "GATC".into(),
                kind: "5'".into(),
                enzyme: "BamHI".into(),
            },
            right: FragEnd {
                overhang_seq: "GATC".into(),
                kind: "5'".into(),
                enzyme: "BamHI".into(),
            },
            features: Vec::new(),
            source_label: "vector".into(),
        };
        let result = simulate_traditional_cloning(&insert, &vector);
        assert!(!result.forward.compatible);
        assert!(!result.reverse.compatible);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Neither orientation"))
        );
    }

    #[test]
    fn gibson_wrap_junction_three_fragments() {
        let oh_a = "AAAAAAAAAAAAAAAAAAAA";
        let oh_b = "CCCCCCCCCCCCCCCCCCCC";
        let oh_c = "GGGGGGGGGGGGGGGGGGGG";
        let f1 = GibsonFragment::new("F1", format!("{oh_c}TTTTTTTTTT{oh_a}"));
        let f2 = GibsonFragment::new("F2", format!("{oh_a}TTTTTTTTTT{oh_b}"));
        let f3 = GibsonFragment::new("F3", format!("{oh_b}TTTTTTTTTT{oh_c}"));
        let r = simulate_gibson_assembly(&[f1, f2, f3], 15, true);
        assert!(r.success, "{:?}", r.errors);
        let expected = format!("{oh_c}TTTTTTTTTT{oh_a}TTTTTTTTTT{oh_b}TTTTTTTTTT");
        assert_eq!(r.product_seq, expected);
        assert_eq!(r.product_seq.len(), 90);
        assert_eq!(r.overlaps.len(), 3);
        assert!(r.overlaps.iter().all(|o| o.ok));
        assert!(r.overlaps.last().unwrap().is_wrap);
        assert!(!r.overlaps[0].is_wrap);
    }

    #[test]
    fn gibson_overlap_prefers_longest_and_respects_min() {
        let oh20 = "ACGTACGTACGTACGTACGT";
        assert_eq!(
            gibson_overlap_len(&format!("AAAAA{oh20}"), &format!("{oh20}TTTTT"), 15, 200),
            20
        );
        assert_eq!(
            gibson_overlap_len(
                "AAAA".repeat(10).as_str(),
                "CCCC".repeat(10).as_str(),
                15,
                200
            ),
            0
        );
        let short = "ACGTACGTAC";
        assert_eq!(
            gibson_overlap_len(&format!("GG{short}"), &format!("{short}TT"), 15, 200),
            0
        );
        assert_eq!(
            gibson_overlap_len(&format!("GG{short}"), &format!("{short}TT"), 10, 200),
            10
        );
    }

    #[test]
    fn homology_arms_are_idempotent() {
        let mut lane = vec![
            GibsonFragment::new("up", "AAAAACCCCCCCCCCCCCCCC"),
            GibsonFragment::new("down", "GGGGGGGGGG"),
        ];
        let (armed, already, skipped) = design_homology_arms(&mut lane, 15, false).unwrap();
        assert_eq!(armed, 1);
        assert_eq!(already, 0);
        assert!(skipped.is_empty());
        let first = lane[1].sequence.clone();
        let (armed2, already2, _) = design_homology_arms(&mut lane, 15, false).unwrap();
        assert_eq!(armed2, 1);
        assert_eq!(already2, 0);
        assert_eq!(lane[1].sequence, first);
    }

    #[test]
    fn domestication_tails_follow_pad_site_spacer_overhang() {
        let g = gb_l0();
        let template = "ATGC".repeat(80);
        let r = design_gb_primers(&template, 0, 200, "CDS", &g, 60.0, None).unwrap();
        let prefix = format!("{}{}{}{}", GB_PAD, GB_L0_ENZYME_SITE, GB_SPACER, r.oh5);
        assert!(r.fwd_full.starts_with(&prefix), "fwd {}", r.fwd_full);
        assert_eq!(
            &r.fwd_full[GB_PAD.len()..GB_PAD.len() + GB_L0_ENZYME_SITE.len()],
            GB_L0_ENZYME_SITE
        );
        assert_eq!(
            &r.fwd_full[GB_PAD.len() + GB_L0_ENZYME_SITE.len()
                ..GB_PAD.len() + GB_L0_ENZYME_SITE.len() + GB_SPACER.len()],
            GB_SPACER
        );
        assert_eq!(r.oh5, "AATG");
        assert_eq!(r.oh3, "GCTT");
        let rev_oh = bio::rc("GCTT");
        let rev_prefix = format!("{}{}{}{rev_oh}", GB_PAD, GB_L0_ENZYME_SITE, GB_SPACER);
        assert!(r.rev_full.starts_with(&rev_prefix), "rev {}", r.rev_full);
        assert_eq!(r.enzyme_pad, GB_PAD);
        assert_eq!(r.enzyme_site, GB_L0_ENZYME_SITE);
        assert_eq!(r.enzyme_spacer, GB_SPACER);
    }

    #[test]
    fn moclo_plant_tails_use_bsai_site() {
        let g = moclo_plant();
        let template = "ATGC".repeat(80);
        let r = design_gb_primers(&template, 0, 200, "Promoter", &g, 60.0, None).unwrap();
        assert!(r.fwd_full.contains("GGTCTC"));
        assert_eq!(r.oh5, "GGAG");
        assert_eq!(r.oh3, "AATG");
        let prefix = format!("{}{}{}{}", g.pad, g.site, g.spacer, r.oh5);
        assert!(r.fwd_full.starts_with(&prefix));
    }

    #[test]
    fn syn_frag_wrong_part_type_is_refused() {
        let g = gb_l0();
        let body = format!("ATG{}TAA", "GCTAGCTAGCTAGCATCGATCGGATCC".repeat(3));
        let cds = g.position_for_type("CDS").unwrap();
        let built = build_synthesis_l0_fragment(&body, &cds.oh5, &cds.oh3, &g, "CDS", None);
        let vec = stub_entry_vector(&g, &built.entry_oh5, &built.entry_oh3);
        let err =
            l0_part_from_syn_fragment(&built.fragment, &vec, &g, "Promoter", "nope", &[], &[])
                .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("different part type") || msg.contains("GGAG"),
            "{msg}"
        );
    }

    #[test]
    fn syn_frag_matching_cds_files_body_without_overhangs() {
        let g = gb_l0();
        let body = format!("ATG{}TAA", "GCTAGCTAGCTAGCATCGATCGGATCC".repeat(3));
        let cds = g.position_for_type("CDS").unwrap();
        let built = build_synthesis_l0_fragment(&body, &cds.oh5, &cds.oh3, &g, "CDS", None);
        let vec = stub_entry_vector(&g, &built.entry_oh5, &built.entry_oh3);
        let part = l0_part_from_syn_fragment(&built.fragment, &vec, &g, "CDS", "MyCDS", &[], &[])
            .expect("matching CDS should file");
        assert_eq!(part.oh5, "AATG");
        assert_eq!(part.oh3, "GCTT");
        assert!(!part.sequence.starts_with("AATG"));
        assert!(!part.sequence.ends_with("GCTT"));
        assert!(!part.cloned_seq.is_empty());
        assert!(part.cloned_seq.len() > part.sequence.len());
        let mut bin = PartsBinStore::default();
        bin.file(PartRecord::from_l0(&part)).unwrap();
        assert_eq!(bin.for_grammar("gb_l0").len(), 1);
    }

    #[test]
    fn syn_frag_plain_sequence_is_refused() {
        let err = released_insert_from_fragment("ATGCATGCATGC", "Esp3I").unwrap_err();
        assert!(err.to_string().contains("plain fragment"), "{err}");
    }

    #[test]
    fn empty_part_cannot_be_filed() {
        let mut bin = PartsBinStore::default();
        let err = bin
            .file(PartRecord {
                name: "empty".into(),
                type_name: "CDS".into(),
                oh5: "AATG".into(),
                oh3: "GCTT".into(),
                sequence: String::new(),
                grammar: "gb_l0".into(),
                ..PartRecord::default()
            })
            .unwrap_err();
        assert!(err.to_string().contains("cannot assemble"));
    }

    #[test]
    fn custom_grammar_round_trips_through_persist_chokepoint() {
        let tmp = tempfile::tempdir().unwrap();
        authorize_writes_for_sandbox(tmp.path()).unwrap();
        let layout = DataLayout::from_xdg_home(tmp.path()).unwrap();
        assert!(layout.root.starts_with(tmp.path()));
        let mut store = GrammarStore::load(&layout);
        let mut g = moclo_plant();
        g.id = "lab_l0".into();
        g.name = "Lab L0".into();
        g.editable = true;
        store.upsert_custom(g).unwrap();
        store.persist(&layout).unwrap();
        let again = GrammarStore::load(&layout);
        assert!(again.custom.iter().any(|g| g.id == "lab_l0"));
        assert!(again.get("gb_l0").is_some());
        assert!(again.get("moclo_plant").is_some());
    }

    #[test]
    fn rc_fragment_swaps_synthetic_ends() {
        let frag = make_synthetic_fragment("ATGAAACG", "EcoRI", "BamHI", "", Vec::new()).unwrap();
        let flipped = rc_fragment(&frag);
        assert_eq!(flipped.top_seq, "CGTTTCAT");
        assert_eq!(flipped.left.enzyme, "BamHI");
        assert_eq!(flipped.right.enzyme, "EcoRI");
        assert_eq!(flipped.left.overhang_seq, "GATC");
        assert_eq!(flipped.right.overhang_seq, "AATT");
    }

    #[test]
    fn linearize_at_rotates() {
        assert_eq!(linearize_at("ABCDEF", 2), "CDEFAB");
    }
}
