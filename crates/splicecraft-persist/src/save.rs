//! [`safe_save_json`]: envelope + backup + shrink guard + atomic replace. [INV-07]

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use splicecraft_util::now;

use crate::atomic::{atomic_write_bytes, refuse_symlink_chain, stage_bytes_tempfile};
use crate::auth::{catastrophic_shrink_armed, mirror_swap_armed, refuse_unauthorized_write};
use crate::envelope::{
    BACKUP_MIN_KEEP, BACKUP_RETENTION_COUNT, BACKUP_TOTAL_SIZE_CAP_BYTES, CURRENT_SCHEMA_VERSION,
    SAFE_LOAD_JSON_MAX_BYTES, SAVE_READBACK_FULL_PARSE_MAX_BYTES, extract_entries,
    schema_version_for_save, wrap_envelope,
};
use crate::error::PersistError;
use crate::event::log_event;
use crate::paths::{LOST_ENTRIES_DIR_NAME, refuse_python_data_dir};

/// Optional knobs for [`safe_save_json_with`].
#[derive(Clone, Debug, Default)]
pub struct SaveOptions {
    /// Override the envelope schema stamp.
    pub schema_version: Option<i64>,
}

/// Atomically write `entries` as a schema envelope. [INV-07]
pub fn safe_save_json(path: &Path, entries: &[Value], label: &str) -> Result<(), PersistError> {
    safe_save_json_with(path, entries, label, &SaveOptions::default())
}

/// [`safe_save_json`] with an explicit schema stamp.
pub fn safe_save_json_with(
    path: &Path,
    entries: &[Value],
    label: &str,
    opts: &SaveOptions,
) -> Result<(), PersistError> {
    refuse_unauthorized_write(path, label)?;
    refuse_python_data_dir(path)?;
    refuse_symlink_chain(path, label)?;

    if path.exists() {
        let existing_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if existing_size > SAFE_LOAD_JSON_MAX_BYTES {
            return Err(PersistError::Oversized {
                label: label.to_owned(),
                size: existing_size,
                cap: SAFE_LOAD_JSON_MAX_BYTES,
            });
        }
    }

    let schema_version = schema_version_for_save(path, opts.schema_version);
    let (existing_count, prev_entries) = backup_prior(path, label)?;

    if existing_count > 0 && entries.len() < existing_count {
        apply_shrink_guard(
            path,
            entries,
            label,
            existing_count,
            prev_entries.as_deref(),
        )?;
    }

    let payload = wrap_envelope(entries, schema_version);
    let bytes = serde_json::to_vec_pretty(&payload)?;
    write_json_atomic(path, &bytes, entries.len(), label)?;
    prune_backups(path);
    log_event(
        "persist.saved",
        &[
            ("label", label),
            ("n", &entries.len().to_string()),
            ("schema", &schema_version.to_string()),
        ],
    );
    let _ = CURRENT_SCHEMA_VERSION;
    Ok(())
}

/// Write the envelope to a sibling tempfile **without** replacing `path`.
///
/// Simulates a crash between write and `os.replace`. The live file is untouched.
pub fn stage_json_tempfile(
    path: &Path,
    entries: &[Value],
    label: &str,
) -> Result<PathBuf, PersistError> {
    refuse_unauthorized_write(path, label)?;
    refuse_python_data_dir(path)?;
    let payload = wrap_envelope(entries, CURRENT_SCHEMA_VERSION);
    let bytes = serde_json::to_vec_pretty(&payload)?;
    stage_bytes_tempfile(path, &bytes)
}

fn backup_prior(path: &Path, label: &str) -> Result<(usize, Option<Vec<Value>>), PersistError> {
    if !path.exists() {
        return Ok((0, None));
    }
    let existing = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            return Err(PersistError::UnreadablePrior {
                label: label.to_owned(),
                path: path.to_path_buf(),
                source: e,
            });
        }
    };
    if existing.iter().all(u8::is_ascii_whitespace) {
        return Ok((0, None));
    }

    let skip_redundant = newest_timestamped_backup(path).is_some_and(|bak| {
        !bak.extension().is_some_and(|e| e == "gz")
            && fs::metadata(&bak)
                .map(|m| m.len() == existing.len() as u64)
                .unwrap_or(false)
            && fs::read(&bak).ok().as_deref() == Some(existing.as_slice())
    });

    if !skip_redundant {
        let ts = compact_utc_stamp();
        let mut bak_ts = path.with_file_name(format!("{}.bak.{ts}", file_name(path)));
        let mut bump = 0u32;
        while bak_ts.exists() {
            bump += 1;
            bak_ts = path.with_file_name(format!("{}.bak.{ts}.{bump}", file_name(path)));
        }
        atomic_write_bytes(&bak_ts, &existing).map_err(|e| {
            PersistError::Commit(format!(
                "backup rotation failed for {label} ({}): {e}. Save aborted.",
                path.display()
            ))
        })?;
        let bak_legacy = legacy_bak_path(path);
        atomic_write_bytes(&bak_legacy, &existing).map_err(|e| {
            PersistError::Commit(format!(
                "legacy .bak write failed for {label} ({}): {e}. Save aborted.",
                path.display()
            ))
        })?;
    }

    match serde_json::from_slice::<Value>(&existing) {
        Ok(prev) => {
            let (extracted, _) = extract_entries(&prev, label);
            if let Some(entries) = extracted {
                let n = entries.len();
                Ok((n, Some(entries)))
            } else {
                spill_raw_bytes(path, &existing, label, "extract-returned-none");
                Ok((0, None))
            }
        }
        Err(_) => {
            spill_raw_bytes(path, &existing, label, "invalid-json");
            Ok((0, None))
        }
    }
}

fn apply_shrink_guard(
    path: &Path,
    entries: &[Value],
    label: &str,
    existing_count: usize,
    prev_entries: Option<&[Value]>,
) -> Result<(), PersistError> {
    let mirror = mirror_swap_armed();
    log::warn!(
        "SHRINK GUARD: {label} is being overwritten with {} entries (was {existing_count}){}",
        entries.len(),
        if mirror {
            " [expected mirror swap]"
        } else {
            ""
        }
    );
    if mirror {
        return Ok(());
    }
    let suspicious = existing_count >= 5 && entries.len() < existing_count / 2;
    let catastrophic = existing_count >= 10 && entries.len() * 10 < existing_count;
    if suspicious && let Some(prev) = prev_entries {
        let lost = diff_lost_entries(prev, entries);
        if let Some(spilled) = spill_lost_entries(path, &lost, label) {
            log::warn!(
                "SHRINK GUARD: {} {label} entries dumped to {} before overwrite",
                lost.len(),
                spilled.display()
            );
        }
    }
    if catastrophic && !catastrophic_shrink_armed() {
        return Err(PersistError::CatastrophicShrink {
            label: label.to_owned(),
            from: existing_count,
            to: entries.len(),
        });
    }
    Ok(())
}

fn write_json_atomic(
    path: &Path,
    bytes: &[u8],
    n_entries: usize,
    label: &str,
) -> Result<(), PersistError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = file_name(path);
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".tmp")
        .tempfile_in(parent)?;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    let tmp_path = tmp.path().to_path_buf();
    validate_staged_json(&tmp_path, n_entries, label)?;
    match tmp.persist(path) {
        Ok(_) => {
            if let Some(parent) = path.parent()
                && let Ok(dir) = fs::File::open(parent)
            {
                let _ = dir.sync_all();
            }
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            Err(PersistError::Io(e.error))
        }
    }
}

fn validate_staged_json(tmp: &Path, n_entries: usize, label: &str) -> Result<(), PersistError> {
    let tmp_size = fs::metadata(tmp).map(|m| m.len()).unwrap_or(0);
    if tmp_size == 0 {
        return Err(PersistError::Commit(format!(
            "refusing to commit {label}: temp file is empty after write ({n_entries} entries expected)"
        )));
    }
    if tmp_size <= SAVE_READBACK_FULL_PARSE_MAX_BYTES {
        let reparsed: Value = serde_json::from_slice(&fs::read(tmp)?)?;
        let ok = reparsed
            .get("entries")
            .and_then(Value::as_array)
            .is_some_and(|a| a.len() == n_entries);
        if !ok {
            return Err(PersistError::Commit(format!(
                "refusing to commit {label}: temp-file read-back did not match the payload \
                 ({n_entries} entries written)"
            )));
        }
    } else {
        let data = fs::read(tmp)?;
        let tail_ok = data
            .iter()
            .rev()
            .find(|b| !b.is_ascii_whitespace())
            .copied()
            == Some(b'}');
        if !tail_ok {
            return Err(PersistError::Commit(format!(
                "refusing to commit {label}: temp file appears truncated"
            )));
        }
    }
    Ok(())
}

fn diff_lost_entries(prev: &[Value], new: &[Value]) -> Vec<Value> {
    let new_ids: Vec<&Value> = new
        .iter()
        .filter_map(|e| e.get("id"))
        .filter(|id| !id.is_null())
        .collect();
    let new_no_id: Vec<&Value> = new
        .iter()
        .filter(|e| e.get("id").is_none_or(Value::is_null))
        .collect();
    let mut lost = Vec::new();
    for e in prev {
        if !e.is_object() {
            lost.push(e.clone());
            continue;
        }
        match e.get("id") {
            Some(id) if !id.is_null() => {
                if !new_ids.contains(&id) {
                    lost.push(e.clone());
                }
            }
            _ => {
                if !new_no_id.contains(&e) {
                    lost.push(e.clone());
                }
            }
        }
    }
    lost
}

fn spill_lost_entries(path: &Path, lost: &[Value], label: &str) -> Option<PathBuf> {
    if lost.is_empty() {
        return None;
    }
    let lost_dir = path.parent()?.join(LOST_ENTRIES_DIR_NAME);
    if fs::create_dir_all(&lost_dir).is_err() {
        return None;
    }
    let ts = compact_utc_stamp();
    let stem = path.file_stem()?.to_string_lossy();
    let mut out = lost_dir.join(format!("{stem}-{ts}.json"));
    let mut bump = 0u32;
    while out.exists() {
        bump += 1;
        out = lost_dir.join(format!("{stem}-{ts}.{bump}.json"));
    }
    let payload = serde_json::json!({
        "_schema_version": CURRENT_SCHEMA_VERSION,
        "_label": label,
        "_recovered_from": path.to_string_lossy(),
        "_recovered_at": ts,
        "entries": lost,
    });
    let Ok(bytes) = serde_json::to_vec_pretty(&payload) else {
        return None;
    };
    atomic_write_bytes(&out, &bytes).ok()?;
    prune_lost_entries(&lost_dir);
    Some(out)
}

fn spill_raw_bytes(path: &Path, data: &[u8], label: &str, reason: &str) {
    let Some(parent) = path.parent() else {
        return;
    };
    let lost_dir = parent.join(LOST_ENTRIES_DIR_NAME);
    if fs::create_dir_all(&lost_dir).is_err() {
        return;
    }
    let ts = compact_utc_stamp();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let out = lost_dir.join(format!("{stem}-raw-{ts}{ext}"));
    if atomic_write_bytes(&out, data).is_ok() {
        log_event("persist.raw_spill", &[("label", label), ("reason", reason)]);
    }
}

fn prune_backups(path: &Path) {
    let mut candidates = iter_timestamped_backups(path);
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    for (old, _) in candidates.iter().skip(BACKUP_RETENTION_COUNT) {
        let _ = fs::remove_file(old);
    }
    let mut survivors = iter_timestamped_backups(path);
    survivors.sort_by(|a, b| b.1.cmp(&a.1));
    let mut total: u64 = survivors
        .iter()
        .filter_map(|(p, _)| fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    let mut idx = survivors.len();
    while total > BACKUP_TOTAL_SIZE_CAP_BYTES && idx > BACKUP_MIN_KEEP {
        idx -= 1;
        if let Some((victim, _)) = survivors.get(idx)
            && let Ok(meta) = fs::metadata(victim)
            && fs::remove_file(victim).is_ok()
        {
            total = total.saturating_sub(meta.len());
        }
    }
}

fn prune_lost_entries(lost_dir: &Path) {
    let mut files: Vec<(PathBuf, SystemTime)> = match fs::read_dir(lost_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .map(|p| {
                let m = fs::metadata(&p)
                    .and_then(|m| m.modified())
                    .unwrap_or(UNIX_EPOCH);
                (p, m)
            })
            .collect(),
        Err(_) => return,
    };
    files.sort_by_key(|b| std::cmp::Reverse(b.1));
    for (old, _) in files
        .iter()
        .skip(crate::envelope::LOST_ENTRIES_RETENTION_COUNT)
    {
        let _ = fs::remove_file(old);
    }
}

pub(crate) fn legacy_bak_path(path: &Path) -> PathBuf {
    path.with_file_name(format!("{}.bak", file_name(path)))
}

pub(crate) fn iter_timestamped_backups(path: &Path) -> Vec<(PathBuf, String)> {
    let parent = match path.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let prefix = format!("{}.bak.", file_name(path));
    let Ok(rd) = fs::read_dir(parent) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .filter_map(|p| {
            let key = p.file_name()?.to_string_lossy().into_owned();
            Some((p, key))
        })
        .collect()
}

fn newest_timestamped_backup(path: &Path) -> Option<PathBuf> {
    let mut baks = iter_timestamped_backups(path);
    baks.sort_by(|a, b| a.1.cmp(&b.1));
    baks.pop().map(|(p, _)| p)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".into())
}

pub(crate) fn compact_utc_stamp() -> String {
    let secs = now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, m, d, hh, mm, ss) = unix_to_utc_ymdhms(secs);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

fn unix_to_utc_ymdhms(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    const SECS_PER_DAY: u64 = 86_400;
    let days = (secs / SECS_PER_DAY) as i64;
    let rem = secs % SECS_PER_DAY;
    let hh = (rem / 3600) as u32;
    let mm = ((rem % 3600) / 60) as u32;
    let ss = (rem % 60) as u32;
    let (y, m, d) = civil_from_days(days);
    (y, m, d, hh, mm, ss)
}

/// Howard Hinnant `civil_from_days` (Unix epoch = days since 1970-01-01).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

pub(crate) fn read_backup_bytes(bak: &Path) -> io::Result<Vec<u8>> {
    fs::read(bak)
}
