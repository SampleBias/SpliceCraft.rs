//! Primer Tm, SOE/QuikChange design, primer-check, and restriction-site scrub.
//!
//! Filled in during stage 07. Do not take the GPL `primer3` crate as a default
//! dependency. See `docs/stages/07-enzymes-primers.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_codon as codon;
pub use splicecraft_core as core;

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
}
