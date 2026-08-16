//! Sequence records, wrap-aware features, and circular plasmid math.
//!
//! Filled in during stage 01. See `docs/stages/01-core-biology.md`.

#![forbid(unsafe_code)]

/// Stage that implements this crate's real types.
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
        assert_eq!(crate_name(), "splicecraft-core");
    }
}
