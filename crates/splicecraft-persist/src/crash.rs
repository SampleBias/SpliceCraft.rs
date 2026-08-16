//! Crash-recovery autosave slot. The TUI writer lands in stage 05.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use splicecraft_util::sanitize_filename;

use crate::atomic::atomic_write_text;
use crate::auth::refuse_unauthorized_write;
use crate::error::PersistError;
use crate::event::log_event;
use crate::paths::CRASH_RECOVERY_DIR_NAME;

/// Debounce used by the stage-05 editor before calling [`write_crash_recovery`].
pub const AUTOSAVE_DEBOUNCE: Duration = Duration::from_secs(3);

/// `<data_dir>/crash_recovery`.
#[must_use]
pub fn crash_recovery_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(CRASH_RECOVERY_DIR_NAME)
}

/// Per-record snapshot path: sanitized id + 12-hex hash, matching upstream.
#[must_use]
pub fn crash_recovery_path(data_dir: &Path, record_id: &str) -> Option<PathBuf> {
    if record_id.is_empty() {
        return None;
    }
    let safe = sanitize_recovery_id(record_id);
    if safe.is_empty() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    record_id.hash(&mut hasher);
    let h = hasher.finish() & 0x0000_ffff_ffff_ffff;
    Some(crash_recovery_dir(data_dir).join(format!("{safe}-{h:012x}.gb")))
}

/// Atomic `.gb` write through the authorisation gate. Stage 05 supplies the text.
pub fn write_crash_recovery(path: &Path, gb_text: &str) -> Result<(), PersistError> {
    refuse_unauthorized_write(path, "autosave")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write_text(path, gb_text)?;
    log_event("persist.autosave", &[("bytes", &gb_text.len().to_string())]);
    Ok(())
}

/// Delete a crash-recovery snapshot (after a real save / abandon).
pub fn clear_crash_recovery(path: &Path) -> Result<(), PersistError> {
    crate::auth::refuse_unauthorized_delete(path, "autosave")?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn sanitize_recovery_id(record_id: &str) -> String {
    let cleaned: String = sanitize_filename(record_id)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    cleaned.chars().take(80).collect()
}
