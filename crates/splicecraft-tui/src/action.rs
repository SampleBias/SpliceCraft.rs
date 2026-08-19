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
    /// Memory-only basic demo plasmid (`pDemo`). Never persisted.
    LoadDemo,
    /// Memory-only advanced teaching plasmid (`pDemoAdv`). Never persisted.
    LoadDemoAdvanced,
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
    /// Previous tool-overlay tab (BLAST ← / Shift+Tab).
    ToolTabPrev,
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
    /// All constructor tools, left-to-right.
    pub const ALL: [Self; 5] = [
        Self::Traditional,
        Self::Gibson,
        Self::Domesticator,
        Self::Parts,
        Self::SynFrag,
    ];

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

    /// Previous tab.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Traditional => Self::SynFrag,
            Self::Gibson => Self::Traditional,
            Self::Domesticator => Self::Gibson,
            Self::Parts => Self::Domesticator,
            Self::SynFrag => Self::Parts,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.chip()
    }

    /// Chip on the Constructor tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::Traditional => "Traditional",
            Self::Gibson => "Gibson",
            Self::Domesticator => "Domesticator",
            Self::Parts => "Parts",
            Self::SynFrag => "Syn-frag",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Traditional => "Two-enzyme digest + ligation of the loaded plasmid.",
            Self::Gibson => "Overlap assembly; a adds homology arms.",
            Self::Domesticator => "Type IIS primers for the active grammar.",
            Self::Parts => "Parts bin for the active grammar.",
            Self::SynFrag => "File a synthetic fragment as an L0 part.",
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
    /// All Mutato tools, left-to-right.
    pub const ALL: [Self; 3] = [Self::Sdm, Self::ScrubQc, Self::ScrubGb];

    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Sdm => Self::ScrubQc,
            Self::ScrubQc => Self::ScrubGb,
            Self::ScrubGb => Self::Sdm,
        }
    }

    /// Previous tab.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Sdm => Self::ScrubGb,
            Self::ScrubQc => Self::Sdm,
            Self::ScrubGb => Self::ScrubQc,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.chip()
    }

    /// Chip on the Mutato tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::Sdm => "SDM",
            Self::ScrubQc => "QuikChange",
            Self::ScrubGb => "Golden Braid",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Sdm => "SOE 4-primer mutagenesis, or a 2-primer shortcut near a CDS end.",
            Self::ScrubQc => "Clone-free restriction-site removal (QuikChange primers).",
            Self::ScrubGb => "Split-and-reassemble cures; product must match the cured plasmid.",
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
    /// All synthesis tools, left-to-right.
    pub const ALL: [Self; 3] = [Self::Dna, Self::Protein, Self::Operon];

    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Dna => Self::Protein,
            Self::Protein => Self::Operon,
            Self::Operon => Self::Dna,
        }
    }

    /// Previous tab.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Dna => Self::Operon,
            Self::Protein => Self::Dna,
            Self::Operon => Self::Protein,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.chip()
    }

    /// Chip on the Synthesis tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::Dna => "DNA",
            Self::Protein => "Protein",
            Self::Operon => "Operon",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Dna => "Linear IUPAC buffer — type bases, Enter to keep.",
            Self::Protein => "AA composer; back-translates with the active codon table.",
            Self::Operon => "SOE domestication of CDS features on the loaded record.",
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
    /// All simulator tools, left-to-right.
    pub const ALL: [Self; 2] = [Self::Pcr, Self::Gel];

    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Pcr => Self::Gel,
            Self::Gel => Self::Pcr,
        }
    }

    /// Previous tab.
    #[must_use]
    pub fn prev(self) -> Self {
        self.next()
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.chip()
    }

    /// Chip on the Simulator tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::Pcr => "PCR",
            Self::Gel => "Gel",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Pcr => "Exact-match PCR. Wrap amplicons are legal on circular templates.",
            Self::Gel => "Helling–Goodman–Boyer agarose (g pins a PCR lane, s saves).",
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
    /// All sequencing tools, left-to-right.
    pub const ALL: [Self; 4] = [Self::Zip, Self::Align, Self::Sanger, Self::Report];

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

    /// Previous tab.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Zip => Self::Report,
            Self::Align => Self::Zip,
            Self::Sanger => Self::Align,
            Self::Report => Self::Sanger,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.chip()
    }

    /// Chip on the Sequencing tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::Zip => "Zip",
            Self::Align => "Align",
            Self::Sanger => "Sanger",
            Self::Report => "Report",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Zip => "Import a Plasmidsaurus zip (tagged, never overwrites).",
            Self::Align => {
                "Pairwise overlay vs the loaded plasmid. Partial identity is never rounded up."
            }
            Self::Sanger => "Load an AB1/ABIF trace (Phred) and align if a plasmid is open.",
            Self::Report => "Bulk-align a folder of reads: verified / near / partial / divergent.",
        }
    }

    /// Empty query placeholder.
    #[must_use]
    pub fn query_hint(self) -> &'static str {
        match self {
            Self::Zip => "path to .zip",
            Self::Align => "read DNA or path",
            Self::Sanger => "path to .ab1",
            Self::Report => "reads folder",
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
    /// All notebook tools, left-to-right.
    pub const ALL: [Self; 3] = [Self::List, Self::Compose, Self::Attach];

    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::List => Self::Compose,
            Self::Compose => Self::Attach,
            Self::Attach => Self::List,
        }
    }

    /// Previous tab.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::List => Self::Attach,
            Self::Compose => Self::List,
            Self::Attach => Self::Compose,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.chip()
    }

    /// Chip on the Experiments tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::List => "List",
            Self::Compose => "Compose",
            Self::Attach => "Attach",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::List => "Notebook entries in the active project. Enter opens compose.",
            Self::Compose => "Markdown body. @plasmid !action &gel · Ctrl+G jump · F7 spellcheck.",
            Self::Attach => "Image path — Enter writes through the persist chokepoint.",
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
    /// All BLAST tools, left-to-right.
    pub const ALL: [Self; 5] = [
        Self::Local,
        Self::Orf,
        Self::Online,
        Self::HmmDb,
        Self::Find,
    ];

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

    /// Previous tab.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Local => Self::Find,
            Self::Orf => Self::Local,
            Self::Online => Self::Orf,
            Self::HmmDb => Self::Online,
            Self::Find => Self::HmmDb,
        }
    }

    /// Overlay title (short, used in status text).
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

    /// Chip label on the BLAST tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::Local => "Local BLAST",
            Self::Orf => "Find ORFs",
            Self::Online => "Online",
            Self::HmmDb => "HMM DBs",
            Self::Find => "Find plasmid",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Local => "Ungapped BLASTN / BLASTP against your library.",
            Self::Orf => "Six-frame ORF scan of the loaded record (Enter).",
            Self::Online => "NCBI / EBI — needs Settings → allow online search.",
            Self::HmmDb => "Local HMM catalog (no Pfam download in default CI).",
            Self::Find => "Fuzzy plasmid name across every collection.",
        }
    }

    /// Empty query placeholder.
    #[must_use]
    pub fn query_hint(self) -> &'static str {
        match self {
            Self::Local => "DNA or protein query",
            Self::Orf => "optional — Enter scans the canvas",
            Self::Online => "accession or sequence (online must be armed)",
            Self::HmmDb => "optional filter",
            Self::Find => "plasmid name",
        }
    }
}

impl HistoryTab {
    /// All history views, left-to-right.
    pub const ALL: [Self; 3] = [Self::Protocol, Self::Tree, Self::Detail];

    /// Next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Protocol => Self::Tree,
            Self::Tree => Self::Detail,
            Self::Detail => Self::Protocol,
        }
    }

    /// Previous tab.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Protocol => Self::Detail,
            Self::Tree => Self::Protocol,
            Self::Detail => Self::Tree,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.chip()
    }

    /// Chip on the History tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::Protocol => "Protocol",
            Self::Tree => "Tree",
            Self::Detail => "Detail",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Protocol => "Numbered construction steps, left to right.",
            Self::Tree => "Lineage tree of parents and products.",
            Self::Detail => "Step claims vs the current sequence (warnings are read-only).",
        }
    }
}

impl DesignKind {
    /// All primer designers, left-to-right.
    pub const ALL: [Self; 4] = [
        Self::Generic,
        Self::Cloning,
        Self::Detection,
        Self::GoldenBraid,
    ];

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

    /// Previous designer.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::Generic => Self::GoldenBraid,
            Self::Cloning => Self::Generic,
            Self::Detection => Self::Cloning,
            Self::GoldenBraid => Self::Detection,
        }
    }

    /// Overlay title.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.chip()
    }

    /// Chip on the Primers tab bar.
    #[must_use]
    pub fn chip(self) -> &'static str {
        match self {
            Self::Generic => "Generic",
            Self::Cloning => "Cloning",
            Self::Detection => "Detection",
            Self::GoldenBraid => "Golden Braid",
        }
    }

    /// One-line what-this-tab-does.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Generic => {
                "Binding primers, no tails. Uses the selected feature or whole record."
            }
            Self::Cloning => "Pad + EcoRI / BamHI tails for traditional cloning.",
            Self::Detection => "Pair inside the selected region for a diagnostic amplicon.",
            Self::GoldenBraid => "BsaI Golden Braid tails for the active grammar.",
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
