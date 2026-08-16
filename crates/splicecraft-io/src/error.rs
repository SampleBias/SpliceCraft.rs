//! I/O errors. Never swallow — callers notify.

use std::io;
use std::path::PathBuf;

/// Failures from format detect, parse, export, or (opt-in) NCBI fetch.
#[derive(Debug, thiserror::Error)]
pub enum IoError {
    /// Empty or oversized GenBank text.
    #[error("{0}")]
    Parse(String),
    /// Path failed a size / symlink / format check.
    #[error("{0}")]
    Rejected(String),
    /// Accession failed [`crate::sanitize_accession`].
    #[error("invalid NCBI accession: {0:?}")]
    InvalidAccession(String),
    /// Default suite / demo: no egress.
    #[error("NCBI fetch is disabled (enable the `ncbi` feature and pass an online policy)")]
    NetworkDisabled,
    /// Host is not on the NCBI allowlist.
    #[error("refusing fetch to host {0:?}: not an allowlisted NCBI host")]
    HostNotAllowlisted(String),
    /// Literal or resolved address is loopback / RFC1918 / link-local / …
    #[error("refusing fetch to non-public address {0}")]
    NonPublicAddress(String),
    /// `.dna` / Commercial SaaS is deferred (stage 11).
    #[error("popular commercial .dna format is not implemented yet ({path})")]
    DnaDeferred { path: PathBuf },
    /// Filesystem error.
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl IoError {
    pub(crate) fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }

    pub(crate) fn rejected(msg: impl Into<String>) -> Self {
        Self::Rejected(msg.into())
    }
}
