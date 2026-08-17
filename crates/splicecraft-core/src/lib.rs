//! Sequence records, wrap-aware features, and circular plasmid math.
//!
//! Coordinates are 0-based half-open. See `docs/stages/01-core-biology.md`.

#![forbid(unsafe_code)]

pub mod circular;
pub mod edit;
pub mod orient;
pub mod record;

pub use circular::{bp_in, feat_len, slice_circular, wrap_midpoint};
pub use edit::{EditMode, rebuild_record_with_edit};
pub use orient::rotate_record;
pub use record::{Feature, FeaturePart, Record};

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

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn wrap_feat_len_matches_forward_distance(
            total in 2_usize..400,
            start in 0_usize..400,
            end in 0_usize..400,
        ) {
            prop_assume!(start < total && end < total);
            if end < start {
                prop_assert_eq!(feat_len(start, end, total), (total - start) + end);
            } else {
                prop_assert_eq!(feat_len(start, end, total), end - start);
            }
        }
    }
}
