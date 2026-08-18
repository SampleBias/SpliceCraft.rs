//! Workbench state. Library writes go through the persist chokepoint.

use splicecraft_bio::{
    BlastProgram, CustomEnzyme, Orf, extract_feature, find_orfs, record_fingerprint,
    results_are_stale, reverse_complement_record,
};
use splicecraft_clone::{
    GibsonFragment, GrammarStore, HistoryCheckNode, HistoryNode, PartRecord, PartsBinStore,
    assemble_parts, classify_part_from_plasmid, design_gb_primers, design_gb_scrub,
    design_homology_arms, design_operon_soe_primers, excise_fragment_pair, gb_l0,
    history_detail_lines, history_node_warnings, history_protocol_lines, history_tree_lines,
    l0_part_from_syn_fragment, parse_history_xml, product_record, simulate_gibson_assembly,
    simulate_traditional_cloning, stub_entry_vector, traditional_closed,
};
use splicecraft_codon::{CodonMode, CodonTableStore, DnaBuffer, MotifStore, ProteinBuffer};
use splicecraft_core::{Record, rotate_record};
use splicecraft_gels::{
    GEL_UI_MAX_LANES, GelLane, GelRenderOpts, GelStore, PcrAmplicon, amplicon_to_record,
    append_pcr_gel_lane, render_gel_image, simulate_pcr, snapshot_gel,
};
use splicecraft_persist::{
    AlignmentBadge, CollisionChoice, CollisionClass, DataLayout, EnzymeStore, ExperimentEntry,
    ExperimentStore, FeatureSnippet, HmmDbEntry, KeepOutcome, LibraryEntry, LibraryStore,
    MASTER_DELETE_SENTINEL, SETTING_ALLOW_ONLINE_LOOKUPS, SETTING_ALLOW_ONLINE_SEARCH,
    allow_online_lookups, allow_online_search, experiment_jump_table, load_hmm_catalog,
    new_experiment_id, normalise_experiment_entry, resolve_plasmid_jump, save_experiment_image,
    set_setting_bool, spellcheck_body,
};
use splicecraft_primer::{
    PrimerRecord, PrimerStatus, PrimerStore, design_cloning_primers, design_detection_primers,
    design_generic_primers, design_golden_braid_primers, design_mutagenesis,
    insilico_pcr_amplicons, primer_binding_sites, primer_check_confidence, primer_tm, qc_primers,
    rederive_primer_binding, scrub_design,
};

use std::time::{Duration, Instant};

use crate::action::{
    Action, ConstructorTab, DesignKind, ExperimentsTab, FocusMode, HistoryTab, MutatoTab, Overlay,
    Pane, PathKind, SearchTab, SequencingTab, SimulatorTab, SynthTab,
};
use crate::autolab::{compile_protocol, confirm_motion, fixture_deck};
use crate::babs::{BabsCommand, parse_command};
use crate::commands::{Command, filter_commands, fuzzy_text_match};
use crate::editor::{UndoStack, delete_span, insert_bases, smallest_enclosing};
use crate::mapimage::{MapImageOpts, export_plasmid_map};

/// Master Delete final-confirm cooldown (upstream `_MASTER_DELETE_CONFIRM_COOLDOWN_S`).
pub const MASTER_DELETE_CONFIRM_COOLDOWN: Duration = Duration::from_secs(3);

/// In-memory workbench. Disk writes require authorisation + a layout.
#[derive(Clone, Debug)]
pub struct AppState {
    /// Help / palette / none.
    pub overlay: Overlay,
    /// Split vs single-pane.
    pub focus_mode: FocusMode,
    /// Which pane is highlighted when the split is showing.
    pub focus: Pane,
    /// Palette search box.
    pub palette_query: String,
    /// Highlighted row in the filtered palette list.
    pub palette_selected: usize,
    /// Status-bar toast (never contains sequence bases).
    pub toast: Option<String>,
    /// Loaded record, if any.
    pub record: Option<Record>,
    /// Display name for the status bar.
    pub source_label: String,
    /// Circular map vs linear backbone (view; record topology is separate).
    pub map_circular: bool,
    /// Restriction overlay.
    pub show_restr: bool,
    /// Unique-cutter filter.
    pub restr_unique: bool,
    /// When true, skip recognition sites shorter than 6 bp.
    pub restr_min_six: bool,
    /// Enzyme collections + custom catalog.
    pub enzymes: EnzymeStore,
    /// Primer library.
    pub primers: PrimerStore,
    /// Designer currently selected in the overlay.
    pub design_kind: DesignKind,
    /// Last design result (display only; not logged).
    pub design_summary: Option<String>,
    /// Last designed forward oligo (for save).
    pub design_fwd: Option<String>,
    /// Last designed reverse oligo (for save).
    pub design_rev: Option<String>,
    /// Primer-check query box.
    pub tool_query: String,
    /// Primer-check result text.
    pub check_summary: Option<String>,
    /// Highlighted enzyme collection.
    pub enzyme_selected: usize,
    /// Feature labels on the map.
    pub show_labels: bool,
    /// Display rotation in bp.
    pub view_origin: usize,
    /// Sequence cursor (0-based).
    pub cursor: usize,
    /// Selected feature index.
    pub selected_feat: Option<usize>,
    /// Deep-clone undo / redo. [INV-10]
    pub undo: UndoStack,
    /// Unsaved in-memory edits (crash-recovery candidate).
    pub dirty: bool,
    /// Named collections + live plasmid list.
    pub library: LibraryStore,
    /// Sandboxed or resolved data dir. None → keep stays in memory only.
    pub layout: Option<DataLayout>,
    /// Highlighted row in the library pane.
    pub selected_lib: usize,
    /// Keep / feature waiting on skip-copy-overwrite.
    pub pending_collision: Option<PendingCollision>,
    /// Path prompt buffer.
    pub path_query: String,
    /// What [`Action::PathSubmit`] will do.
    pub path_kind: PathKind,
    /// Constructor tab.
    pub ctor_tab: ConstructorTab,
    /// Last constructor / domestication summary (no sequence dump).
    pub ctor_summary: Option<String>,
    /// Product waiting to be kept.
    pub ctor_product: Option<Record>,
    /// 4-source picker (0 record, 1 feature, 2 library, 3 feature library).
    pub ctor_source: usize,
    /// Active grammar id.
    pub grammar_id: String,
    /// Highlighted row in constructor / parts lists.
    pub tool_selected: usize,
    /// User-defined + built-in grammars.
    pub grammars: GrammarStore,
    /// Parts bin.
    pub parts: PartsBinStore,
    /// Session stack of last-N library deletes.
    pub deleted_stack: Vec<(usize, LibraryEntry)>,
    /// Mutato tab.
    pub mutato_tab: MutatoTab,
    /// Mutation string (`V40F`) or extra enzyme names.
    pub mutato_query: String,
    /// Last Mutato / scrub summary (no sequence dump).
    pub mutato_summary: Option<String>,
    /// Synthesis tab.
    pub synth_tab: SynthTab,
    /// Linear DNA composer.
    pub dna_buf: DnaBuffer,
    /// Protein composer.
    pub protein_buf: ProteinBuffer,
    /// Last synthesis summary.
    pub synth_summary: Option<String>,
    /// Codon-table registry (K12 seeded).
    pub codon_tables: CodonTableStore,
    /// Protein motif library.
    pub motifs: MotifStore,
    /// Simulator tab.
    pub sim_tab: SimulatorTab,
    /// `FWD/REV` primer box.
    pub sim_query: String,
    /// Last PCR products (sequence stays out of logs).
    pub sim_amplicons: Vec<PcrAmplicon>,
    /// Highlighted PCR product.
    pub sim_selected: usize,
    /// Last PCR / gel summary (no sequence dump).
    pub sim_summary: Option<String>,
    /// Live gel lanes.
    pub sim_lanes: Vec<GelLane>,
    /// Agarose % for the gel tab.
    pub sim_agarose: f64,
    /// Last rendered gel image.
    pub sim_gel_image: Option<String>,
    /// True until the user sends a PCR lane (demo ladder/uncut).
    pub sim_gel_demo: bool,
    /// Saved gel snapshots.
    pub gels: GelStore,
    /// Sequencing overlay tab.
    pub seq_tab: SequencingTab,
    /// Zip / AB1 / folder path box.
    pub seq_query: String,
    /// Last sequencing summary (no DNA).
    pub seq_summary: Option<String>,
    /// Alignment overlay segments in target coordinates.
    pub seq_segments: Vec<(usize, usize, splicecraft_io::AlignState)>,
    /// Variants for jump-to-sequence.
    pub seq_variants: Vec<splicecraft_io::AlignVariant>,
    /// Highlighted variant.
    pub seq_variant_idx: usize,
    /// Zip samples listed.
    pub seq_zip_n: usize,
    /// Lab notebook.
    pub experiments: ExperimentStore,
    /// Experiments overlay tab.
    pub exp_tab: ExperimentsTab,
    /// Highlighted notebook row.
    pub exp_selected: usize,
    /// Compose title.
    pub exp_title: String,
    /// Compose markdown body.
    pub exp_body: String,
    /// Id of the entry being edited.
    pub exp_id: String,
    /// Last notebook summary (no DNA).
    pub exp_summary: Option<String>,
    /// History overlay tab.
    pub hist_tab: HistoryTab,
    /// Last history summary / warnings (no DNA).
    pub hist_summary: Option<String>,
    /// Protocol / tree / detail lines.
    pub hist_lines: Vec<String>,
    /// Warnings for the loaded molecule. Never used to edit DNA.
    pub hist_warnings: Vec<String>,
    /// Search overlay tab.
    pub search_tab: SearchTab,
    /// BLAST / find query box (never logged).
    pub search_query: String,
    /// Last search summary (counts / errors; no DNA).
    pub search_summary: Option<String>,
    /// Result rows (no sequence content).
    pub search_lines: Vec<String>,
    /// Highlighted result row.
    pub search_selected: usize,
    /// Local / online program.
    pub search_program: BlastProgram,
    /// `allow_online_search` (off until ticked).
    pub allow_online_search: bool,
    /// `allow_online_lookups` (off until ticked).
    pub allow_online_lookups: bool,
    /// Highlighted settings row.
    pub settings_selected: usize,
    /// BABS input box.
    pub babs_query: String,
    /// Transcript lines (no plasmid sequence).
    pub babs_lines: Vec<String>,
    /// Selected Ollama model name.
    pub babs_model: String,
    /// Last AUTOLAB summary.
    pub autolab_summary: Option<String>,
    /// Last compiled protocol text.
    pub autolab_protocol: Option<String>,
    /// Human armed the motion confirm (still no live robot).
    pub autolab_motion_armed: bool,
    /// Master Delete step 0..=2.
    pub master_delete_step: u8,
    /// Focus on Yes (default is No).
    pub master_delete_yes: bool,
    /// Typed confirm token.
    pub master_delete_typed: String,
    /// When step 2 started.
    pub master_delete_cooldown_start: Option<Instant>,
    /// Fingerprint of the record a search was started against.
    pub search_submitted: Option<splicecraft_bio::RecordFingerprint>,
    /// HMM-DB catalog (builtins re-injected).
    pub hmm_catalog: Vec<HmmDbEntry>,
    /// Cancel token for an in-flight online poll.
    pub search_cancel: splicecraft_io::CancellationToken,
}

/// Collision modal payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingCollision {
    /// Keep a plasmid.
    Keep(LibraryEntry),
    /// Save a feature snippet.
    Feature(FeatureSnippet),
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Empty canvas — no demo, no library write.
    #[must_use]
    pub fn new() -> Self {
        Self {
            overlay: Overlay::None,
            focus_mode: FocusMode::All,
            focus: Pane::Map,
            palette_query: String::new(),
            palette_selected: 0,
            toast: None,
            record: None,
            source_label: "(no record)".into(),
            map_circular: true,
            show_restr: false,
            restr_unique: false,
            restr_min_six: true,
            enzymes: EnzymeStore::default(),
            primers: PrimerStore::default(),
            design_kind: DesignKind::Generic,
            design_summary: None,
            design_fwd: None,
            design_rev: None,
            tool_query: String::new(),
            check_summary: None,
            enzyme_selected: 0,
            show_labels: true,
            view_origin: 0,
            cursor: 0,
            selected_feat: None,
            undo: UndoStack::new(),
            dirty: false,
            library: LibraryStore::new(),
            layout: None,
            selected_lib: 0,
            pending_collision: None,
            path_query: String::new(),
            path_kind: PathKind::OpenFile,
            ctor_tab: ConstructorTab::Traditional,
            ctor_summary: None,
            ctor_product: None,
            ctor_source: 0,
            grammar_id: "gb_l0".into(),
            tool_selected: 0,
            grammars: GrammarStore::default(),
            parts: PartsBinStore::default(),
            deleted_stack: Vec::new(),
            mutato_tab: MutatoTab::Sdm,
            mutato_query: String::new(),
            mutato_summary: None,
            synth_tab: SynthTab::Dna,
            dna_buf: DnaBuffer::default(),
            protein_buf: ProteinBuffer::default(),
            synth_summary: None,
            codon_tables: CodonTableStore::with_builtin_k12(),
            motifs: MotifStore::default(),
            sim_tab: SimulatorTab::Pcr,
            sim_query: String::new(),
            sim_amplicons: Vec::new(),
            sim_selected: 0,
            sim_summary: None,
            sim_lanes: Vec::new(),
            sim_agarose: 1.0,
            sim_gel_image: None,
            sim_gel_demo: true,
            gels: GelStore::new(),
            seq_tab: SequencingTab::Zip,
            seq_query: String::new(),
            seq_summary: None,
            seq_segments: Vec::new(),
            seq_variants: Vec::new(),
            seq_variant_idx: 0,
            seq_zip_n: 0,
            experiments: ExperimentStore::new(),
            exp_tab: ExperimentsTab::List,
            exp_selected: 0,
            exp_title: String::new(),
            exp_body: String::new(),
            exp_id: String::new(),
            exp_summary: None,
            hist_tab: HistoryTab::Protocol,
            hist_summary: None,
            hist_lines: Vec::new(),
            hist_warnings: Vec::new(),
            search_tab: SearchTab::Local,
            search_query: String::new(),
            search_summary: None,
            search_lines: Vec::new(),
            search_selected: 0,
            search_program: BlastProgram::Blastn,
            allow_online_search: false,
            allow_online_lookups: false,
            settings_selected: 0,
            babs_query: String::new(),
            babs_lines: Vec::new(),
            babs_model: "llama".into(),
            autolab_summary: None,
            autolab_protocol: None,
            autolab_motion_armed: false,
            master_delete_step: 0,
            master_delete_yes: false,
            master_delete_typed: String::new(),
            master_delete_cooldown_start: None,
            search_submitted: None,
            hmm_catalog: splicecraft_persist::builtin_hmm_db_catalog().to_vec(),
            search_cancel: splicecraft_io::CancellationToken::new(),
        }
    }

    /// Attach a data dir and load collections from disk.
    pub fn attach_layout(&mut self, layout: DataLayout) {
        self.library = LibraryStore::load(&layout);
        self.enzymes = EnzymeStore::load(&layout);
        self.primers = PrimerStore::load(&layout);
        self.grammars = GrammarStore::load(&layout);
        self.parts = PartsBinStore::load(&layout);
        self.codon_tables = CodonTableStore::load(&layout);
        self.motifs = MotifStore::load(&layout);
        self.gels = GelStore::load(&layout);
        self.experiments = ExperimentStore::load(&layout);
        self.allow_online_search = allow_online_search(&layout);
        self.allow_online_lookups = allow_online_lookups(&layout);
        self.hmm_catalog = load_hmm_catalog(&layout);
        self.layout = Some(layout);
        self.clamp_lib_selection();
        self.clamp_enzyme_selection();
    }

    /// Apply `action`. Returns `false` when the event loop should exit.
    pub fn reduce(&mut self, action: Action) -> bool {
        match action {
            Action::Quit => return false,
            Action::ToggleHelp => {
                self.toast = None;
                if self.overlay == Overlay::Help {
                    self.overlay = Overlay::None;
                } else {
                    self.overlay = Overlay::Help;
                }
            }
            Action::CloseOverlay => {
                self.search_cancel.cancel();
                self.overlay = Overlay::None;
                self.reset_palette();
                self.path_query.clear();
                self.tool_query.clear();
                self.pending_collision = None;
                self.reset_master_delete();
            }
            Action::OpenPalette => {
                self.toast = None;
                self.overlay = Overlay::Palette;
                self.reset_palette();
            }
            Action::PaletteInput(c) => {
                if !c.is_control() {
                    self.palette_query.push(c);
                    self.clamp_palette_selection();
                }
            }
            Action::PaletteBackspace => {
                self.palette_query.pop();
                self.clamp_palette_selection();
            }
            Action::PaletteMove(delta) => {
                let n = self.visible_commands().len();
                if n == 0 {
                    self.palette_selected = 0;
                } else {
                    let cur = self.palette_selected as i32 + delta;
                    self.palette_selected = cur.rem_euclid(n as i32) as usize;
                }
            }
            Action::PaletteExecute => {
                if let Some(cmd) = self.selected_command() {
                    let next = cmd.action;
                    self.overlay = Overlay::None;
                    self.reset_palette();
                    return self.reduce(next);
                }
            }
            Action::FocusPane(pane) => {
                self.focus = pane;
                self.focus_mode = FocusMode::Single(pane);
            }
            Action::FocusAll => {
                self.focus_mode = FocusMode::All;
            }
            Action::LoadDemo => {
                self.record = Some(demo_record());
                self.source_label = "pDemo (memory)".into();
                self.cursor = 0;
                self.view_origin = 0;
                self.undo = UndoStack::new();
                self.dirty = false;
                self.toast = Some("Loaded memory-only demo — not saved".into());
            }
            Action::ToggleMapView => {
                self.map_circular = !self.map_circular;
            }
            Action::ToggleRestr => {
                self.show_restr = !self.show_restr;
            }
            Action::ToggleRestrUnique => {
                self.restr_unique = !self.restr_unique;
                self.toast = Some(if self.restr_unique {
                    "RE overlay: unique cutters".into()
                } else {
                    "RE overlay: all sites".into()
                });
            }
            Action::ToggleRestrSixPlus => {
                self.restr_min_six = !self.restr_min_six;
                self.toast = Some(if self.restr_min_six {
                    "RE overlay: 6+ recognition".into()
                } else {
                    "RE overlay: 4+ recognition".into()
                });
            }
            Action::CycleEnzymeCollection(delta) => self.cycle_enzyme_collection(delta),
            Action::OpenPrimerDesign => {
                self.overlay = Overlay::PrimerDesign;
                self.toast = None;
            }
            Action::OpenPrimerCheck => {
                self.overlay = Overlay::PrimerCheck;
                self.tool_query.clear();
                self.check_summary = None;
                self.toast = None;
            }
            Action::OpenEnzymes => {
                self.overlay = Overlay::Enzymes;
                self.clamp_enzyme_selection();
                self.toast = None;
            }
            Action::OpenConstructor => {
                self.overlay = Overlay::Constructor;
                self.ctor_summary = None;
                self.toast = None;
            }
            Action::OpenParts => {
                self.overlay = Overlay::Parts;
                self.tool_selected = 0;
                self.toast = None;
            }
            Action::OpenMutato => {
                self.overlay = Overlay::Mutato;
                self.mutato_summary = None;
                self.toast = None;
            }
            Action::OpenSynthesis => {
                self.overlay = Overlay::Synthesis;
                self.synth_summary = None;
                self.toast = None;
            }
            Action::OpenSimulator => {
                self.overlay = Overlay::Simulator;
                self.sim_summary = None;
                self.toast = None;
                if self.sim_lanes.is_empty() {
                    self.seed_demo_gel();
                }
            }
            Action::OpenSequencing => {
                self.overlay = Overlay::Sequencing;
                self.seq_summary = None;
                self.toast = None;
            }
            Action::OpenExperiments => {
                self.overlay = Overlay::Experiments;
                self.exp_summary = None;
                self.toast = None;
                self.clamp_exp_selection();
            }
            Action::OpenHistory => {
                self.overlay = Overlay::History;
                self.toast = None;
                self.refresh_history();
            }
            Action::OpenSearch => {
                self.overlay = Overlay::Search;
                self.search_summary = None;
                self.toast = None;
                self.search_cancel = splicecraft_io::CancellationToken::new();
                if let Some(layout) = &self.layout {
                    self.allow_online_search = allow_online_search(layout);
                    self.hmm_catalog = load_hmm_catalog(layout);
                }
            }
            Action::OpenSettings => {
                self.overlay = Overlay::Settings;
                self.settings_selected = 0;
                self.toast = None;
                if let Some(layout) = &self.layout {
                    self.allow_online_search = allow_online_search(layout);
                    self.allow_online_lookups = allow_online_lookups(layout);
                }
            }
            Action::OpenBabs => {
                self.overlay = Overlay::Babs;
                self.toast = None;
                if self.babs_lines.is_empty() {
                    self.babs_lines
                        .push("Local Ollama only. Sequences are never sent off-loopback.".into());
                }
            }
            Action::OpenAutolab => {
                self.overlay = Overlay::Autolab;
                self.toast = None;
            }
            Action::OpenMasterDelete => {
                self.overlay = Overlay::MasterDelete;
                self.reset_master_delete();
                self.toast = None;
            }
            Action::ExportMapPrompt => {
                self.overlay = Overlay::Path;
                self.path_kind = PathKind::MapExport;
                self.path_query.clear();
            }
            Action::ExportMigratePrompt => {
                self.overlay = Overlay::Path;
                self.path_kind = PathKind::MigrateExport;
                self.path_query.clear();
            }
            Action::ImportMigratePrompt => {
                self.overlay = Overlay::Path;
                self.path_kind = PathKind::MigrateImport;
                self.path_query.clear();
            }
            Action::ToggleOnlineSearch => self.toggle_online_search(),
            Action::ToggleOnlineLookups => self.toggle_online_lookups(),
            Action::AutolabCompile => self.run_autolab_compile(),
            Action::AutolabArmMotion => {
                self.autolab_motion_armed = true;
                self.toast = Some("Motion armed — still requires confirm; no robot in CI".into());
            }
            Action::RecoverHistory => self.run_history_recover(true),
            Action::ExperimentJump => self.jump_experiment_ref(),
            Action::ExperimentSpellcheck => self.spellcheck_experiment(),
            Action::SequencingJump => self.jump_to_variant(),
            Action::ConstructorSave => self.save_constructor_product(),
            Action::SimulatorSave => self.save_simulator(),
            Action::SimulatorSendToGel => self.send_pcr_to_gel(),
            Action::ConstructorDesignArms => self.run_gibson_arms(),
            Action::LibraryDelete => self.delete_selected_plasmid(),
            Action::LibraryUndelete => self.undelete_plasmid(),
            Action::ToolTab => {
                if self.overlay == Overlay::PrimerDesign {
                    self.design_kind = self.design_kind.next();
                    self.design_summary = None;
                } else if self.overlay == Overlay::Constructor {
                    self.ctor_tab = self.ctor_tab.next();
                    self.ctor_summary = None;
                    self.ctor_product = None;
                } else if self.overlay == Overlay::Mutato {
                    self.mutato_tab = self.mutato_tab.next();
                    self.mutato_summary = None;
                } else if self.overlay == Overlay::Synthesis {
                    self.synth_tab = self.synth_tab.next();
                    self.synth_summary = None;
                } else if self.overlay == Overlay::Simulator {
                    self.sim_tab = self.sim_tab.next();
                    self.sim_summary = None;
                } else if self.overlay == Overlay::Sequencing {
                    self.seq_tab = self.seq_tab.next();
                    self.seq_summary = None;
                } else if self.overlay == Overlay::Experiments {
                    self.exp_tab = self.exp_tab.next();
                    self.exp_summary = None;
                } else if self.overlay == Overlay::History {
                    self.hist_tab = self.hist_tab.next();
                    self.refresh_history_tab();
                } else if self.overlay == Overlay::Search {
                    self.search_tab = self.search_tab.next();
                    self.search_summary = None;
                } else if self.overlay == Overlay::Autolab {
                    self.autolab_motion_armed = !self.autolab_motion_armed;
                    self.toast = Some(if self.autolab_motion_armed {
                        "Motion armed — confirm still required; no live robot".into()
                    } else {
                        "Motion disarmed".into()
                    });
                }
            }
            Action::ToolEnter => match self.overlay {
                Overlay::PrimerDesign => self.run_primer_design(),
                Overlay::PrimerCheck => self.run_primer_check(),
                Overlay::Enzymes => self.activate_selected_collection(),
                Overlay::Constructor => self.run_constructor(),
                Overlay::Mutato => self.run_mutato(),
                Overlay::Synthesis => self.run_synthesis(),
                Overlay::Simulator => self.run_simulator(),
                Overlay::Sequencing => self.run_sequencing(),
                Overlay::Experiments => self.run_experiments(),
                Overlay::History => self.run_history(),
                Overlay::Search => self.run_search(),
                Overlay::Parts => self.classify_into_parts_bin(),
                Overlay::Settings => self.toggle_selected_setting(),
                Overlay::Babs => self.run_babs(),
                Overlay::Autolab => self.run_autolab_compile(),
                Overlay::MasterDelete => self.advance_master_delete(),
                _ => {}
            },
            Action::PrimerDesignSave => self.save_designed_primers(),
            Action::ToolInput(c) => {
                if matches!(
                    self.overlay,
                    Overlay::PrimerCheck
                        | Overlay::Constructor
                        | Overlay::Mutato
                        | Overlay::Synthesis
                        | Overlay::Simulator
                        | Overlay::Sequencing
                        | Overlay::Experiments
                        | Overlay::History
                        | Overlay::Search
                        | Overlay::Babs
                        | Overlay::MasterDelete
                ) && !c.is_control()
                {
                    if self.overlay == Overlay::Mutato {
                        self.mutato_query.push(c);
                    } else if self.overlay == Overlay::Simulator {
                        self.sim_query.push(c);
                    } else if self.overlay == Overlay::Sequencing {
                        self.seq_query.push(c);
                    } else if self.overlay == Overlay::Experiments {
                        match self.exp_tab {
                            ExperimentsTab::Compose => self.exp_body.push(c),
                            ExperimentsTab::Attach | ExperimentsTab::List => {
                                self.tool_query.push(c)
                            }
                        }
                    } else if self.overlay == Overlay::History {
                        self.tool_query.push(c);
                    } else if self.overlay == Overlay::Search {
                        self.search_query.push(c);
                    } else if self.overlay == Overlay::Babs {
                        self.babs_query.push(c);
                    } else if self.overlay == Overlay::MasterDelete {
                        if self.master_delete_step == 1 {
                            self.master_delete_typed.push(c);
                        }
                    } else if self.overlay == Overlay::Synthesis {
                        match self.synth_tab {
                            SynthTab::Dna => self.dna_buf.insert(&c.to_string()),
                            SynthTab::Protein => self.protein_buf.insert(&c.to_string()),
                            SynthTab::Operon => self.tool_query.push(c),
                        }
                    } else {
                        self.tool_query.push(c);
                    }
                }
            }
            Action::ToolBackspace => {
                if matches!(
                    self.overlay,
                    Overlay::PrimerCheck
                        | Overlay::Constructor
                        | Overlay::Mutato
                        | Overlay::Synthesis
                        | Overlay::Simulator
                        | Overlay::Sequencing
                        | Overlay::Experiments
                        | Overlay::History
                        | Overlay::Search
                        | Overlay::Babs
                        | Overlay::MasterDelete
                ) {
                    if self.overlay == Overlay::Mutato {
                        self.mutato_query.pop();
                    } else if self.overlay == Overlay::Simulator {
                        self.sim_query.pop();
                    } else if self.overlay == Overlay::Sequencing {
                        self.seq_query.pop();
                    } else if self.overlay == Overlay::Experiments {
                        match self.exp_tab {
                            ExperimentsTab::Compose => {
                                self.exp_body.pop();
                            }
                            ExperimentsTab::Attach | ExperimentsTab::List => {
                                self.tool_query.pop();
                            }
                        }
                    } else if self.overlay == Overlay::History {
                        self.tool_query.pop();
                    } else if self.overlay == Overlay::Search {
                        self.search_query.pop();
                    } else if self.overlay == Overlay::Babs {
                        self.babs_query.pop();
                    } else if self.overlay == Overlay::MasterDelete {
                        self.master_delete_typed.pop();
                    } else if self.overlay == Overlay::Synthesis {
                        match self.synth_tab {
                            SynthTab::Dna => {
                                let c = self.dna_buf.cursor;
                                if c > 0 {
                                    self.dna_buf.delete_range(c - 1, c);
                                }
                            }
                            SynthTab::Protein => {
                                let c = self.protein_buf.cursor;
                                if c > 0 {
                                    let lo = c - 1;
                                    self.protein_buf.aa = {
                                        let mut chars: Vec<char> =
                                            self.protein_buf.aa.chars().collect();
                                        if lo < chars.len() {
                                            chars.remove(lo);
                                        }
                                        chars.into_iter().collect()
                                    };
                                    self.protein_buf.cursor = lo;
                                }
                            }
                            SynthTab::Operon => {
                                self.tool_query.pop();
                            }
                        }
                    } else {
                        self.tool_query.pop();
                    }
                }
            }
            Action::ToolMove(delta) => {
                if self.overlay == Overlay::Enzymes {
                    let n = self.enzymes.collections.len();
                    if n == 0 {
                        self.enzyme_selected = 0;
                    } else {
                        let cur = self.enzyme_selected as i32 + delta;
                        self.enzyme_selected = cur.rem_euclid(n as i32) as usize;
                    }
                } else if self.overlay == Overlay::PrimerDesign {
                    let n = self.primers.primers.len();
                    if n > 0 {
                        let cur = self.enzyme_selected as i32 + delta;
                        self.enzyme_selected = cur.rem_euclid(n as i32) as usize;
                    }
                } else if self.overlay == Overlay::Constructor {
                    self.ctor_source = (self.ctor_source as i32 + delta).rem_euclid(4) as usize;
                } else if self.overlay == Overlay::Parts {
                    let n = self.parts.parts.len();
                    if n > 0 {
                        self.tool_selected =
                            (self.tool_selected as i32 + delta).rem_euclid(n as i32) as usize;
                    }
                } else if self.overlay == Overlay::Simulator {
                    if self.sim_tab == SimulatorTab::Pcr {
                        let n = self.sim_amplicons.len();
                        if n > 0 {
                            self.sim_selected =
                                (self.sim_selected as i32 + delta).rem_euclid(n as i32) as usize;
                        }
                    } else {
                        let choices = splicecraft_gels::AGAROSE_RANGES;
                        let cur = choices
                            .iter()
                            .enumerate()
                            .min_by(|(_, a), (_, b)| {
                                (a.0 - self.sim_agarose)
                                    .abs()
                                    .partial_cmp(&(b.0 - self.sim_agarose).abs())
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|(i, _)| i)
                            .unwrap_or(3);
                        let next = (cur as i32 + delta).rem_euclid(choices.len() as i32) as usize;
                        self.sim_agarose = choices[next].0;
                    }
                } else if self.overlay == Overlay::Sequencing {
                    let n = self.seq_variants.len();
                    if n > 0 {
                        self.seq_variant_idx =
                            (self.seq_variant_idx as i32 + delta).rem_euclid(n as i32) as usize;
                    }
                } else if self.overlay == Overlay::Experiments {
                    let n = self.experiments.entries.len();
                    if n > 0 {
                        self.exp_selected =
                            (self.exp_selected as i32 + delta).rem_euclid(n as i32) as usize;
                    }
                } else if self.overlay == Overlay::Search {
                    if self.search_tab == SearchTab::Local {
                        self.search_program = match (self.search_program, delta.signum()) {
                            (BlastProgram::Blastn, 1) => BlastProgram::Blastp,
                            (BlastProgram::Blastp, 1) => BlastProgram::Hmmscan,
                            (BlastProgram::Hmmscan, 1) => BlastProgram::Blastn,
                            (BlastProgram::Blastn, _) => BlastProgram::Hmmscan,
                            (BlastProgram::Blastp, _) => BlastProgram::Blastn,
                            (BlastProgram::Hmmscan, _) => BlastProgram::Blastp,
                        };
                    } else {
                        let n = self.search_lines.len();
                        if n > 0 {
                            self.search_selected =
                                (self.search_selected as i32 + delta).rem_euclid(n as i32) as usize;
                        }
                    }
                } else if self.overlay == Overlay::Settings {
                    self.settings_selected =
                        (self.settings_selected as i32 + delta).rem_euclid(2) as usize;
                } else if self.overlay == Overlay::MasterDelete {
                    self.master_delete_yes = delta > 0;
                }
            }
            Action::PrimerLibCycleStatus => {
                if !self.primers.primers.is_empty() {
                    let i = self.enzyme_selected.min(self.primers.primers.len() - 1);
                    self.primers.cycle_status(i);
                    self.persist_primers();
                    if let Some(p) = self.primers.primers.get(i) {
                        self.toast = Some(format!("{} → {}", p.name, p.status));
                    }
                }
            }
            Action::ToggleLabels => {
                self.show_labels = !self.show_labels;
            }
            Action::MoveCursor(delta) => {
                if let Some(rec) = &self.record {
                    let n = rec.len();
                    if n == 0 {
                        self.cursor = 0;
                    } else {
                        let cur = self.cursor as i64 + i64::from(delta);
                        self.cursor = cur.rem_euclid(n as i64) as usize;
                    }
                }
            }
            Action::RotateView(delta) => {
                if let Some(rec) = &self.record {
                    let n = rec.len();
                    if n > 0 {
                        let cur = self.view_origin as i64 + i64::from(delta);
                        self.view_origin = cur.rem_euclid(n as i64) as usize;
                    }
                }
            }
            Action::ResetView => {
                self.view_origin = 0;
                self.cursor = 0;
            }
            Action::InsertBase(ch) => self.edit_insert(ch),
            Action::DeleteBack => self.edit_delete(),
            Action::EnterPickFeature => {
                if let Some(rec) = &self.record {
                    self.selected_feat = smallest_enclosing(rec, self.cursor);
                    if let Some(i) = self.selected_feat {
                        self.toast = Some(format!("feature {}", rec.features[i].label));
                    }
                }
            }
            Action::Undo => self.apply_undo(),
            Action::Redo => self.apply_redo(),
            Action::FlipRecord => self.flip(),
            Action::SetOriginHere => self.set_origin(),
            Action::KeepRecord => self.keep_record(),
            Action::CollisionPick(choice) => self.resolve_collision(choice),
            Action::SaveSelectedFeature => self.save_selected_feature(),
            Action::LibraryMove(delta) => {
                let n = self.library.plasmids.len();
                if n == 0 {
                    self.selected_lib = 0;
                } else {
                    let cur = self.selected_lib as i32 + delta;
                    self.selected_lib = cur.rem_euclid(n as i32) as usize;
                }
            }
            Action::LibraryOpen => self.open_library_entry(),
            Action::OpenPathPrompt => {
                self.overlay = Overlay::Path;
                self.path_kind = PathKind::OpenFile;
                self.path_query.clear();
            }
            Action::BulkImportPrompt => {
                self.overlay = Overlay::Path;
                self.path_kind = PathKind::BulkImport;
                self.path_query.clear();
            }
            Action::BulkExportPrompt => {
                self.overlay = Overlay::Path;
                self.path_kind = PathKind::BulkExport;
                self.path_query.clear();
            }
            Action::PathInput(c) => {
                if !c.is_control() {
                    self.path_query.push(c);
                }
            }
            Action::PathBackspace => {
                self.path_query.pop();
            }
            Action::PathSubmit => self.submit_path(),
            Action::Stub { name, stage: _ } => {
                self.toast = Some(format!("{name} — tracked gap; see docs/parity.md"));
            }
        }
        true
    }

    /// Commands matching the current query.
    #[must_use]
    pub fn visible_commands(&self) -> Vec<Command> {
        filter_commands(&self.palette_query)
    }

    fn selected_command(&self) -> Option<Command> {
        let cmds = self.visible_commands();
        cmds.get(self.palette_selected).copied()
    }

    fn reset_palette(&mut self) {
        self.palette_query.clear();
        self.palette_selected = 0;
    }

    fn clamp_palette_selection(&mut self) {
        let n = self.visible_commands().len();
        if n == 0 {
            self.palette_selected = 0;
        } else {
            self.palette_selected = self.palette_selected.min(n - 1);
        }
    }

    fn with_record_mut(&mut self, f: impl FnOnce(&mut Self, Record)) {
        if let Some(rec) = self.record.take() {
            f(self, rec);
        }
    }

    fn edit_insert(&mut self, ch: char) {
        self.with_record_mut(|st, rec| {
            st.undo.push(&rec);
            let at = st.cursor.min(rec.len());
            let next = insert_bases(&rec, at, &ch.to_string());
            st.cursor = (at + 1).min(next.len());
            st.record = Some(next);
            st.dirty = true;
        });
    }

    fn edit_delete(&mut self) {
        self.with_record_mut(|st, rec| {
            if st.cursor == 0 || rec.is_empty() {
                st.record = Some(rec);
                return;
            }
            st.undo.push(&rec);
            let from = st.cursor - 1;
            let next = delete_span(&rec, from, st.cursor);
            st.cursor = from.min(next.len());
            st.record = Some(next);
            st.dirty = true;
        });
    }

    fn apply_undo(&mut self) {
        self.with_record_mut(|st, rec| {
            if let Some(prev) = st.undo.undo(&rec) {
                st.cursor = st.cursor.min(prev.len().saturating_sub(1));
                st.record = Some(prev);
                st.dirty = true;
            } else {
                st.record = Some(rec);
                st.toast = Some("Nothing to undo".into());
            }
        });
    }

    fn apply_redo(&mut self) {
        self.with_record_mut(|st, rec| {
            if let Some(next) = st.undo.redo(&rec) {
                st.cursor = st.cursor.min(next.len().saturating_sub(1));
                st.record = Some(next);
                st.dirty = true;
            } else {
                st.record = Some(rec);
                st.toast = Some("Nothing to redo".into());
            }
        });
    }

    fn flip(&mut self) {
        self.with_record_mut(|st, rec| {
            if rec.is_empty() {
                st.record = Some(rec);
                st.toast = Some("Nothing loaded to flip".into());
                return;
            }
            st.undo.push(&rec);
            let n = rec.len();
            let next = reverse_complement_record(&rec);
            if next.len() != n {
                st.undo.pop_silent();
                st.record = Some(rec);
                st.toast = Some("Flip aborted — length changed".into());
                return;
            }
            st.cursor = if n == 0 {
                0
            } else {
                n - 1 - st.cursor.min(n - 1)
            };
            st.record = Some(next);
            st.dirty = true;
            st.toast = Some("Flipped (reverse complement)".into());
        });
    }

    fn set_origin(&mut self) {
        self.with_record_mut(|st, rec| {
            if !rec.circular {
                st.record = Some(rec);
                st.toast =
                    Some("Set origin needs a CIRCULAR molecule — this record is linear".into());
                return;
            }
            if rec.is_empty() {
                st.record = Some(rec);
                return;
            }
            st.undo.push(&rec);
            let offset = st.cursor % rec.len();
            let next = rotate_record(&rec, offset);
            st.cursor = 0;
            st.view_origin = 0;
            st.record = Some(next);
            st.dirty = true;
            st.toast = Some("Origin set at cursor".into());
        });
    }

    /// Custom enzymes in the bio-layer shape the scanner expects.
    #[must_use]
    pub fn custom_for_scan(&self) -> Vec<CustomEnzyme> {
        self.enzymes
            .custom
            .iter()
            .map(|e| CustomEnzyme {
                name: e.name.clone(),
                site: e.site.clone(),
                fwd_cut: e.fwd_cut,
                rev_cut: e.rev_cut,
            })
            .collect()
    }

    fn clamp_enzyme_selection(&mut self) {
        let n = self.enzymes.collections.len();
        self.enzyme_selected = if n == 0 {
            0
        } else {
            self.enzyme_selected.min(n - 1)
        };
    }

    fn cycle_enzyme_collection(&mut self, delta: i32) {
        let n = self.enzymes.collections.len();
        if n == 0 {
            self.enzymes.active = None;
            self.toast = Some("Full NEB catalog (no collections)".into());
            return;
        }
        let names: Vec<Option<String>> = std::iter::once(None)
            .chain(
                self.enzymes
                    .collections
                    .iter()
                    .map(|c| Some(c.name.clone())),
            )
            .collect();
        let cur = names
            .iter()
            .position(|n| n.as_deref() == self.enzymes.active.as_deref())
            .unwrap_or(0);
        let next = (cur as i32 + delta).rem_euclid(names.len() as i32) as usize;
        self.enzymes.active = names[next].clone();
        self.persist_enzymes();
        self.toast = Some(match &self.enzymes.active {
            Some(name) => format!("Enzyme collection: {name}"),
            None => "Enzyme collection: all NEB".into(),
        });
    }

    fn activate_selected_collection(&mut self) {
        let Some(name) = self
            .enzymes
            .collections
            .get(self.enzyme_selected)
            .map(|c| c.name.clone())
        else {
            self.enzymes.active = None;
            self.persist_enzymes();
            self.toast = Some("Full NEB catalog".into());
            return;
        };
        if self.enzymes.active.as_deref() == Some(name.as_str()) {
            self.enzymes.active = None;
            self.toast = Some("Enzyme collection: all NEB".into());
        } else {
            self.enzymes.active = Some(name.clone());
            self.toast = Some(format!("Enzyme collection: {name}"));
        }
        self.persist_enzymes();
    }

    fn persist_enzymes(&mut self) {
        let Some(layout) = &self.layout else {
            return;
        };
        if !splicecraft_persist::writes_authorized() {
            return;
        }
        if let Err(e) = self.enzymes.persist(layout) {
            self.toast = Some(format!("Enzyme save failed: {e}"));
        }
    }

    fn persist_primers(&mut self) {
        let Some(layout) = &self.layout else {
            return;
        };
        if !splicecraft_persist::writes_authorized() {
            return;
        }
        if let Err(e) = self.primers.persist(layout) {
            self.toast = Some(format!("Primer save failed: {e}"));
        }
    }

    fn design_span(&self) -> Option<(usize, usize)> {
        let rec = self.record.as_ref()?;
        if let Some(i) = self.selected_feat
            && let Some(f) = rec.features.get(i)
        {
            return Some((f.start, f.end));
        }
        Some((0, rec.len()))
    }

    fn run_primer_design(&mut self) {
        let Some(rec) = &self.record else {
            self.toast = Some("Nothing loaded to design against".into());
            return;
        };
        let Some((start, end)) = self.design_span() else {
            self.toast = Some("Nothing loaded to design against".into());
            return;
        };
        let extra = self.custom_for_scan();
        let result = match self.design_kind {
            DesignKind::Generic => {
                design_generic_primers(&rec.sequence, start, end, 60.0).map(|g| {
                    (
                        g.fwd_seq,
                        g.rev_seq,
                        g.fwd_tm,
                        g.rev_tm,
                        format!(
                            "  generic  Tm {:.1}/{:.1}  pos {}..{} / {}..{}",
                            g.fwd_tm, g.rev_tm, g.fwd_pos.0, g.fwd_pos.1, g.rev_pos.0, g.rev_pos.1
                        ),
                    )
                })
            }
            DesignKind::Cloning => design_cloning_primers(
                &rec.sequence,
                start,
                end,
                "EcoRI",
                "BamHI",
                60.0,
                "GCGC",
                &extra,
            )
            .map(|c| {
                (
                    c.fwd_full,
                    c.rev_full,
                    c.fwd_tm,
                    c.rev_tm,
                    format!("  cloning EcoRI/BamHI  Tm {:.1}/{:.1}", c.fwd_tm, c.rev_tm),
                )
            }),
            DesignKind::Detection => {
                let lo = 80.min(rec.len().saturating_sub(1));
                let hi = rec.len();
                design_detection_primers(
                    &rec.sequence,
                    start.min(lo),
                    end.min(hi).max(start + 1),
                    80,
                    200,
                    60.0,
                )
                .or_else(|_| {
                    design_detection_primers(&rec.sequence, start, end, 18, rec.len().max(18), 60.0)
                })
                .map(|d| {
                    (
                        d.fwd_seq,
                        d.rev_seq,
                        d.fwd_tm,
                        d.rev_tm,
                        format!(
                            "  detection  {} bp  Tm {:.1}/{:.1}",
                            d.product_size, d.fwd_tm, d.rev_tm
                        ),
                    )
                })
            }
            DesignKind::GoldenBraid => {
                design_golden_braid_primers(&rec.sequence, start, end, 60.0, &extra).map(|c| {
                    (
                        c.fwd_full,
                        c.rev_full,
                        c.fwd_tm,
                        c.rev_tm,
                        format!("  golden braid BsaI  Tm {:.1}/{:.1}", c.fwd_tm, c.rev_tm),
                    )
                })
            }
        };
        match result {
            Ok((fwd, rev, _ft, _rt, summary)) => {
                // Re-derive so the toast positions match the anneal after rotation.
                let _ = rederive_primer_binding(&fwd, 1, &rec.sequence, start, rec.circular);
                self.design_fwd = Some(fwd);
                self.design_rev = Some(rev);
                self.design_summary = Some(summary);
                self.toast = Some(format!("Designed {} pair", self.design_kind.label()));
            }
            Err(e) => {
                self.design_fwd = None;
                self.design_rev = None;
                self.design_summary = Some(format!("  {e}"));
                self.toast = Some("Design failed".into());
            }
        }
    }

    fn save_designed_primers(&mut self) {
        let (Some(fwd), Some(rev)) = (self.design_fwd.clone(), self.design_rev.clone()) else {
            self.toast = Some("Nothing designed to save".into());
            return;
        };
        let kind = self.design_kind.label();
        self.primers.add(PrimerRecord {
            name: format!("{kind}-fwd"),
            sequence: fwd.clone(),
            tm: primer_tm(&fwd),
            status: PrimerStatus::Designed,
            primer_type: kind.into(),
            ..PrimerRecord::default()
        });
        self.primers.add(PrimerRecord {
            name: format!("{kind}-rev"),
            sequence: rev.clone(),
            tm: primer_tm(&rev),
            status: PrimerStatus::Designed,
            primer_type: kind.into(),
            ..PrimerRecord::default()
        });
        self.persist_primers();
        self.toast = Some("Saved designed pair (Designed)".into());
    }

    fn run_primer_check(&mut self) {
        let Some(rec) = &self.record else {
            self.toast = Some("Nothing loaded to check against".into());
            return;
        };
        let raw = self.tool_query.trim();
        if raw.is_empty() {
            self.toast = Some("Type one or two oligos".into());
            return;
        }
        let parts: Vec<&str> = raw
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .filter(|s| !s.is_empty())
            .collect();
        let top = rec.sequence.to_ascii_uppercase();
        match parts.as_slice() {
            [one] => match primer_binding_sites(one, &top, rec.circular, 12, 0.0) {
                Ok(sites) => {
                    let n = sites.len();
                    let ident = sites.first().map(|s| s.ident_pct);
                    let (glyph, _) = primer_check_confidence(ident);
                    self.check_summary = Some(format!(
                        "  {n} site(s)  {glyph}  {}",
                        ident
                            .map(|v| format!("{v:.1}%"))
                            .unwrap_or_else(|| "—".into())
                    ));
                    self.toast = Some(format!("Primer-check: {n} site(s)"));
                }
                Err(e) => {
                    self.check_summary = Some(format!("  {e}"));
                    self.toast = Some("Primer-check failed".into());
                }
            },
            [a, b] => {
                let s1 = primer_binding_sites(a, &top, rec.circular, 12, 0.0);
                let s2 = primer_binding_sites(b, &top, rec.circular, 12, 0.0);
                match (s1, s2) {
                    (Ok(s1), Ok(s2)) => {
                        let amps =
                            insilico_pcr_amplicons(&s1, &s2, top.len(), rec.circular, 20_000);
                        if let Some(amp) = amps.first() {
                            let (glyph, _) = primer_check_confidence(Some(amp.certainty));
                            self.check_summary = Some(format!(
                                "  amplicon {} bp at {}  {glyph}  {:.1}%",
                                amp.length, amp.start, amp.certainty
                            ));
                            self.toast = Some(format!("Amplicon {} bp", amp.length));
                        } else {
                            self.check_summary = Some("  no amplicon".into());
                            self.toast = Some("No amplicon".into());
                        }
                    }
                    (Err(e), _) | (_, Err(e)) => {
                        self.check_summary = Some(format!("  {e}"));
                        self.toast = Some("Primer-check failed".into());
                    }
                }
            }
            _ => {
                self.toast = Some("Type one or two oligos".into());
            }
        }
    }

    fn clamp_lib_selection(&mut self) {
        let n = self.library.plasmids.len();
        self.selected_lib = if n == 0 {
            0
        } else {
            self.selected_lib.min(n - 1)
        };
    }

    fn persist_library(&mut self) {
        let Some(layout) = &self.layout else {
            self.toast = Some("Kept in memory — no data dir attached".into());
            return;
        };
        if !splicecraft_persist::writes_authorized() {
            self.toast = Some("Kept in memory — writes are not authorised".into());
            return;
        }
        match self.library.persist(layout) {
            Ok(()) => {}
            Err(e) => {
                self.toast = Some(format!("Library save failed: {e}"));
            }
        }
    }

    fn keep_record(&mut self) {
        let Some(rec) = &self.record else {
            self.toast = Some("Nothing loaded to keep".into());
            return;
        };
        let entry = match crate::io::record_to_library_entry(rec) {
            Ok(e) => e,
            Err(e) => {
                self.toast = Some(format!("Keep failed: {e}"));
                return;
            }
        };
        self.apply_keep(entry, None);
    }

    fn apply_keep(&mut self, entry: LibraryEntry, choice: Option<CollisionChoice>) {
        match self.library.keep(entry.clone(), choice) {
            KeepOutcome::NeedsChoice {
                class,
                existing_name,
            } => {
                self.pending_collision = Some(PendingCollision::Keep(entry));
                self.overlay = Overlay::Collision;
                self.toast = Some(collision_toast(class, &existing_name));
            }
            KeepOutcome::Cancelled => {
                self.overlay = Overlay::None;
                self.pending_collision = None;
                self.toast = Some("Keep cancelled".into());
            }
            KeepOutcome::Applied { name } => {
                self.overlay = Overlay::None;
                self.pending_collision = None;
                self.clamp_lib_selection();
                self.toast = None;
                self.persist_library();
                if self.toast.is_none() {
                    self.toast = Some(format!("Kept {name}"));
                }
            }
        }
    }

    fn resolve_collision(&mut self, choice: CollisionChoice) {
        match self.pending_collision.take() {
            Some(PendingCollision::Keep(entry)) => self.apply_keep(entry, Some(choice)),
            Some(PendingCollision::Feature(snip)) => self.apply_feature(snip, Some(choice)),
            None => {
                self.overlay = Overlay::None;
                self.toast = Some("Nothing to resolve".into());
            }
        }
    }

    fn save_selected_feature(&mut self) {
        let Some(rec) = &self.record else {
            self.toast = Some("Nothing loaded".into());
            return;
        };
        let Some(i) = self.selected_feat else {
            self.toast = Some("No feature selected — Enter on the sequence first".into());
            return;
        };
        let Some(feat) = rec.features.get(i) else {
            return;
        };
        let snippet = FeatureSnippet {
            name: if feat.label.is_empty() {
                feat.kind.clone()
            } else {
                feat.label.clone()
            },
            feature_type: feat.kind.clone(),
            sequence: extract_feature(rec, feat),
            strand: feat.strand,
            description: String::new(),
            color: feat.qualifiers.get("color").cloned(),
            qualifiers: Default::default(),
        };
        self.apply_feature(snippet, None);
    }

    fn apply_feature(&mut self, snippet: FeatureSnippet, choice: Option<CollisionChoice>) {
        let Some(layout) = &self.layout else {
            self.toast = Some("Feature library needs a data dir".into());
            return;
        };
        if !splicecraft_persist::writes_authorized() {
            self.toast = Some("Feature save refused — writes are not authorised".into());
            return;
        }
        match splicecraft_persist::upsert_feature(layout, snippet.clone(), choice) {
            Ok(KeepOutcome::NeedsChoice {
                class,
                existing_name,
            }) => {
                self.pending_collision = Some(PendingCollision::Feature(snippet));
                self.overlay = Overlay::Collision;
                self.toast = Some(collision_toast(class, &existing_name));
            }
            Ok(KeepOutcome::Cancelled) => {
                self.overlay = Overlay::None;
                self.toast = Some("Feature save cancelled".into());
            }
            Ok(KeepOutcome::Applied { name }) => {
                self.overlay = Overlay::None;
                self.pending_collision = None;
                self.toast = Some(format!("Saved feature {name}"));
            }
            Err(e) => {
                self.toast = Some(format!("Feature save failed: {e}"));
            }
        }
    }

    fn open_library_entry(&mut self) {
        let Some(entry) = self.library.plasmids.get(self.selected_lib) else {
            self.toast = Some("Library is empty".into());
            return;
        };
        if entry.gb_text.is_empty() {
            self.toast = Some(format!("No sequence stored for {}", entry.name));
            return;
        }
        match crate::io::gb_text_to_record(&entry.gb_text) {
            Ok(rec) => {
                self.source_label = format!("{} ({})", rec.name, self.library.active);
                self.record = Some(rec);
                self.cursor = 0;
                self.undo = UndoStack::new();
                self.dirty = false;
                self.toast = Some(format!("Loaded {}", entry.name));
            }
            Err(e) => {
                self.toast = Some(format!("Load failed: {e}"));
            }
        }
    }

    fn submit_path(&mut self) {
        let raw = self.path_query.trim().to_owned();
        self.overlay = Overlay::None;
        self.path_query.clear();
        if raw.is_empty() {
            self.toast = Some("Empty path".into());
            return;
        }
        let Some(path) = splicecraft_persist::util::sanitize_path(&raw) else {
            self.toast = Some("Refused path".into());
            return;
        };
        match self.path_kind {
            PathKind::OpenFile => match crate::io::load_path(&path) {
                Ok(rec) => {
                    self.source_label = rec.name.clone();
                    self.record = Some(rec);
                    self.cursor = 0;
                    self.undo = UndoStack::new();
                    self.dirty = false;
                    self.toast = Some("Opened file (memory)".into());
                }
                Err(e) => {
                    self.toast = Some(format!("Open failed: {e}"));
                }
            },
            PathKind::BulkImport => self.bulk_import(&path),
            PathKind::BulkExport => self.bulk_export(&path),
            PathKind::BulkAlign => {
                self.seq_query = path.display().to_string();
                self.seq_tab = SequencingTab::Report;
                self.overlay = Overlay::Sequencing;
                self.run_sequencing();
            }
            PathKind::MapExport => self.export_map_to(&path),
            PathKind::MigrateExport => self.export_migrate_to(&path),
            PathKind::MigrateImport => self.import_migrate_from(&path),
        }
    }

    fn bulk_import(&mut self, folder: &std::path::Path) {
        let report = crate::io::bulk_import_folder(folder);
        let mut added = 0;
        let mut skipped_coll = 0;
        for rec in report.records {
            let Ok(entry) = crate::io::record_to_library_entry(&rec) else {
                continue;
            };
            match self.library.keep(entry, None) {
                KeepOutcome::Applied { .. } => added += 1,
                KeepOutcome::NeedsChoice { .. } => skipped_coll += 1,
                KeepOutcome::Cancelled => {}
            }
        }
        self.clamp_lib_selection();
        if added > 0 {
            self.persist_library();
        }
        self.toast = Some(format!(
            "Import: {added} kept, {} failed, {} .dna skipped, {skipped_coll} collisions (not overwritten)",
            report.failures.len(),
            report.skipped_dna
        ));
    }

    fn bulk_export(&mut self, folder: &std::path::Path) {
        let mut records = Vec::new();
        for entry in &self.library.plasmids {
            if entry.gb_text.is_empty() {
                continue;
            }
            if let Ok(rec) = crate::io::gb_text_to_record(&entry.gb_text) {
                records.push(rec);
            }
        }
        match crate::io::bulk_export_folder(folder, &records, crate::io::BulkExportFormat::GenBank)
        {
            Ok(paths) => {
                self.toast = Some(format!("Exported {} file(s)", paths.len()));
            }
            Err(e) => {
                self.toast = Some(format!("Export failed: {e}"));
            }
        }
    }

    fn export_map_to(&mut self, path: &std::path::Path) {
        let Some(rec) = &self.record else {
            self.toast = Some("No plasmid loaded".into());
            return;
        };
        let fmt = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("svg")
            .to_ascii_lowercase();
        let fmt = if fmt == "png" { "png" } else { "svg" };
        match export_plasmid_map(rec, path, fmt, &MapImageOpts::default()) {
            Ok(r) => {
                self.toast = Some(format!("Map exported ({} bytes, {})", r.bytes, r.fmt));
            }
            Err(e) => self.toast = Some(format!("Map export failed: {e}")),
        }
    }

    fn export_migrate_to(&mut self, path: &std::path::Path) {
        let Some(layout) = &self.layout else {
            self.toast = Some("No data dir attached".into());
            return;
        };
        match splicecraft_persist::export_migrate_archive(layout, path, false) {
            Ok(r) => {
                self.toast = Some(format!(
                    "Migrate zip: {} files, {} bytes",
                    r.n_files, r.bytes
                ));
            }
            Err(e) => self.toast = Some(format!("Migrate export failed: {e}")),
        }
    }

    fn import_migrate_from(&mut self, path: &std::path::Path) {
        let Some(layout) = &self.layout else {
            self.toast = Some("No data dir attached".into());
            return;
        };
        match splicecraft_persist::import_migrate_archive(layout, path) {
            Ok(r) => {
                self.attach_layout(layout.clone());
                self.toast = Some(format!("Migrate import: {} files restored", r.n_files));
            }
            Err(e) => self.toast = Some(format!("Migrate import failed: {e}")),
        }
    }

    fn toggle_online_search(&mut self) {
        self.allow_online_search = !self.allow_online_search;
        self.persist_setting_bool(SETTING_ALLOW_ONLINE_SEARCH, self.allow_online_search);
        self.toast = Some(format!(
            "allow_online_search {}",
            if self.allow_online_search {
                "ON"
            } else {
                "off"
            }
        ));
    }

    fn toggle_online_lookups(&mut self) {
        self.allow_online_lookups = !self.allow_online_lookups;
        self.persist_setting_bool(SETTING_ALLOW_ONLINE_LOOKUPS, self.allow_online_lookups);
        self.toast = Some(format!(
            "allow_online_lookups {}",
            if self.allow_online_lookups {
                "ON"
            } else {
                "off"
            }
        ));
    }

    fn persist_setting_bool(&mut self, key: &str, value: bool) {
        let Some(layout) = &self.layout else {
            return;
        };
        if !splicecraft_persist::writes_authorized() {
            return;
        }
        if let Err(e) = set_setting_bool(layout, key, value) {
            self.toast = Some(format!("Settings save failed: {e}"));
        }
    }

    fn toggle_selected_setting(&mut self) {
        if self.settings_selected == 0 {
            self.toggle_online_search();
        } else {
            self.toggle_online_lookups();
        }
    }

    fn run_babs(&mut self) {
        let line = self.babs_query.trim().to_owned();
        self.babs_query.clear();
        if line.is_empty() {
            return;
        }
        if let Some(cmd) = parse_command(&line) {
            match cmd {
                BabsCommand::Help => self
                    .babs_lines
                    .push("/help /clear /export /model <name> — loopback Ollama only".into()),
                BabsCommand::Clear => self.babs_lines.clear(),
                BabsCommand::Export => {
                    self.babs_lines.push(format!(
                        "transcript {} lines (not persisted)",
                        self.babs_lines.len()
                    ));
                }
                BabsCommand::Model(name) => {
                    self.babs_model = name;
                    self.babs_lines.push(format!("model → {}", self.babs_model));
                }
            }
            return;
        }
        match crate::babs::ollama_chat(
            &splicecraft_io::OfflineTransport,
            &crate::babs::ollama_base(),
            &self.babs_model,
            &[("user", line.as_str())],
        ) {
            Ok(reply) => self.babs_lines.push(reply),
            Err(e) => self.babs_lines.push(format!("(offline) {e}")),
        }
    }

    fn run_autolab_compile(&mut self) {
        match compile_protocol(&fixture_deck()) {
            Ok(c) => {
                self.autolab_protocol = Some(c.protocol_text.clone());
                self.autolab_summary = Some(format!(
                    "compiled {} ({} bytes JSON)",
                    c.json["name"],
                    c.json.to_string().len()
                ));
                if self.autolab_motion_armed {
                    match confirm_motion(true) {
                        Ok(()) => {
                            self.toast =
                                Some("Motion confirm noted — no live robot in this build".into());
                        }
                        Err(e) => self.toast = Some(e.to_string()),
                    }
                }
            }
            Err(e) => self.autolab_summary = Some(e.to_string()),
        }
    }

    fn reset_master_delete(&mut self) {
        self.master_delete_step = 0;
        self.master_delete_yes = false;
        self.master_delete_typed.clear();
        self.master_delete_cooldown_start = None;
    }

    /// Remaining cooldown on the final Master Delete confirm.
    #[must_use]
    pub fn master_delete_cooldown_remaining(&self) -> Duration {
        let Some(start) = self.master_delete_cooldown_start else {
            return MASTER_DELETE_CONFIRM_COOLDOWN;
        };
        MASTER_DELETE_CONFIRM_COOLDOWN.saturating_sub(start.elapsed())
    }

    fn advance_master_delete(&mut self) {
        match self.master_delete_step {
            0 => {
                if !self.master_delete_yes {
                    self.overlay = Overlay::None;
                    self.reset_master_delete();
                    self.toast = Some("Master Delete cancelled — data kept".into());
                } else {
                    self.master_delete_step = 1;
                    self.master_delete_typed.clear();
                }
            }
            1 => {
                if self.master_delete_typed == "DELETE" {
                    self.master_delete_step = 2;
                    self.master_delete_cooldown_start = Some(Instant::now());
                } else {
                    self.toast = Some("Type DELETE to continue".into());
                }
            }
            _ => {
                if !self.master_delete_cooldown_remaining().is_zero() {
                    self.toast = Some("Wait for the cooldown".into());
                    return;
                }
                self.execute_master_delete();
            }
        }
    }

    fn execute_master_delete(&mut self) {
        let Some(layout) = self.layout.clone() else {
            self.toast = Some("No data dir attached".into());
            return;
        };
        match splicecraft_persist::perform_master_delete(&layout, MASTER_DELETE_SENTINEL) {
            Ok(r) => {
                self.library = LibraryStore::new();
                self.record = None;
                self.source_label = "(no record)".into();
                self.dirty = false;
                self.overlay = Overlay::None;
                self.reset_master_delete();
                self.toast = Some(format!(
                    "Master Delete: {} files, {} dirs removed",
                    r.files_removed, r.dirs_removed
                ));
            }
            Err(e) => self.toast = Some(format!("Master Delete refused: {e}")),
        }
    }

    fn persist_parts(&mut self) {
        let Some(layout) = &self.layout else {
            return;
        };
        if !splicecraft_persist::writes_authorized() {
            return;
        }
        if let Err(e) = self.parts.persist(layout) {
            self.toast = Some(format!("Parts bin save failed: {e}"));
        }
    }

    fn active_grammar(&self) -> splicecraft_clone::Grammar {
        self.grammars.get(&self.grammar_id).unwrap_or_else(gb_l0)
    }

    fn ctor_source_record(&self) -> Option<Record> {
        match self.ctor_source {
            1 => {
                let rec = self.record.as_ref()?;
                let i = self.selected_feat?;
                let feat = rec.features.get(i)?;
                let seq = extract_feature(rec, feat);
                Some(Record::new(feat.label.clone(), seq, false))
            }
            2 => {
                let entry = self.library.plasmids.get(self.selected_lib)?;
                crate::io::gb_text_to_record(&entry.gb_text).ok()
            }
            3 => {
                let Some(layout) = &self.layout else {
                    return None;
                };
                let feats = splicecraft_persist::feature_library(layout);
                let snip = feats.first()?;
                Some(Record::new(snip.name.clone(), snip.sequence.clone(), false))
            }
            _ => self.record.clone(),
        }
    }

    fn parse_enzyme_pair(&self) -> (String, String) {
        let q = self.tool_query.trim();
        let mut parts = q.split_whitespace().filter(|s| !s.is_empty());
        let a = parts.next().unwrap_or("EcoRI").to_owned();
        let b = parts.next().unwrap_or("BamHI").to_owned();
        (a, b)
    }

    fn ctor_part_type(&self, grammar: &splicecraft_clone::Grammar) -> String {
        let q = self.tool_query.trim();
        if grammar.position_for_type(q).is_some() {
            return q.to_owned();
        }
        grammar
            .position_for_type("CDS")
            .or_else(|| grammar.positions.first())
            .map(|p| p.type_name.clone())
            .unwrap_or_else(|| "CDS".into())
    }

    fn run_constructor(&mut self) {
        match self.ctor_tab {
            ConstructorTab::Traditional => self.run_traditional(),
            ConstructorTab::Gibson => self.run_gibson(),
            ConstructorTab::Domesticator => self.run_domesticator(),
            ConstructorTab::Parts => self.run_assemble_parts(),
            ConstructorTab::SynFrag => self.run_syn_frag(),
        }
    }

    fn run_traditional(&mut self) {
        let Some(rec) = self.record.clone() else {
            self.toast = Some("Load a plasmid first".into());
            return;
        };
        let (e1, e2) = self.parse_enzyme_pair();
        let names = [e1.as_str(), e2.as_str()];
        match excise_fragment_pair(
            &rec.sequence,
            &names,
            rec.circular,
            &rec.features,
            &rec.name,
        ) {
            Ok(frags) if frags.len() == 2 => {
                let (insert, vector) = if frags[0].top_seq.len() <= frags[1].top_seq.len() {
                    (&frags[0], &frags[1])
                } else {
                    (&frags[1], &frags[0])
                };
                let result = simulate_traditional_cloning(insert, vector);
                let fwd = if result.forward.compatible {
                    "ok"
                } else {
                    "no"
                };
                let rev = if result.reverse.compatible {
                    "ok"
                } else {
                    "no"
                };
                self.ctor_summary = Some(format!(
                    "Traditional {e1}/{e2}: fwd {fwd}  rev {rev}  ({} bp / {} bp)\n{}",
                    result.forward.top_seq.len(),
                    result.reverse.top_seq.len(),
                    result
                        .warnings
                        .iter()
                        .chain(result.errors.iter())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
                let reverse = result.reverse.compatible && !result.forward.compatible;
                if let Some(closed) = traditional_closed(insert, vector, reverse) {
                    let hist = HistoryNode::new(
                        if reverse { "ligateRev" } else { "ligateFwd" },
                        format!("{}_clone", rec.name),
                        closed.top_seq.len(),
                        true,
                        vec![rec.name.clone()],
                        format!("{e1}/{e2}"),
                    );
                    self.ctor_product = Some(product_record(
                        &format!("{}_clone", rec.name),
                        &closed.top_seq,
                        true,
                        &closed.features,
                        &hist,
                    ));
                    self.toast = Some("Traditional product ready — s to keep".into());
                } else {
                    self.ctor_product = None;
                    self.toast = Some("Neither orientation ligates".into());
                }
            }
            Ok(frags) => {
                self.ctor_product = None;
                self.ctor_summary = Some(format!(
                    "Digest produced {} fragment(s) — need exactly 2.",
                    frags.len()
                ));
                self.toast = Some("Need exactly two fragments".into());
            }
            Err(e) => {
                self.ctor_product = None;
                self.ctor_summary = Some(e.to_string());
                self.toast = Some("Digest refused".into());
            }
        }
    }

    fn gibson_lane(&self) -> Vec<GibsonFragment> {
        let mut lane = Vec::new();
        if let Some(rec) = &self.record {
            lane.push(GibsonFragment::from_record(rec));
        }
        if let Some(entry) = self.library.plasmids.get(self.selected_lib)
            && let Ok(lib_rec) = crate::io::gb_text_to_record(&entry.gb_text)
            && self.record.as_ref().is_none_or(|r| r.name != lib_rec.name)
        {
            lane.push(GibsonFragment::from_record(&lib_rec));
        }
        lane
    }

    fn run_gibson(&mut self) {
        let lane = self.gibson_lane();
        if lane.is_empty() {
            self.toast = Some("Load a record (and optionally highlight a library plasmid)".into());
            return;
        }
        let circular = lane.len() >= 2;
        let r = simulate_gibson_assembly(&lane, 15, circular);
        let wrap = r.overlaps.iter().filter(|o| o.is_wrap).count();
        self.ctor_summary = Some(format!(
            "Gibson: {}  {} bp  {} junctions ({} wrap)\n{}",
            if r.success { "ok" } else { "failed" },
            r.product_seq.len(),
            r.overlaps.len(),
            wrap,
            r.errors
                .iter()
                .chain(r.warnings.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        ));
        if r.success {
            let hist = HistoryNode::new(
                "gibson",
                "gibson_product",
                r.product_seq.len(),
                r.circular,
                lane.iter().map(|f| f.name.clone()).collect(),
                format!("{} junctions", r.overlaps.len()),
            );
            self.ctor_product = Some(product_record(
                "gibson_product",
                &r.product_seq,
                r.circular,
                &r.features,
                &hist,
            ));
            self.toast = Some("Gibson product ready — s to keep".into());
        } else {
            self.ctor_product = None;
            self.toast = Some("Gibson failed — see overlay".into());
        }
    }

    fn run_gibson_arms(&mut self) {
        let mut lane = self.gibson_lane();
        let circular = lane.len() >= 2;
        match design_homology_arms(&mut lane, 15, circular) {
            Ok((armed, already, skipped)) => {
                self.ctor_summary = Some(format!(
                    "Homology arms: added {armed}, {already} already overlapped, {} skipped",
                    skipped.len()
                ));
                self.toast = Some("Arms designed — Enter to simulate".into());
            }
            Err(e) => {
                self.toast = Some(e.to_string());
            }
        }
    }

    fn run_domesticator(&mut self) {
        let Some(rec) = self.ctor_source_record() else {
            self.toast = Some("No source in the 4-picker".into());
            return;
        };
        let g = self.active_grammar();
        let part_type = self.ctor_part_type(&g);
        let end = rec.len();
        let table = self.codon_tables.get("83333").map(|e| e.raw.clone());
        match design_gb_primers(
            &rec.sequence,
            0,
            end,
            &part_type,
            &g,
            60.0,
            None,
            table.as_ref(),
        ) {
            Ok(p) => {
                self.ctor_summary = Some(format!(
                    "Domesticator {part_type} ({})  pad+{}+spacer+{}/{}  amplicon {} bp",
                    p.position, p.enzyme_site, p.oh5, p.oh3, p.amplicon_len
                ));
                self.design_fwd = Some(p.fwd_full);
                self.design_rev = Some(p.rev_full);
                self.toast = Some("Primers designed — s saves the pair".into());
            }
            Err(e) => {
                self.ctor_summary = Some(e.to_string());
                self.toast = Some("Domestication refused".into());
            }
        }
    }

    fn run_syn_frag(&mut self) {
        let Some(rec) = self.ctor_source_record() else {
            self.toast = Some("No source in the 4-picker".into());
            return;
        };
        let g = self.active_grammar();
        let part_type = self.ctor_part_type(&g);
        let Some(pos) = g.position_for_type(&part_type) else {
            self.toast = Some("Unknown part type".into());
            return;
        };
        let built = splicecraft_clone::build_synthesis_l0_fragment(
            &rec.sequence,
            &pos.oh5,
            &pos.oh3,
            &g,
            &part_type,
            None,
        );
        let vec = stub_entry_vector(&g, &built.entry_oh5, &built.entry_oh3);
        match l0_part_from_syn_fragment(&built.fragment, &vec, &g, &part_type, &rec.name, &[], &[])
        {
            Ok(part) => {
                let hist = HistoryNode::new(
                    "l0FromSynFrag",
                    part.name.clone(),
                    part.cloned_seq.len(),
                    true,
                    vec![rec.name.clone()],
                    format!("{}/{}", part.oh5, part.oh3),
                );
                self.ctor_product = Some(product_record(
                    &part.name,
                    &part.cloned_seq,
                    true,
                    &part.cloned_features,
                    &hist,
                ));
                if let Err(e) = self.parts.file(PartRecord::from_l0(&part)) {
                    self.toast = Some(e.to_string());
                    return;
                }
                self.persist_parts();
                self.ctor_summary = Some(format!(
                    "Filed {} ({}) {}/{}  {} bp body",
                    part.name,
                    part.type_name,
                    part.oh5,
                    part.oh3,
                    part.sequence.len()
                ));
                self.toast = Some("Part filed — s keeps the cloned plasmid".into());
            }
            Err(e) => {
                self.ctor_product = None;
                self.ctor_summary = Some(e.to_string());
                self.toast = Some("Syn-frag refused".into());
            }
        }
    }

    fn run_assemble_parts(&mut self) {
        let g = self.active_grammar();
        let mine: Vec<PartRecord> = self.parts.for_grammar(&g.id).into_iter().cloned().collect();
        match assemble_parts(&mine, &g, None) {
            Ok(r) => {
                let hist = HistoryNode::new(
                    "goldenGate",
                    "gg_product",
                    r.product_seq.len(),
                    true,
                    mine.iter().map(|p| p.name.clone()).collect(),
                    g.enzyme.clone(),
                );
                self.ctor_product = Some(product_record(
                    "gg_product",
                    &r.product_seq,
                    true,
                    &[],
                    &hist,
                ));
                self.ctor_summary = Some(format!(
                    "Golden Gate {} parts → {} bp  {} residual sites",
                    mine.len(),
                    r.product_seq.len(),
                    r.n_residual_sites
                ));
                self.toast = Some("Assembly ready — s to keep".into());
            }
            Err(e) => {
                self.ctor_product = None;
                self.ctor_summary = Some(e.to_string());
                self.toast = Some("Assembly refused".into());
            }
        }
    }

    fn classify_into_parts_bin(&mut self) {
        let Some(rec) = self.record.clone() else {
            self.toast = Some("Load a plasmid first".into());
            return;
        };
        match classify_part_from_plasmid(&rec.sequence, rec.circular, &rec.features, &self.grammars)
        {
            Some(hit) => {
                self.ctor_summary = Some(format!(
                    "Classified as {} {} ({}) {}/{} via {}",
                    hit.grammar_id,
                    hit.position,
                    hit.type_name,
                    hit.oh5,
                    hit.oh3,
                    hit.release_enzyme
                ));
                self.toast = Some(format!("Classified: {} {}", hit.grammar_name, hit.position));
            }
            None => {
                self.toast = Some("No grammar matched this digest".into());
            }
        }
    }

    fn save_constructor_product(&mut self) {
        if self.ctor_tab == ConstructorTab::Domesticator {
            self.save_designed_primers();
            return;
        }
        let Some(rec) = self.ctor_product.clone() else {
            self.toast = Some("No product to keep — Enter to simulate first".into());
            return;
        };
        let entry = match crate::io::record_to_library_entry(&rec) {
            Ok(e) => e,
            Err(e) => {
                self.toast = Some(format!("Keep failed: {e}"));
                return;
            }
        };
        self.apply_keep(entry, None);
    }

    fn delete_selected_plasmid(&mut self) {
        if self.library.plasmids.is_empty() {
            self.toast = Some("Library is empty".into());
            return;
        }
        let idx = self.selected_lib.min(self.library.plasmids.len() - 1);
        let Some(entry) = self.library.remove_at(idx) else {
            return;
        };
        let name = entry.name.clone();
        self.deleted_stack.push((idx, entry));
        if self.deleted_stack.len() > 20 {
            self.deleted_stack.remove(0);
        }
        self.clamp_lib_selection();
        self.persist_library();
        self.toast = Some(format!(
            "Deleted {name} — palette: Undo last library delete"
        ));
    }

    fn undelete_plasmid(&mut self) {
        let Some((idx, entry)) = self.deleted_stack.pop() else {
            self.toast = Some("Nothing to undelete".into());
            return;
        };
        let name = entry.name.clone();
        self.library.restore_at(idx, entry);
        self.clamp_lib_selection();
        self.persist_library();
        self.toast = Some(format!("Restored {name}"));
    }

    fn k12(&self) -> Option<splicecraft_codon::UsageTable> {
        self.codon_tables.get("83333").map(|e| e.raw.clone())
    }

    fn mutato_cds(rec: &Record) -> String {
        rec.features
            .iter()
            .find(|f| f.kind.eq_ignore_ascii_case("CDS"))
            .map(|f| splicecraft_primer::extract_cds(&rec.sequence, f.start, f.end, f.strand))
            .filter(|s| s.len() >= 21)
            .unwrap_or_else(|| rec.sequence.to_ascii_uppercase())
    }

    fn run_mutato(&mut self) {
        let Some(rec) = self.record.clone() else {
            self.toast = Some("Load a record first".into());
            return;
        };
        let table = self.k12();
        let extra: Vec<_> = self
            .enzymes
            .custom
            .iter()
            .map(|c| splicecraft_bio::CustomEnzyme {
                name: c.name.clone(),
                site: c.site.clone(),
                fwd_cut: c.fwd_cut,
                rev_cut: c.rev_cut,
            })
            .collect();
        match self.mutato_tab {
            MutatoTab::Sdm => {
                let cds = Self::mutato_cds(&rec);
                let mutation = if self.mutato_query.trim().is_empty() {
                    "V40F"
                } else {
                    self.mutato_query.trim()
                };
                match design_mutagenesis(&cds, mutation, table.as_ref()) {
                    Ok((outer, inner)) => {
                        let path = if inner.edge_case.is_some() {
                            "2-primer"
                        } else {
                            "SOE 4-primer"
                        };
                        self.mutato_summary = Some(format!(
                            "{}  {} nt change(s)  {path}  {}→{}",
                            inner.mutation, inner.nt_changes, inner.wt_codon, inner.mut_codon
                        ));
                        self.design_fwd = Some(outer.fwd.full);
                        self.design_rev = Some(outer.rev.full);
                        self.toast = Some("Mutato primers designed".into());
                    }
                    Err(e) => {
                        self.mutato_summary = Some(e.to_string());
                        self.toast = Some("Mutato refused".into());
                    }
                }
            }
            MutatoTab::ScrubQc => {
                let plan = scrub_design(
                    &rec.sequence,
                    &rec.features,
                    None,
                    rec.circular,
                    table.as_ref(),
                    &extra,
                );
                self.mutato_summary = Some(format!(
                    "QuikChange scrub: {} edits, {} removed, {} skipped, {} rounds",
                    plan.edits.len(),
                    plan.sites_removed.len(),
                    plan.sites_skipped.len(),
                    plan.n_rounds
                ));
                if let Some(cl) = plan.clusters.first() {
                    let p = qc_primers(&plan.cured_seq, cl, rec.circular, "improved", 1);
                    if p.error.is_none() {
                        self.design_fwd = Some(p.fwd_seq);
                        self.design_rev = Some(p.rev_seq);
                    }
                }
                self.toast = Some("Scrub planned".into());
            }
            MutatoTab::ScrubGb => {
                let plan = design_gb_scrub(
                    &rec.sequence,
                    &rec.features,
                    None,
                    rec.circular,
                    table.as_ref(),
                    &extra,
                );
                self.mutato_summary = Some(format!(
                    "GB scrub: ok={} verified={} fragments={}  {}",
                    plan.ok,
                    plan.verified,
                    plan.n_fragments(),
                    if plan.errors.is_empty() {
                        "recirc matched".into()
                    } else {
                        format!("{} error(s)", plan.errors.len())
                    }
                ));
                self.toast = Some(if plan.ok && plan.verified {
                    "GB recirc verified".into()
                } else {
                    "GB recirc failed closed".into()
                });
            }
        }
    }

    fn run_synthesis(&mut self) {
        let table = self.k12().unwrap_or_else(splicecraft_codon::builtin_k12);
        match self.synth_tab {
            SynthTab::Dna => {
                self.synth_summary = Some(format!(
                    "DNA buffer {} bp  {} features  (Enter commits a linear record)",
                    self.dna_buf.seq.len(),
                    self.dna_buf.features.len()
                ));
                if !self.dna_buf.seq.is_empty() {
                    self.record = Some(Record::new("synth_dna", self.dna_buf.seq.clone(), false));
                    self.source_label = "synth_dna".into();
                    self.toast = Some("Linear DNA fragment loaded".into());
                } else {
                    self.toast = Some("Type IUPAC bases, then Enter".into());
                }
            }
            SynthTab::Protein => match self.protein_buf.to_dna(&table, 1, CodonMode::Frequency) {
                Ok(dna) => {
                    self.synth_summary = Some(format!(
                        "Protein {} aa → {} bp (K12 frequency)  motifs {}",
                        self.protein_buf.aa.chars().count(),
                        dna.len(),
                        self.motifs.merged().len()
                    ));
                    self.dna_buf.seq = dna;
                    self.dna_buf.cursor = self.dna_buf.seq.len();
                    self.toast = Some("Reverse-translated from codon table".into());
                }
                Err(e) => {
                    self.synth_summary = Some(e.to_string());
                    self.toast = Some("Protein compose refused".into());
                }
            },
            SynthTab::Operon => {
                let Some(rec) = self.record.clone() else {
                    self.toast = Some("Load an operon record first".into());
                    return;
                };
                let g = self.active_grammar();
                match design_operon_soe_primers(
                    &rec.sequence,
                    &rec.features,
                    &g,
                    &[],
                    &[],
                    Some(&table),
                    &[],
                    60.0,
                ) {
                    Ok(res) if res.ok => {
                        self.synth_summary = Some(format!(
                            "Operon SOE: {} primers, {} clusters, {} edits",
                            res.primers.len(),
                            res.n_clusters,
                            res.edits.len()
                        ));
                        self.toast = Some("Operon primers designed".into());
                    }
                    Ok(res) if res.needs_manual => {
                        self.synth_summary = Some(format!(
                            "Operon needs manual edits at {} non-coding site(s)",
                            res.sites_skipped.len()
                        ));
                        self.toast = Some("Flagged for manual cure".into());
                    }
                    Ok(res) => {
                        self.synth_summary =
                            Some(res.error.unwrap_or_else(|| "operon refused".into()));
                        self.toast = Some("Operon SOE refused".into());
                    }
                    Err(e) => {
                        self.synth_summary = Some(e.to_string());
                        self.toast = Some("Operon SOE refused".into());
                    }
                }
            }
        }
    }

    fn seed_demo_gel(&mut self) {
        self.sim_lanes = vec![
            GelLane::ladder("Ladder", "1 kb Plus"),
            GelLane {
                name: "Uncut".into(),
                source: "plasmid".into(),
                detail: String::new(),
                pcr_bp: None,
            },
        ];
        self.sim_gel_demo = true;
    }

    fn parse_sim_primers(&self) -> Option<(String, String)> {
        let q = self.sim_query.trim();
        if q.is_empty() {
            return None;
        }
        if let Some((a, b)) = q.split_once('/') {
            let fwd = a.trim();
            let rev = b.trim();
            if fwd.is_empty() || rev.is_empty() {
                return None;
            }
            return Some((fwd.to_owned(), rev.to_owned()));
        }
        let parts: Vec<&str> = q.split_whitespace().collect();
        if parts.len() >= 2 {
            Some((parts[0].to_owned(), parts[1].to_owned()))
        } else {
            None
        }
    }

    fn run_simulator(&mut self) {
        match self.sim_tab {
            SimulatorTab::Pcr => self.run_sim_pcr(),
            SimulatorTab::Gel => self.run_sim_gel(),
        }
    }

    fn run_sim_pcr(&mut self) {
        let Some(rec) = self.record.clone() else {
            self.toast = Some("Load a record first".into());
            return;
        };
        let Some((fwd, rev)) = self.parse_sim_primers() else {
            self.toast = Some("Type FWD/REV primers".into());
            return;
        };
        match simulate_pcr(&rec.sequence, &fwd, &rev, rec.circular, 20_000) {
            Ok(amps) => {
                let n = amps.len();
                let wraps = amps.iter().filter(|a| a.wraps).count();
                let preview: Vec<String> = amps
                    .iter()
                    .take(6)
                    .map(|a| {
                        format!(
                            "{}bp{} @{}",
                            a.length,
                            if a.wraps { " wrap" } else { "" },
                            a.start
                        )
                    })
                    .collect();
                self.sim_amplicons = amps;
                self.sim_selected = 0;
                self.sim_summary = Some(format!(
                    "{n} amplicon(s), {wraps} wrap  {}",
                    preview.join(" · ")
                ));
                self.toast = if n >= splicecraft_gels::PCR_MAX_AMPLICONS {
                    Some("PCR cap hit — mispriming?".into())
                } else {
                    Some(format!("PCR: {n} product(s)"))
                };
            }
            Err(e) => {
                self.sim_amplicons.clear();
                self.sim_summary = Some(e.to_string());
                self.toast = Some("PCR refused".into());
            }
        }
    }

    fn run_sim_gel(&mut self) {
        let rec = self.record.clone();
        let pcr_len = self.sim_amplicons.get(self.sim_selected).map(|a| a.length);
        let img = render_gel_image(
            &self.sim_lanes,
            &GelRenderOpts {
                template_seq: rec.as_ref().map(|r| r.sequence.as_str()).unwrap_or(""),
                template_circular: rec.as_ref().is_some_and(|r| r.circular),
                pcr_length: pcr_len,
                agarose_pct: self.sim_agarose,
                height: 16,
                lane_width: 5,
                label_col: 7,
            },
        );
        self.sim_gel_image = Some(img);
        self.sim_summary = Some(format!(
            "{}% agarose · {} lanes",
            self.sim_agarose,
            self.sim_lanes.len()
        ));
        self.toast = Some("Gel rendered".into());
    }

    fn send_pcr_to_gel(&mut self) {
        let Some(amp) = self.sim_amplicons.get(self.sim_selected).cloned() else {
            self.toast = Some("Run PCR first".into());
            return;
        };
        if self.sim_gel_demo {
            self.sim_lanes = vec![GelLane::ladder("Ladder", "1 kb")];
            self.sim_gel_demo = false;
        }
        let name = format!("PCR {} bp", amp.length);
        let (idx, at_cap) =
            append_pcr_gel_lane(&mut self.sim_lanes, name, amp.length, GEL_UI_MAX_LANES);
        if at_cap {
            self.toast = Some("Gel is at 8 lanes".into());
            return;
        }
        self.sim_tab = SimulatorTab::Gel;
        self.toast = Some(format!("Lane {} pinned to {} bp", idx + 1, amp.length));
        self.run_sim_gel();
    }

    fn save_simulator(&mut self) {
        match self.sim_tab {
            SimulatorTab::Pcr => {
                let Some(amp) = self.sim_amplicons.get(self.sim_selected).cloned() else {
                    self.toast = Some("Run PCR first".into());
                    return;
                };
                let rec = amplicon_to_record(&amp);
                self.record = Some(rec.clone());
                self.source_label = rec.name.clone();
                match crate::io::record_to_library_entry(&rec) {
                    Ok(entry) => self.apply_keep(entry, None),
                    Err(e) => self.toast = Some(format!("Save amplicon failed: {e}")),
                }
            }
            SimulatorTab::Gel => {
                let snap = snapshot_gel("Simulator gel", self.sim_agarose, &self.sim_lanes, "");
                let id = snap.id.clone();
                self.gels.upsert(snap);
                self.persist_gels();
                if self.toast.is_none() {
                    self.toast = Some(format!("Saved &{id}"));
                }
            }
        }
    }

    fn persist_gels(&mut self) {
        let Some(layout) = &self.layout else {
            self.toast = Some("Gel kept in memory — no data dir".into());
            return;
        };
        if !splicecraft_persist::writes_authorized() {
            self.toast = Some("Gel kept in memory — writes are not authorised".into());
            return;
        }
        match self.gels.persist(layout) {
            Ok(()) => {}
            Err(e) => self.toast = Some(format!("Gel save failed: {e}")),
        }
    }

    fn clamp_exp_selection(&mut self) {
        let n = self.experiments.entries.len();
        self.exp_selected = if n == 0 {
            0
        } else {
            self.exp_selected.min(n - 1)
        };
    }

    fn persist_experiments(&mut self) {
        let Some(layout) = &self.layout else {
            self.toast = Some("Notebook kept in memory — no data dir".into());
            return;
        };
        if !splicecraft_persist::writes_authorized() {
            self.toast = Some("Notebook kept in memory — writes are not authorised".into());
            return;
        }
        match self.experiments.persist(layout) {
            Ok(()) => {}
            Err(e) => self.toast = Some(format!("Experiments save failed: {e}")),
        }
    }

    fn run_experiments(&mut self) {
        match self.exp_tab {
            ExperimentsTab::List => {
                if let Some(e) = self.experiments.entries.get(self.exp_selected).cloned() {
                    self.exp_id = e.id;
                    self.exp_title = e.title;
                    self.exp_body = e.body_md;
                    self.exp_tab = ExperimentsTab::Compose;
                    self.exp_summary = Some("  Loaded into compose. Enter saves.".into());
                } else {
                    self.exp_id.clear();
                    self.exp_title.clear();
                    self.exp_body.clear();
                    self.exp_tab = ExperimentsTab::Compose;
                    self.exp_summary = Some("  New entry. Type markdown, Enter to save.".into());
                }
            }
            ExperimentsTab::Compose => self.save_experiment_entry(),
            ExperimentsTab::Attach => self.attach_experiment_image(),
        }
    }

    fn save_experiment_entry(&mut self) {
        let existing: std::collections::HashSet<String> = self
            .experiments
            .entries
            .iter()
            .map(|e| e.id.clone())
            .collect();
        if self.exp_id.is_empty() {
            self.exp_id = new_experiment_id(&existing);
        }
        let mut title = self.exp_title.trim().to_owned();
        if title.is_empty() {
            title = self
                .exp_body
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("Untitled")
                .chars()
                .take(80)
                .collect();
        }
        let entry = normalise_experiment_entry(
            ExperimentEntry {
                id: self.exp_id.clone(),
                title,
                body_md: self.exp_body.clone(),
                ..ExperimentEntry::default()
            },
            self.experiments.entries.iter().all(|e| e.id != self.exp_id),
        );
        let id = entry.id.clone();
        self.experiments.upsert(entry);
        self.persist_experiments();
        self.exp_summary = Some(format!("  Saved {id}"));
        self.toast = Some(format!("Saved experiment {id}"));
    }

    fn attach_experiment_image(&mut self) {
        if self.exp_id.is_empty() {
            self.toast = Some("Save an entry before attaching an image".into());
            return;
        }
        let Some(path) = splicecraft_persist::util::sanitize_path(self.tool_query.trim()) else {
            self.toast = Some("Type an image path".into());
            return;
        };
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                self.toast = Some(format!("Read failed: {e}"));
                return;
            }
        };
        let Some(layout) = &self.layout else {
            self.toast = Some("No data dir attached".into());
            return;
        };
        let name = path.file_name().and_then(|s| s.to_str());
        match save_experiment_image(layout, &self.exp_id, &data, name) {
            Ok(saved) => {
                let fname = saved
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("image")
                    .to_owned();
                for e in &mut self.experiments.entries {
                    if e.id == self.exp_id && !e.image_paths.contains(&fname) {
                        e.image_paths.push(fname.clone());
                        break;
                    }
                }
                self.persist_experiments();
                self.exp_summary = Some(format!(
                    "  attached {fname}\n{}",
                    splicecraft_persist::halfblock_preview(&data, 24, 6)
                ));
                self.toast = Some(format!("Attached {fname}"));
            }
            Err(e) => self.toast = Some(format!("Attach failed: {e}")),
        }
    }

    fn jump_experiment_ref(&mut self) {
        let body = if self.exp_body.is_empty() {
            self.experiments
                .entries
                .get(self.exp_selected)
                .map(|e| e.body_md.as_str())
                .unwrap_or("")
        } else {
            self.exp_body.as_str()
        };
        let table = experiment_jump_table(body);
        if let Some(id) = table.plasmids.first() {
            if let Some(hit) = resolve_plasmid_jump(
                id,
                &self.library.plasmids,
                |e| e.id.as_str(),
                |e| e.name.as_str(),
            ) {
                let name = hit.name.clone();
                if let Some(idx) = self.library.plasmids.iter().position(|e| e.name == name) {
                    self.selected_lib = idx;
                    self.open_library_entry();
                }
                self.toast = Some(format!("Jump @{id} → {name}"));
                return;
            }
            self.toast = Some(format!("No plasmid @{id}"));
            return;
        }
        if let Some(id) = table.actions.first() {
            self.toast = Some(format!("Action !{id}"));
            return;
        }
        if let Some(id) = table.gels.first() {
            self.toast = Some(format!("Gel &{id}"));
            return;
        }
        self.toast = Some("No @plasmid / !action / &gel in this entry".into());
    }

    fn spellcheck_experiment(&mut self) {
        let misses = spellcheck_body(&self.exp_body);
        if misses.is_empty() {
            self.toast = Some("Spellcheck: no unknown words".into());
            self.exp_summary = Some("  Spellcheck clean.".into());
        } else {
            let shown = misses
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            self.toast = Some(format!("Spellcheck: {} unknown", misses.len()));
            self.exp_summary = Some(format!("  unknown: {shown}"));
        }
    }

    fn run_history(&mut self) {
        let apply = self.tool_query.trim().eq_ignore_ascii_case("apply");
        self.run_history_recover(!apply);
    }

    fn run_history_recover(&mut self, dry_run: bool) {
        let Some(layout) = self.layout.clone() else {
            self.toast = Some("No data dir — cannot scan dna_originals".into());
            return;
        };
        match crate::io::recover_history_from_dna(&layout, &mut self.library, dry_run) {
            Ok(report) => {
                let n = report.updated.len();
                let mode = if report.dry_run { "dry-run" } else { "applied" };
                let mut lines = vec![format!("  {mode}: {n} plasmid(s)")];
                for hit in report.updated.iter().take(8) {
                    lines.push(format!(
                        "  {}  {} → {} nodes  ({})",
                        hit.name, hit.nodes_before, hit.nodes_after, hit.source
                    ));
                }
                if !report.note.is_empty() {
                    lines.push(format!("  {}", report.note));
                }
                self.hist_summary = Some(lines.join("\n"));
                self.toast = Some(format!("History recover {mode}: {n}"));
                if !dry_run {
                    self.refresh_history();
                }
            }
            Err(e) => self.toast = Some(format!("Recover failed: {e}")),
        }
    }

    fn refresh_history(&mut self) {
        let seq = self
            .record
            .as_ref()
            .map(|r| r.sequence.clone())
            .unwrap_or_default();
        let xml = self
            .record
            .as_ref()
            .and_then(|rec| {
                self.library
                    .plasmids
                    .iter()
                    .find(|e| e.name == rec.name || e.id == rec.id)
                    .map(|e| e.history_xml.clone())
            })
            .unwrap_or_default();
        let node = parse_history_xml(&xml).unwrap_or_else(|| HistoryCheckNode {
            name: self
                .record
                .as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "(no record)".into()),
            operation: "createDocument".into(),
            seq_len: seq.len(),
            circular: self.record.as_ref().map(|r| r.circular).unwrap_or(true),
            ..HistoryCheckNode::default()
        });
        self.hist_warnings = history_node_warnings(&node, &seq);
        self.refresh_history_tab_with(&node);
        if !self.hist_warnings.is_empty() {
            self.hist_summary = Some(format!(
                "  ⚠ {} recorded detail(s) the sequence doesn't support",
                self.hist_warnings.len()
            ));
        } else if self.hist_summary.is_none() {
            self.hist_summary = Some("  History matches this molecule.".into());
        }
    }

    fn refresh_history_tab(&mut self) {
        let seq = self
            .record
            .as_ref()
            .map(|r| r.sequence.clone())
            .unwrap_or_default();
        let xml = self
            .record
            .as_ref()
            .and_then(|rec| {
                self.library
                    .plasmids
                    .iter()
                    .find(|e| e.name == rec.name || e.id == rec.id)
                    .map(|e| e.history_xml.clone())
            })
            .unwrap_or_default();
        let node = parse_history_xml(&xml).unwrap_or_else(|| HistoryCheckNode {
            name: self
                .record
                .as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "(no record)".into()),
            operation: "createDocument".into(),
            seq_len: seq.len(),
            circular: self.record.as_ref().map(|r| r.circular).unwrap_or(true),
            ..HistoryCheckNode::default()
        });
        self.refresh_history_tab_with(&node);
    }

    fn refresh_history_tab_with(&mut self, node: &HistoryCheckNode) {
        self.hist_lines = match self.hist_tab {
            HistoryTab::Protocol => history_protocol_lines(node),
            HistoryTab::Tree => history_tree_lines(node),
            HistoryTab::Detail => {
                let mut lines = history_detail_lines(node);
                if !self.hist_warnings.is_empty() {
                    lines.push(String::new());
                    lines.push("warnings (sequence not edited):".into());
                    lines.extend(self.hist_warnings.iter().cloned());
                }
                lines
            }
        };
    }

    fn run_sequencing(&mut self) {
        match self.seq_tab {
            SequencingTab::Zip => self.run_seq_zip(),
            SequencingTab::Align => self.run_seq_align(),
            SequencingTab::Sanger => self.run_seq_sanger(),
            SequencingTab::Report => self.run_seq_report(),
        }
    }

    fn jump_to_variant(&mut self) {
        let Some(v) = self.seq_variants.get(self.seq_variant_idx).cloned() else {
            self.toast = Some("No alignment variants to jump".into());
            return;
        };
        let max = self.record.as_ref().map(|r| r.len()).unwrap_or(0);
        self.cursor = if max == 0 {
            v.target_pos
        } else {
            v.target_pos.min(max.saturating_sub(1))
        };
        self.focus = Pane::Sequence;
        self.toast = Some(format!("Jumped to {} @{}", v.kind, self.cursor));
    }

    fn seq_path(&self) -> Option<std::path::PathBuf> {
        let raw = self.seq_query.trim();
        if raw.is_empty() {
            return None;
        }
        splicecraft_persist::util::sanitize_path(raw)
    }

    fn seq_query_dna(&self) -> Result<(String, String), String> {
        let raw = self.seq_query.trim();
        if raw.is_empty() {
            return Err("Type a read sequence or file path".into());
        }
        if let Some(path) = splicecraft_persist::util::sanitize_path(raw)
            && path.is_file()
        {
            let rec = crate::io::load_path(&path).map_err(|e| e.to_string())?;
            let label = rec.name.clone();
            return Ok((rec.sequence, label));
        }
        Ok((raw.to_ascii_uppercase(), "read".into()))
    }

    fn run_seq_zip(&mut self) {
        let Some(path) = self.seq_path() else {
            self.toast = Some("Type a Plasmidsaurus zip path".into());
            return;
        };
        match crate::io::parse_plasmidsaurus_zip(&path) {
            Ok(zip) => {
                self.seq_zip_n = zip.samples.len();
                match crate::io::plasmidsaurus_zip_to_entries(&path, &zip.run_id) {
                    Ok((entries, warnings)) => {
                        let mut imported = 0usize;
                        for entry in entries {
                            if matches!(
                                self.library.import_without_overwrite(entry),
                                KeepOutcome::Applied { .. }
                            ) {
                                imported += 1;
                            }
                        }
                        self.clamp_lib_selection();
                        if imported > 0 {
                            self.persist_library();
                        }
                        let warn = if warnings.is_empty() {
                            String::new()
                        } else {
                            format!(" · {} warning(s)", warnings.len())
                        };
                        self.seq_summary = Some(format!(
                            "run {} · {} sample(s) · {imported} imported (never overwrite){warn}",
                            zip.run_id, self.seq_zip_n
                        ));
                        self.toast = Some(format!("Zip: {} sample(s)", self.seq_zip_n));
                        if self.record.is_none()
                            && let Ok(rec) = crate::io::first_gbk_record_from_zip(&path)
                        {
                            self.source_label = rec.name.clone();
                            self.record = Some(rec);
                            self.cursor = 0;
                            self.undo = UndoStack::new();
                            self.dirty = false;
                        }
                    }
                    Err(e) => {
                        self.seq_summary = Some(e.to_string());
                        self.toast = Some("Zip import failed".into());
                    }
                }
            }
            Err(e) => {
                self.seq_summary = Some(e.to_string());
                self.toast = Some("Zip refused".into());
            }
        }
    }

    fn run_seq_align(&mut self) {
        match self.seq_query_dna() {
            Ok((query, label)) => self.apply_alignment(&query, &label),
            Err(e) => {
                self.toast = Some(e);
            }
        }
    }

    fn apply_alignment(&mut self, query: &str, label: &str) {
        let Some(rec) = self.record.clone() else {
            self.toast = Some("Load a plasmid first".into());
            return;
        };
        match crate::io::pairwise_align(query, &rec.sequence, crate::io::AlignMode::Global) {
            Ok(result) => {
                let segs = crate::io::alignment_to_target_segments(
                    &result.aligned_q,
                    &result.aligned_t,
                    0,
                )
                .unwrap_or_default();
                let vars = crate::io::extract_variants_from_alignment(
                    &result.aligned_q,
                    &result.aligned_t,
                );
                let status = crate::io::alignment_quality_status(&result, rec.len());
                let ident = crate::io::format_identity_pct(Some(result.identity_pct), 1);
                self.seq_segments = segs;
                self.seq_variants = vars;
                self.seq_variant_idx = 0;
                self.map_circular = false;
                self.seq_summary = Some(format!(
                    "{ident}  {}  {} mismatch(es)  {} indel(s)  {} variant(s)  j jump",
                    status.code(),
                    result.n_mismatches,
                    crate::io::alignment_indel_events(&result),
                    self.seq_variants.len()
                ));
                let badge = crate::io::badge_from_result(label, &result, rec.len());
                self.attach_alignment_badges(&rec.name, vec![badge]);
                self.toast = Some(format!("Aligned {ident}"));
            }
            Err(e) => {
                self.seq_segments.clear();
                self.seq_variants.clear();
                self.seq_summary = Some(e.to_string());
                self.toast = Some("Align refused".into());
            }
        }
    }

    fn attach_alignment_badges(&mut self, rec_name: &str, badges: Vec<AlignmentBadge>) {
        if badges.is_empty() {
            return;
        }
        let Some(entry) = self
            .library
            .plasmids
            .iter_mut()
            .find(|e| e.name == rec_name)
        else {
            return;
        };
        for badge in badges {
            if let Some(existing) = entry.alignments.iter_mut().find(|b| b.label == badge.label) {
                *existing = badge;
            } else {
                entry.alignments.push(badge);
            }
        }
        self.persist_library();
    }

    fn run_seq_sanger(&mut self) {
        let Some(path) = self.seq_path() else {
            self.toast = Some("Type an AB1 path".into());
            return;
        };
        match crate::io::load_ab1(&path) {
            Ok(tr) => {
                let phred = tr
                    .mean_phred()
                    .map(|p| format!("{p:.1}"))
                    .unwrap_or_else(|| "—".into());
                let n = tr.sequence.len();
                if self.record.is_some() {
                    let seq = tr.sequence.clone();
                    let label = tr.name.clone();
                    self.apply_alignment(&seq, &label);
                    if let Some(summary) = &mut self.seq_summary {
                        *summary = format!("{}  {n} bp  mean Phred {phred}  ·  {summary}", tr.name);
                    } else {
                        self.seq_summary = Some(format!("{}  {n} bp  mean Phred {phred}", tr.name));
                    }
                } else {
                    let rec = tr.to_record();
                    self.source_label = rec.name.clone();
                    self.record = Some(rec);
                    self.cursor = 0;
                    self.undo = UndoStack::new();
                    self.dirty = false;
                    self.map_circular = false;
                    self.seq_summary = Some(format!("{}  {n} bp  mean Phred {phred}", tr.name));
                    self.toast = Some("Loaded AB1 (linear)".into());
                }
            }
            Err(e) => {
                self.seq_summary = Some(e.to_string());
                self.toast = Some("AB1 refused".into());
            }
        }
    }

    fn run_seq_report(&mut self) {
        let Some(rec) = self.record.clone() else {
            self.toast = Some("Load a plasmid first".into());
            return;
        };
        let Some(path) = self.seq_path() else {
            self.toast = Some("Type a folder of reads".into());
            return;
        };
        let mut rows = crate::io::bulk_align_folder(&path, &rec.sequence, rec.circular);
        rows.sort_by_key(|row| match &row.result {
            Some(res) => crate::io::alignment_quality_status(res, rec.len()).report_priority(),
            None => 0,
        });
        let mut lines = Vec::new();
        let mut badges = Vec::new();
        for row in rows.iter().take(12) {
            if let Some(res) = &row.result {
                let st = crate::io::alignment_quality_status(res, rec.len());
                let ident = crate::io::format_identity_pct(Some(res.identity_pct), 1);
                lines.push(format!("{}  {}  {ident}", row.label, st.code()));
                badges.push(crate::io::badge_from_result(&row.label, res, rec.len()));
            } else {
                lines.push(format!(
                    "{}  error  {}",
                    row.label,
                    row.error.as_deref().unwrap_or("failed")
                ));
            }
        }
        self.attach_alignment_badges(&rec.name, badges);
        self.seq_summary = Some(format!("{} file(s)\n{}", rows.len(), lines.join("\n")));
        self.toast = Some(format!("Report: {} file(s)", rows.len()));
    }

    fn run_search(&mut self) {
        match self.search_tab {
            SearchTab::Local => self.run_local_blast(),
            SearchTab::Orf => self.run_orf_finder(),
            SearchTab::Online => self.run_online_search(),
            SearchTab::HmmDb => self.run_hmm_db_list(),
            SearchTab::Find => self.run_find_plasmid(),
        }
    }

    fn run_local_blast(&mut self) {
        let query = self.search_query.clone();
        if query.trim().is_empty() {
            self.search_summary = Some("Paste a query (DNA or protein).".into());
            return;
        }
        if let Some(rec) = &self.record {
            self.search_submitted = Some(record_fingerprint(&rec.name, &rec.sequence));
        }
        let (program, cleaned) = splicecraft_bio::detect_query_program(&query, self.search_program);
        let db = splicecraft_io::blast_db_from_library(&self.library, program, false);
        let hits = if program == BlastProgram::Hmmscan {
            splicecraft_bio::hmmscan_ungapped(&cleaned, &db, 25)
        } else {
            splicecraft_bio::blast_search(&cleaned, &db, 25)
        };
        if let Some(rec) = &self.record
            && let Some(fp) = &self.search_submitted
            && results_are_stale(fp, &rec.name, &rec.sequence)
        {
            self.search_lines.clear();
            self.search_summary = Some("Stale — canvas moved; results dropped.".into());
            self.toast = Some("Search results discarded (record changed)".into());
            return;
        }
        self.search_lines = hits
            .iter()
            .map(|h| {
                format!(
                    "{}  {}  {:+}  {:.1}%  {}..{}",
                    h.subject_name, h.kind, h.strand, h.identity_pct, h.s_start, h.s_end
                )
            })
            .collect();
        self.search_selected = 0;
        self.search_summary = Some(format!(
            "{}  {} hit(s)  (ungapped{}; query {} {})",
            program.as_str(),
            hits.len(),
            if cleaned.len() < splicecraft_bio::PYHMMER_MIN_QUERY_BLASTN {
                "; short-query fallback"
            } else {
                ""
            },
            cleaned.len(),
            if program == BlastProgram::Blastn {
                "bp"
            } else {
                "aa"
            }
        ));
        self.toast = Some(format!("{}: {} hit(s)", program.as_str(), hits.len()));
    }

    fn run_orf_finder(&mut self) {
        let Some(rec) = &self.record else {
            self.search_summary = Some("Load a plasmid first.".into());
            return;
        };
        self.search_submitted = Some(record_fingerprint(&rec.name, &rec.sequence));
        let include_alt = self.search_query.to_ascii_lowercase().contains("alt");
        let orfs = find_orfs(&rec.sequence, rec.circular, 30, include_alt);
        if let Some(fp) = &self.search_submitted
            && results_are_stale(fp, &rec.name, &rec.sequence)
        {
            self.search_lines.clear();
            self.search_summary = Some("Stale — canvas moved; results dropped.".into());
            return;
        }
        self.search_lines = orfs.iter().map(format_orf_row).collect();
        self.search_selected = 0;
        let wraps = orfs.iter().filter(|o| o.end < o.start).count();
        let laps = orfs.iter().filter(|o| o.exceeds_one_lap).count();
        self.search_summary = Some(format!(
            "{} ORF(s)  {wraps} wrap  {laps} full-lap  (length_aa, not start/end)",
            orfs.len()
        ));
        self.toast = Some(format!("{} ORF(s)", orfs.len()));
    }

    fn run_online_search(&mut self) {
        if !self.allow_online_search {
            self.search_lines.clear();
            self.search_summary = Some(
                "Online search is off. Tick allow_online_search — sequences are never uploaded silently."
                    .into(),
            );
            self.toast = Some("online search disabled".into());
            return;
        }
        let cancel = self.search_cancel.clone();
        if cancel.is_cancelled() {
            self.search_summary = Some("Cancelled.".into());
            return;
        }
        let policy = splicecraft_io::OnlineSearchPolicy {
            enabled: true,
            transport: &splicecraft_io::OfflineTransport,
            cancel: &cancel,
            poll_interval: splicecraft_io::ONLINE_POLL_INTERVAL,
            max_wait: splicecraft_io::ONLINE_MAX_WAIT,
        };
        let err = if self.search_program == BlastProgram::Hmmscan {
            splicecraft_io::hmmer_web_hmmscan(&self.search_query, 5, &policy).err()
        } else {
            splicecraft_io::ncbi_blast_online(
                &self.search_query,
                self.search_program.as_str(),
                None,
                5,
                &policy,
            )
            .err()
        };
        self.search_summary = Some(match err {
            Some(e) => e.to_string(),
            None => "online search returned no hits".into(),
        });
    }

    fn run_hmm_db_list(&mut self) {
        if let Some(layout) = &self.layout {
            self.hmm_catalog = load_hmm_catalog(layout);
        } else {
            self.hmm_catalog = splicecraft_persist::builtin_hmm_db_catalog().to_vec();
        }
        self.search_lines = self
            .hmm_catalog
            .iter()
            .map(|e| {
                format!(
                    "{}  {}  {}",
                    e.id,
                    e.name,
                    if e.builtin { "builtin" } else { "custom" }
                )
            })
            .collect();
        self.search_summary = Some(format!(
            "{} database(s) — download is chokepoint-gated; default CI never fetches Pfam",
            self.hmm_catalog.len()
        ));
    }

    fn run_find_plasmid(&mut self) {
        let q = self.search_query.clone();
        let mut hits = Vec::new();
        for col in &self.library.collections {
            for p in &col.plasmids {
                let hay = format!("{} {} {}", p.name, p.id, col.name);
                if fuzzy_text_match(&q, &hay) {
                    hits.push(format!("{}  {}  {} bp", p.name, col.name, p.size));
                }
            }
        }
        if hits.is_empty() {
            for p in &self.library.plasmids {
                let hay = format!("{} {}", p.name, p.id);
                if fuzzy_text_match(&q, &hay) {
                    hits.push(format!("{}  {} bp", p.name, p.size));
                }
            }
        }
        self.search_lines = hits;
        self.search_selected = 0;
        self.search_summary = Some(format!("{} plasmid(s)", self.search_lines.len()));
    }
}

fn format_orf_row(o: &Orf) -> String {
    let flag = if o.exceeds_one_lap {
        "full-lap"
    } else if o.end < o.start {
        "wrap"
    } else {
        "linear"
    };
    format!(
        "{flag}  {:+}  {} aa  {} bp  {}..{}",
        o.strand, o.length_aa, o.nt_len, o.start, o.end
    )
}

fn collision_toast(class: CollisionClass, name: &str) -> String {
    match class {
        CollisionClass::ExactCopy => {
            format!("{name} is already in the library — s skip / c copy / o overwrite")
        }
        CollisionClass::NameClash => {
            format!("{name} exists with different content — s skip / c copy / o overwrite")
        }
        CollisionClass::New => format!("{name} — unexpected prompt"),
    }
}

/// Tiny circular filler with a wrap feature + CDS. Sequence stays off logs.
pub fn demo_record() -> Record {
    let mut seq = String::from("ATGAAATAG");
    seq.push_str(&"ATGC".repeat(28));
    seq.truncate(120);
    let mut rec = Record::new("pDemo", seq, true);
    rec.features
        .push(splicecraft_core::Feature::new("CDS", 0, 9, 1, "orf"));
    rec.features.push(splicecraft_core::Feature::new(
        "misc_feature",
        110,
        8,
        1,
        "wrap_ori",
    ));
    rec
}
