//! Cloning errors. Messages stay sequence-free.

use thiserror::Error;

/// Failures from ligation, Gibson, domestication, and syn-frag filing.
#[derive(Debug, Error)]
pub enum CloneError {
    /// Enzyme name missing from the catalog.
    #[error("unknown enzyme: {0}")]
    UnknownEnzyme(String),
    /// Type IIS cannot stamp a canonical overhang without the flanking bases.
    #[error(
        "{0} cuts outside its recognition site (Type IIS); use a literal digest, not a synthetic stamp"
    )]
    TypeIisSynthetic(String),
    /// Digest did not produce a usable insert / vector pair.
    #[error("{0}")]
    Digest(String),
    /// Overhangs do not ligate.
    #[error("{0}")]
    Incompatible(String),
    /// Gibson / Golden Gate / syn-frag refused the design.
    #[error("{0}")]
    Assembly(String),
    /// Grammar or part-type lookup failed.
    #[error("{0}")]
    Grammar(String),
    /// Persist chokepoint failed.
    #[error("{0}")]
    Persist(#[from] splicecraft_persist::PersistError),
    /// JSON (de)serialise failed.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl CloneError {
    pub(crate) fn digest(msg: impl Into<String>) -> Self {
        Self::Digest(msg.into())
    }

    pub(crate) fn assembly(msg: impl Into<String>) -> Self {
        Self::Assembly(msg.into())
    }

    pub(crate) fn grammar(msg: impl Into<String>) -> Self {
        Self::Grammar(msg.into())
    }
}
