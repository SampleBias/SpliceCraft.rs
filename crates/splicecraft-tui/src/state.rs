//! Workbench state. Library writes go through the persist chokepoint.

use splicecraft_bio::{CustomEnzyme, extract_feature, reverse_complement_record};
use splicecraft_core::{Record, rotate_record};
use splicecraft_persist::{
    CollisionChoice, CollisionClass, DataLayout, EnzymeStore, FeatureSnippet, KeepOutcome,
    LibraryEntry, LibraryStore,
};
use splicecraft_primer::{
    PrimerRecord, PrimerStatus, PrimerStore, design_cloning_primers, design_detection_primers,
    design_generic_primers, design_golden_braid_primers, insilico_pcr_amplicons,
    primer_binding_sites, primer_check_confidence, primer_tm, rederive_primer_binding,
};

use crate::action::{Action, DesignKind, FocusMode, Overlay, Pane, PathKind};
use crate::commands::{Command, filter_commands};
use crate::editor::{UndoStack, delete_span, insert_bases, smallest_enclosing};

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
        }
    }

    /// Attach a data dir and load collections from disk.
    pub fn attach_layout(&mut self, layout: DataLayout) {
        self.library = LibraryStore::load(&layout);
        self.enzymes = EnzymeStore::load(&layout);
        self.primers = PrimerStore::load(&layout);
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
                self.overlay = Overlay::None;
                self.reset_palette();
                self.path_query.clear();
                self.tool_query.clear();
                self.pending_collision = None;
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
            Action::ToolTab => {
                if self.overlay == Overlay::PrimerDesign {
                    self.design_kind = self.design_kind.next();
                    self.design_summary = None;
                }
            }
            Action::ToolEnter => match self.overlay {
                Overlay::PrimerDesign => self.run_primer_design(),
                Overlay::PrimerCheck => self.run_primer_check(),
                Overlay::Enzymes => self.activate_selected_collection(),
                _ => {}
            },
            Action::PrimerDesignSave => self.save_designed_primers(),
            Action::ToolInput(c) => {
                if self.overlay == Overlay::PrimerCheck && !c.is_control() {
                    self.tool_query.push(c);
                }
            }
            Action::ToolBackspace => {
                if self.overlay == Overlay::PrimerCheck {
                    self.tool_query.pop();
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
            Action::Stub { name, stage } => {
                self.toast = Some(format!("{name} — not implemented until stage {stage:02}"));
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
