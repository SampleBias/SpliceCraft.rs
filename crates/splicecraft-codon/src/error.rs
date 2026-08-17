//! Codon-optimizer errors. Messages stay sequence-free.

use thiserror::Error;

/// Failures from tables, optimize, TSV parse, and genome build.
#[derive(Debug, Error)]
pub enum CodonError {
    /// Amino acid missing from the usage table.
    #[error("No codons for amino acid '{0}' in this table")]
    NoCodons(char),
    /// Unknown optimizer strategy (notably `harmonize`).
    #[error("unknown codon mode {0} — expected one of 'frequency', 'max_cai'")]
    UnknownMode(String),
    /// Internal stop in a protein body.
    #[error("stop codon '*' is only allowed at the end of the protein")]
    InternalStop,
    /// TSV / FASTA parse failed.
    #[error("{0}")]
    Parse(String),
    /// Persist chokepoint failed.
    #[error("{0}")]
    Persist(#[from] splicecraft_persist::PersistError),
    /// JSON (de)serialise failed.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// Unknown host-hazard group.
    #[error("unknown host {0:?}")]
    UnknownHost(String),
    /// GC band inverted.
    #[error("min_gc ({min}) must not exceed max_gc ({max})")]
    InvertedGcBand {
        /// Requested floor.
        min: f64,
        /// Requested ceiling.
        max: f64,
    },
}

impl CodonError {
    pub(crate) fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }
}
