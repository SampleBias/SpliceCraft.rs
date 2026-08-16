//! Shared helpers, sanitizers, and a single time source.
//!
//! Filled in during stage 01. See `docs/stages/01-core-biology.md`.

#![forbid(unsafe_code)]

/// Stage that implements this crate's real helpers.
pub const IMPLEMENTATION_STAGE: u8 = 1;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Workspace package version, used by GenBank provenance stamps later.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-util");
    }

    #[test]
    fn version_is_semver() {
        assert!(version().split('.').count() >= 3);
    }
}
