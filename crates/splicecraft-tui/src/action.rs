//! Event-loop actions. Keys map here; [`crate::AppState::reduce`] applies them.

/// A single user intent. No I/O happens in the enum itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Leave the process. `q` / Esc / Ctrl+Q on the main view.
    Quit,
    /// Open or close the `?` keyboard overlay.
    ToggleHelp,
    /// Dismiss help or the command palette.
    CloseOverlay,
    /// Open (or re-focus) the Ctrl+K palette.
    OpenPalette,
    /// Type into the palette query.
    PaletteInput(char),
    /// Delete the last palette query character.
    PaletteBackspace,
    /// Move the palette highlight. Negative is up.
    PaletteMove(i32),
    /// Run the highlighted palette command.
    PaletteExecute,
    /// F1–F4 single-pane focus.
    FocusPane(Pane),
    /// F5 — restore the split layout.
    FocusAll,
    /// Memory-only demo plasmid. Never persisted.
    LoadDemo,
    /// Circular ↔ linear map (`v`).
    ToggleMapView,
    /// Restriction overlay (`r`).
    ToggleRestr,
    /// Feature label connectors (`l`).
    ToggleLabels,
    /// Move the sequence cursor. Negative is left.
    MoveCursor(i32),
    /// Rotate the map display origin (not a record edit).
    RotateView(i32),
    /// Put display origin and cursor at 0.
    ResetView,
    /// Insert one IUPAC base at the cursor.
    InsertBase(char),
    /// Delete the base before the cursor.
    DeleteBack,
    /// Highlight the smallest feature enclosing the cursor.
    EnterPickFeature,
    /// Undo last record edit. [INV-10]
    Undo,
    /// Redo.
    Redo,
    /// Reverse-complement the whole record.
    FlipRecord,
    /// Re-cut origin at the cursor (circular only).
    SetOriginHere,
    /// Alt+K — keep the loaded record in the active collection.
    KeepRecord,
    /// Answer the name-collision modal.
    CollisionPick(splicecraft_persist::CollisionChoice),
    /// Save the selected record feature into the feature library.
    SaveSelectedFeature,
    /// Move the library highlight. Negative is up.
    LibraryMove(i32),
    /// Load the highlighted library entry into the editor.
    LibraryOpen,
    /// Prompt for a file path (Ctrl+O).
    OpenPathPrompt,
    /// Prompt for a folder to bulk-import.
    BulkImportPrompt,
    /// Prompt for a folder to bulk-export the active collection.
    BulkExportPrompt,
    /// Type into the path prompt.
    PathInput(char),
    /// Delete the last path-prompt character.
    PathBackspace,
    /// Submit the path prompt.
    PathSubmit,
    /// Tool that is chrome-only until a later stage.
    Stub {
        /// Palette / menu title (no sequence content).
        name: &'static str,
        /// Stage that will implement the handler.
        stage: u8,
    },
}

/// Which workbench pane has keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    /// Left plasmid library list.
    Library,
    /// Centre map placeholder.
    Map,
    /// Right feature list placeholder.
    Features,
    /// Bottom sequence placeholder.
    Sequence,
}

/// Overlay stacked on the workbench.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    /// No modal.
    None,
    /// `?` keyboard reference.
    Help,
    /// Ctrl+K command palette.
    Palette,
    /// Skip / copy / overwrite (never implied).
    Collision,
    /// Path / folder prompt.
    Path,
}

/// What the path prompt will do on submit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    /// Load one `.gb` / `.fasta` into memory.
    OpenFile,
    /// Import a folder of plasmids.
    BulkImport,
    /// Export the active collection.
    BulkExport,
}

/// Re-export so keys/state stay on one collision vocabulary.
pub use splicecraft_persist::CollisionChoice;

/// Split vs single-pane layout (upstream F1–F5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusMode {
    /// Library + map + features over the sequence strip.
    All,
    /// One pane fills the body.
    Single(Pane),
}
