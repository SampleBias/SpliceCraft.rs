//! Headless workbench session (library + optional loaded record).

use std::path::PathBuf;

use splicecraft_core::Record;
use splicecraft_persist::{DataLayout, LibraryStore};

/// In-memory agent canvas. Not the TUI `AppState`.
#[derive(Clone, Debug)]
pub struct AgentSession {
    /// Sandboxed or resolved data dir (`splicecraft-rs` leaf).
    pub layout: DataLayout,
    /// Plasmid collections + active list.
    pub library: LibraryStore,
    /// Currently loaded record, if any.
    pub record: Option<Record>,
    /// Unsaved canvas changes (`_unsaved`).
    pub dirty: bool,
    /// File the record was loaded from, if any (export/save home).
    pub source_path: Option<PathBuf>,
    /// Headless flag surfaced by `/healthz`.
    pub headless: bool,
}

impl AgentSession {
    /// Load the library from `layout`. No record on the canvas.
    #[must_use]
    pub fn load(layout: DataLayout) -> Self {
        let library = LibraryStore::load(&layout);
        Self {
            layout,
            library,
            record: None,
            dirty: false,
            source_path: None,
            headless: false,
        }
    }

    /// Reload collections from disk (after an external write).
    pub fn reload_library(&mut self) {
        self.library = LibraryStore::load(&self.layout);
    }
}
