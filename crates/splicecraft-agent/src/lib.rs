//! Localhost JSON agent API.
//!
//! Filled in during stage 14. See `docs/stages/14-agent-api-cli.md`.

#![forbid(unsafe_code)]

pub use splicecraft_clone as clone;
pub use splicecraft_core as core;
pub use splicecraft_io as io;
pub use splicecraft_persist as persist;

/// Stage that implements this crate's real HTTP surface.
pub const IMPLEMENTATION_STAGE: u8 = 14;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-agent");
    }
}
