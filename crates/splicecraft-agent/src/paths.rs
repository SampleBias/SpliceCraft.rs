//! Path sanitiser and symlink-hardening for agent file arguments.
//!
//! Ports `_sanitize_path` + `_check_agent_write_path` +
//! `_check_agent_read_path_ancestors`. `~otheruser` and `..` are refused.

use std::fs;
use std::path::{Path, PathBuf};

use splicecraft_util::{path_is_safe_under, sanitize_path};

/// Expand `~` for the current user only; refuse `~other` and `..`.
pub fn sanitize_agent_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("missing path".into());
    }
    let path = sanitize_path(trimmed).ok_or_else(|| {
        "invalid path (`~otheruser` expansion is refused; use an absolute path)".to_owned()
    })?;
    if !path_is_safe_under(&path) {
        return Err("path must not contain '..'".into());
    }
    Ok(path)
}

/// Refuse dest symlink, missing parent, and ancestor symlink redirects.
pub fn check_write_path(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(format!(
                "refusing to write through symlink at {}",
                path.display()
            ));
        }
        Ok(_) | Err(_) => {}
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        return Err(format!(
            "parent directory does not exist: {}",
            parent.display()
        ));
    }
    check_ancestors(parent)
}

/// Refuse existing ancestor components that are symlinks.
pub fn check_read_path(path: &Path) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        return Ok(());
    }
    check_ancestors(parent)
}

fn check_ancestors(parent: &Path) -> Result<(), String> {
    let resolved = parent
        .canonicalize()
        .map_err(|e| format!("could not resolve parent directory: {e}"))?;
    let lexical = lexical_absolute(parent)
        .map_err(|e| format!("could not normalise parent directory: {e}"))?;
    if resolved != lexical {
        return Err(format!(
            "parent path resolves through a symlink: {} → {}",
            lexical.display(),
            resolved.display()
        ));
    }
    let mut cur = parent.to_path_buf();
    let mut seen = std::collections::HashSet::new();
    loop {
        match fs::symlink_metadata(&cur) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "ancestor directory is a symlink: {}",
                    cur.display()
                ));
            }
            Ok(_) => {}
            Err(e) => return Err(format!("could not stat ancestor: {}: {e}", cur.display())),
        }
        let next = match cur.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
        if next == cur || !seen.insert(cur.clone()) {
            break;
        }
        cur = next;
    }
    Ok(())
}

fn lexical_absolute(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(normalize_dots(path))
    } else {
        Ok(normalize_dots(&std::env::current_dir()?.join(path)))
    }
}

fn normalize_dots(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Collapse `$HOME` to `~` so responses do not leak a username path shape.
#[must_use]
pub fn scrub_path(text: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return text.replace(&home, "~");
    }
    text.to_owned()
}
