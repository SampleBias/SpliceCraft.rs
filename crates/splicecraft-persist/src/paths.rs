//! Data-directory resolution. Leaf is always [`XDG_DATA_DIR_LEAF`].

use std::env;
use std::path::{Path, PathBuf};

use crate::XDG_DATA_DIR_LEAF;
use crate::error::PersistError;

/// Python SpliceCraft's XDG leaf — never write here.
pub const PYTHON_XDG_DATA_DIR_LEAF: &str = "splicecraft";

/// On-disk names matching upstream `splicecraft_state` (Rust data dir only).
pub const LIBRARY_FILE_NAME: &str = "plasmid_library.json";
/// Named collections (source of truth).
pub const COLLECTIONS_FILE_NAME: &str = "collections.json";
/// Active parts bin.
pub const PARTS_BIN_FILE_NAME: &str = "parts_bin.json";
/// Primer library.
pub const PRIMERS_FILE_NAME: &str = "primers.json";
/// Feature snippets.
pub const FEATURES_FILE_NAME: &str = "features.json";
/// Named enzyme collections (`{name, enzymes}`).
pub const ENZYME_COLLECTIONS_FILE_NAME: &str = "enzyme_collections.json";
/// User-defined enzymes merged into the NEB catalog.
pub const CUSTOM_ENZYMES_FILE_NAME: &str = "custom_enzymes.json";
/// Active enzyme-collection pointer (`[{name}]` or empty).
pub const ENZYME_ACTIVE_FILE_NAME: &str = "enzyme_active.json";
/// User-defined cloning grammars (built-ins live in `splicecraft-clone`).
pub const GRAMMARS_FILE_NAME: &str = "grammars.json";
/// Codon-usage table registry (K12 seeded on first load).
pub const CODON_TABLES_FILE_NAME: &str = "codon_tables.json";
/// User protein-motif overrides (built-ins live in `splicecraft-codon`).
pub const PROTEIN_MOTIFS_FILE_NAME: &str = "protein_motifs.json";
/// Saved agarose-gel snapshots (`&gel` ids for stage 12).
pub const GELS_FILE_NAME: &str = "gels.json";
/// Lab-notebook entries.
pub const EXPERIMENTS_FILE_NAME: &str = "experiments.json";
/// Named experiment projects.
pub const EXPERIMENT_PROJECTS_FILE_NAME: &str = "experiment_projects.json";
/// Per-entry image blobs (`experiments/<id>/`).
pub const EXPERIMENTS_DIR_NAME: &str = "experiments";
/// Saved `.dna` originals for history recovery.
pub const DNA_ORIGINALS_DIR_NAME: &str = "dna_originals";
/// App preferences (dict-shaped; still saved through the JSON chokepoint later).
pub const SETTINGS_FILE_NAME: &str = "settings.json";
/// Debounced `.gb` crash-recovery snapshots.
pub const CRASH_RECOVERY_DIR_NAME: &str = "crash_recovery";
/// Shrink-guard spill directory.
pub const LOST_ENTRIES_DIR_NAME: &str = "lost_entries";
/// Rotating diagnostic logs.
pub const LOG_DIR_NAME: &str = "logs";

/// Layout of files under a resolved data dir.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataLayout {
    /// `$XDG_DATA_HOME/splicecraft-rs` (or platform equivalent + same leaf).
    pub root: PathBuf,
}

impl DataLayout {
    /// Build from an already-resolved data dir. The last component must be the Rust leaf.
    pub fn new(root: PathBuf) -> Result<Self, PersistError> {
        check_leaf(&root)?;
        if path_has_python_leaf(&root) {
            return Err(PersistError::PythonDataDir { path: root });
        }
        Ok(Self { root })
    }

    /// Resolve from `XDG_DATA_HOME` / platform data dir, then join the Rust leaf.
    pub fn resolve() -> Result<Self, PersistError> {
        Self::new(data_dir()?)
    }

    /// Join the Rust leaf onto an XDG/base prefix (the sandbox helper).
    pub fn from_xdg_home(xdg_data_home: &Path) -> Result<Self, PersistError> {
        Self::new(join_leaf(xdg_data_home)?)
    }

    /// Plasmid library JSON.
    #[must_use]
    pub fn library_file(&self) -> PathBuf {
        self.root.join(LIBRARY_FILE_NAME)
    }

    /// Collections JSON.
    #[must_use]
    pub fn collections_file(&self) -> PathBuf {
        self.root.join(COLLECTIONS_FILE_NAME)
    }

    /// Parts-bin JSON.
    #[must_use]
    pub fn parts_bin_file(&self) -> PathBuf {
        self.root.join(PARTS_BIN_FILE_NAME)
    }

    /// Primers JSON.
    #[must_use]
    pub fn primers_file(&self) -> PathBuf {
        self.root.join(PRIMERS_FILE_NAME)
    }

    /// Feature library JSON.
    #[must_use]
    pub fn features_file(&self) -> PathBuf {
        self.root.join(FEATURES_FILE_NAME)
    }

    /// Enzyme collections JSON.
    #[must_use]
    pub fn enzyme_collections_file(&self) -> PathBuf {
        self.root.join(ENZYME_COLLECTIONS_FILE_NAME)
    }

    /// Custom enzymes JSON.
    #[must_use]
    pub fn custom_enzymes_file(&self) -> PathBuf {
        self.root.join(CUSTOM_ENZYMES_FILE_NAME)
    }

    /// Active enzyme-collection pointer JSON.
    #[must_use]
    pub fn enzyme_active_file(&self) -> PathBuf {
        self.root.join(ENZYME_ACTIVE_FILE_NAME)
    }

    /// User-defined cloning grammars JSON.
    #[must_use]
    pub fn grammars_file(&self) -> PathBuf {
        self.root.join(GRAMMARS_FILE_NAME)
    }

    /// Codon-usage table registry JSON.
    #[must_use]
    pub fn codon_tables_file(&self) -> PathBuf {
        self.root.join(CODON_TABLES_FILE_NAME)
    }

    /// Protein-motif overrides JSON.
    #[must_use]
    pub fn protein_motifs_file(&self) -> PathBuf {
        self.root.join(PROTEIN_MOTIFS_FILE_NAME)
    }

    /// Saved agarose-gel snapshots JSON.
    #[must_use]
    pub fn gels_file(&self) -> PathBuf {
        self.root.join(GELS_FILE_NAME)
    }

    /// Lab-notebook JSON.
    #[must_use]
    pub fn experiments_file(&self) -> PathBuf {
        self.root.join(EXPERIMENTS_FILE_NAME)
    }

    /// Experiment-projects JSON.
    #[must_use]
    pub fn experiment_projects_file(&self) -> PathBuf {
        self.root.join(EXPERIMENT_PROJECTS_FILE_NAME)
    }

    /// Per-entry attachment root.
    #[must_use]
    pub fn experiments_dir(&self) -> PathBuf {
        self.root.join(EXPERIMENTS_DIR_NAME)
    }

    /// Saved `.dna` originals.
    #[must_use]
    pub fn dna_originals_dir(&self) -> PathBuf {
        self.root.join(DNA_ORIGINALS_DIR_NAME)
    }

    /// Settings JSON.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.root.join(SETTINGS_FILE_NAME)
    }

    /// Crash-recovery directory (`*.gb`).
    #[must_use]
    pub fn crash_recovery_dir(&self) -> PathBuf {
        self.root.join(CRASH_RECOVERY_DIR_NAME)
    }

    /// Lost-entries spill directory.
    #[must_use]
    pub fn lost_entries_dir(&self) -> PathBuf {
        self.root.join(LOST_ENTRIES_DIR_NAME)
    }

    /// Log directory.
    #[must_use]
    pub fn log_dir(&self) -> PathBuf {
        self.root.join(LOG_DIR_NAME)
    }
}

/// `$XDG_DATA_HOME/splicecraft-rs`, or the platform data dir + the same leaf.
///
/// In unit/integration tests this **errors** unless `XDG_DATA_HOME` is set, so a
/// forgotten sandbox cannot resolve `~/.local/share/splicecraft-rs`.
pub fn data_dir() -> Result<PathBuf, PersistError> {
    if let Some(xdg) = env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
        return join_leaf(Path::new(&xdg));
    }
    if cfg!(test) {
        return Err(PersistError::UnsandboxedTest);
    }
    let base = directories::BaseDirs::new()
        .map(|d| d.data_dir().to_path_buf())
        .ok_or_else(|| {
            PersistError::Commit("could not resolve a platform data directory".into())
        })?;
    join_leaf(&base)
}

/// Join [`XDG_DATA_DIR_LEAF`] onto an XDG/base prefix.
pub fn join_leaf(xdg_or_base: &Path) -> Result<PathBuf, PersistError> {
    let p = xdg_or_base.join(XDG_DATA_DIR_LEAF);
    check_leaf(&p)?;
    Ok(p)
}

/// Last path component must be the Rust leaf, never the Python app's.
pub fn check_leaf(path: &Path) -> Result<(), PersistError> {
    match path.file_name().and_then(|s| s.to_str()) {
        Some(XDG_DATA_DIR_LEAF) => Ok(()),
        Some(other) => Err(PersistError::WrongLeaf {
            found: other.to_owned(),
        }),
        None => Err(PersistError::WrongLeaf {
            found: String::new(),
        }),
    }
}

/// True if any path component is exactly the Python leaf `splicecraft`.
#[must_use]
pub fn path_has_python_leaf(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == PYTHON_XDG_DATA_DIR_LEAF)
}

/// Refuse writes whose path walks through the Python data-dir leaf.
pub(crate) fn refuse_python_data_dir(path: &Path) -> Result<(), PersistError> {
    if path_has_python_leaf(path) {
        Err(PersistError::PythonDataDir {
            path: path.to_path_buf(),
        })
    } else {
        Ok(())
    }
}
