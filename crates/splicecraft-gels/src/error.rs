//! Gel / PCR simulator errors.

use splicecraft_persist::PersistError;

/// Recoverable simulator failure.
#[derive(Debug, thiserror::Error)]
pub enum GelError {
    /// Exact-match PCR refused an IUPAC / non-ACGT primer.
    #[error("{0}")]
    Pcr(String),
    /// Persist chokepoint.
    #[error(transparent)]
    Persist(#[from] PersistError),
}
