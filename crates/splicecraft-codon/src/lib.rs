//! Codon tables, optimization, CAI, and forbidden-site scrub.
//!
//! Filled in during stage 09. See `docs/stages/09-mutato-codon-synthesis.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_core as core;
pub use splicecraft_io as io;
pub use splicecraft_util as util;

/// Stage that implements this crate's real optimizer.
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
        assert_eq!(crate_name(), "splicecraft-codon");
    }
}
