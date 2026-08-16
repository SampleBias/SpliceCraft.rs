//! Traditional, Gibson, Golden Braid, and MoClo construction simulation.
//!
//! Filled in during stage 08. See `docs/stages/08-cloning.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_codon as codon;
pub use splicecraft_core as core;
pub use splicecraft_primer as primer;

/// Stage that implements this crate's real assemblers.
pub const IMPLEMENTATION_STAGE: u8 = 8;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-clone");
    }
}
