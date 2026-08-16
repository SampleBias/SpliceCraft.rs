//! Tempfile + fsync + replace. The only bytes-to-disk primitive.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::PersistError;

/// Atomically write `data` to `path` (tempfile in the same directory, fsync, replace).
///
/// Does **not** check write authorisation — callers that persist user data must
/// call [`crate::refuse_unauthorized_write`] first. [`crate::safe_save_json`]
/// is the sanctioned JSON path.
pub fn atomic_write_bytes(path: &Path, data: &[u8]) -> Result<(), PersistError> {
    refuse_symlink_target(path, "atomic write")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("data");
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".tmp")
        .tempfile_in(parent)?;
    tmp.write_all(data)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    let tmp_path = tmp.path().to_path_buf();
    match tmp.persist(path) {
        Ok(_) => {
            fsync_parent(path);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            Err(PersistError::Io(e.error))
        }
    }
}

/// UTF-8 counterpart of [`atomic_write_bytes`].
pub fn atomic_write_text(path: &Path, text: &str) -> Result<(), PersistError> {
    atomic_write_bytes(path, text.as_bytes())
}

/// Write `data` to a sibling tempfile and **do not** replace `path`.
///
/// Used to simulate a crash between write and `rename`. The previous file
/// stays intact. Caller should delete the tempfile when done.
pub fn stage_bytes_tempfile(path: &Path, data: &[u8]) -> Result<PathBuf, PersistError> {
    refuse_symlink_target(path, "staged write")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("data");
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".tmp")
        .tempfile_in(parent)?;
    tmp.write_all(data)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    let (_, tmp_path) = tmp.keep().map_err(|e| PersistError::Io(e.error))?;
    Ok(tmp_path)
}

pub(crate) fn refuse_symlink_chain(path: &Path, label: &str) -> Result<(), PersistError> {
    refuse_symlink_target(path, label)?;
    let mut cur = path.parent().unwrap_or_else(|| Path::new("."));
    let mut seen = 0u32;
    loop {
        match fs::symlink_metadata(cur) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(PersistError::Symlink {
                    label: label.to_owned(),
                    path: cur.to_path_buf(),
                });
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(PersistError::Commit(format!(
                    "refusing to save {label}: could not stat ancestor {}: {e}",
                    cur.display()
                )));
            }
        }
        let parent = cur.parent();
        match parent {
            Some(p) if p != cur && seen < 64 => {
                cur = p;
                seen += 1;
            }
            _ => break,
        }
    }
    Ok(())
}

fn refuse_symlink_target(path: &Path, label: &str) -> Result<(), PersistError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(PersistError::Symlink {
            label: label.to_owned(),
            path: path.to_path_buf(),
        }),
        Ok(_) | Err(_) => Ok(()),
    }
}

/// Size + regular-file check (symlink / FIFO / oversized → fail).
pub(crate) fn safe_file_size_check(path: &Path, max_bytes: u64, label: &str) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{label} could not stat: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{label} is a symlink — refusing for safety"));
    }
    if !meta.file_type().is_file() {
        return Err(format!("{label} is not a regular file"));
    }
    if meta.len() > max_bytes {
        return Err(format!(
            "{label} file is {} bytes (cap {max_bytes}); refusing to load",
            meta.len()
        ));
    }
    Ok(())
}

fn fsync_parent(path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
}
