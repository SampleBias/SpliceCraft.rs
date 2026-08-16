//! Atomic JSON persistence, backups, and the data-safety chokepoint.
//!
//! Sacred invariant 7 lives here. See `docs/invariants.md` and
//! `docs/stages/02-persist.md`.

#![forbid(unsafe_code)]

pub use splicecraft_util as util;

/// Stage that implements this crate's real save engine.
pub const IMPLEMENTATION_STAGE: u8 = 2;

/// XDG data-directory leaf for this rewrite.
///
/// Must never be `splicecraft` — that path belongs to the Python SpliceCraft
/// app. Sharing it would put years of user plasmids one bug away from a
/// Rust-side overwrite.
pub const XDG_DATA_DIR_LEAF: &str = "splicecraft-rs";

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-persist");
    }

    #[test]
    fn data_dir_does_not_collide_with_python_app() {
        assert_eq!(XDG_DATA_DIR_LEAF, "splicecraft-rs");
        assert_ne!(XDG_DATA_DIR_LEAF, "splicecraft");
    }

    #[test]
    fn layer_below_is_wired() {
        assert_eq!(util::crate_name(), "splicecraft-util");
    }
}
