//! Sequence-format I/O: GenBank, FASTA, GFF, NCBI fetch, sequencing, `.dna`.
//!
//! Stages 03 and 11. See `docs/stages/03-file-io.md` and `docs/stages/11-sequencing.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_core as core;
pub use splicecraft_persist as persist;
pub use splicecraft_util as util;

mod ab1;
mod align;
mod bulk;
mod detect;
mod dna;
mod error;
mod fasta;
mod genbank;
mod gff;
mod history_recover;
mod locus;
mod net;
mod plasmidsaurus;
mod zip;

pub use ab1::{Ab1Trace, is_ab1_path, load_ab1, write_test_ab1};
pub use align::{
    AlignMode, AlignResult, AlignState, AlignVariant, BulkAlignRow, PAIRWISE_MAX_LEN, SeqStatus,
    alignment_bar_columns, alignment_indel_events, alignment_quality_status,
    alignment_to_target_segments, badge_from_result, bulk_align_folder, coverage_pct_from_result,
    extract_variants_from_alignment, library_entry_alignment_summary, pairwise_align,
    render_alignment_bar,
};
pub use bulk::{
    BULK_IMPORT_MAX_FILES, BulkExportFormat, BulkFailure, BulkImportReport, bulk_export_folder,
    bulk_import_folder, record_to_library_entry,
};
pub use detect::{SeqFormat, detect_format};
pub use dna::{
    PACKET_COOKIE, PACKET_DNA, PACKET_FEATURES, PACKET_HISTORY, build_cookie_packet,
    build_dna_packet, build_dna_seq_packet, dna_bytes_to_record, extract_history_xml,
    inject_history_xml, iter_dna_packets, load_dna_path, write_dna_bytes,
};
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
pub use history_recover::{
    HISTORY_RECOVER_MAX_INDEX_BYTES, HISTORY_RECOVER_MAX_SIDECARS, HistoryRecoverHit,
    HistoryRecoverReport, history_node_count_of_xml, recover_history_from_dna,
    scan_dna_originals_for_history,
};
pub use locus::{GB_LOCUS_NAME_MAX, display_name_needs_comment, sanitize_locus_name};
pub use net::{
    NCBI_ALLOWLIST, assert_ncbi_host, assert_public_ip, fetch_genbank, ip_is_non_public,
    ncbi_efetch_url, sanitize_accession,
};
pub use plasmidsaurus::{
    HttpRequest, HttpResponse, HttpTransport, OfflineTransport, PLASMIDSAURUS_API_HOST,
    PLASMIDSAURUS_API_URL, PLASMIDSAURUS_CACHE_TTL, PLASMIDSAURUS_DOWNLOAD_MAX_BYTES,
    PLASMIDSAURUS_ITEMS_LIMIT, PLASMIDSAURUS_RATE_PER_MIN, PsItem, PsSample, PsZip,
    assert_plasmidsaurus_host, clear_plasmidsaurus_orders_cache, first_gbk_record_from_zip,
    parse_plasmidsaurus_zip, plasmidsaurus_credential_hint, plasmidsaurus_credentials,
    plasmidsaurus_item_has_results, plasmidsaurus_list_items, plasmidsaurus_list_items_offline,
    plasmidsaurus_oauth_token, plasmidsaurus_orders_cached, plasmidsaurus_zip_to_entries,
    sanitize_plasmidsaurus_item_code,
};
pub use splicecraft_util::{format_identity_pct, identity_pct_color};
pub use zip::{
    ZIP_MAX_BYTES, ZIP_MAX_MEMBERS, ZIP_MEMBER_MAX_BYTES, ZipMember, extract_gbk_member,
    extract_zip_member, is_safe_zip_member_name, list_gbk_members_in_zip, write_test_zip,
};

/// Stage that implements this crate's real parsers.
pub const IMPLEMENTATION_STAGE: u8 = 11;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Load a path by detected format.
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
        SeqFormat::CommercialDna => load_dna_path(path),
        SeqFormat::Ab1 => Ok(load_ab1(path)?.to_record()),
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
    fn commercial_dna_and_gff3_load() {
        let tmp = persist_temp();
        let rec = Record::new("pDna", "ATGCATGCATGC", true);
        let bytes = write_dna_bytes(&rec, Some("<HistoryTree/>")).unwrap();
        let path = tmp.path().join("construct.dna");
        std::fs::write(&path, &bytes).unwrap();
        let loaded = load_path(&path).unwrap();
        assert_eq!(loaded.sequence, "ATGCATGCATGC");
        assert!(loaded.circular);
        let xml = extract_history_xml(&bytes).unwrap().expect("history");
        assert!(xml.contains("HistoryTree"));
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
    fn identity_99_6_never_formats_as_100() {
        assert_eq!(format_identity_pct(Some(99.6), 0), "99.6%");
        assert_eq!(format_identity_pct(Some(99.6), 1), "99.6%");
        assert_ne!(format_identity_pct(Some(99.6), 0), "100%");
        let v = 100.0 * 18093.0 / 18094.0;
        assert_ne!(format_identity_pct(Some(v), 1), "100%");
    }

    #[test]
    fn zip_parent_dir_member_is_rejected() {
        assert!(!is_safe_zip_member_name("../escape.gbk"));
        assert!(!is_safe_zip_member_name("foo/../../etc/passwd.gbk"));
        assert!(!is_safe_zip_member_name("/etc/passwd.gbk"));
        assert!(!is_safe_zip_member_name("C:/Windows/x.gbk"));
        assert!(!is_safe_zip_member_name("ok/../x.gbk"));
        assert!(is_safe_zip_member_name("34XK5N_genbank-files/DEMO.gbk"));
        let tmp = persist_temp();
        let zpath = tmp.path().join("run.zip");
        write_test_zip(
            &zpath,
            &[
                ("../escape.gbk", b"LOCUS bad"),
                (
                    "good.gbk",
                    b"LOCUS good 12 bp ds-DNA circular\nORIGIN\n        1 atgcatgcatgc\n//\n",
                ),
            ],
        )
        .unwrap();
        let members = list_gbk_members_in_zip(&zpath).unwrap();
        assert!(
            members.iter().all(|m| !m.name.contains("..")),
            "{members:?}"
        );
        let err = extract_gbk_member(&zpath, "../escape.gbk").unwrap_err();
        assert!(err.to_string().contains("unsafe"), "{err}");
    }

    #[test]
    fn alignment_mismatch_positions_on_tiny_pair() {
        let r = pairwise_align("ATGC", "ATGG", AlignMode::Global).unwrap();
        assert_eq!(r.n_matches, 3);
        assert_eq!(r.n_mismatches, 1);
        assert!(r.identity_pct < 100.0);
        let vars = extract_variants_from_alignment(&r.aligned_q, &r.aligned_t);
        assert!(
            vars.iter().any(|v| v.kind == "snp" && v.target_pos == 3),
            "{vars:?} aq={} at={}",
            r.aligned_q,
            r.aligned_t
        );
        let segs = alignment_to_target_segments(&r.aligned_q, &r.aligned_t, 0).unwrap();
        assert!(
            segs.iter().any(|(_, _, s)| *s == AlignState::Mismatch),
            "{segs:?}"
        );
        let n = pairwise_align("ANGC", "ATGC", AlignMode::Global).unwrap();
        assert_eq!(n.n_matches, 4);
        assert_eq!(n.identity_pct, 100.0);
        let perfect = pairwise_align("ATGC", "ATGC", AlignMode::Global).unwrap();
        assert_eq!(alignment_quality_status(&perfect, 4), SeqStatus::Verified);
        assert_ne!(
            format_identity_pct(Some(r.identity_pct), 1).as_str(),
            "100%"
        );
    }

    #[test]
    fn plasmidsaurus_api_is_mocked_and_offline_by_default() {
        assert!(sanitize_plasmidsaurus_item_code("abc123").as_deref() == Some("ABC123"));
        assert!(sanitize_plasmidsaurus_item_code("ABC123/../x").is_none());
        assert!(assert_plasmidsaurus_host(PLASMIDSAURUS_API_HOST).is_ok());
        assert!(assert_plasmidsaurus_host("evil.example").is_err());
        let hint = plasmidsaurus_credential_hint("seb@example.org", "hunter2");
        assert!(hint.contains("email address"), "{hint}");
        struct Mock;
        impl HttpTransport for Mock {
            fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, IoError> {
                if req.url.contains("/oauth/token") {
                    return Ok(HttpResponse {
                        status: 200,
                        body: br#"{"access_token":"tok"}"#.to_vec(),
                    });
                }
                if req.url.contains("/api/items") {
                    return Ok(HttpResponse {
                        status: 200,
                        body: br#"[{"code":"ABC123","status":"complete","done_date":"2026-01-01","product_name":"plasmidsaurus","order_name":"run"}]"#.to_vec(),
                    });
                }
                Err(IoError::NetworkDisabled)
            }
        }
        clear_plasmidsaurus_orders_cache();
        let token = plasmidsaurus_oauth_token(&Mock, "a", "b").unwrap();
        assert_eq!(token, "tok");
        let items = plasmidsaurus_list_items(&Mock, &token, 1000).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].has_results());
        let err = plasmidsaurus_oauth_token(&OfflineTransport, "a", "b").unwrap_err();
        assert!(matches!(err, IoError::NetworkDisabled));
        let shipping = PsItem {
            code: "SHIP01".into(),
            status: "complete".into(),
            done_date: String::new(),
            product_name: "ups_shipping_label".into(),
            order_name: "label".into(),
        };
        assert!(!plasmidsaurus_item_has_results(&shipping));
    }

    #[test]
    fn ab1_roundtrip_carries_phred() {
        let bytes = write_test_ab1("ATGCATGC", &[40; 8], "traceA");
        let tmp = persist_temp();
        let path = tmp.path().join("t.ab1");
        std::fs::write(&path, bytes).unwrap();
        let tr = load_ab1(&path).unwrap();
        assert_eq!(tr.sequence, "ATGCATGC");
        assert_eq!(tr.phred.len(), 8);
        assert!(tr.mean_phred().unwrap() > 30.0);
        assert!(!tr.sequence.is_empty());
    }

    #[test]
    fn plasmidsaurus_zip_tags_source_and_skips_traversal() {
        let tmp = persist_temp();
        let gb = record_to_gb_text(&Record::new("samp", "ATGAAACGCATT", true)).unwrap();
        let zpath = tmp.path().join("ps.zip");
        write_test_zip(
            &zpath,
            &[
                ("34XK5N_genbank-files/DEMO34.gbk", gb.as_bytes()),
                ("../evil.gbk", b"LOCUS x"),
            ],
        )
        .unwrap();
        let parsed = parse_plasmidsaurus_zip(&zpath).unwrap();
        assert_eq!(parsed.run_id, "34XK5N");
        assert!(parsed.samples.iter().any(|s| s.gbk.is_some()));
        let (entries, _warn) = plasmidsaurus_zip_to_entries(&zpath, "ABC123").unwrap();
        assert!(!entries.is_empty());
        assert!(
            entries
                .iter()
                .all(|e| e.source.starts_with("plasmidsaurus:")),
            "{:?}",
            entries.iter().map(|e| &e.source).collect::<Vec<_>>()
        );
        assert!(entries.iter().all(|e| !e.source.contains("evil")));
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

    #[test]
    fn recover_history_matches_exact_sequence_identity() {
        use splicecraft_persist::{LibraryStore, authorize_writes_for_sandbox};
        use std::fs;

        let tmp = tempfile::tempdir().expect("tempdir");
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = persist::DataLayout::from_xdg_home(tmp.path()).expect("layout");
        assert!(layout.root.starts_with(tmp.path()));

        let seq = "ATGCATGCATGC".repeat(10);
        let rec = Record::new("pRich", &seq, true);
        let thin = "<HistoryTree><Node name=\"thin.dna\"/></HistoryTree>";
        let rich = "<HistoryTree><Node name=\"prod.dna\" operation=\"goldenGateAssembly\">\
            <Node name=\"a.dna\"/><Node name=\"b.dna\"/></Node></HistoryTree>";
        assert!(history_node_count_of_xml(rich) > history_node_count_of_xml(thin));

        let dna = write_dna_bytes(&rec, Some(rich)).unwrap();
        let dir = layout.dna_originals_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("filed_as_other.dna"), dna).unwrap();

        let mut store = LibraryStore::new();
        let mut entry = record_to_library_entry(&rec).unwrap();
        entry.name = "renamed_after_build".into();
        entry.id = "renamed_after_build".into();
        entry.history_xml = thin.into();
        store.keep(entry, None);
        let gb_before = store.plasmids[0].gb_text.clone();

        let dry = recover_history_from_dna(&layout, &mut store, true).unwrap();
        assert!(dry.dry_run);
        assert_eq!(dry.updated.len(), 1);
        assert_eq!(dry.updated[0].name, "renamed_after_build");
        assert_eq!(dry.updated[0].nodes_before, 1);
        assert_eq!(dry.updated[0].nodes_after, 3);
        assert_eq!(store.plasmids[0].history_xml, thin);
        assert_eq!(store.plasmids[0].gb_text, gb_before);

        let applied = recover_history_from_dna(&layout, &mut store, false).unwrap();
        assert!(!applied.dry_run);
        assert_eq!(store.plasmids[0].history_xml, rich);
        assert_eq!(store.plasmids[0].name, "renamed_after_build");
        assert_eq!(store.plasmids[0].gb_text, gb_before);
        let seq_after = gb_text_to_record(&store.plasmids[0].gb_text)
            .unwrap()
            .sequence;
        assert_eq!(seq_after, seq);

        let again = recover_history_from_dna(&layout, &mut store, false).unwrap();
        assert!(
            again.updated.is_empty(),
            "must not thin an existing lineage"
        );
    }

    fn persist_temp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }
}
