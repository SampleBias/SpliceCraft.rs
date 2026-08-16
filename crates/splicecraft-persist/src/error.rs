//! Persist errors. Callers must surface these — never swallow. [INV-07]

use std::io;
use std::path::PathBuf;

/// Failures from the save/load chokepoint.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// Process has not opted in via [`crate::authorize_writes`] or the sandbox helper.
    #[error(
        "refusing to write {label:?} → {path}: data-dir writes are not authorised in this process. \
         Sandbox XDG_DATA_HOME to a temp dir and call authorize_writes_for_sandbox"
    )]
    Unauthorized { label: String, path: PathBuf },
    /// Same gate for unlink/rmdir under the data dir.
    #[error(
        "refusing to delete {label:?} → {path}: data-dir deletes are not authorised in this process"
    )]
    UnauthorizedDelete { label: String, path: PathBuf },
    /// >90% entry loss on a populated file, unless [`crate::allow_catastrophic_shrink`] is armed.
    #[error(
        "refusing to write {label:?}: catastrophic shrink ({from} → {to} entries). \
         Wrap the call in allow_catastrophic_shrink() if this is intentional"
    )]
    CatastrophicShrink {
        label: String,
        from: usize,
        to: usize,
    },
    /// Target or ancestor is a symlink (would redirect the write).
    #[error("refusing to save {label}: symlink at {path}")]
    Symlink { label: String, path: PathBuf },
    /// Existing file is larger than the load cap; overwriting would nuke unread data.
    #[error("refusing to overwrite oversized {label} ({size} bytes > {cap} cap)")]
    Oversized { label: String, size: u64, cap: u64 },
    /// [`crate::authorize_writes_for_sandbox`] was pointed at a non-temp path.
    #[error("refusing to authorise writes against {data_dir}: not under {tmp_root}")]
    NotUnderTemp {
        data_dir: PathBuf,
        tmp_root: PathBuf,
    },
    /// Tests must set `XDG_DATA_HOME` before resolving the data dir.
    #[error("tests must set XDG_DATA_HOME to a temp dir before resolving the data dir")]
    UnsandboxedTest,
    /// Resolved data dir leaf is not [`crate::XDG_DATA_DIR_LEAF`].
    #[error("data-dir leaf must be splicecraft-rs, not {found:?}")]
    WrongLeaf { found: String },
    /// Path would write into the Python app's `splicecraft/` data dir.
    #[error("refusing to write through the Python data-dir leaf splicecraft: {path}")]
    PythonDataDir { path: PathBuf },
    /// Prior file exists but could not be read for backup — refuse rather than clobber.
    #[error(
        "refusing to save {label}: prior file at {path} exists but could not be backed up ({source})"
    )]
    UnreadablePrior {
        label: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Tempfile was empty or failed read-back validation.
    #[error("{0}")]
    Commit(String),
    /// Filesystem error (disk full, permission, …).
    #[error(transparent)]
    Io(#[from] io::Error),
    /// JSON (de)serialise failure during a save.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
