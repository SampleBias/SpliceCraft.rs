//! Safe zip listing / extract. Path traversal is refused before any read.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use crate::error::IoError;

/// Whole-archive cap (upstream `_PLASMIDSAURUS_ZIP_MAX_BYTES`).
pub const ZIP_MAX_BYTES: u64 = 500 * 1024 * 1024;
/// Per-member uncompressed cap.
pub const ZIP_MEMBER_MAX_BYTES: u64 = 50 * 1024 * 1024;
/// Listing cap.
pub const ZIP_MAX_MEMBERS: usize = 2000;

const GBK_EXTS: &[&str] = &[".gbk", ".gb", ".genbank"];

/// One GenBank-shaped member inside a zip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipMember {
    /// Central-directory name (forward slashes).
    pub name: String,
    /// Declared uncompressed size.
    pub size: u64,
}

/// True when `name` is safe to surface and to pass back into the archive.
#[must_use]
pub fn is_safe_zip_member_name(name: &str) -> bool {
    if name.is_empty() || name.contains('\0') || name.contains('\u{1b}') {
        return false;
    }
    if name.chars().any(|c| {
        let o = c as u32;
        o < 0x20 || (0x7f..=0x9f).contains(&o)
    }) {
        return false;
    }
    let bytes = name.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let norm = name.replace('\\', "/");
    if norm.starts_with('/') {
        return false;
    }
    !norm.split('/').any(|p| p == ".." || p == ".")
}

/// Fold Windows separators.
#[must_use]
pub fn normalize_zip_member(name: &str) -> String {
    name.replace('\\', "/")
}

fn open_capped(path: &Path) -> Result<File, IoError> {
    let file = File::open(path).map_err(|e| IoError::zip(format!("could not open zip: {e}")))?;
    let meta = file
        .metadata()
        .map_err(|e| IoError::zip(format!("could not stat zip: {e}")))?;
    if !meta.file_type().is_file() {
        return Err(IoError::zip(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    if meta.len() > ZIP_MAX_BYTES {
        return Err(IoError::zip(format!(
            "zip too large ({} bytes; cap {ZIP_MAX_BYTES})",
            meta.len()
        )));
    }
    Ok(file)
}

/// List `.gbk` / `.gb` / `.genbank` members. Unsafe names are skipped, not extracted.
pub fn list_gbk_members_in_zip(path: &Path) -> Result<Vec<ZipMember>, IoError> {
    let file = open_capped(path)?;
    let mut zf =
        ZipArchive::new(file).map_err(|e| IoError::zip(format!("could not open zip: {e}")))?;
    let mut members = Vec::new();
    for i in 0..zf.len() {
        if members.len() >= ZIP_MAX_MEMBERS {
            break;
        }
        let item = zf
            .by_index(i)
            .map_err(|e| IoError::zip(format!("could not read zip: {e}")))?;
        if item.is_dir() {
            continue;
        }
        let name = normalize_zip_member(item.name());
        if !is_safe_zip_member_name(&name) {
            continue;
        }
        let base = name.rsplit('/').next().unwrap_or(&name);
        if base.is_empty() || base.starts_with('.') {
            continue;
        }
        let low = name.to_ascii_lowercase();
        if !GBK_EXTS.iter().any(|e| low.ends_with(e)) {
            continue;
        }
        if item.size() > ZIP_MEMBER_MAX_BYTES {
            continue;
        }
        members.push(ZipMember {
            name,
            size: item.size(),
        });
    }
    members.sort_by_key(|a| splicecraft_util::natural_sort_key(&a.name));
    Ok(members)
}

/// Read one member as text (UTF-8, latin-1 fallback). Refuses traversal names.
pub fn extract_zip_member(path: &Path, member_name: &str) -> Result<Vec<u8>, IoError> {
    if !is_safe_zip_member_name(member_name) {
        return Err(IoError::zip(format!(
            "unsafe zip member name: {member_name:?}"
        )));
    }
    let file = open_capped(path)?;
    let mut zf =
        ZipArchive::new(file).map_err(|e| IoError::zip(format!("could not read zip: {e}")))?;
    let idx = find_member_index(&mut zf, member_name)?;
    let item = zf
        .by_index(idx)
        .map_err(|e| IoError::zip(format!("could not read zip: {e}")))?;
    let size = item.size();
    if size > ZIP_MEMBER_MAX_BYTES {
        return Err(IoError::zip(format!(
            "member too large ({size} bytes; cap {ZIP_MEMBER_MAX_BYTES})"
        )));
    }
    let cap = ZIP_MEMBER_MAX_BYTES as usize;
    let mut raw = Vec::new();
    item.take(cap as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| IoError::zip(format!("could not read zip: {e}")))?;
    if raw.len() > cap {
        return Err(IoError::zip(format!(
            "member exceeded cap during decompression (claimed {size} bytes; cap {cap}) — possible zip-bomb"
        )));
    }
    Ok(raw)
}

/// Decode a GenBank member (UTF-8 then latin-1).
pub fn extract_gbk_member(path: &Path, member_name: &str) -> Result<String, IoError> {
    let raw = extract_zip_member(path, member_name)?;
    match String::from_utf8(raw.clone()) {
        Ok(s) => Ok(s),
        Err(_) => Ok(raw.into_iter().map(|b| b as char).collect()),
    }
}

fn find_member_index<R: Read + std::io::Seek>(
    zf: &mut ZipArchive<R>,
    name: &str,
) -> Result<usize, IoError> {
    let target = normalize_zip_member(name);
    for i in 0..zf.len() {
        let item = zf
            .by_index(i)
            .map_err(|e| IoError::zip(format!("could not read zip: {e}")))?;
        if normalize_zip_member(item.name()) == target {
            return Ok(i);
        }
    }
    Err(IoError::zip(format!("member not in zip: {name:?}")))
}

/// Bounded per-base TSV coverage summary (`reads_all` in column 2).
pub fn summarize_perbase_tsv(text: &str) -> std::collections::BTreeMap<String, f64> {
    let mut total = 0.0;
    let mut n = 0.0;
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    let mut above_20x = 0.0;
    let mut header_seen = false;
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 3 {
            continue;
        }
        if !header_seen {
            header_seen = true;
            if cols[2].parse::<f64>().is_err() {
                continue;
            }
        }
        let Ok(v) = cols[2].parse::<f64>() else {
            continue;
        };
        n += 1.0;
        total += v;
        lo = lo.min(v);
        hi = hi.max(v);
        if v >= 20.0 {
            above_20x += 1.0;
        }
    }
    let mut out = std::collections::BTreeMap::new();
    if n > 0.0 {
        out.insert("mean".into(), total / n);
        out.insert("min".into(), lo);
        out.insert("max".into(), hi);
        out.insert("n_pos".into(), n);
        out.insert("above_20x".into(), above_20x);
    }
    out
}

/// Create a zip at `path` (tests).
pub fn write_test_zip(path: &Path, members: &[(&str, &[u8])]) -> Result<(), IoError> {
    let file = File::create(path)?;
    let mut zw = zip::ZipWriter::new(file);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, body) in members {
        zw.start_file(*name, opts)
            .map_err(|e| IoError::zip(e.to_string()))?;
        std::io::Write::write_all(&mut zw, body)?;
    }
    zw.finish().map_err(|e| IoError::zip(e.to_string()))?;
    Ok(())
}
