//! In-silico agarose gels (Helling–Goodman–Boyer mobility) and PCR.
//!
//! Stage 10. See `docs/stages/10-simulator-gels.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_clone as clone;
pub use splicecraft_core as core;
pub use splicecraft_persist as persist;
pub use splicecraft_primer as primer;

mod entry;
mod error;
mod ladders;
mod lanes;
mod mobility;
mod pcr;
mod render;
mod store;

pub use entry::{
    GEL_AGAROSE_MAX, GEL_AGAROSE_MIN, GEL_CHIP_COLOR, GEL_LANE_DETAIL_MAX_LEN,
    GEL_LANE_NAME_MAX_LEN, GEL_LANE_SOURCE_MAX_LEN, GEL_NAME_MAX_LEN, GEL_NOTES_MAX_LEN, GelEntry,
    GelLaneJson, extract_gel_refs, find_gel, gel_name_taken, new_gel_id, normalise_gel_entry,
    sanitize_gel_id,
};
pub use error::GelError;
pub use ladders::{GEL_LADDERS, ladder_bands, ladder_names};
pub use lanes::{
    GEL_LANES_MAX, GEL_UI_MAX_LANES, GelLane, append_pcr_gel_lane, gel_bands_for_lane,
};
pub use mobility::{AGAROSE_RANGES, DnaForm, GEL_EDGE_BAND, agarose_mobility, snap_agarose};
pub use pcr::{
    PCR_AMPLICON_HARD_CAP, PCR_DEFAULT_MAX_AMPLICON, PCR_MAX_AMPLICONS, PCR_MAX_PRIMER_HITS,
    PCR_MAX_PRIMER_LEN, PCR_MAX_TEMPLATE_BP, PCR_MIN_PRIMER_LEN, PCR_UI_DEFAULT_MAX_AMPLICON,
    PcrAmplicon, amplicon_to_record, simulate_pcr,
};
pub use render::{GEL_HEIGHT_DEFAULT, GelRenderOpts, render_gel_image};
pub use store::{GelStore, snapshot_gel};

/// Stage that implements this crate's real gel renderer.
pub const IMPLEMENTATION_STAGE: u8 = 10;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_bio::rc;
    use splicecraft_persist::{DataLayout, authorize_writes_for_sandbox};

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-gels");
    }

    #[test]
    fn circular_template_wrap_amplicon_included() {
        let seq = format!(
            "{}{}{}{}{}",
            "A".repeat(20),
            "ATGCGATCGATCGATCGCGT",
            "A".repeat(10),
            "GCATCGTAGCTAGCTGATCG",
            "A".repeat(30)
        );
        let fwd = "GCATCGTAGCTAGCTGATCG";
        let rev = rc("ATGCGATCGATCGATCGCGT");
        let amps = simulate_pcr(&seq, fwd, &rev, true, 200).unwrap();
        let wrap: Vec<_> = amps.into_iter().filter(|a| a.wraps).collect();
        assert!(!wrap.is_empty(), "expected at least one wrapping amplicon");
        let a = &wrap[0];
        assert_eq!(a.start, 50);
        assert_eq!(a.end, 40);
        assert_eq!(a.length, 90);
    }

    #[test]
    fn linear_one_amplicon() {
        let seq = format!(
            "{}{}{}",
            "ATGCGATCGATCGATCGCGT",
            "A".repeat(60),
            "GCATCGTAGCTAGCTGATCG"
        );
        let fwd = "ATGCGATCGATCGATCGCGT";
        let rev = rc("GCATCGTAGCTAGCTGATCG");
        let amps = simulate_pcr(&seq, fwd, &rev, false, PCR_DEFAULT_MAX_AMPLICON as i64).unwrap();
        assert_eq!(amps.len(), 1);
        let a = &amps[0];
        assert_eq!(a.start, 0);
        assert_eq!(a.end, 100);
        assert_eq!(a.length, 100);
        assert!(!a.wraps);
        assert_eq!(a.amplicon_seq, seq);
        let rec = amplicon_to_record(a);
        assert!(!rec.circular);
        assert_eq!(
            rec.features
                .iter()
                .filter(|f| f.kind == "primer_bind")
                .count(),
            2
        );
    }

    #[test]
    fn mobility_larger_band_migrates_less_at_1pct() {
        let small = agarose_mobility(500, 1.0, DnaForm::Linear);
        let large = agarose_mobility(10_000, 1.0, DnaForm::Linear);
        assert!(
            large < small,
            "larger must sit closer to the well: small={small} large={large}"
        );
        let m1000 = agarose_mobility(1000, 1.0, DnaForm::Linear);
        assert!(m1000 > large && m1000 < small);
    }

    #[test]
    fn mobility_is_helling_goodman_boyer_in_window() {
        let m = agarose_mobility(1000, 1.0, DnaForm::Linear);
        let log_lo = 500f64.log10();
        let log_hi = 10_000f64.log10();
        let raw = (log_hi - 1000f64.log10()) / (log_hi - log_lo);
        let band = GEL_EDGE_BAND;
        let expected = band + (1.0 - 2.0 * band) * raw;
        assert!((m - expected).abs() < 1e-9, "m={m} expected={expected}");
    }

    #[test]
    fn supercoiled_faster_than_linear() {
        let lin = agarose_mobility(3000, 1.0, DnaForm::Linear);
        let sc = agarose_mobility(3000, 1.0, DnaForm::Supercoiled);
        assert!(sc > lin, "sc={sc} lin={lin}");
        let nick = agarose_mobility(3000, 1.0, DnaForm::Nicked);
        assert!(nick < lin);
        assert_eq!(agarose_mobility(3000, 1.0, DnaForm::Relaxed), nick);
    }

    #[test]
    fn gel_reload_from_sandboxed_json() {
        let tmp = tempfile::tempdir().unwrap();
        authorize_writes_for_sandbox(tmp.path()).unwrap();
        let layout = DataLayout::from_xdg_home(tmp.path()).unwrap();
        assert!(layout.root.starts_with(tmp.path()));
        let mut store = GelStore::new();
        store.upsert(snapshot_gel(
            "Friday digest",
            1.0,
            &[
                GelLane::ladder("Ladder", "1 kb"),
                GelLane {
                    name: "Digest".into(),
                    source: "digest".into(),
                    detail: "EcoRI".into(),
                    pcr_bp: None,
                },
            ],
            "",
        ));
        store.persist(&layout).unwrap();
        let again = GelStore::load(&layout);
        assert_eq!(again.entries.len(), 1);
        assert_eq!(again.entries[0].name, "Friday digest");
        assert_eq!(again.entries[0].lanes.len(), 2);
        assert!(again.entries[0].id.starts_with("gel-"));
        assert_eq!(
            layout.gels_file().file_name().and_then(|s| s.to_str()),
            Some(persist::GELS_FILE_NAME)
        );
    }

    #[test]
    fn iupac_primer_errors() {
        let err = simulate_pcr(
            "ATGC".repeat(100).as_str(),
            "NNNNNNNNNNNNNNNNNNNN",
            "GCATGCATGCATGCAT",
            false,
            500,
        )
        .unwrap_err();
        assert!(err.to_string().contains("IUPAC"));
    }

    #[test]
    fn amplicon_count_capped() {
        let fwd_site = "GATCGATCGATCGATCGATC";
        let rev_site_on_top = "GTACGTACGTACGTACGTAC";
        let spacer = "A".repeat(10);
        let mut seq = String::new();
        for _ in 0..60 {
            seq.push_str(fwd_site);
            seq.push_str(&spacer);
        }
        seq.push_str(rev_site_on_top);
        seq.push_str(&"A".repeat(50));
        let amps = simulate_pcr(&seq, fwd_site, &rc(rev_site_on_top), false, 5000).unwrap();
        assert!(amps.len() <= PCR_MAX_AMPLICONS);
    }

    #[test]
    fn gel_bands_ladder_and_forms() {
        let ladder = gel_bands_for_lane(&GelLane::ladder("L", "1 kb"), "", false, None);
        let bps: Vec<_> = ladder.iter().map(|(b, _)| *b).collect();
        assert!(bps.contains(&1000));
        assert!(bps.contains(&250));
        let circ = gel_bands_for_lane(
            &GelLane {
                name: "P".into(),
                source: "plasmid".into(),
                detail: String::new(),
                pcr_bp: None,
            },
            &"ATGC".repeat(100),
            true,
            None,
        );
        let mut forms: Vec<_> = circ.iter().map(|(_, f)| f.as_str()).collect();
        forms.sort_unstable();
        assert_eq!(forms, vec!["nicked", "supercoiled"]);
    }

    #[test]
    fn digest_two_ecori() {
        let seq = format!(
            "{}GAATTC{}GAATTC{}",
            "G".repeat(100),
            "A".repeat(200),
            "C".repeat(100)
        );
        let bands = gel_bands_for_lane(
            &GelLane {
                name: "D".into(),
                source: "digest".into(),
                detail: "EcoRI".into(),
                pcr_bp: None,
            },
            &seq,
            true,
            None,
        );
        assert_eq!(bands.len(), 2);
        assert!(bands.iter().all(|(_, f)| *f == DnaForm::Linear));
        assert_eq!(bands.iter().map(|(bp, _)| *bp).sum::<usize>(), seq.len());
    }

    #[test]
    fn render_emits_ladder_label_and_dye_front() {
        let lanes = vec![GelLane::ladder("L", "1 kb")];
        let text = render_gel_image(
            &lanes,
            &GelRenderOpts {
                template_seq: "",
                template_circular: false,
                pcr_length: None,
                agarose_pct: 1.0,
                height: 22,
                lane_width: 7,
                label_col: 7,
            },
        );
        assert!(text.contains("10.0k"), "{text}");
        assert!(text.contains('░'), "{text}");
        assert!(text.contains('━') || text.contains('─'), "{text}");
    }

    #[test]
    fn sanitize_and_extract_gel_refs() {
        assert_eq!(
            sanitize_gel_id("gel-aaaaaaaa").as_deref(),
            Some("gel-aaaaaaaa")
        );
        assert_eq!(sanitize_gel_id("../etc/passwd"), None);
        assert_eq!(sanitize_gel_id(".hidden"), None);
        assert_eq!(sanitize_gel_id(&"a".repeat(65)), None);
        assert_eq!(
            extract_gel_refs("Today: &runA then &runB"),
            vec!["runA", "runB"]
        );
        assert_eq!(
            extract_gel_refs("&pcr first, &gibson, then &pcr again"),
            vec!["pcr", "gibson"]
        );
        assert!(extract_gel_refs("foo&bar").is_empty());
        assert!(extract_gel_refs("&&double").is_empty());
        assert!(extract_gel_refs("&1abc").is_empty());
        assert_eq!(GEL_CHIP_COLOR, "#FFB347");
    }

    #[test]
    fn append_pcr_lane_freezes_size() {
        let mut lanes = vec![GelLane::ladder("Ladder", "1 kb")];
        let (idx, at_cap) = append_pcr_gel_lane(&mut lanes, "PCR 1,234 bp", 1234, 8);
        assert!(!at_cap);
        assert_eq!(idx, 1);
        assert_eq!(lanes[1].pcr_bp, Some(1234));
        let bands = gel_bands_for_lane(&lanes[1], "", false, Some(999));
        assert_eq!(bands, vec![(1234, DnaForm::Linear)]);
    }
}
