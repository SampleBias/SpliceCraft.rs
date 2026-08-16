//! Workbench state. Reduce is pure: no persist, no network, no sequence logs.

use splicecraft_core::Record;

use crate::action::{Action, FocusMode, Overlay, Pane};
use crate::commands::{Command, filter_commands};

/// In-memory workbench. The optional record is never written from stage 04.
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
    /// Loaded record, if any. Memory only.
    pub record: Option<Record>,
    /// Display name for the status bar (`(no record)`, `pDemo (memory)`, …).
    pub source_label: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Empty canvas — no demo, no library, no persist.
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
                self.toast = Some("Loaded memory-only demo — not saved".into());
            }
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
}

/// Tiny circular filler. Sequence stays off the log path.
fn demo_record() -> Record {
    Record::new("pDemo", "ATGC".repeat(12), true)
}
