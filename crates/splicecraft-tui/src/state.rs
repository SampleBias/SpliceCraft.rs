//! Workbench state. Reduce is pure except optional authorised autosave.

use splicecraft_bio::reverse_complement_record;
use splicecraft_core::{Record, rotate_record};

use crate::action::{Action, FocusMode, Overlay, Pane};
use crate::commands::{Command, filter_commands};
use crate::editor::{UndoStack, delete_span, insert_bases, smallest_enclosing};

/// In-memory workbench. Library JSON is never written from this crate.
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
            show_labels: true,
            view_origin: 0,
            cursor: 0,
            selected_feat: None,
            undo: UndoStack::new(),
            dirty: false,
        }
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
