//! Codon tables, optimization, CAI, and forbidden-site scrub.
//!
//! Stage 09. See `docs/stages/09-mutato-codon-synthesis.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_core as core;
pub use splicecraft_io as io;
pub use splicecraft_persist as persist;
pub use splicecraft_util as util;

mod composer;
mod error;
mod fix;
mod genome;
mod metrics;
mod motifs;
mod optimize;
mod parse;
mod store;
mod table;

pub use composer::{DnaBuffer, ProteinBuffer, SYNTH_MAX_AA, SYNTH_MAX_BP, codon_cache};
pub use error::CodonError;
pub use fix::{
    GC_WINDOW_DEFAULT, REPEAT_RUN_DEFAULT, diversify, expand_sites, fix_gc_window,
    fix_mutation_positions, fix_sites, gc_window_range, kmer_set, shared_runs, swap_ok,
};
pub use genome::{GenomeStats, build_from_cds_fasta};
pub use metrics::{cai, gc_pct, gc3};
pub use motifs::{MotifStore, ProteinMotif, builtin_motifs};
pub use optimize::{CodonMode, optimize};
pub use parse::{parse_kazusa_html, parse_tsv};
pub use store::CodonTableStore;
pub use table::{
    AaCodonMap, CodonFracMap, TableEntry, UsageTable, allocate, build_aa_map, builtin_k12,
    default_forbidden, hazard_motifs, name_parts, search_tables,
};

/// Stage that implements this crate's real optimizer.
pub const IMPLEMENTATION_STAGE: u8 = 9;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use splicecraft_persist::{DataLayout, authorize_writes_for_sandbox};

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-codon");
    }

    #[test]
    fn translate_of_optimize_is_original() {
        let aa = "MAEVKLAGHIKQRSTVWYFND";
        let dna = optimize(aa, &builtin_k12(), 1, 1, CodonMode::Frequency).unwrap();
        assert_eq!(mut_translate(&dna), aa);
        assert!(dna.ends_with("TAA"));
    }

    #[test]
    fn optimize_met_trp_use_only_codon() {
        let dna = optimize("MWMW", &builtin_k12(), 1, 1, CodonMode::Frequency).unwrap();
        assert_eq!(&dna[..3], "ATG");
        assert_eq!(&dna[3..6], "TGG");
        assert_eq!(&dna[6..9], "ATG");
        assert_eq!(&dna[9..12], "TGG");
    }

    #[test]
    fn optimize_rejects_unknown_aa() {
        let err = optimize("MAXA", &builtin_k12(), 1, 1, CodonMode::Frequency).unwrap_err();
        assert!(matches!(err, CodonError::NoCodons('X')));
    }

    #[test]
    fn leucine_prefers_ctg() {
        let dna = optimize(&"L".repeat(100), &builtin_k12(), 1, 1, CodonMode::Frequency).unwrap();
        let mut max = 0usize;
        let mut counts = std::collections::HashMap::new();
        for i in (0..300).step_by(3) {
            let c = &dna[i..i + 3];
            *counts.entry(c.to_owned()).or_insert(0) += 1;
        }
        for n in counts.values() {
            max = max.max(*n);
        }
        let ctg = *counts.get("CTG").unwrap_or(&0);
        assert!(ctg >= 40);
        assert_eq!(ctg, max);
    }

    #[test]
    fn max_cai_uses_peak_synonym() {
        let aa = "MRRRSSRRLLRRGGAARRVVRRDDRRKKRREE".repeat(2);
        let (aa_codons, _) = build_aa_map(&builtin_k12(), 1);
        let dna = optimize(&aa, &builtin_k12(), 1, 1, CodonMode::MaxCai).unwrap();
        for (i, res) in aa.chars().enumerate() {
            let codon = &dna[i * 3..i * 3 + 3];
            assert_eq!(codon, aa_codons[&res][0].0);
        }
        let cai_best = cai(&dna, &builtin_k12(), 1);
        let freq = optimize(&aa, &builtin_k12(), 1, 1, CodonMode::Frequency).unwrap();
        assert!(cai_best > cai(&freq, &builtin_k12(), 1));
        assert!((cai_best - 1.0).abs() < 1e-9);
    }

    #[test]
    fn unknown_mode_rejected() {
        assert!(CodonMode::parse("harmonize").is_err());
    }

    #[test]
    fn empty_protein_is_lone_stop() {
        assert_eq!(
            optimize("", &builtin_k12(), 1, 1, CodonMode::Frequency).unwrap(),
            "TAA"
        );
        assert_eq!(
            optimize("M", &builtin_k12(), 1, 1, CodonMode::Frequency).unwrap(),
            "ATGTAA"
        );
        assert_eq!(
            optimize("MGK", &builtin_k12(), 0, 1, CodonMode::Frequency)
                .unwrap()
                .len(),
            9
        );
    }

    #[test]
    fn trailing_stops_override_kwarg() {
        let dna = optimize("MGK***", &builtin_k12(), 1, 1, CodonMode::Frequency).unwrap();
        assert_eq!(dna.len(), 18);
        assert!(optimize("MGK*M", &builtin_k12(), 1, 1, CodonMode::Frequency).is_err());
    }

    #[test]
    fn fix_sites_removes_ecori() {
        let dna = "ATGGAATTCGCGAAATAA";
        let (fixed, fixes) = fix_sites(
            dna,
            "MEFAK",
            &builtin_k12(),
            Some(&BTreeMap::from([("EcoRI".into(), "GAATTC".into())])),
            true,
            1,
        );
        assert!(!fixed.contains("GAATTC"));
        assert_eq!(fixes.len(), 1);
        assert_eq!(mut_translate(&fixed), mut_translate(dna));
        assert_eq!(fixed.len(), dna.len());
    }

    #[test]
    fn fix_sites_removes_bsai_both_strands() {
        let seed = "ATGGCGAGTGGTCTCCGTGAGGAGGAGGAGTAA";
        assert!(seed.contains("GGTCTC"));
        let (fixed, _) = fix_sites(
            seed,
            &mut_translate(seed),
            &builtin_k12(),
            Some(&BTreeMap::from([("BsaI".into(), "GGTCTC".into())])),
            true,
            1,
        );
        assert!(!fixed.contains("GGTCTC"));
        assert!(!fixed.contains("GAGACC"));
        assert_eq!(mut_translate(&fixed), mut_translate(seed));
    }

    #[test]
    fn swap_ok_windowed_equals_fullscan() {
        let seq = "ATGGCGAGTGGTCTCCGTGAGGAGGAGGAGTAA";
        let sites = ["GGTCTC", "GAGACC"];
        let before = bio::forbidden_hit_set(seq, &sites);
        let maxlen = 6;
        for codon_start in (0..seq.len().saturating_sub(3)).step_by(3) {
            for alt in ["GCA", "GCC", "GCG", "GCT"] {
                if seq[codon_start..codon_start + 3] == *alt {
                    continue;
                }
                for (site, idx) in &before {
                    let w = swap_ok(
                        seq,
                        codon_start,
                        alt,
                        site,
                        *idx,
                        &sites,
                        &before,
                        maxlen,
                        true,
                    );
                    let f = swap_ok(
                        seq,
                        codon_start,
                        alt,
                        site,
                        *idx,
                        &sites,
                        &before,
                        maxlen,
                        false,
                    );
                    assert_eq!(w, f, "codon {codon_start} alt {alt} site {site}@{idx}");
                }
            }
        }
    }

    #[test]
    fn parse_tsv_and_search() {
        let text = "GCT A 120\nGCC 10\n# comment\nTTT F 5\n";
        let raw = parse_tsv(text).unwrap();
        assert_eq!(raw.get("GCT"), Some(('A', 120)));
        assert!(parse_tsv("ATG\tL\t5\n").is_err());
        let entries = vec![
            TableEntry {
                name: "Escherichia coli A".into(),
                taxid: "900101".into(),
                source: "user".into(),
                added: String::new(),
                raw: builtin_k12(),
            },
            TableEntry {
                name: "Notesch species".into(),
                taxid: "900102".into(),
                source: "user".into(),
                added: String::new(),
                raw: builtin_k12(),
            },
        ];
        let hits = search_tables("esch", &entries);
        let names: Vec<_> = hits.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names
                .iter()
                .position(|n| *n == "Escherichia coli A")
                .unwrap()
                < names.iter().position(|n| *n == "Notesch species").unwrap()
        );
    }

    #[test]
    fn store_seeds_k12_and_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        authorize_writes_for_sandbox(tmp.path()).unwrap();
        let layout = DataLayout::from_xdg_home(tmp.path()).unwrap();
        assert!(layout.root.starts_with(tmp.path()));
        let mut store = CodonTableStore::load(&layout);
        assert!(store.get("83333").is_some());
        store.add("Fake sp.", "999999", builtin_k12(), "user");
        store.persist(&layout).unwrap();
        let again = CodonTableStore::load(&layout);
        assert!(again.get("999999").is_some());
        assert!(again.get("fake sp.").is_some());
    }

    #[test]
    fn motif_library_has_essentials() {
        let names: std::collections::HashSet<_> =
            builtin_motifs().into_iter().map(|m| m.name).collect();
        for required in ["His6", "FLAG", "HA", "Myc", "TEV", "P2A", "(GGGGS)x3"] {
            assert!(names.contains(required), "missing {required}");
        }
    }

    #[test]
    fn protein_composer_fills_from_table() {
        let mut p = ProteinBuffer::default();
        p.insert("MWMW");
        let dna = p.to_dna(&builtin_k12(), 1, CodonMode::Frequency).unwrap();
        assert_eq!(&dna[..12], "ATGTGGATGTGG");
        assert!(dna.ends_with("TAA"));
    }

    #[test]
    fn dna_buffer_shifts_features() {
        let mut b = DnaBuffer::default();
        b.insert("ATGCATGC");
        b.features
            .push(splicecraft_core::Feature::new("misc", 2, 6, 1, "x"));
        b.cursor = 2;
        b.insert("AA");
        assert_eq!(&b.seq, "ATAAGCATGC");
        assert_eq!(b.features[0].start, 4);
        assert_eq!(b.features[0].end, 8);
        b.delete_range(4, 6);
        assert_eq!(&b.seq, "ATAAATGC");
        assert_eq!(b.features[0].start, 4);
        assert_eq!(b.features[0].end, 6);
    }

    #[test]
    fn gc_helpers() {
        assert_eq!(gc_pct(""), 0.0);
        assert_eq!(gc_pct("GCGC"), 100.0);
        assert_eq!(gc_pct("ATAT"), 0.0);
        assert_eq!(gc3("ATAAAATAA"), 0.0);
        assert_eq!(gc3("GCG"), 100.0);
    }

    #[test]
    fn heg_from_inline_fasta() {
        let fasta = "\
>lcl|x [gene=rplB] ribosomal protein L2
ATGAAATGC
>lcl|y transferase ribosomal protein
ATGTTTTGG
>lcl|z hypothetical
ATGAAATGG
";
        let (raw, stats) = build_from_cds_fasta(fasta, "heg").unwrap();
        assert!(stats.n_cds_heg >= 1);
        assert!(raw.get("AAA").is_some());
        let dna = optimize("MC", &raw, 0, 1, CodonMode::Frequency).unwrap();
        assert_eq!(mut_translate(&dna), "MC");
    }

    #[test]
    fn alt_code_table4_tga_trp() {
        let mut raw = builtin_k12();
        raw.insert("TGA", 'W', 100);
        let dna = optimize("MW", &raw, 1, 4, CodonMode::MaxCai).unwrap();
        assert_eq!(bio::codon_aa_table(&dna[3..6], 4), 'W');
        assert_eq!(bio::translate_cds_table(&dna, 0, 6, 1, 1, 4), "MW");
    }

    fn mut_translate(dna: &str) -> String {
        let mut aa = String::new();
        let b = dna.as_bytes();
        let mut i = 0;
        while i + 2 < b.len() {
            let c = dna[i..i + 3].to_ascii_uppercase();
            if matches!(c.as_str(), "TAA" | "TAG" | "TGA") {
                break;
            }
            aa.push(bio::codon_aa(&c));
            i += 3;
        }
        aa
    }
}
