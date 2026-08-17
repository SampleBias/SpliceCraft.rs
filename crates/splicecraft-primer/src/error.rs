//! Primer-design and export errors. Messages stay sequence-free.

use thiserror::Error;

/// Failures from designers, binding, and CSV export.
#[derive(Debug, Error)]
pub enum PrimerError {
    /// Empty included region.
    #[error("Target region is empty.")]
    EmptyRegion,
    /// Region shorter than the requested product.
    #[error("Region ({len} bp) is shorter than minimum product size ({min} bp).")]
    RegionShorter {
        /// Actual region length.
        len: usize,
        /// Requested minimum product.
        min: usize,
    },
    /// Region too short to anneal.
    #[error("Region too short (< 18 bp).")]
    RegionTooShort,
    /// No pair satisfied the product / Tm window.
    #[error("No valid primer pair for the given constraints.")]
    NoPair,
    /// Name missing from NEB ∪ custom catalog.
    #[error("Unknown enzyme: {0}")]
    UnknownEnzyme(String),
    /// Recognition site is empty or non-IUPAC.
    #[error("Invalid site sequence")]
    InvalidSite,
    /// Primer contained a non-DNA character.
    #[error("primer has non-DNA characters")]
    NonDna,
    /// Nothing to write.
    #[error("No primers to export.")]
    NothingToExport,
    /// Unknown CSV layout.
    #[error("unknown order_format {0:?} (expected 'generic' or 'idt')")]
    UnknownFormat(String),
    /// Catastrophic: refuse the whole order if any oligo is malformed.
    #[error(
        "Refusing to export — these primers have non-DNA characters in their oligo (fix them before ordering): {0}"
    )]
    MalformedOligos(String),
    /// Biology primitive failed (foreign char, empty site).
    #[error("{0}")]
    Bio(String),
    /// Mutagenesis / scrub designer refused the request.
    #[error("{0}")]
    Design(String),
}

impl From<splicecraft_bio::BioError> for PrimerError {
    fn from(e: splicecraft_bio::BioError) -> Self {
        match e {
            splicecraft_bio::BioError::NonIupac { .. } => Self::NonDna,
            other => Self::Bio(other.to_string()),
        }
    }
}
