//! Migrate zip + Master Delete. [INV-07] / [INV-116]
//!
//! Import and wipe go through the write/delete chokepoint. Export writes a
//! user-chosen zip via [`crate::atomic_write_bytes`] (not `safe_save_json`).

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

use crate::atomic::atomic_write_bytes;
use crate::auth::{refuse_unauthorized_delete, refuse_unauthorized_write};
use crate::error::PersistError;
use crate::paths::{DataLayout, HMM_DATABASES_DIR_NAME, path_has_python_leaf};

/// Top-level marker member inside a migrate zip.
pub const MIGRATE_MARKER_NAME: &str = "splicecraft-migrate.json";
/// Snapshot directory inside the archive.
pub const MIGRATE_ARCHIVE_SUBDIR: &str = "data";
/// Current migrate format.
pub const MIGRATE_FORMAT_VERSION: u32 = 1;
/// Token the TUI / sandbox tests must pass. Not an agent endpoint.
pub const MASTER_DELETE_SENTINEL: &str = "splicecraft-rs-master-delete-v1";

/// Production uncompressed cap (64 GiB). Tests pass a smaller [`MigrateLimits`].
pub const MIGRATE_MAX_UNCOMPRESSED: u64 = 64 * 1024 * 1024 * 1024;
/// Production member-count cap.
pub const MIGRATE_MAX_MEMBERS: usize = 2_000_000;

static MASTER_DELETE_RUNNING: AtomicBool = AtomicBool::new(false);

/// Zip-bomb / member caps. Tests use [`MigrateLimits::for_tests`].
#[derive(Clone, Copy, Debug)]
pub struct MigrateLimits {
    /// Claimed + actual uncompressed bytes.
    pub max_uncompressed: u64,
    /// Central-directory member count.
    pub max_members: usize,
}

impl Default for MigrateLimits {
    fn default() -> Self {
        Self {
            max_uncompressed: MIGRATE_MAX_UNCOMPRESSED,
            max_members: MIGRATE_MAX_MEMBERS,
        }
    }
}

impl MigrateLimits {
    /// Tight caps for unit tests.
    #[must_use]
    pub fn for_tests() -> Self {
        Self {
            max_uncompressed: 8 * 1024 * 1024,
            max_members: 64,
        }
    }
}

/// Summary of a migrate export or import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrateReport {
    /// Destination (export) or source (import) path.
    pub path: PathBuf,
    /// Archive bytes (export) or restored files (import).
    pub bytes: u64,
    /// Files packed / restored.
    pub n_files: usize,
    /// Directories packed / restored.
    pub n_dirs: usize,
    /// Whether HMM download bytes were included.
    pub included_hmm: bool,
}

/// Summary of a Master Delete pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MasterDeleteReport {
    /// Regular files unlinked.
    pub files_removed: usize,
    /// Directories removed.
    pub dirs_removed: usize,
}

/// Export user data to `dest` (atomic tempfile + fsync + replace).
///
/// Read-only on the data dir. HMM download bytes are omitted unless
/// `include_hmm`.
pub fn export_migrate_archive(
    layout: &DataLayout,
    dest: &Path,
    include_hmm: bool,
) -> Result<MigrateReport, PersistError> {
    export_migrate_archive_with(layout, dest, include_hmm, MigrateLimits::default())
}

/// Export with explicit zip-bomb caps (tests).
pub fn export_migrate_archive_with(
    layout: &DataLayout,
    dest: &Path,
    include_hmm: bool,
    limits: MigrateLimits,
) -> Result<MigrateReport, PersistError> {
    if path_has_python_leaf(dest) {
        return Err(PersistError::PythonDataDir {
            path: dest.to_path_buf(),
        });
    }
    let snapshot = collect_snapshot(layout, include_hmm)?;
    if snapshot.files.len() + snapshot.dirs.len() > limits.max_members {
        return Err(PersistError::Migrate("too many snapshot members".into()));
    }
    let total: u64 = snapshot.files.iter().map(|f| f.bytes.len() as u64).sum();
    if total > limits.max_uncompressed {
        return Err(PersistError::Migrate(format!(
            "snapshot too large ({total} bytes; cap {})",
            limits.max_uncompressed
        )));
    }

    let marker = json!({
        "format": "splicecraft-migrate",
        "format_version": MIGRATE_FORMAT_VERSION,
        "snapshot_subdir": MIGRATE_ARCHIVE_SUBDIR,
        "included_hmm": include_hmm,
        "files": snapshot.files.iter().map(|f| json!({
            "name": f.rel,
            "sha256": f.sha256,
        })).collect::<Vec<_>>(),
        "directories": snapshot.dirs,
    });
    let marker_bytes = serde_json::to_vec_pretty(&marker)?;

    let mut buf = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zw.start_file(MIGRATE_MARKER_NAME, opts)
            .map_err(|e| PersistError::Migrate(e.to_string()))?;
        zw.write_all(&marker_bytes)?;
        for file in &snapshot.files {
            let arc = format!("{MIGRATE_ARCHIVE_SUBDIR}/{}", file.rel);
            if !is_safe_zip_member_name(&arc) {
                return Err(PersistError::Migrate(format!(
                    "refusing unsafe snapshot name {}",
                    file.rel
                )));
            }
            zw.start_file(&arc, opts)
                .map_err(|e| PersistError::Migrate(e.to_string()))?;
            zw.write_all(&file.bytes)?;
        }
        zw.finish()
            .map_err(|e| PersistError::Migrate(e.to_string()))?;
    }

    atomic_write_bytes(dest, &buf)?;
    Ok(MigrateReport {
        path: dest.to_path_buf(),
        bytes: buf.len() as u64,
        n_files: snapshot.files.len(),
        n_dirs: snapshot.dirs.len(),
        included_hmm: include_hmm,
    })
}

/// Import a migrate zip, replacing user data under `layout`.
pub fn import_migrate_archive(
    layout: &DataLayout,
    zip_path: &Path,
) -> Result<MigrateReport, PersistError> {
    import_migrate_archive_with(layout, zip_path, MigrateLimits::default())
}

/// Import with explicit zip-bomb caps (tests).
pub fn import_migrate_archive_with(
    layout: &DataLayout,
    zip_path: &Path,
    limits: MigrateLimits,
) -> Result<MigrateReport, PersistError> {
    refuse_unauthorized_write(&layout.root, "migrate-archive import")?;
    if path_has_python_leaf(&layout.root) {
        return Err(PersistError::PythonDataDir {
            path: layout.root.clone(),
        });
    }
    if !zip_path.is_file() {
        return Err(PersistError::Migrate(format!(
            "migrate archive not found: {}",
            zip_path.display()
        )));
    }

    let file = File::open(zip_path)?;
    let mut zf =
        ZipArchive::new(file).map_err(|e| PersistError::Migrate(format!("not a zip: {e}")))?;
    if zf.len() > limits.max_members {
        return Err(PersistError::Migrate(format!(
            "too many zip members ({})",
            zf.len()
        )));
    }

    let mut claimed = 0u64;
    for i in 0..zf.len() {
        let item = zf
            .by_index(i)
            .map_err(|e| PersistError::Migrate(e.to_string()))?;
        let name = normalize_zip_member(item.name());
        if !is_safe_zip_member_name(&name) {
            return Err(PersistError::Migrate(format!(
                "unsafe zip member name: {name:?}"
            )));
        }
        claimed = claimed.saturating_add(item.size());
        if claimed > limits.max_uncompressed {
            return Err(PersistError::Migrate(
                "zip uncompressed size exceeds cap (possible zip-bomb)".into(),
            ));
        }
    }

    let marker_raw = read_zip_member(&mut zf, MIGRATE_MARKER_NAME, limits.max_uncompressed)?;
    let marker: Value = serde_json::from_slice(&marker_raw)
        .map_err(|_| PersistError::Migrate("marker is corrupt".into()))?;
    if marker.get("format").and_then(Value::as_str) != Some("splicecraft-migrate") {
        return Err(PersistError::Migrate(
            "not a SpliceCraft migrate archive".into(),
        ));
    }
    let fmt = marker
        .get("format_version")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    if fmt > u64::from(MIGRATE_FORMAT_VERSION) {
        return Err(PersistError::Migrate(format!(
            "archive format v{fmt} is newer than this build (≤ v{MIGRATE_FORMAT_VERSION})"
        )));
    }
    let subdir = marker
        .get("snapshot_subdir")
        .and_then(Value::as_str)
        .unwrap_or(MIGRATE_ARCHIVE_SUBDIR);
    if subdir.is_empty() || subdir.contains('/') || subdir.contains('\\') || subdir == ".." {
        return Err(PersistError::Migrate("invalid snapshot_subdir".into()));
    }
    let included_hmm = marker
        .get("included_hmm")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let expected: Vec<(String, String)> = marker
        .get("files")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let name = e.get("name")?.as_str()?.to_owned();
                    let sha = e.get("sha256")?.as_str()?.to_owned();
                    Some((name, sha))
                })
                .collect()
        })
        .unwrap_or_default();

    let prefix = format!("{subdir}/");
    let mut restored_files = 0usize;
    let mut actual_bytes = 0u64;
    let n_dirs = marker
        .get("directories")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    for i in 0..zf.len() {
        let item = zf
            .by_index(i)
            .map_err(|e| PersistError::Migrate(e.to_string()))?;
        if item.is_dir() {
            continue;
        }
        let name = normalize_zip_member(item.name());
        if name == MIGRATE_MARKER_NAME {
            continue;
        }
        if !name.starts_with(&prefix) {
            return Err(PersistError::Migrate(format!(
                "member outside snapshot subdir: {name}"
            )));
        }
        let rel = &name[prefix.len()..];
        if rel.is_empty() || !is_safe_zip_member_name(rel) {
            return Err(PersistError::Migrate(format!(
                "unsafe snapshot member: {rel:?}"
            )));
        }
        let cap = limits.max_uncompressed as usize;
        let mut raw = Vec::new();
        item.take(cap as u64 + 1)
            .read_to_end(&mut raw)
            .map_err(|e| PersistError::Migrate(e.to_string()))?;
        if raw.len() > cap {
            return Err(PersistError::Migrate(
                "member exceeded cap during decompression — possible zip-bomb".into(),
            ));
        }
        actual_bytes = actual_bytes.saturating_add(raw.len() as u64);
        if actual_bytes > limits.max_uncompressed {
            return Err(PersistError::Migrate(
                "actual uncompressed bytes exceed cap".into(),
            ));
        }
        let digest = sha256_hex(&raw);
        if let Some((_, expect)) = expected.iter().find(|(n, _)| n == rel)
            && expect != &digest
        {
            return Err(PersistError::Migrate(format!("sha256 mismatch for {rel}")));
        }
        let dest = layout.root.join(rel);
        if !dest.starts_with(&layout.root) {
            return Err(PersistError::Migrate(format!(
                "refusing restore outside data dir: {rel}"
            )));
        }
        refuse_unauthorized_write(&dest, "migrate-archive restore")?;
        atomic_write_bytes(&dest, &raw)?;
        restored_files += 1;
    }

    Ok(MigrateReport {
        path: zip_path.to_path_buf(),
        bytes: actual_bytes,
        n_files: restored_files,
        n_dirs,
        included_hmm,
    })
}

/// Every file Master Delete will try to unlink (including `.bak` siblings).
#[must_use]
pub fn master_delete_file_targets(layout: &DataLayout) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in layout
        .user_data_files()
        .into_iter()
        .chain(layout.operational_files())
    {
        out.push(p.clone());
        let bak = bak_sibling(&p);
        out.push(bak);
        if let Some(parent) = p.parent()
            && parent.is_dir()
            && let Ok(rd) = fs::read_dir(parent)
        {
            let prefix = format!(
                "{}.bak.",
                p.file_name().and_then(|s| s.to_str()).unwrap_or("")
            );
            for ent in rd.flatten() {
                let name = ent.file_name();
                if name.to_string_lossy().starts_with(&prefix) {
                    out.push(ent.path());
                }
            }
        }
    }
    out
}

/// Directories Master Delete removes (not `logs/`, not the data-dir root).
#[must_use]
pub fn master_delete_dir_targets(layout: &DataLayout) -> Vec<PathBuf> {
    let mut out = layout.user_data_dirs();
    for name in ["snapshots", "clipboard"] {
        out.push(layout.root.join(name));
    }
    out
}

/// Wipe user data under `layout`. Requires the sentinel and write authorisation.
///
/// Preserves `splicecraft.lock` and `logs/`. Tests must pass a sandboxed layout.
pub fn perform_master_delete(
    layout: &DataLayout,
    sentinel: &str,
) -> Result<MasterDeleteReport, PersistError> {
    if sentinel != MASTER_DELETE_SENTINEL {
        return Err(PersistError::SentinelMismatch);
    }
    refuse_unauthorized_delete(&layout.root, "master-delete")?;
    if path_has_python_leaf(&layout.root) {
        return Err(PersistError::PythonDataDir {
            path: layout.root.clone(),
        });
    }
    if !MASTER_DELETE_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        return Err(PersistError::MasterDelete(
            "master delete already in progress".into(),
        ));
    }
    let result = perform_master_delete_inner(layout);
    MASTER_DELETE_RUNNING.store(false, Ordering::SeqCst);
    result
}

fn perform_master_delete_inner(layout: &DataLayout) -> Result<MasterDeleteReport, PersistError> {
    let mut report = MasterDeleteReport::default();
    let lock = layout.lock_file();
    let logs = layout.log_dir();

    for p in master_delete_file_targets(layout) {
        if same_path(&p, &lock) {
            continue;
        }
        if remove_if_exists(&p)? {
            report.files_removed += 1;
        }
    }
    for p in master_delete_dir_targets(layout) {
        if same_path(&p, &logs) {
            continue;
        }
        if remove_dir_if_exists(&p)? {
            report.dirs_removed += 1;
        }
    }

    if layout.root.is_dir()
        && let Ok(rd) = fs::read_dir(&layout.root)
    {
        for ent in rd.flatten() {
            let p = ent.path();
            if same_path(&p, &lock) || same_path(&p, &logs) {
                continue;
            }
            let ft = match fs::symlink_metadata(&p) {
                Ok(m) => m.file_type(),
                Err(_) => continue,
            };
            if ft.is_dir() {
                if remove_dir_if_exists(&p)? {
                    report.dirs_removed += 1;
                }
            } else if remove_if_exists(&p)? {
                report.files_removed += 1;
            }
        }
    }
    Ok(report)
}

struct SnapFile {
    rel: String,
    bytes: Vec<u8>,
    sha256: String,
}

struct Snapshot {
    files: Vec<SnapFile>,
    dirs: Vec<String>,
}

fn collect_snapshot(layout: &DataLayout, include_hmm: bool) -> Result<Snapshot, PersistError> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for p in layout.user_data_files() {
        if p.is_file() {
            push_file(&mut files, &layout.root, &p)?;
        }
    }
    for p in layout.user_data_dirs() {
        if !p.is_dir() {
            continue;
        }
        if !include_hmm && p.file_name().and_then(|s| s.to_str()) == Some(HMM_DATABASES_DIR_NAME) {
            continue;
        }
        let rel = rel_posix(&layout.root, &p)?;
        dirs.push(rel);
        walk_dir_files(&layout.root, &p, &mut files)?;
    }
    Ok(Snapshot { files, dirs })
}

fn walk_dir_files(root: &Path, dir: &Path, files: &mut Vec<SnapFile>) -> Result<(), PersistError> {
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for ent in rd {
        let ent = ent?;
        let p = ent.path();
        let ft = ent.file_type()?;
        if ft.is_dir() {
            walk_dir_files(root, &p, files)?;
        } else if ft.is_file() {
            push_file(files, root, &p)?;
        }
    }
    Ok(())
}

fn push_file(files: &mut Vec<SnapFile>, root: &Path, path: &Path) -> Result<(), PersistError> {
    let bytes = fs::read(path)?;
    files.push(SnapFile {
        rel: rel_posix(root, path)?,
        sha256: sha256_hex(&bytes),
        bytes,
    });
    Ok(())
}

fn rel_posix(root: &Path, path: &Path) -> Result<String, PersistError> {
    let rel = path.strip_prefix(root).map_err(|_| {
        PersistError::Migrate(format!("path {} not under data dir", path.display()))
    })?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn bak_sibling(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    path.with_file_name(format!("{name}.bak"))
}

fn same_path(a: &Path, b: &Path) -> bool {
    let ca = fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

fn remove_if_exists(path: &Path) -> Result<bool, PersistError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() || meta.file_type().is_symlink() => {
            fs::remove_file(path)?;
            Ok(true)
        }
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<bool, PersistError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => {
            fs::remove_dir_all(path)?;
            Ok(true)
        }
        Ok(meta) if meta.file_type().is_symlink() => {
            fs::remove_file(path)?;
            Ok(true)
        }
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn normalize_zip_member(name: &str) -> String {
    name.replace('\\', "/")
}

/// Same contract as `splicecraft_io::is_safe_zip_member_name` (persist cannot
/// import io).
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

fn read_zip_member<R: Read + std::io::Seek>(
    zf: &mut ZipArchive<R>,
    name: &str,
    max: u64,
) -> Result<Vec<u8>, PersistError> {
    let item = zf
        .by_name(name)
        .map_err(|_| PersistError::Migrate(format!("missing {name}")))?;
    let mut raw = Vec::new();
    item.take(max + 1)
        .read_to_end(&mut raw)
        .map_err(|e| PersistError::Migrate(e.to_string()))?;
    if raw.len() as u64 > max {
        return Err(PersistError::Migrate("marker too large".into()));
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{authorize_writes_for_sandbox, revoke_thread_writes};
    use crate::domain::{load_library, save_library};
    use serde_json::json;
    use std::time::SystemTime;

    fn sandbox() -> (tempfile::TempDir, DataLayout) {
        let tmp = tempfile::tempdir().expect("tempdir");
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        (tmp, layout)
    }

    fn dirs_home() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/home/nobody"))
    }

    fn probe_share(leaf: &str) -> (PathBuf, bool, Option<SystemTime>) {
        let p = dirs_home().join(".local/share").join(leaf);
        let exists = p.exists();
        let mtime = fs::metadata(&p).ok().and_then(|m| m.modified().ok());
        (p, exists, mtime)
    }

    #[test]
    fn migrate_zip_round_trip_in_tempdirs() {
        let (tmp, layout) = sandbox();
        assert!(layout.root.starts_with(tmp.path()));
        save_library(&layout, &[json!({"id": "pUC19", "name": "pUC19"})]).unwrap();
        fs::create_dir_all(layout.crash_recovery_dir()).unwrap();
        fs::write(layout.crash_recovery_dir().join("note.gb"), b"LOCUS x\n").unwrap();

        let dest = tmp.path().join("bundle.zip");
        let exported =
            export_migrate_archive_with(&layout, &dest, false, MigrateLimits::for_tests()).unwrap();
        assert!(dest.exists());
        assert!(exported.n_files >= 1);
        assert!(!dest.starts_with(&layout.root));

        save_library(&layout, &[json!({"id": "gone"})]).unwrap();
        assert_eq!(load_library(&layout).entries[0]["id"], "gone");

        let imported =
            import_migrate_archive_with(&layout, &dest, MigrateLimits::for_tests()).unwrap();
        assert!(imported.n_files >= 1);
        assert_eq!(load_library(&layout).entries[0]["id"], "pUC19");
        assert!(layout.crash_recovery_dir().join("note.gb").exists());
        assert!(layout.root.starts_with(tmp.path()));
    }

    #[test]
    fn migrate_import_refuses_unauthorized() {
        let (tmp, layout) = sandbox();
        save_library(&layout, &[json!({"id": "keep"})]).unwrap();
        let dest = tmp.path().join("bundle.zip");
        export_migrate_archive_with(&layout, &dest, false, MigrateLimits::for_tests()).unwrap();
        revoke_thread_writes();
        let err = import_migrate_archive_with(&layout, &dest, MigrateLimits::for_tests())
            .expect_err("unauth");
        assert!(matches!(err, PersistError::Unauthorized { .. }));
        authorize_writes_for_sandbox(tmp.path()).unwrap();
        assert_eq!(load_library(&layout).entries[0]["id"], "keep");
    }

    #[test]
    fn migrate_refuses_traversal_member() {
        assert!(!is_safe_zip_member_name("../escape.json"));
        assert!(!is_safe_zip_member_name("/etc/passwd"));
        assert!(!is_safe_zip_member_name("C:/Windows/x"));
        assert!(is_safe_zip_member_name("data/plasmid_library.json"));
    }

    #[test]
    fn master_delete_wrong_sentinel_touches_no_disk() {
        let (tmp, layout) = sandbox();
        save_library(&layout, &[json!({"id": "keep"})]).unwrap();
        let before = fs::read(layout.library_file()).unwrap();
        let err = perform_master_delete(&layout, "yes").expect_err("sentinel");
        assert!(matches!(err, PersistError::SentinelMismatch));
        assert_eq!(fs::read(layout.library_file()).unwrap(), before);
        assert!(layout.root.starts_with(tmp.path()));
    }

    #[test]
    fn master_delete_sandbox_leaves_real_home_untouched() {
        let (python, python_existed, python_mtime) = probe_share("splicecraft");
        let (rust_share, rust_existed, rust_mtime) = probe_share("splicecraft-rs");

        let (tmp, layout) = sandbox();
        assert!(layout.root.starts_with(tmp.path()));
        assert!(
            !layout.root.starts_with(&rust_share),
            "sandbox must not be the real rust data dir"
        );
        save_library(&layout, &[json!({"id": "wipe-me"})]).unwrap();
        fs::write(layout.agent_token_file(), b"token\n").unwrap();
        fs::write(layout.lock_file(), b"lock\n").unwrap();
        fs::create_dir_all(layout.log_dir()).unwrap();
        fs::write(layout.log_dir().join("app.log"), b"log\n").unwrap();
        fs::create_dir_all(layout.crash_recovery_dir()).unwrap();
        fs::write(layout.crash_recovery_dir().join("x.gb"), b"LOCUS\n").unwrap();

        let report = perform_master_delete(&layout, MASTER_DELETE_SENTINEL).unwrap();
        assert!(report.files_removed >= 1);
        assert!(!layout.library_file().exists());
        assert!(!layout.agent_token_file().exists());
        assert!(!layout.crash_recovery_dir().exists());
        assert!(layout.lock_file().exists(), "lockfile must survive");
        assert!(layout.log_dir().join("app.log").exists(), "logs survive");

        assert_eq!(python.exists(), python_existed);
        assert_eq!(rust_share.exists(), rust_existed);
        assert_eq!(
            fs::metadata(&python).ok().and_then(|m| m.modified().ok()),
            python_mtime
        );
        assert_eq!(
            fs::metadata(&rust_share)
                .ok()
                .and_then(|m| m.modified().ok()),
            rust_mtime
        );
        assert!(layout.root.starts_with(tmp.path()));
    }

    #[test]
    fn master_delete_unauthorized_refused() {
        let (tmp, layout) = sandbox();
        save_library(&layout, &[json!({"id": "keep"})]).unwrap();
        revoke_thread_writes();
        let err = perform_master_delete(&layout, MASTER_DELETE_SENTINEL).expect_err("unauth");
        assert!(matches!(err, PersistError::UnauthorizedDelete { .. }));
        authorize_writes_for_sandbox(tmp.path()).unwrap();
        assert!(layout.library_file().exists());
    }
}
