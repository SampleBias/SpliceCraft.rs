//! Event-loop actions. Keys map here; [`crate::AppState::reduce`] applies them.

/// A single user intent. No I/O happens in the enum itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Leave the process. `q` / Esc / Ctrl+Q on the main view.
    Quit,
    /// Dismiss the startup DNA splash and show the workbench.
    DismissSplash,
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
    /// Unique-cutter filter (`u`).
    ToggleRestrUnique,
    /// 6+ vs all-length recognition filter (`6`).
    ToggleRestrSixPlus,
    /// Cycle the active enzyme collection. Negative is previous.
    CycleEnzymeCollection(i32),
    /// Feature label connectors (`l`).
    ToggleLabels,
    /// Open the primer-design overlay.
    OpenPrimerDesign,
    /// Open primer-check.
    OpenPrimerCheck,
    /// Open enzyme collections.
    OpenEnzymes,
    /// Open the cloning Constructor (Traditional / Gibson / Domesticator / Syn-frag).
    OpenConstructor,
    /// Open Mutato (SDM / Scrub).
    OpenMutato,
    /// Open Synthesis (DNA / Protein / Operon).
    OpenSynthesis,
    /// Open the in-silico PCR + agarose gel Simulator.
    OpenSimulator,
    /// Open Sequencing (zip / align / Sanger / report).
    OpenSequencing,
    /// Jump the sequence cursor to the first alignment variant.
    SequencingJump,
    /// Open the lab notebook.
    OpenExperiments,
    /// Jump the first `@` / `!` / `&` cross-ref (Ctrl+G analog).
    ExperimentJump,
    /// Spellcheck the compose body (F7).
    ExperimentSpellcheck,
    /// Open the construction-history viewer.
    OpenHistory,
    /// Open BLAST / ORF / online search.
    OpenSearch,
    /// Open the settings overlay (online toggles).
    OpenSettings,
    /// Open BABS (local Ollama).
    OpenBabs,
    /// Open AUTOLAB (OT-2 compiler).
    OpenAutolab,
    /// Open Master Delete (palette only; no keybinding).
    OpenMasterDelete,
    /// Prompt for a map-image export path.
    ExportMapPrompt,
    /// Prompt for a migrate-zip export path.
    ExportMigratePrompt,
    /// Prompt for a migrate-zip import path.
    ImportMigratePrompt,
    /// Flip `allow_online_search` (human only).
    ToggleOnlineSearch,
    /// Flip `allow_online_lookups` (human only).
    ToggleOnlineLookups,
    /// Compile the fixture OT-2 deck (no motion).
    AutolabCompile,
    /// Arm the OT-2 motion confirm (still does not move hardware).
    AutolabArmMotion,
    /// Dry-run recover history from saved `.dna` originals.
    RecoverHistory,
    /// Open the Parts Bin.
    OpenParts,
    /// Save the last constructor product into the library.
    ConstructorSave,
    /// Save the selected PCR amplicon or gel snapshot.
    SimulatorSave,
    /// Pin the selected PCR amplicon as a gel lane.
    SimulatorSendToGel,
    /// Append a 5′ Gibson homology arm (idempotent).
    ConstructorDesignArms,
    /// Delete the highlighted library plasmid (session-undoable).
    LibraryDelete,
    /// Restore the last session library delete.
    LibraryUndelete,
    /// Cycle designer kind (generic / cloning / detection / GB).
    ToolTab,
    /// Confirm the current tool overlay (design / check / activate).
    ToolEnter,
    /// Save the last designed pair into the primer library.
    PrimerDesignSave,
    /// Type into a tool overlay.
    ToolInput(char),
    /// Delete the last tool-overlay character.
    ToolBackspace,
    /// Move a tool-overlay highlight. Negative is up.
    ToolMove(i32),
    /// Cycle Designed → Ordered → Validated on the highlighted primer.
    PrimerLibCycleStatus,
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
    /// F10 — keyboard focus on the top menu bar.
    ToggleMenuFocus,
    /// Move the menu-bar highlight. Negative is left.
    MenuMove(i32),
    /// Enter on the highlighted menu (File opens the dropdown).
    MenuActivate,
    /// NCBI accession prompt (`f`).
    OpenFetch,
    /// New-plasmid sequence prompt (`Ctrl+N`).
    OpenNewPlasmid,
    /// Find DNA subsequence (`Ctrl+F`).
    OpenFindDna,
    /// Add-feature prompt (`Alt+Shift+F`).
    OpenAddFeature,
    /// Save the loaded record through the persist chokepoint (`Ctrl+S`).
    SaveRecord,
    /// Select the whole sequence (`Ctrl+A`).
    SelectAll,
    /// Copy the selection top strand (`Ctrl+C`).
    CopyTop,
    /// Copy the selection bottom-strand RC (`Alt+C`).
    CopyBottom,
    /// Tool that is a documented post-1.0 gap (`docs/parity.md`).
    Stub {
        /// Palette / menu title (no sequence content).
        name: &'static str,
        /// Historical stage hint (unused; gaps are listed in `docs/parity.md`).
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
    /// Primer designers.
    PrimerDesign,
    /// In-silico primer-check / PCR listing.
    PrimerCheck,
    /// Enzyme collections + custom catalog.
    Enzymes,
    /// Cloning workbench tabs.
    Constructor,
    /// Mutato SDM / Scrub.
    Mutato,
    /// DNA / protein / operon composers.
    Synthesis,
    /// In-silico PCR + agarose gel.
    Simulator,
    /// Sequencing verification (zip / align / Sanger / report).
    Sequencing,
    /// Markdown lab notebook.
    Experiments,
    /// Construction-history viewer (read-only warnings).
    History,
    /// BLAST / ORF / online / HMM-DB / find.
    Search,
    /// Parts Bin.
    Parts,
    /// Online / lookup settings.
    Settings,
    /// Local Ollama chat.
    Babs,
    /// OT-2 protocol compiler.
    Autolab,
    /// Triple-gated data wipe.
    MasterDelete,
    /// File menu dropdown (Open / Fetch / Quit).
    FileMenu,
}

/// Which designer the primer overlay will run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DesignKind {
    /// No tails.
    #[default]
    Generic,
    /// Pad + RE-site tails (EcoRI / BamHI).
    Cloning,
    /// Pair inside the selected region.
    Detection,
    /// BsaI / BsaI Golden Braid tails.
    GoldenBraid,
}

/// Constructor overlay tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConstructorTab {
    /// Two-enzyme directional ligation.
    #[default]
    Traditional,
    /// N-fragment Gibson.
    Gibson,
    /// Type IIS domestication primers.
    Domesticator,
    /// Classify / list the parts bin.
    Parts,
    /// New part from a synthetic fragment.
    SynFrag,
}

impl ConstructorTab {
    /// Next tab in the cycle.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Traditional => Self::Gibson,
            Self::Gibson => Self::Domesticator,
            Self::Domesticator => Self::Parts,
            Self::Parts => Self::SynFrag,
            Self::SynFrag => Self::Traditional,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Traditional => "traditional",
            Self::Gibson => "gibson",
            Self::Domesticator => "domesticator",
            Self::Parts => "parts",
            Self::SynFrag => "syn-frag",
        }
    }
}

/// Mutato overlay tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MutatoTab {
    /// Site-directed mutagenesis (SOE / modified-outer).
    #[default]
    Sdm,
    /// Clone-free QuikChange scrub.
    ScrubQc,
    /// Golden Braid recirc scrub.
    ScrubGb,
}

impl MutatoTab {
    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Sdm => Self::ScrubQc,
            Self::ScrubQc => Self::ScrubGb,
            Self::ScrubGb => Self::Sdm,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Sdm => "SDM",
            Self::ScrubQc => "scrub QuikChange",
            Self::ScrubGb => "scrub GB",
        }
    }
}

/// Synthesis overlay tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SynthTab {
    /// Linear DNA buffer.
    #[default]
    Dna,
    /// Protein composer.
    Protein,
    /// Operon SOE.
    Operon,
}

impl SynthTab {
    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Dna => Self::Protein,
            Self::Protein => Self::Operon,
            Self::Operon => Self::Dna,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dna => "DNA",
            Self::Protein => "protein",
            Self::Operon => "operon",
        }
    }
}

/// Simulator overlay tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SimulatorTab {
    /// Exact-match PCR enumeration.
    #[default]
    Pcr,
    /// Agarose gel image.
    Gel,
}

impl SimulatorTab {
    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Pcr => Self::Gel,
            Self::Gel => Self::Pcr,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pcr => "PCR",
            Self::Gel => "gel",
        }
    }
}

/// Sequencing overlay tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SequencingTab {
    /// Plasmidsaurus zip ingest.
    #[default]
    Zip,
    /// Pairwise overlay vs the loaded plasmid.
    Align,
    /// Sanger AB1 traces.
    Sanger,
    /// Verification report / bulk folder.
    Report,
}

impl SequencingTab {
    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Zip => Self::Align,
            Self::Align => Self::Sanger,
            Self::Sanger => Self::Report,
            Self::Report => Self::Zip,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Align => "align",
            Self::Sanger => "Sanger",
            Self::Report => "report",
        }
    }
}

/// Experiments overlay tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExperimentsTab {
    /// Project entry list.
    #[default]
    List,
    /// Markdown compose.
    Compose,
    /// Image attachments.
    Attach,
}

impl ExperimentsTab {
    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::List => Self::Compose,
            Self::Compose => Self::Attach,
            Self::Attach => Self::List,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Compose => "compose",
            Self::Attach => "attach",
        }
    }
}

/// History overlay tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryTab {
    /// Numbered protocol (left → right).
    #[default]
    Protocol,
    /// Lineage tree.
    Tree,
    /// Step detail + warnings.
    Detail,
}

/// Search overlay tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchTab {
    /// Local BLASTN / BLASTP / HMMscan (ungapped).
    #[default]
    Local,
    /// Six-frame ORF finder.
    Orf,
    /// NCBI / EBI (setting-gated).
    Online,
    /// HMM-DB catalog.
    HmmDb,
    /// Fuzzy plasmid find across collections.
    Find,
}

impl SearchTab {
    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Local => Self::Orf,
            Self::Orf => Self::Online,
            Self::Online => Self::HmmDb,
            Self::HmmDb => Self::Find,
            Self::Find => Self::Local,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Orf => "ORF",
            Self::Online => "online",
            Self::HmmDb => "HMM-DB",
            Self::Find => "find",
        }
    }
}

impl HistoryTab {
    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Protocol => Self::Tree,
            Self::Tree => Self::Detail,
            Self::Detail => Self::Protocol,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Tree => "tree",
            Self::Detail => "detail",
        }
    }
}

impl DesignKind {
    /// Next designer in the Tab cycle.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Generic => Self::Cloning,
            Self::Cloning => Self::Detection,
            Self::Detection => Self::GoldenBraid,
            Self::GoldenBraid => Self::Generic,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Cloning => "cloning",
            Self::Detection => "detection",
            Self::GoldenBraid => "golden braid",
        }
    }
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
    /// Bulk-align a folder of reads against the loaded plasmid.
    BulkAlign,
    /// Export a publication map (SVG/PNG) to a user path.
    MapExport,
    /// Write a migrate zip to a user path.
    MigrateExport,
    /// Restore a migrate zip into the sandboxed data dir.
    MigrateImport,
    /// NCBI accession (not a filesystem path).
    FetchNcbi,
    /// DNA subsequence find (both strands).
    FindDna,
    /// Paste a sequence into a memory record.
    NewPlasmid,
    /// Label [+ type] for a feature covering the selection.
    AddFeature,
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
