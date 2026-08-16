//! [`safe_load_json`]: envelope + legacy list + `.bak` recovery.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::atomic::{atomic_write_bytes, safe_file_size_check};
use crate::envelope::{
    LoadResult, SAFE_LOAD_JSON_MAX_BYTES, extract_entries, record_observed_schema,
};
use crate::event::log_event;
use crate::save::{
    compact_utc_stamp, iter_timestamped_backups, legacy_bak_path, read_backup_bytes,
};

/// Load entries from `path`. Missing → empty; corrupt → try `.bak` then rotations.
#[must_use]
pub fn safe_load_json(path: &Path, label: &str) -> LoadResult {
    if !path.exists() {
        return recover_missing_main(path, label);
    }
    if let Err(reason) = safe_file_size_check(path, SAFE_LOAD_JSON_MAX_BYTES, label) {
        log::warn!("{}: {reason}", path.display());
        return LoadResult {
            entries: Vec::new(),
            warning: Some(reason),
        };
    }
    match load_extract(path, label) {
        Ok((entries, warning, raw)) => {
            record_observed_schema(path, &raw);
            LoadResult { entries, warning }
        }
        Err(main_warning) => recover_from_backups(path, label, Some(main_warning)),
    }
}

fn recover_missing_main(path: &Path, label: &str) -> LoadResult {
    let mut candidates = vec![legacy_bak_path(path)];
    let mut rotated = iter_timestamped_backups(path);
    rotated.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.extend(rotated.into_iter().map(|(p, _)| p));
    for cand in candidates {
        if !cand.exists() {
            continue;
        }
        if safe_file_size_check(&cand, SAFE_LOAD_JSON_MAX_BYTES, label).is_err() {
            continue;
        }
        let Ok(bytes) = read_backup_bytes(&cand) else {
            continue;
        };
        let Ok(raw) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let (Some(entries), _) = extract_entries(&raw, label) else {
            continue;
        };
        log::warn!(
            "{label}: main file {} is missing but recovered {} entries from backup {}",
            path.display(),
            entries.len(),
            cand.display()
        );
        let _ = atomic_write_bytes(path, &bytes);
        let bak_name = cand
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        return LoadResult {
            warning: Some(format!(
                "{label} main file was missing — restored {} entries from backup {bak_name}.",
                entries.len()
            )),
            entries,
        };
    }
    LoadResult {
        entries: Vec::new(),
        warning: None,
    }
}

fn recover_from_backups(path: &Path, label: &str, main_warning: Option<String>) -> LoadResult {
    let bak = legacy_bak_path(path);
    if bak.exists() {
        if safe_file_size_check(&bak, SAFE_LOAD_JSON_MAX_BYTES, label).is_err() {
            return LoadResult {
                entries: Vec::new(),
                warning: Some(main_warning.unwrap_or_else(|| {
                    format!("{label} is corrupt and the backup was rejected. Starting empty.")
                })),
            };
        }
        if let Ok(bytes) = fs::read(&bak)
            && let Ok(raw) = serde_json::from_slice::<Value>(&bytes)
            && let (Some(entries), _) = extract_entries(&raw, label)
        {
            record_observed_schema(path, &raw);
            preserve_corrupt_aside(path);
            let _ = atomic_write_bytes(path, &bytes);
            log_event(
                "persist.restored",
                &[("label", label), ("n", &entries.len().to_string())],
            );
            return LoadResult {
                warning: Some(format!(
                    "{label} was corrupt — restored {} entries from backup.",
                    entries.len()
                )),
                entries,
            };
        }
    }

    let mut rotated = iter_timestamped_backups(path);
    rotated.sort_by(|a, b| b.1.cmp(&a.1));
    for (chain_bak, _) in rotated {
        if safe_file_size_check(&chain_bak, SAFE_LOAD_JSON_MAX_BYTES, label).is_err() {
            continue;
        }
        let Ok(bytes) = read_backup_bytes(&chain_bak) else {
            continue;
        };
        let Ok(raw) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let (Some(entries), _) = extract_entries(&raw, label) else {
            continue;
        };
        record_observed_schema(path, &raw);
        preserve_corrupt_aside(path);
        let _ = atomic_write_bytes(path, &bytes);
        let name = chain_bak
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        return LoadResult {
            warning: Some(format!(
                "{label} and its .bak were corrupt — restored {} entries from rotated backup {name}.",
                entries.len()
            )),
            entries,
        };
    }

    LoadResult {
        entries: Vec::new(),
        warning: Some(main_warning.unwrap_or_else(|| {
            format!("{label} is corrupt and no valid backup was found. Starting empty.")
        })),
    }
}

fn load_extract(path: &Path, label: &str) -> Result<(Vec<Value>, Option<String>, Value), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{label} read failed: {e}"))?;
    let raw: Value = serde_json::from_str(&text).map_err(|_| format!("Corrupt {label} file"))?;
    match extract_entries(&raw, label) {
        (Some(entries), warning) => Ok((entries, warning, raw)),
        (None, warning) => {
            Err(warning.unwrap_or_else(|| format!("{label}: unexpected JSON shape")))
        }
    }
}

fn preserve_corrupt_aside(path: &Path) {
    let ts = compact_utc_stamp();
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".into());
    let mut aside = path.with_file_name(format!("{name}.corrupt-{ts}"));
    let mut bump = 0u32;
    while aside.exists() {
        bump += 1;
        aside = path.with_file_name(format!("{name}.corrupt-{ts}.{bump}"));
    }
    let _ = fs::rename(path, aside);
}
