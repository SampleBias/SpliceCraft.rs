//! Sequence-format I/O: GenBank, FASTA, GFF, NCBI fetch, later `.dna` / AB1.
//!
//! Filled in during stage 03. See `docs/stages/03-file-io.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_core as core;
pub use splicecraft_persist as persist;
pub use splicecraft_util as util;

/// Stage that implements this crate's real parsers.
pub const IMPLEMENTATION_STAGE: u8 = 3;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-io");
    }

    #[test]
    fn persist_data_dir_is_rs_leaf() {
        assert_eq!(persist::XDG_DATA_DIR_LEAF, "splicecraft-rs");
    }
}
