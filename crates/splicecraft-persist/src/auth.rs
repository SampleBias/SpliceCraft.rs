//! Write/delete authorisation chokepoint.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::PersistError;

static PROCESS_AUTHORIZED: AtomicBool = AtomicBool::new(false);
static PROCESS_REASON: Mutex<String> = Mutex::new(String::new());

thread_local! {
    static THREAD_AUTHORIZED: Cell<bool> = const { Cell::new(false) };
    static CATASTROPHIC_SHRINK_DEPTH: Cell<u32> = const { Cell::new(0) };
    static MIRROR_SWAP_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Whether this thread or the process has opted in to data-dir writes.
#[must_use]
pub fn writes_authorized() -> bool {
    THREAD_AUTHORIZED.with(Cell::get) || PROCESS_AUTHORIZED.load(Ordering::SeqCst)
}

/// Process-wide opt-in for the real app (`main`, agent server).
pub fn authorize_writes(reason: impl Into<String>) {
    let reason = reason.into();
    PROCESS_AUTHORIZED.store(true, Ordering::SeqCst);
    if let Ok(mut slot) = PROCESS_REASON.lock() {
        *slot = reason;
    }
    THREAD_AUTHORIZED.with(|c| c.set(true));
}

/// Verifier / test entry: authorise **this thread** only, and only if `data_dir`
/// lives under the OS temp directory.
pub fn authorize_writes_for_sandbox(data_dir: &Path) -> Result<(), PersistError> {
    let tmp_root = canonicalize_or_clone(&std::env::temp_dir());
    let dd = canonicalize_or_clone(data_dir);
    if !dd.starts_with(&tmp_root) {
        return Err(PersistError::NotUnderTemp {
            data_dir: dd,
            tmp_root,
        });
    }
    THREAD_AUTHORIZED.with(|c| c.set(true));
    Ok(())
}

/// Drop this thread's sandbox authorisation (process-wide flag is unchanged).
pub fn revoke_thread_writes() {
    THREAD_AUTHORIZED.with(|c| c.set(false));
}

/// First line of every sanctioned write.
pub fn refuse_unauthorized_write(path: &Path, label: &str) -> Result<(), PersistError> {
    if writes_authorized() {
        Ok(())
    } else {
        Err(PersistError::Unauthorized {
            label: label.to_owned(),
            path: path.to_path_buf(),
        })
    }
}

/// First line of every sanctioned delete under the data dir.
pub fn refuse_unauthorized_delete(path: &Path, label: &str) -> Result<(), PersistError> {
    if writes_authorized() {
        Ok(())
    } else {
        Err(PersistError::UnauthorizedDelete {
            label: label.to_owned(),
            path: path.to_path_buf(),
        })
    }
}

/// Arm the L3 shrink-guard bypass for the lifetime of the returned guard.
#[must_use]
pub fn allow_catastrophic_shrink() -> CatastrophicShrinkGuard {
    CATASTROPHIC_SHRINK_DEPTH.with(|d| d.set(d.get().saturating_add(1)));
    CatastrophicShrinkGuard { _priv: () }
}

/// Arm the mirror-swap bypass (collection/bin switch: dropped entries live in a sibling file).
#[must_use]
pub fn expected_mirror_swap() -> MirrorSwapGuard {
    MIRROR_SWAP_DEPTH.with(|d| d.set(d.get().saturating_add(1)));
    MirrorSwapGuard { _priv: () }
}

/// RAII token for [`allow_catastrophic_shrink`].
#[derive(Debug)]
pub struct CatastrophicShrinkGuard {
    _priv: (),
}

impl Drop for CatastrophicShrinkGuard {
    fn drop(&mut self) {
        CATASTROPHIC_SHRINK_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// RAII token for [`expected_mirror_swap`].
#[derive(Debug)]
pub struct MirrorSwapGuard {
    _priv: (),
}

impl Drop for MirrorSwapGuard {
    fn drop(&mut self) {
        MIRROR_SWAP_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

pub(crate) fn catastrophic_shrink_armed() -> bool {
    CATASTROPHIC_SHRINK_DEPTH.with(|d| d.get() > 0)
}

pub(crate) fn mirror_swap_armed() -> bool {
    MIRROR_SWAP_DEPTH.with(|d| d.get() > 0)
}

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
