//! Sequence-format I/O: GenBank, FASTA, GFF, NCBI fetch, later `.dna` / AB1.
//!
//! Stage 03. See `docs/stages/03-file-io.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_core as core;
pub use splicecraft_persist as persist;
pub use splicecraft_util as util;

mod detect;
mod error;
mod fasta;
mod genbank;
mod gff;
mod locus;
mod net;

pub use detect::{SeqFormat, detect_format};
pub use error::IoError;
pub use fasta::{
    BULK_IMPORT_MAX_BYTES, FastaRecord, detect_fasta_topology, export_fasta_to_path,
    export_record_fasta, fasta_to_record, load_fasta, parse_fasta, parse_fasta_single,
    record_to_fasta,
};
pub use genbank::{
    GB_INGEST_MAX_BYTES, GB_TEXT_MAX_BYTES, export_genbank_to_path, gb_text_to_record,
    load_genbank, record_to_gb_text,
};
pub use gff::{export_gff3_to_path, record_to_gff3};
pub use locus::{GB_LOCUS_NAME_MAX, display_name_needs_comment, sanitize_locus_name};
pub use net::{
    NCBI_ALLOWLIST, assert_ncbi_host, assert_public_ip, fetch_genbank, ip_is_non_public,
    ncbi_efetch_url, sanitize_accession,
};

/// Stage that implements this crate's real parsers.
pub const IMPLEMENTATION_STAGE: u8 = 3;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Load a path by detected format. `.dna` is refused (stage 11).
pub fn load_path(path: &std::path::Path) -> Result<core::Record, IoError> {
    match detect_format(path) {
        SeqFormat::Fasta => load_fasta(path),
        SeqFormat::GenBank => load_genbank(path),
        SeqFormat::Gff3 => Err(IoError::rejected(
            "GFF3 import without an embedded sequence is not a stage-03 loader; export is supported",
        )),
        SeqFormat::Embl => Err(IoError::rejected(
            "EMBL import is not implemented in stage 03; detect and GenBank/FASTA loaders are",
        )),
        SeqFormat::CommercialDna => Err(IoError::DnaDeferred {
            path: path.to_path_buf(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_core::{Feature, Record};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-io");
    }

    #[test]
    fn persist_data_dir_is_rs_leaf() {
        assert_eq!(persist::XDG_DATA_DIR_LEAF, "splicecraft-rs");
    }

    fn wrap_plasmid() -> Record {
        let mut rec = Record::new("pWrap", "N".repeat(120), true);
        rec.features
            .push(Feature::new("misc_feature", 110, 5, 1, "wrap_region"));
        rec
    }

    #[test]
    fn wrap_feature_survives_genbank_roundtrip() {
        let src = wrap_plasmid();
        assert!(src.features[0].is_wrap());
        assert_eq!(src.features[0].len_on(src.len()), 15);
        let gb = record_to_gb_text(&src).expect("write");
        assert!(gb.contains("join("), "{gb}");
        let back = gb_text_to_record(&gb).expect("read");
        assert!(back.circular);
        let wraps: Vec<_> = back
            .features
            .iter()
            .filter(|f| f.label == "wrap_region")
            .collect();
        assert_eq!(wraps.len(), 1);
        assert!(
            wraps[0].is_wrap(),
            "wrap flattened to [{}, {})",
            wraps[0].start,
            wraps[0].end
        );
        assert_eq!(wraps[0].start, 110);
        assert_eq!(wraps[0].end, 5);
        assert_eq!(wraps[0].len_on(back.len()), 15);
        assert_eq!(back.sequence, src.sequence);
    }

    #[test]
    fn locus_illegal_characters_are_sanitised() {
        let raw = "FFE 6 ENTRY pCambia2300-GREEN";
        let locus = sanitize_locus_name(raw);
        assert!(!locus.contains(' '), "{locus}");
        assert!(!locus.contains('-'), "{locus}");
        assert!(locus.len() <= GB_LOCUS_NAME_MAX);
        let mut rec = Record::new(raw, "ACGT".repeat(20), true);
        rec.name = raw.into();
        let gb = record_to_gb_text(&rec).expect("write");
        let locus_line = gb.lines().next().unwrap();
        assert!(locus_line.starts_with("LOCUS"));
        assert!(!locus_line.contains("FFE 6 ENTRY"), "{locus_line}");
        assert!(gb.contains("SpliceCraft-name:"), "{gb}");
        let back = gb_text_to_record(&gb).expect("read");
        assert_eq!(back.name, raw);
    }

    #[test]
    fn export_comment_contains_splicecraft_rs() {
        let rec = Record::new("pUC", "ATGCATGCATGC", true);
        let gb = record_to_gb_text(&rec).expect("write");
        assert!(gb.contains("SpliceCraft.rs"), "{gb}");
        assert!(gb.contains("Created by SpliceCraft.rs v"), "{gb}");
        let again = record_to_gb_text(&gb_text_to_record(&gb).unwrap()).unwrap();
        assert_eq!(again.matches("Created by SpliceCraft.rs v").count(), 1);
    }

    #[test]
    fn existing_provenance_stamp_is_not_replaced() {
        let mut rec = Record::new("OLD", "ACGTACGTACGT", true);
        rec.comments
            .push("Created by SpliceCraft v0.9.0 on 2025-01-01".into());
        let gb = record_to_gb_text(&rec).expect("write");
        assert!(gb.contains("v0.9.0 on 2025-01-01"), "{gb}");
        assert_eq!(gb.matches("Created by SpliceCraft").count(), 1, "{gb}");
        assert!(!gb.contains("Created by SpliceCraft.rs v"), "{gb}");
    }

    #[test]
    fn arrowless_and_reverse_strands_survive_genbank() {
        let mut rec = Record::new("t", "ACGT".repeat(40), false);
        rec.features
            .push(Feature::new("misc_feature", 0, 10, 0, "pad"));
        rec.features
            .push(Feature::new("misc_feature", 20, 30, -1, "rev"));
        rec.features
            .push(Feature::new("misc_feature", 40, 50, 1, "fwd"));
        let gb = record_to_gb_text(&rec).expect("write");
        assert!(gb.contains("SpliceCraft_strand"), "{gb}");
        let back = gb_text_to_record(&gb).expect("read");
        let by_label = |name: &str| {
            back.features
                .iter()
                .find(|f| f.label == name)
                .unwrap_or_else(|| panic!("missing {name}"))
        };
        assert_eq!(by_label("pad").strand, 0);
        assert!(
            !by_label("pad")
                .qualifiers
                .contains_key("SpliceCraft_strand")
        );
        assert_eq!(by_label("rev").strand, -1);
        assert_eq!(by_label("fwd").strand, 1);
    }

    #[test]
    fn non_wrap_join_is_not_origin_wrap() {
        use splicecraft_core::FeaturePart;
        let mut rec = Record::new("p", "N".repeat(400), true);
        rec.features.push(Feature {
            kind: "misc_feature".into(),
            start: 100,
            end: 350,
            strand: 1,
            label: "two_exons".into(),
            qualifiers: Default::default(),
            parts: vec![
                FeaturePart {
                    start: 100,
                    end: 200,
                    strand: 1,
                },
                FeaturePart {
                    start: 250,
                    end: 350,
                    strand: 1,
                },
            ],
        });
        let back = gb_text_to_record(&record_to_gb_text(&rec).unwrap()).unwrap();
        let f = back
            .features
            .iter()
            .find(|f| f.label == "two_exons")
            .expect("two_exons");
        assert!(!f.is_wrap(), "compound join flattened to wrap {f:?}");
        assert_eq!(f.parts.len(), 2);
    }

    #[test]
    fn commercial_dna_and_gff3_load_are_refused() {
        let dna = load_path(std::path::Path::new("construct.dna")).unwrap_err();
        assert!(matches!(dna, IoError::DnaDeferred { .. }));
        let gff = load_path(std::path::Path::new("ann.gff3")).unwrap_err();
        assert!(matches!(gff, IoError::Rejected(_)));
        let embl = load_path(std::path::Path::new("x.embl")).unwrap_err();
        assert!(matches!(embl, IoError::Rejected(_)));
    }

    #[test]
    fn fasta_in_out() {
        let text = record_to_fasta("partA", "atgcatgc").unwrap();
        assert_eq!(text, ">partA\nATGCATGC\n");
        let fa = parse_fasta_single(&text).unwrap();
        assert_eq!(fa.id, "partA");
        assert_eq!(fa.sequence, "ATGCATGC");
        let rec = fasta_to_record(&fa);
        assert!(!rec.circular);
        let circ = parse_fasta_single(">x circular plasmid backbone\nACGT\n").unwrap();
        assert!(detect_fasta_topology(&format!(
            "{} {}",
            circ.id, circ.description
        )));
        assert!(fasta_to_record(&circ).circular);
        let lin = parse_fasta_single(">x chromosome chunk\nACGT\n").unwrap();
        assert!(!fasta_to_record(&lin).circular);
    }

    #[test]
    fn fasta_rejects_empty_and_multi() {
        assert!(record_to_fasta("   ", "ATGC").is_err());
        assert!(record_to_fasta("n", "").is_err());
        assert!(parse_fasta_single(">a\nACGT\n>b\nGGCC\n").is_err());
    }

    #[test]
    fn gff3_wrap_emits_two_rows() {
        let rec = wrap_plasmid();
        let gff = record_to_gff3(&rec);
        assert!(gff.contains("##gff-version 3"));
        assert!(gff.contains("Is_circular=true"));
        let wrap_rows: Vec<_> = gff
            .lines()
            .filter(|l| l.contains("wrap_region") || l.contains("misc_feature"))
            .collect();
        assert!(
            wrap_rows.len() >= 2,
            "expected two wrap rows, got {wrap_rows:?}\n{gff}"
        );
    }

    #[test]
    fn detect_extensions() {
        assert_eq!(
            detect_format(std::path::Path::new("x.gb")),
            SeqFormat::GenBank
        );
        assert_eq!(
            detect_format(std::path::Path::new("x.fa")),
            SeqFormat::Fasta
        );
        assert_eq!(
            detect_format(std::path::Path::new("x.gff3")),
            SeqFormat::Gff3
        );
        assert_eq!(
            detect_format(std::path::Path::new("x.dna")),
            SeqFormat::CommercialDna
        );
    }

    #[test]
    fn accession_sanitizer_and_ssrf() {
        assert_eq!(sanitize_accession("L09137").as_deref(), Some("L09137"));
        assert!(sanitize_accession("L09137; rm -rf /").is_none());
        assert!(sanitize_accession("../../etc/passwd").is_none());
        assert!(sanitize_accession("").is_none());
        assert!(ip_is_non_public(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(ip_is_non_public(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(ip_is_non_public(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(ip_is_non_public(IpAddr::V4(Ipv4Addr::new(
            169, 254, 169, 254
        ))));
        assert!(assert_public_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))).is_ok());
        assert!(assert_ncbi_host("eutils.ncbi.nlm.nih.gov").is_ok());
        assert!(assert_ncbi_host("evil.example").is_err());
    }

    #[test]
    fn network_fetch_is_not_invoked_by_default() {
        let err = fetch_genbank("L09137").expect_err("must stay offline");
        assert!(matches!(err, IoError::NetworkDisabled));
        let err = fetch_genbank("not a valid accession!!").expect_err("bad acc");
        assert!(matches!(err, IoError::InvalidAccession(_)));
    }

    #[test]
    fn file_roundtrip_genbank_and_fasta() {
        let tmp = persist_temp();
        let src = wrap_plasmid();
        let gb_path = tmp.path().join("p.gb");
        export_genbank_to_path(&src, &gb_path).unwrap();
        let loaded = load_genbank(&gb_path).unwrap();
        assert!(
            loaded
                .features
                .iter()
                .any(|f| f.is_wrap() && f.label == "wrap_region")
        );
        let fa_path = tmp.path().join("p.fa");
        export_record_fasta(&src, &fa_path).unwrap();
        let fa = load_fasta(&fa_path).unwrap();
        assert_eq!(fa.sequence, src.sequence);
        assert!(!fa.circular, "pWrap header has no circular/plasmid hint");
    }

    fn persist_temp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }
}
