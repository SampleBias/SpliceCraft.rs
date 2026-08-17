//! I/O errors. Never swallow — callers notify.

use std::io;

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
    /// `allow_online_search` is off — sequence upload is refused.
    #[error("online search is disabled until the allow_online_search setting is ticked")]
    OnlineDisabled,
    /// User (or test) cancelled an in-flight online poll.
    #[error("online search cancelled")]
    Cancelled,
    /// NCBI / EBI / HMM-DB client error (never includes sequence).
    #[error("{0}")]
    Online(String),
    /// Persist chokepoint refused a data-dir write.
    #[error("{0}")]
    UnauthorizedWrite(String),
    /// Host is not on the NCBI / search allowlist.
    #[error("refusing fetch to host {0:?}: not an allowlisted host")]
    HostNotAllowlisted(String),
    /// Literal or resolved address is loopback / RFC1918 / link-local / …
    #[error("refusing fetch to non-public address {0}")]
    NonPublicAddress(String),
    /// Pairwise alignment refused (empty / oversize / engine).
    #[error("{0}")]
    Align(String),
    /// Zip listing / extract refused.
    #[error("{0}")]
    Zip(String),
    /// AB1 / ABIF parse failure.
    #[error("{0}")]
    Ab1(String),
    /// Plasmidsaurus API / zip import.
    #[error("{0}")]
    Plasmidsaurus(String),
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

    pub(crate) fn align(msg: impl Into<String>) -> Self {
        Self::Align(msg.into())
    }

    pub(crate) fn zip(msg: impl Into<String>) -> Self {
        Self::Zip(msg.into())
    }

    pub(crate) fn ab1(msg: impl Into<String>) -> Self {
        Self::Ab1(msg.into())
    }

    pub(crate) fn plasmidsaurus(msg: impl Into<String>) -> Self {
        Self::Plasmidsaurus(msg.into())
    }

    pub(crate) fn online(msg: impl Into<String>) -> Self {
        Self::Online(msg.into())
    }
}
