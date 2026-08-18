//! Key table and `KeyEvent` → [`Action`] mapping.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, CollisionChoice, Overlay, Pane};
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
        keys: "F10",
        description: "Menu bar (←/→ highlight, Enter opens)",
    },
    KeyEntry {
        keys: "Alt+S/P/H",
        description: "Settings / Primers / History (more Alt letters)",
    },
    KeyEntry {
        keys: "Ctrl+O",
        description: "Open file path",
    },
    KeyEntry {
        keys: "f",
        description: "Fetch NCBI accession",
    },
    KeyEntry {
        keys: "Ctrl+S",
        description: "Save (keep through persist)",
    },
    KeyEntry {
        keys: "Ctrl+N",
        description: "New plasmid (paste DNA)",
    },
    KeyEntry {
        keys: "Ctrl+F",
        description: "Find DNA (both strands)",
    },
    KeyEntry {
        keys: "Ctrl+A",
        description: "Select all sequence",
    },
    KeyEntry {
        keys: "Ctrl+C",
        description: "Copy selection (top strand)",
    },
    KeyEntry {
        keys: "Alt+C",
        description: "Copy selection (bottom RC)",
    },
    KeyEntry {
        keys: "Ctrl+B",
        description: "BLAST / search overlay",
    },
    KeyEntry {
        keys: "Ctrl+P",
        description: "Primer design",
    },
    KeyEntry {
        keys: "F6 / Alt+H",
        description: "History",
    },
    KeyEntry {
        keys: "Alt+Shift+F",
        description: "Add feature from selection",
    },
    KeyEntry {
        keys: "Alt+K",
        description: "Keep into collection",
    },
    KeyEntry {
        keys: "v / r / l",
        description: "Map view / RE overlay / labels",
    },
    KeyEntry {
        keys: "u / 6",
        description: "Unique cutters / 6+ sites",
    },
    KeyEntry {
        keys: "[ / ]",
        description: "Previous / next enzyme collection",
    },
    KeyEntry {
        keys: "Del (library)",
        description: "Delete plasmid (session undo)",
    },
    KeyEntry {
        keys: "Ctrl+Z / Y",
        description: "Undo / redo",
    },
    KeyEntry {
        keys: "Alt+Shift+R",
        description: "Flip (reverse complement)",
    },
    KeyEntry {
        keys: "Alt+Shift+O",
        description: "Set origin here (circular)",
    },
];

/// Map a keypress to an action. Overlay state wins so `q` closes help
/// instead of quitting.
#[must_use]
pub fn action_from_key(state: &AppState, key: KeyEvent) -> Option<Action> {
    if has_ctrl(key, 'q') {
        return Some(Action::Quit);
    }
    if state.show_splash {
        return Some(Action::DismissSplash);
    }
    match state.overlay {
        Overlay::Help => help_key(key),
        Overlay::Palette => palette_key(key),
        Overlay::Collision => collision_key(key),
        Overlay::Path => path_prompt_key(key),
        Overlay::PrimerDesign
        | Overlay::PrimerCheck
        | Overlay::Enzymes
        | Overlay::Constructor
        | Overlay::Mutato
        | Overlay::Synthesis
        | Overlay::Simulator
        | Overlay::Sequencing
        | Overlay::Experiments
        | Overlay::History
        | Overlay::Search
        | Overlay::Parts
        | Overlay::Settings
        | Overlay::Babs
        | Overlay::Autolab => tool_key(state, key),
        Overlay::MasterDelete => master_delete_key(key),
        Overlay::FileMenu => file_menu_key(key),
        Overlay::None => main_key(state, key),
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

fn collision_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::CollisionPick(CollisionChoice::Cancel)),
        KeyCode::Char('s') => Some(Action::CollisionPick(CollisionChoice::Skip)),
        KeyCode::Char('c') => Some(Action::CollisionPick(CollisionChoice::Copy)),
        KeyCode::Char('o') => Some(Action::CollisionPick(CollisionChoice::Overwrite)),
        _ => None,
    }
}

fn path_prompt_key(key: KeyEvent) -> Option<Action> {
    if has_ctrl(key, 'k') {
        return Some(Action::CloseOverlay);
    }
    match key.code {
        KeyCode::Esc => Some(Action::CloseOverlay),
        KeyCode::Enter => Some(Action::PathSubmit),
        KeyCode::Backspace => Some(Action::PathBackspace),
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PathInput(c))
        }
        _ => None,
    }
}

fn tool_key(state: &AppState, key: KeyEvent) -> Option<Action> {
    if has_ctrl(key, 'k') {
        return Some(Action::OpenPalette);
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseOverlay),
        KeyCode::Tab => Some(Action::ToolTab),
        KeyCode::Enter => Some(Action::ToolEnter),
        KeyCode::Up => Some(Action::ToolMove(-1)),
        KeyCode::Down => Some(Action::ToolMove(1)),
        KeyCode::Backspace => Some(Action::ToolBackspace),
        KeyCode::F(7) if state.overlay == Overlay::Experiments => {
            Some(Action::ExperimentSpellcheck)
        }
        KeyCode::Char('g')
            if state.overlay == Overlay::Experiments
                && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Action::ExperimentJump)
        }
        KeyCode::Char('s')
            if state.overlay == Overlay::PrimerDesign && key.modifiers.is_empty() =>
        {
            Some(Action::PrimerDesignSave)
        }
        KeyCode::Char('s') if state.overlay == Overlay::Constructor && key.modifiers.is_empty() => {
            Some(Action::ConstructorSave)
        }
        KeyCode::Char('s') if state.overlay == Overlay::Simulator && key.modifiers.is_empty() => {
            Some(Action::SimulatorSave)
        }
        KeyCode::Char('g') if state.overlay == Overlay::Simulator && key.modifiers.is_empty() => {
            Some(Action::SimulatorSendToGel)
        }
        KeyCode::Char('j') if state.overlay == Overlay::Sequencing && key.modifiers.is_empty() => {
            Some(Action::SequencingJump)
        }
        KeyCode::Char('a') if state.overlay == Overlay::Constructor && key.modifiers.is_empty() => {
            Some(Action::ConstructorDesignArms)
        }
        KeyCode::Char(c)
            if matches!(
                state.overlay,
                Overlay::PrimerCheck
                    | Overlay::Constructor
                    | Overlay::Mutato
                    | Overlay::Synthesis
                    | Overlay::Simulator
                    | Overlay::Sequencing
            ) && !key.modifiers.contains(KeyModifiers::CONTROL)
                && c != 's'
                && c != 'a'
                && c != 'g'
                && c != 'j' =>
        {
            Some(Action::ToolInput(c))
        }
        KeyCode::Char(c)
            if matches!(
                state.overlay,
                Overlay::Experiments | Overlay::History | Overlay::Search | Overlay::Babs
            ) && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Action::ToolInput(c))
        }
        _ => None,
    }
}

fn master_delete_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') => {
            Some(Action::CloseOverlay)
        }
        KeyCode::Left => Some(Action::ToolMove(-1)),
        KeyCode::Right => Some(Action::ToolMove(1)),
        KeyCode::Enter => Some(Action::ToolEnter),
        KeyCode::Backspace => Some(Action::ToolBackspace),
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::ToolInput(c))
        }
        _ => None,
    }
}

fn file_menu_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseOverlay),
        KeyCode::Up => Some(Action::ToolMove(-1)),
        KeyCode::Down => Some(Action::ToolMove(1)),
        KeyCode::Enter => Some(Action::ToolEnter),
        KeyCode::Left => Some(Action::MenuMove(-1)),
        KeyCode::Right => Some(Action::MenuMove(1)),
        _ => None,
    }
}

fn menu_nav_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ToggleMenuFocus),
        KeyCode::Left => Some(Action::MenuMove(-1)),
        KeyCode::Right => Some(Action::MenuMove(1)),
        KeyCode::Enter => Some(Action::MenuActivate),
        KeyCode::Char('q') if key.modifiers.is_empty() => Some(Action::Quit),
        _ => None,
    }
}

fn main_key(state: &AppState, key: KeyEvent) -> Option<Action> {
    if has_ctrl(key, 'k') {
        return Some(Action::OpenPalette);
    }
    if has_ctrl(key, 'o') {
        return Some(Action::OpenPathPrompt);
    }
    if has_ctrl(key, 'b') {
        return Some(Action::OpenSearch);
    }
    if has_ctrl(key, 'p') {
        return Some(Action::OpenPrimerDesign);
    }
    if has_ctrl(key, 's') {
        return Some(Action::SaveRecord);
    }
    if has_ctrl(key, 'f') {
        return Some(Action::OpenFindDna);
    }
    if has_ctrl(key, 'a') {
        return Some(Action::SelectAll);
    }
    if has_ctrl(key, 'c') {
        return Some(Action::CopyTop);
    }
    if has_ctrl(key, 'n') {
        return Some(Action::OpenNewPlasmid);
    }
    if has_ctrl(key, 'z') {
        return Some(Action::Undo);
    }
    if has_ctrl(key, 'y') {
        return Some(Action::Redo);
    }
    if has_alt_shift(key, 'r') {
        return Some(Action::FlipRecord);
    }
    if has_alt_shift(key, 'o') {
        return Some(Action::SetOriginHere);
    }
    if has_alt_shift(key, 'f') {
        return Some(Action::OpenAddFeature);
    }
    if has_alt(key, 'c') {
        return Some(Action::CopyBottom);
    }
    if has_alt(key, 'k') {
        return Some(Action::KeepRecord);
    }
    if let KeyCode::Char(ch) = key.code
        && has_alt(key, ch)
        && let Some(action) = crate::menu::alt_menu_action(ch)
    {
        return Some(action);
    }
    if key.code == KeyCode::F(10) {
        return Some(Action::ToggleMenuFocus);
    }
    if key.code == KeyCode::F(6) {
        return Some(Action::OpenHistory);
    }
    if state.menu_focus {
        return menu_nav_key(key);
    }
    let seq = state.focus == Pane::Sequence;
    let lib = state.focus == Pane::Library;
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc if key.modifiers.is_empty() => Some(Action::Quit),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::F(1) => Some(Action::FocusPane(Pane::Library)),
        KeyCode::F(2) => Some(Action::FocusPane(Pane::Map)),
        KeyCode::F(3) => Some(Action::FocusPane(Pane::Features)),
        KeyCode::F(4) => Some(Action::FocusPane(Pane::Sequence)),
        KeyCode::F(5) => Some(Action::FocusAll),
        KeyCode::Enter if lib => Some(Action::LibraryOpen),
        KeyCode::Enter => Some(Action::EnterPickFeature),
        KeyCode::Up if lib => Some(Action::LibraryMove(-1)),
        KeyCode::Down if lib => Some(Action::LibraryMove(1)),
        KeyCode::Delete if lib => Some(Action::LibraryDelete),
        KeyCode::Backspace | KeyCode::Delete => Some(Action::DeleteBack),
        KeyCode::Left if seq => Some(Action::MoveCursor(-1)),
        KeyCode::Right if seq => Some(Action::MoveCursor(1)),
        KeyCode::Left => Some(Action::RotateView(-1)),
        KeyCode::Right => Some(Action::RotateView(1)),
        KeyCode::Home => Some(Action::ResetView),
        KeyCode::Char(c) if seq && key.modifiers.is_empty() && is_iupac_base(c) => {
            Some(Action::InsertBase(c))
        }
        KeyCode::Char('v') if key.modifiers.is_empty() => Some(Action::ToggleMapView),
        KeyCode::Char('r') if key.modifiers.is_empty() => Some(Action::ToggleRestr),
        KeyCode::Char('u') if key.modifiers.is_empty() => Some(Action::ToggleRestrUnique),
        KeyCode::Char('6') if key.modifiers.is_empty() => Some(Action::ToggleRestrSixPlus),
        KeyCode::Char('[') if key.modifiers.is_empty() => Some(Action::CycleEnzymeCollection(-1)),
        KeyCode::Char(']') if key.modifiers.is_empty() => Some(Action::CycleEnzymeCollection(1)),
        KeyCode::Char('l') if key.modifiers.is_empty() => Some(Action::ToggleLabels),
        KeyCode::Char('o') if key.modifiers.is_empty() => Some(Action::OpenPathPrompt),
        KeyCode::Char('f') if key.modifiers.is_empty() => Some(Action::OpenFetch),
        _ => None,
    }
}

fn has_ctrl(key: KeyEvent, ch: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char(c) if c.eq_ignore_ascii_case(&ch))
}

fn has_alt(key: KeyEvent, ch: char) -> bool {
    key.modifiers.contains(KeyModifiers::ALT)
        && !key.modifiers.contains(KeyModifiers::SHIFT)
        && matches!(key.code, KeyCode::Char(c) if c.eq_ignore_ascii_case(&ch))
}

fn has_alt_shift(key: KeyEvent, ch: char) -> bool {
    key.modifiers.contains(KeyModifiers::ALT)
        && key.modifiers.contains(KeyModifiers::SHIFT)
        && matches!(key.code, KeyCode::Char(c) if c.eq_ignore_ascii_case(&ch))
}

fn is_iupac_base(c: char) -> bool {
    matches!(
        c.to_ascii_uppercase(),
        'A' | 'C'
            | 'G'
            | 'T'
            | 'U'
            | 'R'
            | 'Y'
            | 'M'
            | 'K'
            | 'S'
            | 'W'
            | 'B'
            | 'D'
            | 'H'
            | 'V'
            | 'N'
    )
}
