//! Key table and `KeyEvent` → [`Action`] mapping.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, Overlay, Pane};
use crate::state::AppState;

/// One row of the `?` overlay (and the status-bar cheat sheet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEntry {
    /// Binding as shown to the user (`q / Esc`).
    pub keys: &'static str,
    /// What it does in this stage.
    pub description: &'static str,
}

/// Static keyboard reference. Quit stays first so a 40×12 help overlay
/// still contains the acceptance binding.
pub const KEY_TABLE: &[KeyEntry] = &[
    KeyEntry {
        keys: "q / Esc",
        description: "Quit (main view)",
    },
    KeyEntry {
        keys: "Ctrl+Q",
        description: "Quit",
    },
    KeyEntry {
        keys: "?",
        description: "Help overlay",
    },
    KeyEntry {
        keys: "Ctrl+K",
        description: "Command palette",
    },
    KeyEntry {
        keys: "F1–F4",
        description: "Focus library / map / features / sequence",
    },
    KeyEntry {
        keys: "F5",
        description: "Restore all panels",
    },
    KeyEntry {
        keys: "Ctrl+O",
        description: "Open file (stage 06)",
    },
    KeyEntry {
        keys: "f",
        description: "Fetch NCBI (stage 13)",
    },
    KeyEntry {
        keys: "v / r / l",
        description: "Map view / RE overlay / labels (stage 05)",
    },
];

/// Map a keypress to an action. Overlay state wins so `q` closes help
/// instead of quitting.
#[must_use]
pub fn action_from_key(state: &AppState, key: KeyEvent) -> Option<Action> {
    if has_ctrl(key, 'q') {
        return Some(Action::Quit);
    }
    match state.overlay {
        Overlay::Help => help_key(key),
        Overlay::Palette => palette_key(key),
        Overlay::None => main_key(key),
    }
}

fn help_key(key: KeyEvent) -> Option<Action> {
    if has_ctrl(key, 'k') {
        return Some(Action::OpenPalette);
    }
    match key.code {
        KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Esc => Some(Action::CloseOverlay),
        _ => None,
    }
}

fn palette_key(key: KeyEvent) -> Option<Action> {
    if has_ctrl(key, 'k') {
        return Some(Action::CloseOverlay);
    }
    match key.code {
        KeyCode::Esc => Some(Action::CloseOverlay),
        KeyCode::Enter => Some(Action::PaletteExecute),
        KeyCode::Up => Some(Action::PaletteMove(-1)),
        KeyCode::Down => Some(Action::PaletteMove(1)),
        KeyCode::Backspace => Some(Action::PaletteBackspace),
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PaletteInput(c))
        }
        _ => None,
    }
}

fn main_key(key: KeyEvent) -> Option<Action> {
    if has_ctrl(key, 'k') {
        return Some(Action::OpenPalette);
    }
    if has_ctrl(key, 'o') {
        return Some(Action::Stub {
            name: "Open file",
            stage: 6,
        });
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::F(1) => Some(Action::FocusPane(Pane::Library)),
        KeyCode::F(2) => Some(Action::FocusPane(Pane::Map)),
        KeyCode::F(3) => Some(Action::FocusPane(Pane::Features)),
        KeyCode::F(4) => Some(Action::FocusPane(Pane::Sequence)),
        KeyCode::F(5) => Some(Action::FocusAll),
        KeyCode::Char('o') if key.modifiers.is_empty() => Some(Action::Stub {
            name: "Open file",
            stage: 6,
        }),
        KeyCode::Char('f') if key.modifiers.is_empty() => Some(Action::Stub {
            name: "Fetch from NCBI",
            stage: 13,
        }),
        _ => None,
    }
}

fn has_ctrl(key: KeyEvent, ch: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char(c) if c.eq_ignore_ascii_case(&ch))
}
