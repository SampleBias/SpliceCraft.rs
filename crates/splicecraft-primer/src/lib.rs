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
pub use tm::{PRIMER_MAX_OLIGO_LEN, binding_max_len, pick_binding_region, primer_tm, wallace_tm};

/// Stage that implements this crate's real designers.
pub const IMPLEMENTATION_STAGE: u8 = 7;

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
}
