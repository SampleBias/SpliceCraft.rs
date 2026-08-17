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
/// App preferences (`[{key, value}, …]` through the JSON chokepoint).
pub const SETTINGS_FILE_NAME: &str = "settings.json";
/// HMM-DB catalog (builtins re-injected on load).
pub const HMM_DB_CATALOG_FILE_NAME: &str = "hmm_db_catalog.json";
/// Per-id HMM downloads (`hmm_databases/<id>/`).
pub const HMM_DATABASES_DIR_NAME: &str = "hmm_databases";
/// Debounced `.gb` crash-recovery snapshots.
pub const CRASH_RECOVERY_DIR_NAME: &str = "crash_recovery";
/// Shrink-guard spill directory.
pub const LOST_ENTRIES_DIR_NAME: &str = "lost_entries";
/// Rotating diagnostic logs.
pub const LOG_DIR_NAME: &str = "logs";
/// Agent bearer token (operational; wiped by Master Delete).
pub const AGENT_TOKEN_FILE_NAME: &str = "agent_token";
/// Process lockfile — Master Delete preserves this.
pub const LOCK_FILE_NAME: &str = "splicecraft.lock";
/// Legacy-migration marker.
pub const MIGRATED_MARKER_NAME: &str = ".migrated";
/// Saved OT-2 protocol designs.
pub const PROTOCOL_COLLECTIONS_FILE_NAME: &str = "protocol_collections.json";
/// Custom Opentrons labware definitions.
pub const CUSTOM_LABWARE_FILE_NAME: &str = "custom_labware.json";

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

    /// HMM-DB catalog JSON.
    #[must_use]
    pub fn hmm_db_catalog_file(&self) -> PathBuf {
        self.root.join(HMM_DB_CATALOG_FILE_NAME)
    }

    /// HMM download root (`hmm_databases/`).
    #[must_use]
    pub fn hmm_databases_dir(&self) -> PathBuf {
        self.root.join(HMM_DATABASES_DIR_NAME)
    }

    /// One downloaded HMM database directory.
    #[must_use]
    pub fn hmm_db_dir(&self, entry_id: &str) -> PathBuf {
        self.hmm_databases_dir().join(entry_id)
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

    /// Agent token file.
    #[must_use]
    pub fn agent_token_file(&self) -> PathBuf {
        self.root.join(AGENT_TOKEN_FILE_NAME)
    }

    /// Process lockfile (preserved across Master Delete).
    #[must_use]
    pub fn lock_file(&self) -> PathBuf {
        self.root.join(LOCK_FILE_NAME)
    }

    /// `.migrated` marker.
    #[must_use]
    pub fn migrated_marker(&self) -> PathBuf {
        self.root.join(MIGRATED_MARKER_NAME)
    }

    /// OT-2 protocol collections JSON.
    #[must_use]
    pub fn protocol_collections_file(&self) -> PathBuf {
        self.root.join(PROTOCOL_COLLECTIONS_FILE_NAME)
    }

    /// Custom OT-2 labware JSON.
    #[must_use]
    pub fn custom_labware_file(&self) -> PathBuf {
        self.root.join(CUSTOM_LABWARE_FILE_NAME)
    }

    /// Every user-data JSON this layout knows about (may not exist on disk).
    #[must_use]
    pub fn user_data_files(&self) -> Vec<PathBuf> {
        vec![
            self.library_file(),
            self.collections_file(),
            self.parts_bin_file(),
            self.primers_file(),
            self.features_file(),
            self.enzyme_collections_file(),
            self.custom_enzymes_file(),
            self.enzyme_active_file(),
            self.grammars_file(),
            self.codon_tables_file(),
            self.protein_motifs_file(),
            self.gels_file(),
            self.experiments_file(),
            self.experiment_projects_file(),
            self.settings_file(),
            self.hmm_db_catalog_file(),
            self.protocol_collections_file(),
            self.custom_labware_file(),
        ]
    }

    /// User-data directories (HMM downloads are omitted from migrate by default).
    #[must_use]
    pub fn user_data_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.crash_recovery_dir(),
            self.dna_originals_dir(),
            self.experiments_dir(),
            self.hmm_databases_dir(),
            self.lost_entries_dir(),
        ]
    }

    /// Operational files wiped by Master Delete (not user plasmids).
    #[must_use]
    pub fn operational_files(&self) -> Vec<PathBuf> {
        vec![self.agent_token_file(), self.migrated_marker()]
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
