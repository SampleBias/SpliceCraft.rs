//! Agent-API errors. Messages never include sequence payloads.

use splicecraft_persist::PersistError;

/// Failures from bind, token I/O, or session setup.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Persist / data-dir failure.
    #[error(transparent)]
    Persist(#[from] PersistError),
    /// Filesystem or bind error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Catch-all (token RNG, thread join, …).
    #[error("{0}")]
    Message(String),
}
