//! IUPAC, reverse-complement, restriction scanning, digest, and translation.
//!
//! Sacred invariants 1–6 and 8–9 live here. See `docs/invariants.md` and
//! `docs/stages/01-core-biology.md`.

#![forbid(unsafe_code)]

pub use splicecraft_core as core;
pub use splicecraft_util as util;

/// Stage that implements this crate's real biology.
pub const IMPLEMENTATION_STAGE: u8 = 1;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-bio");
    }

    #[test]
    fn layer_below_is_wired() {
        assert_eq!(core::crate_name(), "splicecraft-core");
        assert_eq!(util::crate_name(), "splicecraft-util");
    }
}
