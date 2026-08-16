//! Atomic JSON persistence, backups, and the data-safety chokepoint.
//!
//! Sacred invariant 7 lives here. See `docs/invariants.md` and
//! `docs/stages/02-persist.md`.

#![cfg_attr(not(test), forbid(unsafe_code))]

pub use splicecraft_util as util;

mod atomic;
mod auth;
mod crash;
mod domain;
mod envelope;
mod error;
mod event;
mod load;
mod paths;
mod save;

pub use atomic::{atomic_write_bytes, atomic_write_text, stage_bytes_tempfile};
pub use auth::{
    CatastrophicShrinkGuard, MirrorSwapGuard, allow_catastrophic_shrink, authorize_writes,
    authorize_writes_for_sandbox, expected_mirror_swap, refuse_unauthorized_delete,
    refuse_unauthorized_write, revoke_thread_writes, writes_authorized,
};
pub use crash::{
    AUTOSAVE_DEBOUNCE, clear_crash_recovery, crash_recovery_dir, crash_recovery_path,
    write_crash_recovery,
};
pub use domain::{
    load_collections, load_features, load_library, load_parts_bin, load_primers, save_collections,
    save_features, save_library, save_parts_bin, save_primers,
};
pub use envelope::{
    BACKUP_RETENTION_COUNT, CURRENT_SCHEMA_VERSION, LoadResult, SAFE_LOAD_JSON_MAX_BYTES,
    extract_entries,
};
pub use error::PersistError;
pub use event::{format_event, log_event};
pub use load::safe_load_json;
pub use paths::{
    COLLECTIONS_FILE_NAME, CRASH_RECOVERY_DIR_NAME, DataLayout, FEATURES_FILE_NAME,
    LIBRARY_FILE_NAME, LOG_DIR_NAME, LOST_ENTRIES_DIR_NAME, PARTS_BIN_FILE_NAME, PRIMERS_FILE_NAME,
    PYTHON_XDG_DATA_DIR_LEAF, SETTINGS_FILE_NAME, check_leaf, data_dir, join_leaf,
    path_has_python_leaf,
};
pub use save::{SaveOptions, safe_save_json, safe_save_json_with, stage_json_tempfile};

/// Stage that implements this crate's real save engine.
pub const IMPLEMENTATION_STAGE: u8 = 2;

/// XDG data-directory leaf for this rewrite.
///
/// Must never be `splicecraft` — that path belongs to the Python SpliceCraft
/// app. Sharing it would put years of user plasmids one bug away from a
/// Rust-side overwrite.
pub const XDG_DATA_DIR_LEAF: &str = "splicecraft-rs";

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn sandbox() -> (tempfile::TempDir, DataLayout) {
        let tmp = tempfile::tempdir().expect("tempdir");
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox auth");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        (tmp, layout)
    }

    fn entries(n: usize) -> Vec<Value> {
        (0..n).map(|i| json!({"id": i.to_string()})).collect()
    }

    fn with_xdg<R>(xdg: Option<&Path>, f: impl FnOnce() -> R) -> R {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("XDG_DATA_HOME");
        unsafe {
            match xdg {
                Some(p) => std::env::set_var("XDG_DATA_HOME", p),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
        }
        let out = f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var("XDG_DATA_HOME", v),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
        }
        out
    }

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-persist");
    }

    #[test]
    fn data_dir_does_not_collide_with_python_app() {
        assert_eq!(XDG_DATA_DIR_LEAF, "splicecraft-rs");
        assert_ne!(XDG_DATA_DIR_LEAF, "splicecraft");
        assert_ne!(XDG_DATA_DIR_LEAF, PYTHON_XDG_DATA_DIR_LEAF);
    }

    #[test]
    fn layer_below_is_wired() {
        assert_eq!(util::crate_name(), "splicecraft-util");
    }

    #[test]
    fn sandbox_resolved_dir_contains_temp_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        with_xdg(Some(tmp.path()), || {
            let resolved = data_dir().expect("XDG set");
            assert!(
                resolved.starts_with(tmp.path()),
                "resolved {} not under {}",
                resolved.display(),
                tmp.path().display()
            );
            assert_eq!(
                resolved.file_name().and_then(|s| s.to_str()),
                Some(XDG_DATA_DIR_LEAF)
            );
            assert!(!path_has_python_leaf(&resolved));
        });
    }

    #[test]
    fn data_dir_without_xdg_is_refused_in_tests() {
        with_xdg(None, || {
            let err = data_dir().expect_err("must not resolve the real user dir");
            assert!(matches!(err, PersistError::UnsandboxedTest));
        });
    }

    #[test]
    fn resolved_leaf_is_never_python_splicecraft() {
        let tmp = tempfile::tempdir().unwrap();
        let p = join_leaf(tmp.path()).unwrap();
        assert_ne!(p.file_name().unwrap(), PYTHON_XDG_DATA_DIR_LEAF);
        assert!(p.ends_with(XDG_DATA_DIR_LEAF));
        assert!(!p.ends_with(PYTHON_XDG_DATA_DIR_LEAF));
    }

    #[test]
    fn authorize_for_sandbox_rejects_non_tempdir() {
        let home_local = dirs_home().join(".local");
        let err = authorize_writes_for_sandbox(&home_local).expect_err("home is not a sandbox");
        assert!(matches!(err, PersistError::NotUnderTemp { .. }));
        let msg = err.to_string();
        assert!(msg.contains("not under"), "{msg}");
    }

    fn dirs_home() -> std::path::PathBuf {
        std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("/home/nobody"))
    }

    #[test]
    fn unauthorized_save_returns_error() {
        revoke_thread_writes();
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x.json");
        let err = safe_save_json(&p, &[], "test").expect_err("must refuse");
        assert!(matches!(err, PersistError::Unauthorized { .. }));
        assert!(!p.exists());
    }

    #[test]
    fn first_save_writes_envelope_without_bak() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("test.json");
        safe_save_json(&p, &[json!({"id": "A"})], "test").unwrap();
        let raw: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(raw["_schema_version"], CURRENT_SCHEMA_VERSION);
        assert_eq!(raw["entries"], json!([{"id": "A"}]));
        assert!(!tmp.path().join("test.json.bak").exists());
    }

    #[test]
    fn bak_exists_after_second_save() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("test.json");
        safe_save_json(&p, &[json!({"id": "first"})], "test").unwrap();
        safe_save_json(&p, &[json!({"id": "second"})], "test").unwrap();
        let bak = tmp.path().join("test.json.bak");
        assert!(bak.exists(), "legacy .bak after second save");
        let bak_raw: Value = serde_json::from_str(&fs::read_to_string(&bak).unwrap()).unwrap();
        assert_eq!(bak_raw["entries"], json!([{"id": "first"}]));
        let live: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(live["entries"], json!([{"id": "second"}]));
    }

    #[test]
    fn crash_between_write_and_replace_leaves_previous_intact() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("lib.json");
        safe_save_json(&p, &[json!({"id": "good"})], "test").unwrap();
        let staged = stage_json_tempfile(&p, &[json!({"id": "evil"})], "test").unwrap();
        assert!(staged.exists());
        let live = fs::read_to_string(&p).unwrap();
        assert!(live.contains("good"), "{live}");
        assert!(!live.contains("evil"), "{live}");
        let _ = fs::remove_file(staged);
    }

    #[test]
    fn shrink_refuse_does_not_overwrite_large_fixture_with_empty() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("lib.json");
        safe_save_json(&p, &entries(20), "library").unwrap();
        let err = safe_save_json(&p, &[], "library").expect_err("catastrophic");
        assert!(matches!(
            err,
            PersistError::CatastrophicShrink {
                from: 20,
                to: 0,
                ..
            }
        ));
        let loaded = safe_load_json(&p, "library");
        assert_eq!(loaded.entries.len(), 20);
        assert!(tmp.path().join("lost_entries").exists());
    }

    #[test]
    fn catastrophic_shrink_with_bypass_succeeds() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("lib.json");
        safe_save_json(&p, &entries(20), "library").unwrap();
        {
            let _g = allow_catastrophic_shrink();
            safe_save_json(&p, &[json!({"id": "0"})], "library").unwrap();
        }
        assert_eq!(
            safe_load_json(&p, "library").entries,
            vec![json!({"id": "0"})]
        );
    }

    #[test]
    fn empty_file_no_bak() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("test.json");
        fs::write(&p, "").unwrap();
        safe_save_json(&p, &[json!({"id": "new"})], "test").unwrap();
        assert!(!tmp.path().join("test.json.bak").exists());
    }

    #[test]
    fn missing_file_returns_empty_no_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = safe_load_json(&tmp.path().join("nope.json"), "test");
        assert!(loaded.entries.is_empty());
        assert!(loaded.warning.is_none());
    }

    #[test]
    fn legacy_flat_list_loads_without_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("legacy.json");
        fs::write(&p, r#"[{"id":"L1"},{"id":"L2"}]"#).unwrap();
        let loaded = safe_load_json(&p, "test");
        assert_eq!(loaded.entries, vec![json!({"id":"L1"}), json!({"id":"L2"})]);
        assert!(loaded.warning.is_none());
    }

    #[test]
    fn envelope_loads_and_roundtrips() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("roundtrip.json");
        let original = vec![json!({"id":"A","name":"pUC"}), json!({"id":"B"})];
        safe_save_json(&p, &original, "test").unwrap();
        let loaded = safe_load_json(&p, "test");
        assert_eq!(loaded.entries, original);
        assert!(loaded.warning.is_none());
    }

    #[test]
    fn future_schema_version_warns_but_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("future.json");
        fs::write(
            &p,
            serde_json::to_string(&json!({
                "_schema_version": CURRENT_SCHEMA_VERSION + 99,
                "entries": [{"id":"F1","new_field":"unknown"}]
            }))
            .unwrap(),
        )
        .unwrap();
        let loaded = safe_load_json(&p, "test");
        assert_eq!(loaded.entries[0]["id"], "F1");
        assert!(
            loaded
                .warning
                .as_deref()
                .unwrap()
                .to_ascii_lowercase()
                .contains("newer")
        );
    }

    #[test]
    fn corrupt_file_with_valid_bak_restores() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("test.json");
        fs::write(tmp.path().join("test.json.bak"), r#"[{"id":"rescued"}]"#).unwrap();
        fs::write(&p, "{garbage").unwrap();
        let loaded = safe_load_json(&p, "test");
        assert_eq!(loaded.entries, vec![json!({"id":"rescued"})]);
        assert!(
            loaded
                .warning
                .as_deref()
                .unwrap()
                .to_ascii_lowercase()
                .contains("restored")
        );
        let live: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(live, json!([{"id":"rescued"}]));
    }

    #[test]
    fn corrupt_file_without_bak_returns_empty_with_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("test.json");
        fs::write(&p, "{not valid json").unwrap();
        let loaded = safe_load_json(&p, "test");
        assert!(loaded.entries.is_empty());
        assert!(
            loaded
                .warning
                .as_deref()
                .unwrap()
                .to_ascii_lowercase()
                .contains("corrupt")
        );
    }

    #[test]
    fn non_list_json_treated_as_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("test.json");
        fs::write(&p, r#"{"not":"a list"}"#).unwrap();
        let loaded = safe_load_json(&p, "test");
        assert!(loaded.entries.is_empty());
        assert!(loaded.warning.is_some());
    }

    #[test]
    fn legacy_file_rewrites_as_envelope_on_next_save() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("upgrade.json");
        fs::write(&p, r#"[{"id":"OLD"}]"#).unwrap();
        assert_eq!(
            safe_load_json(&p, "test").entries,
            vec![json!({"id":"OLD"})]
        );
        safe_save_json(&p, &[json!({"id":"NEW"})], "test").unwrap();
        let raw: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(raw["_schema_version"], CURRENT_SCHEMA_VERSION);
        assert_eq!(raw["entries"], json!([{"id":"NEW"}]));
    }

    #[test]
    fn refuse_python_data_dir_component() {
        let (tmp, _) = sandbox();
        let evil = tmp.path().join("splicecraft").join("library.json");
        let err = safe_save_json(&evil, &[], "test").expect_err("python leaf");
        assert!(matches!(err, PersistError::PythonDataDir { .. }));
    }

    #[test]
    fn domain_library_save_is_sandboxed() {
        let (tmp, layout) = sandbox();
        assert!(layout.root.starts_with(tmp.path()));
        save_library(&layout, &[json!({"id":"p1"})]).unwrap();
        let loaded = load_library(&layout);
        assert_eq!(loaded.entries, vec![json!({"id":"p1"})]);
        assert!(layout.library_file().starts_with(&layout.root));
    }

    #[test]
    fn crash_recovery_write_is_gated_and_atomic() {
        let (_tmp, layout) = sandbox();
        let path = crash_recovery_path(&layout.root, "pUC19").expect("path");
        write_crash_recovery(&path, "LOCUS pUC19\n").unwrap();
        assert!(path.exists());
        assert!(path.starts_with(layout.crash_recovery_dir()));
        clear_crash_recovery(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn crash_recovery_unauthorized() {
        revoke_thread_writes();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("crash_recovery").join("x.gb");
        let err = write_crash_recovery(&path, "LOCUS\n").expect_err("gated");
        assert!(matches!(err, PersistError::Unauthorized { .. }));
    }

    #[test]
    fn log_event_redacts_dna() {
        let line = format_event("persist.saved", &[("seq", "ATGCATGCATGCATGC")]);
        assert!(!line.contains("ATGC"), "{line}");
        assert!(line.contains("<dna 16 bp>"), "{line}");
    }

    #[test]
    fn default_tests_cannot_resolve_real_share_dirs() {
        with_xdg(None, || {
            assert!(data_dir().is_err());
        });
        let home = dirs_home();
        let python = home.join(".local/share/splicecraft");
        let rust = home.join(".local/share/splicecraft-rs");
        // This crate's tests only write under tempfile roots.
        assert!(!python.exists() || !path_is_under_our_temp(&python));
        assert!(!rust.exists() || !path_is_under_our_temp(&rust));
    }

    fn path_is_under_our_temp(path: &Path) -> bool {
        path.starts_with(std::env::temp_dir())
    }

    #[test]
    fn small_base_empty_save_is_allowed() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("lib.json");
        safe_save_json(&p, &entries(9), "library").unwrap();
        safe_save_json(&p, &[], "library").unwrap();
        assert!(safe_load_json(&p, "library").entries.is_empty());
    }

    #[test]
    fn under_threshold_fifty_percent_not_refused() {
        let (tmp, _) = sandbox();
        let p = tmp.path().join("lib.json");
        safe_save_json(&p, &entries(10), "library").unwrap();
        safe_save_json(&p, &entries(5), "library").unwrap();
        assert_eq!(safe_load_json(&p, "library").entries.len(), 5);
    }
}
