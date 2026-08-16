//! Ratatui workbench shell for SpliceCraft.rs.
//!
//! Stage 04: menus, pane chrome, `?` help, Ctrl+K palette.
//! Event → [`Action`] → [`AppState::reduce`] → draw. No persist writes.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_clone as clone;
pub use splicecraft_core as core;
pub use splicecraft_gels as gels;
pub use splicecraft_persist as persist;

mod action;
mod commands;
mod draw;
mod keys;
mod state;

pub use action::{Action, FocusMode, Overlay, Pane};
pub use commands::{Command, filter_commands, palette_commands};
pub use draw::draw_workbench;
pub use keys::{KEY_TABLE, KeyEntry, action_from_key};
pub use state::AppState;

use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};

/// Stage this crate currently satisfies (workbench chrome).
pub const IMPLEMENTATION_STAGE: u8 = 4;

/// Title painted on the menu bar and help overlay.
pub const WELCOME_TITLE: &str = "SpliceCraft.rs";

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Run the interactive TUI until Quit.
pub fn run() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut AppState::new());
    ratatui::restore();
    result
}

fn run_app(terminal: &mut DefaultTerminal, state: &mut AppState) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw_workbench(frame, state))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if !apply_key(state, key) {
                    break;
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

/// Apply a key to `state`. `false` means the process should exit.
pub fn apply_key(state: &mut AppState, key: KeyEvent) -> bool {
    if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
        return true;
    }
    match action_from_key(state, key) {
        Some(action) => state.reduce(action),
        None => true,
    }
}

/// Stage-00 name kept so older call sites still compile. Draws the workbench.
pub fn draw_welcome(frame: &mut Frame<'_>) {
    draw_workbench(frame, &AppState::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn buffer_text(backend: &TestBackend) -> String {
        let buffer = backend.buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                out.push_str(cell.symbol());
            }
            out.push('\n');
        }
        out
    }

    fn draw_text(width: u16, height: u16, state: &AppState) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| draw_workbench(frame, state))
            .expect("draw");
        buffer_text(terminal.backend())
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-tui");
    }

    #[test]
    fn persist_data_dir_is_isolated_from_python() {
        assert_eq!(persist::XDG_DATA_DIR_LEAF, "splicecraft-rs");
    }

    #[test]
    fn welcome_frame_paints_title() {
        let text = draw_text(60, 16, &AppState::new());
        assert!(
            text.contains(WELCOME_TITLE),
            "workbench missing title, got:\n{text}"
        );
        assert!(
            text.contains("stage 04") || text.contains("stage 4"),
            "status bar missing stage, got:\n{text}"
        );
    }

    #[test]
    fn empty_canvas_when_no_record() {
        let text = draw_text(80, 24, &AppState::new());
        assert!(
            text.contains("Empty canvas") || text.contains("no plasmid"),
            "expected empty-canvas copy, got:\n{text}"
        );
        assert!(
            text.contains("(no record)"),
            "status missing empty label:\n{text}"
        );
    }

    #[test]
    fn help_overlay_contains_q_quit() {
        let mut state = AppState::new();
        assert!(state.reduce(Action::ToggleHelp));
        assert_eq!(state.overlay, Overlay::Help);
        let text = draw_text(80, 24, &state);
        let lower = text.to_ascii_lowercase();
        assert!(
            text.contains("keyboard shortcuts"),
            "help overlay missing title:\n{text}"
        );
        assert!(
            text.contains("q / Esc") || text.contains("q /"),
            "missing q:\n{text}"
        );
        assert!(lower.contains("quit"), "help overlay missing quit:\n{text}");
    }

    #[test]
    fn palette_lists_open_help_quit() {
        let mut state = AppState::new();
        assert!(state.reduce(Action::OpenPalette));
        assert_eq!(state.overlay, Overlay::Palette);
        let titles: Vec<_> = state.visible_commands().iter().map(|c| c.title).collect();
        assert!(titles.iter().any(|t| t.contains("Open")), "{titles:?}");
        assert!(titles.contains(&"Help"), "{titles:?}");
        assert!(titles.contains(&"Quit"), "{titles:?}");
        let text = draw_text(80, 24, &state);
        assert!(text.contains("Open"), "palette draw missing Open:\n{text}");
        assert!(text.contains("Help"), "palette draw missing Help:\n{text}");
        assert!(text.contains("Quit"), "palette draw missing Quit:\n{text}");
    }

    #[test]
    fn q_and_esc_quit_from_main_view() {
        let mut state = AppState::new();
        assert!(!apply_key(&mut state, key(KeyCode::Char('q'))));
        let mut state = AppState::new();
        assert!(!apply_key(&mut state, key(KeyCode::Esc)));
        let mut state = AppState::new();
        assert!(!apply_key(&mut state, ctrl('q')));
    }

    #[test]
    fn q_closes_help_instead_of_quitting() {
        let mut state = AppState::new();
        assert!(state.reduce(Action::ToggleHelp));
        assert!(apply_key(&mut state, key(KeyCode::Char('q'))));
        assert_eq!(state.overlay, Overlay::None);
    }

    #[test]
    fn ctrl_k_opens_palette() {
        let mut state = AppState::new();
        assert!(apply_key(&mut state, ctrl('k')));
        assert_eq!(state.overlay, Overlay::Palette);
    }

    #[test]
    fn resize_does_not_panic() {
        let state = AppState::new();
        let _ = draw_text(40, 12, &state);
        let _ = draw_text(160, 40, &state);
        let mut help = AppState::new();
        help.reduce(Action::ToggleHelp);
        let _ = draw_text(40, 12, &help);
        let _ = draw_text(160, 40, &help);
        let mut pal = AppState::new();
        pal.reduce(Action::OpenPalette);
        let _ = draw_text(40, 12, &pal);
        let _ = draw_text(160, 40, &pal);
    }

    #[test]
    fn palette_fuzzy_narrows_and_demo_stays_in_memory() {
        let mut state = AppState::new();
        state.reduce(Action::OpenPalette);
        state.reduce(Action::PaletteInput('d'));
        state.reduce(Action::PaletteInput('e'));
        state.reduce(Action::PaletteInput('m'));
        let titles: Vec<_> = state.visible_commands().iter().map(|c| c.title).collect();
        assert!(
            titles.iter().any(|t| t.contains("demo")),
            "expected demo command, got {titles:?}"
        );
        state.palette_selected = titles
            .iter()
            .position(|t| t.contains("demo"))
            .expect("demo");
        assert!(state.reduce(Action::PaletteExecute));
        assert!(state.record.is_some());
        assert_eq!(state.source_label, "pDemo (memory)");
        assert_eq!(state.record.as_ref().map(|r| r.len()), Some(48));
    }

    #[test]
    fn stub_open_toasts_without_persist() {
        let mut state = AppState::new();
        assert!(apply_key(&mut state, ctrl('o')));
        let toast = state.toast.expect("toast");
        assert!(toast.contains("Open file"), "{toast}");
        assert!(toast.contains("stage 06"), "{toast}");
    }
}
