//! In-silico agarose gels (Helling–Goodman–Boyer mobility).
//!
//! Filled in during stage 10. See `docs/stages/10-simulator-gels.md`.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_clone as clone;
pub use splicecraft_core as core;

/// Stage that implements this crate's real gel renderer.
pub const IMPLEMENTATION_STAGE: u8 = 10;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-gels");
    }
}
