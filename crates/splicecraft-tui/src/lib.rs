//! Ratatui workbench: map, sequence editor, help, palette, cloning, Mutato,
//! Simulator, Sequencing, Experiments, History, Search, satellites.
//!
//! Stage 18. Event → [`Action`] → [`AppState::reduce`] → draw.
//! Library writes go through `safe_save_json`. Crash-recovery autosave
//! uses the persist chokepoint only. Map PNG/SVG writes use atomic user paths.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_clone as clone;
pub use splicecraft_codon as codon;
pub use splicecraft_core as core;
pub use splicecraft_gels as gels;
pub use splicecraft_io as io;
pub use splicecraft_persist as persist;
pub use splicecraft_primer as primer;

mod action;
mod autolab;
mod babs;
mod commands;
mod demo;
mod draw;
mod editor;
mod keys;
mod mapimage;
mod menu;
mod render;
mod splash;
mod state;
mod theme;

pub use action::{
    Action, CollisionChoice, ConstructorTab, DesignKind, ExperimentsTab, FocusMode, HistoryTab,
    MutatoTab, Overlay, Pane, PathKind, SearchTab, SequencingTab, SimulatorTab, SynthTab,
};
pub use autolab::{Ot2Compile, Ot2Plan, compile_protocol, confirm_motion, fixture_deck};
pub use babs::{
    BabsCommand, BabsError, DEFAULT_OLLAMA_HOST, assert_ollama_loopback, ollama_base,
    ollama_base_from, ollama_chat, ollama_list_models, parse_command, strip_think, trim_history,
};
pub use commands::{Command, filter_commands, palette_commands};
pub use demo::{ADVANCED_DEMO_LEN, demo_record, demo_record_advanced};
pub use draw::draw_workbench;
pub use editor::{UNDO_LIMIT, UndoStack};
pub use keys::{KEY_TABLE, KeyEntry, action_from_key};
pub use mapimage::{
    MAP_IMAGE_MAX_SIZE, MAP_IMAGE_MIN_SIZE, MapExportReport, MapImageOpts, clamp_size,
    export_plasmid_map, export_plasmid_maps, render_map_bytes, render_plasmid_map_png,
    render_plasmid_map_svg, svg_is_well_formed,
};
pub use menu::{FILE_ITEMS, MENUS, alt_menu_action, menu_action};
pub use render::{
    MapOptions, SeqView, feature_label_bp, lines_contain_braille, render_map, render_map_styled,
    render_sequence, render_sequence_styled,
};
pub use splash::{compose_splash, draw_splash};
pub use state::{AppState, MASTER_DELETE_CONFIRM_COOLDOWN};
pub use theme::{
    AA_GREEN, DEFAULT_TYPE_COLORS, FEATURE_PALETTE_XTERM, FOOTER_SHORTCUTS, SEQUENCE_ROWS,
    SIDE_PANE_COLS, default_type_color, feature_paint_color, parse_color_input,
    resolve_feature_color, xterm_index_to_rgb,
};

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};

use crate::splash::{HELIX_TICK_MS, helix_phase};

/// Stage this crate currently satisfies (interaction keys + menus).
pub const IMPLEMENTATION_STAGE: u8 = 19;

/// Title painted on the menu bar and help overlay.
pub const WELCOME_TITLE: &str = "SpliceCraft.rs";

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Options for [`run_with`].
#[derive(Clone, Copy, Debug)]
pub struct RunOptions {
    /// Show the greyscale DNA splash until any key.
    pub splash: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            splash: splash_enabled_from_env(),
        }
    }
}

/// `SPLICECRAFT_NO_SPLASH=1` skips the entry screen (tests / scripted runs).
#[must_use]
pub fn splash_enabled_from_env() -> bool {
    !std::env::var("SPLICECRAFT_NO_SPLASH")
        .ok()
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
}

/// Run the interactive TUI until Quit.
pub fn run() -> std::io::Result<()> {
    run_with(RunOptions::default())
}

/// Run the TUI with explicit splash / no-splash.
pub fn run_with(opts: RunOptions) -> std::io::Result<()> {
    persist::authorize_writes("splicecraft-tui");
    let mut state = AppState::new();
    state.show_splash = opts.splash;
    if let Ok(layout) = persist::DataLayout::resolve() {
        state.attach_layout(layout);
    }
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut state);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut DefaultTerminal, state: &mut AppState) -> std::io::Result<()> {
    let mut dirty_since: Option<Instant> = None;
    let splash_t0 = Instant::now();
    loop {
        terminal.draw(|frame| {
            if state.show_splash {
                draw_splash(frame, helix_phase(splash_t0.elapsed().as_secs_f64()));
            } else {
                draw_workbench(frame, state);
            }
        })?;
        maybe_autosave(state, &mut dirty_since);
        let poll_ms = if state.show_splash {
            HELIX_TICK_MS
        } else {
            250
        };
        if !event::poll(Duration::from_millis(poll_ms))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if !apply_key(state, key) {
                    break;
                }
                if state.dirty {
                    dirty_since.get_or_insert_with(Instant::now);
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

fn maybe_autosave(state: &AppState, dirty_since: &mut Option<std::time::Instant>) {
    let Some(started) = *dirty_since else {
        return;
    };
    if started.elapsed() < persist::AUTOSAVE_DEBOUNCE {
        return;
    }
    if try_autosave(state).unwrap_or(false) {
        *dirty_since = None;
    }
}

/// Write a crash-recovery `.gb` if writes are authorised. Tests stay offline.
pub fn try_autosave(state: &AppState) -> Result<bool, persist::PersistError> {
    if !persist::writes_authorized() {
        return Ok(false);
    }
    let Some(rec) = &state.record else {
        return Ok(false);
    };
    if rec.id.is_empty() {
        return Ok(false);
    }
    let text =
        io::record_to_gb_text(rec).map_err(|e| persist::PersistError::Commit(e.to_string()))?;
    let dir = persist::data_dir()?;
    let Some(path) = persist::crash_recovery_path(&dir, &rec.id) else {
        return Ok(false);
    };
    persist::write_crash_recovery(&path, &text)?;
    Ok(true)
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

    fn alt(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT)
    }

    fn alt_shift(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT | KeyModifiers::SHIFT)
    }

    #[test]
    fn theme_doc_exists() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text = std::fs::read_to_string(root.join("docs/theme.md"))
            .expect("docs/theme.md is the stage-17 contract");
        assert!(text.contains("_DEFAULT_TYPE_COLORS") || text.contains("CDS"));
        assert!(text.contains("17"));
    }

    #[test]
    fn styled_map_and_sidebar_use_feature_colors() {
        let rec = demo_record();
        let lines = render_map_styled(
            &rec,
            &MapOptions {
                width: 48,
                height: 16,
                show_labels: true,
                ..MapOptions::default()
            },
        );
        let cds = default_type_color("CDS").expect("CDS color");
        let misc = default_type_color("misc_feature").expect("misc color");
        let has_cds = lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.style.fg == Some(cds) && s.content.contains('o'))
        });
        let has_misc = lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.style.fg == Some(misc) && !s.content.trim().is_empty())
        });
        assert!(
            has_cds || has_misc,
            "map labels must carry feature colors, got {lines:?}"
        );

        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).expect("term");
        terminal
            .draw(|frame| draw_workbench(frame, &state))
            .expect("draw");
        let buf = terminal.backend().buffer();
        let mut saw_non_gray_feat = false;
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                let sym = cell.symbol();
                if (sym.contains("CDS") || sym.contains("orf") || sym.contains("misc"))
                    && cell.style().fg != Some(ratatui::style::Color::Gray)
                    && cell.style().fg.is_some()
                {
                    saw_non_gray_feat = true;
                }
                // Also accept orange CDS / teal misc RGB on any glyph in features pane.
                if matches!(
                    cell.style().fg,
                    Some(ratatui::style::Color::Rgb(255, 165, 0))
                        | Some(ratatui::style::Color::Rgb(32, 178, 170))
                ) {
                    saw_non_gray_feat = true;
                }
            }
        }
        assert!(
            saw_non_gray_feat,
            "feature sidebar / map must paint non-gray feature colors"
        );
    }

    #[test]
    fn layout_uses_fixed_side_panes_on_wide_terminal() {
        assert_eq!(SIDE_PANE_COLS, 32);
        assert_eq!(SEQUENCE_ROWS, 14);
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        state.toast = None; // toast temporarily replaces footer shortcuts
        let text = draw_text(120, 36, &state);
        assert!(text.contains("Library") && text.contains("Features") && text.contains("Map"));
        assert!(text.contains("^q Quit"), "footer shortcuts:\n{text}");
        assert!(
            text.contains("Search"),
            "library search chrome missing:\n{text}"
        );
    }

    #[test]
    fn focused_pane_uses_focus_background() {
        use crate::theme::FOCUS_BG;
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        state.focus = Pane::Map;
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).expect("term");
        terminal
            .draw(|frame| draw_workbench(frame, &state))
            .expect("draw");
        let buf = terminal.backend().buffer();
        let mut saw_focus_bg = false;
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                if buf[(x, y)].style().bg == Some(FOCUS_BG) {
                    saw_focus_bg = true;
                }
            }
        }
        assert!(saw_focus_bg, "focused pane should paint FOCUS_BG");
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
    fn parity_doc_exists_and_documents_known_gaps() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text = std::fs::read_to_string(root.join("docs/parity.md"))
            .expect("docs/parity.md is the stage-16 checklist");
        assert!(
            text.contains("## Intentional differences"),
            "parity.md must list intentional differences"
        );
        assert!(
            text.contains("splice") && text.contains("cassette"),
            "parity.md must document the splice/cassette gap"
        );
        assert!(
            text.contains("[INV-01]") && text.contains("[INV-10]"),
            "parity.md must audit the core ten invariants"
        );
    }

    #[test]
    fn welcome_frame_paints_title() {
        let text = draw_text(60, 16, &AppState::new());
        assert!(
            text.contains(WELCOME_TITLE),
            "workbench missing title, got:\n{text}"
        );
        assert!(
            text.contains("^q Quit") || text.contains("Palette"),
            "footer must show shortcut strip, got:\n{text}"
        );
        assert!(
            !text.contains("stage 16")
                && !text.contains("stage 17")
                && !text.contains("stage 18")
                && !text.contains("stage 19"),
            "footer must not lead with stage chrome, got:\n{text}"
        );
        assert!(
            !text.to_ascii_lowercase().contains("python package"),
            "welcome must not present this as a Python wrapper:\n{text}"
        );
    }

    #[test]
    fn any_key_dismisses_splash() {
        let mut state = AppState::new();
        state.show_splash = true;
        assert!(apply_key(&mut state, key(KeyCode::Char('x'))));
        assert!(!state.show_splash, "splash should dismiss on any key");
        state.show_splash = true;
        assert!(
            !apply_key(&mut state, ctrl('q')),
            "Ctrl+Q still quits from the splash"
        );
    }

    #[test]
    fn menu_bar_opens_blast_and_primers_without_palette() {
        let mut state = AppState::new();
        assert!(apply_key(&mut state, key(KeyCode::F(10))));
        assert!(state.menu_focus);
        assert_eq!(state.menu_selected, 0);
        assert!(apply_key(&mut state, key(KeyCode::Right)));
        assert!(apply_key(&mut state, key(KeyCode::Right)));
        assert_eq!(MENUS[state.menu_selected], "BLAST");
        assert!(apply_key(&mut state, key(KeyCode::Enter)));
        assert_eq!(state.overlay, Overlay::Search);
        assert!(!state.menu_focus);

        let mut primers = AppState::new();
        assert!(apply_key(&mut primers, key(KeyCode::F(10))));
        for _ in 0..5 {
            assert!(apply_key(&mut primers, key(KeyCode::Right)));
        }
        assert_eq!(MENUS[primers.menu_selected], "Primers");
        assert!(apply_key(&mut primers, key(KeyCode::Enter)));
        assert_eq!(primers.overlay, Overlay::PrimerDesign);
        let text = draw_text(80, 24, &primers);
        assert!(text.contains("Primer"), "{text}");
    }

    #[test]
    fn file_dropdown_reaches_open_fetch_quit() {
        let mut state = AppState::new();
        assert!(apply_key(&mut state, key(KeyCode::F(10))));
        assert!(apply_key(&mut state, key(KeyCode::Enter)));
        assert_eq!(state.overlay, Overlay::FileMenu);
        let labels: Vec<_> = FILE_ITEMS.iter().map(|(l, _)| *l).collect();
        assert!(labels.iter().any(|l| l.contains("Open")));
        assert!(labels.iter().any(|l| l.contains("Fetch")));
        assert!(labels.iter().any(|l| l.contains("basic")));
        assert!(labels.iter().any(|l| l.contains("advanced")));
        assert!(labels.contains(&"Quit"));
        let text = draw_text(80, 24, &state);
        assert!(text.contains("Open file"), "{text}");
        assert!(text.contains("Fetch"), "{text}");
        assert!(text.contains("Quit"), "{text}");
        assert!(apply_key(&mut state, key(KeyCode::Down)));
        assert!(apply_key(&mut state, key(KeyCode::Enter)));
        assert_eq!(state.overlay, Overlay::Path);
        assert_eq!(state.path_kind, PathKind::FetchNcbi);
    }

    #[test]
    fn daily_driver_keys_open_live_overlays() {
        let mut state = AppState::new();
        assert!(apply_key(&mut state, ctrl('b')));
        assert_eq!(state.overlay, Overlay::Search);
        state.reduce(Action::CloseOverlay);
        assert!(apply_key(&mut state, ctrl('p')));
        assert_eq!(state.overlay, Overlay::PrimerDesign);
        state.reduce(Action::CloseOverlay);
        assert!(apply_key(&mut state, key(KeyCode::F(6))));
        assert_eq!(state.overlay, Overlay::History);
        state.reduce(Action::CloseOverlay);
        assert!(apply_key(&mut state, alt('h')));
        assert_eq!(state.overlay, Overlay::History);
        assert!(KEY_TABLE.iter().any(|e| e.keys.contains("F10")));
        assert!(KEY_TABLE.iter().any(|e| e.keys.contains("Ctrl+B")));
    }

    #[test]
    fn fetch_offline_shows_clear_toast() {
        let mut state = AppState::new();
        assert!(apply_key(&mut state, key(KeyCode::Char('f'))));
        assert_eq!(state.overlay, Overlay::Path);
        assert_eq!(state.path_kind, PathKind::FetchNcbi);
        state.path_query = "L09137".into();
        assert!(state.reduce(Action::PathSubmit));
        let toast = state.toast.clone().unwrap_or_default();
        assert!(
            toast.to_ascii_lowercase().contains("off")
                || toast.to_ascii_lowercase().contains("disabled"),
            "{toast}"
        );
        assert!(!toast.contains("tracked gap"));
        state.allow_online_lookups = true;
        state.reduce(Action::OpenFetch);
        state.path_query = "L09137".into();
        assert!(state.reduce(Action::PathSubmit));
        let toast = state.toast.unwrap_or_default();
        assert!(
            toast.to_ascii_lowercase().contains("disabled")
                || toast.to_ascii_lowercase().contains("failed"),
            "live fetch path, not a stub: {toast}"
        );
    }

    #[test]
    fn copy_selection_is_in_app_clipboard() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        let seq = state.record.as_ref().unwrap().sequence.clone();
        assert!(apply_key(&mut state, ctrl('a')));
        assert_eq!(state.sel, Some((0, seq.len())));
        assert!(apply_key(&mut state, ctrl('c')));
        assert_eq!(state.clipboard, seq);
        assert!(
            state
                .toast
                .as_deref()
                .unwrap_or("")
                .contains("in-app clipboard")
        );
        assert!(apply_key(&mut state, alt('c')));
        assert_eq!(state.clipboard, splicecraft_bio::rc(&seq));
    }

    #[test]
    fn ctrl_s_saves_through_persist_chokepoint() {
        let tmp = tempfile::tempdir().expect("tempdir");
        persist::authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = persist::DataLayout::from_xdg_home(tmp.path()).expect("layout");
        assert!(
            layout.root.starts_with(tmp.path()),
            "save must stay in the sandbox: {}",
            layout.root.display()
        );
        let mut state = AppState::new();
        state.attach_layout(layout.clone());
        state.reduce(Action::LoadDemo);
        assert!(state.reduce(Action::InsertBase('A')));
        assert!(state.dirty);
        assert!(apply_key(&mut state, ctrl('s')));
        persist::revoke_thread_writes();
        let again = persist::LibraryStore::load(&layout);
        assert!(
            again.plasmids.iter().any(|e| e.name == "pDemo"),
            "{:?}",
            again.plasmids
        );
    }

    #[test]
    fn find_and_new_plasmid_and_add_feature() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        assert!(apply_key(&mut state, ctrl('f')));
        assert_eq!(state.path_kind, PathKind::FindDna);
        state.path_query = "ATGAAA".into();
        assert!(state.reduce(Action::PathSubmit));
        assert_eq!(state.cursor, 0);
        assert!(
            state
                .toast
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains("found")
        );
        assert!(apply_key(&mut state, ctrl('n')));
        state.path_query = "ATGCATGC".into();
        assert!(state.reduce(Action::PathSubmit));
        assert_eq!(
            state.record.as_ref().map(|r| r.sequence.as_str()),
            Some("ATGCATGC")
        );
        assert!(apply_key(&mut state, ctrl('a')));
        assert!(apply_key(&mut state, alt_shift('f')));
        state.path_query = "cassette misc_feature".into();
        assert!(state.reduce(Action::PathSubmit));
        let rec = state.record.as_ref().unwrap();
        assert!(
            rec.features.iter().any(|f| f.label == "cassette"),
            "{:?}",
            rec.features
        );
    }

    #[test]
    fn workbench_about_is_splicecraft_rs() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        let text = draw_text(80, 18, &state);
        assert!(
            text.contains(WELCOME_TITLE),
            "menu bar must say SpliceCraft.rs:\n{text}"
        );
        assert!(
            text.contains("^q Quit") || text.contains("pDemo"),
            "footer / source chrome must be live, got:\n{text}"
        );
        let lower = text.to_ascii_lowercase();
        assert!(
            !lower.contains("python package") && !lower.contains("pyo3"),
            "must not present as a Python wrapper:\n{text}"
        );
        if std::env::var_os("SPLICECRAFT_WRITE_SCREENSHOT").is_some() {
            let dest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/screenshot.txt");
            std::fs::write(&dest, &text).expect("write docs/screenshot.txt");
        }
    }

    #[test]
    fn empty_canvas_when_no_record() {
        let text = draw_text(80, 24, &AppState::new());
        assert!(
            text.contains("Empty canvas") || text.contains("no plasmid"),
            "expected empty-canvas copy, got:\n{text}"
        );
        assert!(
            text.contains("^q Quit") || text.contains("Help"),
            "footer shortcuts missing:\n{text}"
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
        let mut coll = AppState::new();
        coll.overlay = Overlay::Collision;
        let _ = draw_text(40, 12, &coll);
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
        assert!(
            titles.iter().any(|t| t.contains("basic")),
            "expected basic demo, got {titles:?}"
        );
        assert!(
            titles.iter().any(|t| t.contains("advanced")),
            "expected advanced demo, got {titles:?}"
        );
        state.palette_selected = titles
            .iter()
            .position(|t| t.contains("basic"))
            .expect("basic demo");
        assert!(state.reduce(Action::PaletteExecute));
        assert!(state.record.is_some());
        assert_eq!(state.source_label, "pDemo (memory)");
        assert_eq!(state.record.as_ref().map(|r| r.len()), Some(120));
    }

    #[test]
    fn advanced_demo_is_richer_on_the_workbench() {
        let mut basic = AppState::new();
        basic.reduce(Action::LoadDemo);
        let mut adv = AppState::new();
        adv.reduce(Action::LoadDemoAdvanced);
        let b = basic.record.as_ref().expect("basic");
        let a = adv.record.as_ref().expect("advanced");
        assert_eq!(a.name, "pDemoAdv");
        assert_eq!(a.len(), ADVANCED_DEMO_LEN);
        assert!(a.len() > b.len() * 10);
        assert!(a.features.len() > b.features.len() + 5);
        assert!(adv.show_restr, "advanced demo turns on the RE overlay");
        assert!(!basic.show_restr);
        let text = draw_text(100, 28, &adv);
        assert!(
            text.contains("bla") && text.contains("lacZ"),
            "sidebar should list the extra genes:\n{text}"
        );
        assert!(
            text.contains("2400") || text.contains(&ADVANCED_DEMO_LEN.to_string()),
            "map/status should show length:\n{text}"
        );
        assert!(
            !adv.toast.as_deref().unwrap_or("").contains("ATG")
                && !adv.toast.as_deref().unwrap_or("").contains("GCA"),
            "toast must not log sequence: {:?}",
            adv.toast
        );
    }

    #[test]
    fn ctrl_o_opens_path_prompt() {
        let mut state = AppState::new();
        assert!(apply_key(&mut state, ctrl('o')));
        assert_eq!(state.overlay, Overlay::Path);
        assert_eq!(state.path_kind, PathKind::OpenFile);
    }

    #[test]
    fn circular_map_contains_braille_and_name_bp() {
        let rec = demo_record();
        let lines = render_map(
            &rec,
            &MapOptions {
                width: 48,
                height: 16,
                circular: true,
                origin: 0,
                show_restr: false,
                show_labels: true,
                ascii: false,
                ..MapOptions::default()
            },
        );
        let blob = lines.join("\n");
        assert!(
            lines_contain_braille(&lines),
            "expected braille dots, got:\n{blob}"
        );
        assert!(blob.contains("pDemo"), "missing name:\n{blob}");
        assert!(blob.contains("120 bp"), "missing bp:\n{blob}");
        assert!(blob.contains('▲'), "missing origin marker:\n{blob}");
        assert!(
            blob.contains("[ v = linear ]"),
            "missing linear toggle hint:\n{blob}"
        );
    }

    #[test]
    fn circular_ring_is_dense_annulus() {
        let rec = demo_record();
        let lines = render_map(
            &rec,
            &MapOptions {
                width: 48,
                height: 16,
                circular: true,
                show_labels: true,
                ..MapOptions::default()
            },
        );
        let blob = lines.join("\n");
        let braille = blob
            .chars()
            .filter(|c| ('\u{2800}'..='\u{28FF}').contains(c))
            .count();
        assert!(
            braille > 80,
            "expected a packed braille ring (>80 cells), got {braille}:\n{blob}"
        );
        assert!(
            blob.contains('┼'),
            "inner scale ticks should be ┼, got:\n{blob}"
        );
        let wide = render_map(
            &rec,
            &MapOptions {
                width: 80,
                height: 24,
                circular: true,
                show_labels: true,
                ..MapOptions::default()
            },
        );
        let wide_blob = wide.join("\n");
        let wide_braille = wide_blob
            .chars()
            .filter(|c| ('\u{2800}'..='\u{28FF}').contains(c))
            .count();
        assert!(
            wide_braille > 150,
            "wide map should pack more of the ring, got {wide_braille}:\n{wide_blob}"
        );
    }

    #[test]
    fn circular_ring_is_feature_colored_not_plain_white() {
        let rec = demo_record();
        let lines = render_map_styled(
            &rec,
            &MapOptions {
                width: 48,
                height: 16,
                circular: true,
                show_labels: true,
                ..MapOptions::default()
            },
        );
        let cds = default_type_color("CDS").expect("CDS");
        let misc = default_type_color("misc_feature").expect("misc");
        let colored = lines.iter().any(|l| {
            l.spans.iter().any(|s| {
                let ch = s.content.chars().next().unwrap_or(' ');
                let braille = ('\u{2800}'..='\u{28FF}').contains(&ch);
                braille && (s.style.fg == Some(cds) || s.style.fg == Some(misc))
            })
        });
        assert!(
            colored,
            "backbone braille must take feature colors, got {lines:?}"
        );
    }

    #[test]
    fn wrap_feature_label_uses_wrap_midpoint() {
        let rec = demo_record();
        let wrap = rec
            .features
            .iter()
            .find(|f| f.label == "wrap_ori")
            .expect("wrap");
        let n = rec.len();
        let mid = feature_label_bp(wrap, n);
        assert_eq!(mid, core::wrap_midpoint(wrap.start, wrap.end, n));
        let naive = (wrap.start as i64 + wrap.end as i64) / 2;
        assert_ne!(
            mid as i64, naive,
            "wrap midpoint must not be the naive average"
        );
        let lines = render_map(
            &rec,
            &MapOptions {
                width: 40,
                height: 14,
                circular: true,
                ..MapOptions::default()
            },
        );
        let blob = lines.join("\n");
        assert!(blob.contains("wrap") || blob.contains("pDemo"), "{blob}");
        let half = lines.len() / 2;
        let top = lines[..half].join("");
        assert!(
            top.contains("wrap") || top.contains("pDemo"),
            "label should sit near 12 o'clock (wrap midpoint 0), got:\n{blob}"
        );
    }

    #[test]
    fn inv10_edit_undo_is_deep_clone() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        let original = state.record.clone().unwrap();
        state.focus = Pane::Sequence;
        assert!(apply_key(&mut state, key(KeyCode::Char('a'))));
        let edited = state.record.clone().unwrap();
        assert_ne!(edited.sequence, original.sequence);
        let stacked = state.undo.peek_undo().expect("undo snapshot").clone();
        assert_eq!(stacked.sequence, original.sequence);
        let mut stacked = stacked;
        stacked.sequence.clear();
        stacked.features.clear();
        assert_eq!(
            state.undo.peek_undo().unwrap().sequence,
            original.sequence,
            "mutating a clone of the snapshot must not change the stack"
        );
        assert!(state.reduce(Action::Undo));
        let restored = state.record.as_ref().unwrap();
        assert_eq!(restored.sequence, original.sequence);
        assert_eq!(restored.features, original.features);
        if let Some(rec) = state.record.as_mut() {
            rec.sequence.clear();
        }
        assert_eq!(
            state.undo.peek_redo().unwrap().sequence,
            edited.sequence,
            "mutating the live record must not change the redo entry"
        );
    }

    #[test]
    fn reorigin_refused_on_linear() {
        let mut state = AppState::new();
        let mut rec = core::Record::new("lin", "ATGCATGCATGC", false);
        rec.features
            .push(core::Feature::new("misc_feature", 2, 6, 1, "x"));
        state.record = Some(rec);
        state.cursor = 4;
        assert!(state.reduce(Action::SetOriginHere));
        let rec = state.record.as_ref().unwrap();
        assert!(!rec.circular);
        assert_eq!(rec.sequence, "ATGCATGCATGC");
        assert_eq!(rec.features[0].start, 2);
        let toast = state.toast.unwrap_or_default();
        assert!(
            toast.to_ascii_lowercase().contains("circular")
                || toast.to_ascii_lowercase().contains("linear"),
            "{toast}"
        );
    }

    #[test]
    fn autosave_is_gated_without_authorisation() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        state.dirty = true;
        let wrote = try_autosave(&state).expect("gated");
        assert!(!wrote);
    }

    #[test]
    fn sequence_bottom_strand_is_column_complement() {
        let rec = demo_record();
        let lines = render_sequence(
            &rec,
            &SeqView {
                width: 12,
                window_start: 0,
                cursor: 0,
            },
        );
        let top = lines.iter().find(|l| l.starts_with("ATG")).expect("top");
        let bot = lines
            .iter()
            .find(|l| l.chars().all(|c| "ACGT ".contains(c)) && l.starts_with('T'))
            .expect("bottom");
        let top_bases: String = top.chars().filter(|c| !c.is_whitespace()).collect();
        let bot_bases: String = bot
            .chars()
            .filter(|c| !c.is_whitespace())
            .take(top_bases.len())
            .collect();
        let expected: String = top_bases
            .chars()
            .map(|c| match c {
                'A' => 'T',
                'T' => 'A',
                'G' => 'C',
                'C' => 'G',
                other => other,
            })
            .collect();
        assert_eq!(bot_bases, expected, "top={top} bot={bot}");
        assert_ne!(
            bot_bases,
            bio::rc(&top_bases),
            "must not reverse the window"
        );
    }

    #[test]
    fn keep_without_layout_stays_in_memory() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        assert!(apply_key(&mut state, alt('k')));
        assert!(
            state.library.plasmids.iter().any(|e| e.name == "pDemo"),
            "{:?}",
            state.library.plasmids
        );
        let toast = state.toast.unwrap_or_default();
        assert!(
            toast.contains("memory") || toast.contains("Kept"),
            "{toast}"
        );
    }

    #[test]
    fn keep_reload_from_disk_sees_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        persist::authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = persist::DataLayout::from_xdg_home(tmp.path()).expect("layout");
        let mut state = AppState::new();
        state.attach_layout(layout.clone());
        state.reduce(Action::LoadDemo);
        assert!(state.reduce(Action::KeepRecord));
        let again = persist::LibraryStore::load(&layout);
        persist::revoke_thread_writes();
        assert!(
            again.plasmids.iter().any(|e| e.name == "pDemo"),
            "{:?}",
            again.plasmids
        );
    }

    #[test]
    fn collision_copy_keeps_original_via_modal() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        state.reduce(Action::KeepRecord);
        state.reduce(Action::KeepRecord);
        assert_eq!(state.overlay, Overlay::Collision);
        let text = draw_text(80, 24, &state);
        assert!(text.to_ascii_lowercase().contains("collision"), "{text}");
        assert!(state.reduce(Action::CollisionPick(CollisionChoice::Copy)));
        let names: Vec<_> = state
            .library
            .plasmids
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(names.contains(&"pDemo"), "{names:?}");
        assert!(names.iter().any(|n| n.contains("COPY")), "{names:?}");
    }

    #[test]
    fn unique_and_six_plus_keys_toggle_scan_filters() {
        let mut state = AppState::new();
        assert!(!state.restr_unique);
        assert!(state.restr_min_six);
        assert!(apply_key(&mut state, key(KeyCode::Char('u'))));
        assert!(state.restr_unique);
        assert!(apply_key(&mut state, key(KeyCode::Char('6'))));
        assert!(!state.restr_min_six);
    }

    #[test]
    fn primer_design_overlay_and_generic_pair() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        assert!(state.reduce(Action::OpenPrimerDesign));
        assert_eq!(state.overlay, Overlay::PrimerDesign);
        assert!(state.reduce(Action::ToolEnter));
        assert!(state.design_fwd.is_some(), "expected a designed oligo");
        let text = draw_text(80, 24, &state);
        assert!(
            text.to_ascii_lowercase().contains("primer"),
            "design overlay missing: {text}"
        );
    }

    #[test]
    fn palette_primer_design_is_live() {
        let mut state = AppState::new();
        state.reduce(Action::OpenPalette);
        let titles: Vec<_> = state.visible_commands().iter().map(|c| c.title).collect();
        assert!(titles.contains(&"Primer design"), "{titles:?}");
        assert!(titles.contains(&"Primer check"), "{titles:?}");
        assert!(titles.contains(&"Enzyme collections"), "{titles:?}");
        assert!(titles.contains(&"Constructor"), "{titles:?}");
        assert!(titles.contains(&"Parts Bin"), "{titles:?}");
        assert!(
            titles.contains(&"Mutato — mutagenesis + Scrub"),
            "{titles:?}"
        );
        assert!(titles.contains(&"Synthesis"), "{titles:?}");
        assert!(titles.contains(&"Simulator"), "{titles:?}");
        assert!(titles.contains(&"Sequencing"), "{titles:?}");
        assert!(titles.contains(&"Experiments"), "{titles:?}");
        assert!(titles.contains(&"History"), "{titles:?}");
        assert!(titles.contains(&"Recover history from .dna"), "{titles:?}");
        let cmd = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "Primer design")
            .expect("primer design");
        assert_eq!(cmd.action, Action::OpenPrimerDesign);
        let ctor = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "Constructor")
            .expect("constructor");
        assert_eq!(ctor.action, Action::OpenConstructor);
    }

    #[test]
    fn constructor_overlay_opens_and_cycles_tabs() {
        let mut state = AppState::new();
        assert!(state.reduce(Action::OpenConstructor));
        assert_eq!(state.overlay, Overlay::Constructor);
        let text = draw_text(80, 24, &state);
        assert!(
            text.to_ascii_lowercase().contains("constructor"),
            "constructor overlay missing: {text}"
        );
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.ctor_tab, ConstructorTab::Gibson);
    }

    #[test]
    fn mutato_and_synthesis_overlays_are_live() {
        let mut state = AppState::new();
        let mutato = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title.starts_with("Mutato"))
            .expect("mutato");
        assert_eq!(mutato.action, Action::OpenMutato);
        let synth = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "Synthesis")
            .expect("synthesis");
        assert_eq!(synth.action, Action::OpenSynthesis);
        assert!(state.reduce(Action::OpenMutato));
        assert_eq!(state.overlay, Overlay::Mutato);
        let text = draw_text(80, 24, &state);
        assert!(text.to_ascii_lowercase().contains("mutato"), "{text}");
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.mutato_tab, MutatoTab::ScrubQc);
        assert!(state.reduce(Action::OpenSynthesis));
        assert_eq!(state.overlay, Overlay::Synthesis);
        let text = draw_text(80, 24, &state);
        assert!(text.to_ascii_lowercase().contains("synthesis"), "{text}");
    }

    #[test]
    fn mutato_soe_on_cds_long() {
        let mut state = AppState::new();
        let cds = concat!(
            "ATG",
            "GCTGAAGTTCAGGATAACCTGGCGAAAGTTCAGGAAGCGGTTGATACCCTGAAACGTGGT",
            "CTGGAAGCGGCGAAAGCGACCCTGGAAAAAGCGGGTGAAGATATCGCGAAAGCGGTTGAT",
            "GGTAAACGTAAAGGCGATCTGGAAAAACTGGCGGAAGCGCTGCAGAAAGTTGAAGCGGAT",
            "ATCGCGAAAGCGGTTGATGGTAAACGTAAAGGCGATCTGGAAAAACTGGCGGAAGCGCTG",
            "TAA",
        );
        let mut rec = core::Record::new("cds", cds, false);
        rec.features
            .push(core::Feature::new("CDS", 0, cds.len(), 1, "orf"));
        state.record = Some(rec);
        state.mutato_query = "V40F".into();
        assert!(state.reduce(Action::OpenMutato));
        assert!(state.reduce(Action::ToolEnter));
        let summary = state.mutato_summary.expect("summary");
        assert!(summary.contains("V40F"), "{summary}");
        assert!(
            summary.contains("SOE") || summary.contains("2-primer"),
            "{summary}"
        );
        assert!(state.design_fwd.is_some());
    }

    #[test]
    fn simulator_overlay_pcr_and_gel_are_live() {
        let mut state = AppState::new();
        let sim = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "Simulator")
            .expect("simulator");
        assert_eq!(sim.action, Action::OpenSimulator);
        assert!(state.reduce(Action::OpenSimulator));
        assert_eq!(state.overlay, Overlay::Simulator);
        let text = draw_text(80, 24, &state);
        assert!(text.to_ascii_lowercase().contains("simulator"), "{text}");
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.sim_tab, SimulatorTab::Gel);

        let seq = format!(
            "{}{}{}",
            "ATGCGATCGATCGATCGCGT",
            "A".repeat(60),
            "GCATCGTAGCTAGCTGATCG"
        );
        state.record = Some(core::Record::new("lin", seq, false));
        state.sim_tab = SimulatorTab::Pcr;
        state.sim_query = format!(
            "{}/{}",
            "ATGCGATCGATCGATCGCGT",
            splicecraft_bio::rc("GCATCGTAGCTAGCTGATCG")
        );
        assert!(state.reduce(Action::ToolEnter));
        assert_eq!(state.sim_amplicons.len(), 1);
        assert_eq!(state.sim_amplicons[0].length, 100);
        assert!(!state.sim_amplicons[0].wraps);
    }

    #[test]
    fn library_delete_is_session_undoable() {
        let mut state = AppState::new();
        state.reduce(Action::LoadDemo);
        state.reduce(Action::KeepRecord);
        assert_eq!(state.library.plasmids.len(), 1);
        assert!(state.reduce(Action::LibraryDelete));
        assert!(state.library.plasmids.is_empty());
        assert_eq!(state.deleted_stack.len(), 1);
        assert!(state.reduce(Action::LibraryUndelete));
        assert_eq!(state.library.plasmids.len(), 1);
        assert_eq!(state.library.plasmids[0].name, "pDemo");
    }

    #[test]
    fn sequencing_overlay_is_live() {
        let mut state = AppState::new();
        let seq = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "Sequencing")
            .expect("sequencing");
        assert_eq!(seq.action, Action::OpenSequencing);
        assert!(state.reduce(Action::OpenSequencing));
        assert_eq!(state.overlay, Overlay::Sequencing);
        assert_eq!(state.seq_tab, SequencingTab::Zip);
        let text = draw_text(80, 24, &state);
        assert!(text.to_ascii_lowercase().contains("sequencing"), "{text}");
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.seq_tab, SequencingTab::Align);
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.seq_tab, SequencingTab::Sanger);
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.seq_tab, SequencingTab::Report);
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.seq_tab, SequencingTab::Zip);

        state.record = Some(core::Record::new("tiny", "ATGG", false));
        state.seq_tab = SequencingTab::Align;
        state.seq_query = "ATGC".into();
        assert!(state.reduce(Action::ToolEnter));
        assert!(!state.seq_segments.is_empty());
        assert!(
            state
                .seq_variants
                .iter()
                .any(|v| v.kind == "snp" && v.target_pos == 3),
            "{:?}",
            state.seq_variants
        );
        let summary = state.seq_summary.clone().unwrap_or_default();
        assert!(!summary.contains("100%"), "{summary}");
        assert!(summary.contains('%'), "{summary}");
        let text = draw_text(80, 24, &state);
        assert!(!text.contains("100%"), "{text}");
        assert!(apply_key(&mut state, key(KeyCode::Char('j'))));
        assert_eq!(state.focus, Pane::Sequence);
        assert_eq!(state.cursor, 3);

        let lines = render_map(
            state.record.as_ref().unwrap(),
            &MapOptions {
                width: 24,
                height: 8,
                circular: false,
                align_segments: state.seq_segments.clone(),
                ..MapOptions::default()
            },
        );
        let blob = lines.join("\n");
        assert!(
            blob.contains('X'),
            "linear overlay missing mismatch:\n{blob}"
        );
    }

    #[test]
    fn experiments_overlay_is_live() {
        let mut state = AppState::new();
        let cmd = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "Experiments")
            .expect("experiments");
        assert_eq!(cmd.action, Action::OpenExperiments);
        assert!(state.reduce(Action::OpenExperiments));
        assert_eq!(state.overlay, Overlay::Experiments);
        assert_eq!(state.exp_tab, ExperimentsTab::List);
        let text = draw_text(80, 24, &state);
        assert!(text.to_ascii_lowercase().contains("experiment"), "{text}");
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.exp_tab, ExperimentsTab::Compose);
        state.exp_body = "Today: @pUC19 then !digest and &runA".into();
        assert!(state.reduce(Action::ToolEnter));
        assert_eq!(state.experiments.entries.len(), 1);
        assert_eq!(
            state.experiments.entries[0].attached_plasmid_ids,
            vec!["pUC19"]
        );
        state.library.plasmids.push(persist::LibraryEntry {
            id: "pUC19".into(),
            name: "pUC19".into(),
            size: 4,
            gb_text: String::new(),
            source: String::new(),
            alignments: Vec::new(),
            history_xml: String::new(),
        });
        assert!(state.reduce(Action::ExperimentJump));
        assert!(
            state.toast.as_deref().unwrap_or("").contains("pUC19"),
            "{:?}",
            state.toast
        );
    }

    #[test]
    fn history_overlay_warns_without_mutating_sequence() {
        let mut state = AppState::new();
        let cmd = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "History")
            .expect("history");
        assert_eq!(cmd.action, Action::OpenHistory);
        let seq = "ATGCATGCATGC";
        let mut rec = core::Record::new("pLie", seq, false);
        rec.id = "pLie".into();
        state.record = Some(rec);
        state.library.plasmids.push(persist::LibraryEntry {
            id: "pLie".into(),
            name: "pLie".into(),
            size: seq.len(),
            gb_text: String::new(),
            source: String::new(),
            alignments: Vec::new(),
            history_xml: "<HistoryTree><Node name=\"pLie.dna\" seqLen=\"12\" circular=\"0\" operation=\"insertFragment\"><RegeneratedSite name=\"EcoRI\" pos=\"12\"/></Node></HistoryTree>".into(),
        });
        assert!(state.reduce(Action::OpenHistory));
        assert_eq!(state.overlay, Overlay::History);
        assert!(
            state
                .hist_warnings
                .iter()
                .any(|w| w.contains("EcoRI") && w.contains("does not occur")),
            "{:?}",
            state.hist_warnings
        );
        assert_eq!(
            state.record.as_ref().map(|r| r.sequence.as_str()),
            Some(seq)
        );
        let text = draw_text(80, 24, &state);
        assert!(text.to_ascii_lowercase().contains("history"), "{text}");
        assert!(text.contains("EcoRI"), "{text}");
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.hist_tab, HistoryTab::Tree);
        let recover = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "Recover history from .dna")
            .expect("recover");
        assert_eq!(recover.action, Action::RecoverHistory);
    }

    #[test]
    fn search_overlay_orfs_wrap_and_online_stays_off() {
        let mut state = AppState::new();
        let cmd = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "BLAST")
            .expect("BLAST");
        assert_eq!(cmd.action, Action::OpenSearch);
        assert!(state.reduce(Action::OpenSearch));
        assert_eq!(state.overlay, Overlay::Search);
        assert_eq!(state.search_tab, SearchTab::Local);
        assert!(state.reduce(Action::ToolTab));
        assert_eq!(state.search_tab, SearchTab::Orf);

        let seq = format!("{}{}{}{}", "AAA".repeat(29), "TAA", "CCC".repeat(5), "ATG");
        let rec = core::Record::new("pWrapOrf", &seq, true);
        state.record = Some(rec);
        assert!(state.reduce(Action::ToolEnter));
        let summary = state.search_summary.clone().unwrap_or_default();
        assert!(summary.contains("wrap") || state.search_lines.iter().any(|l| l.contains("wrap")));
        assert!(
            state
                .search_lines
                .iter()
                .any(|l| l.contains("30 aa") && l.contains("wrap")),
            "{:?} {summary}",
            state.search_lines
        );
        let text = draw_text(80, 24, &state);
        assert!(
            text.to_ascii_lowercase().contains("search") || text.contains("BLAST"),
            "{text}"
        );
        assert!(text.contains("30 aa") || text.contains("wrap"), "{text}");

        state.search_tab = SearchTab::Online;
        state.allow_online_search = false;
        assert!(state.reduce(Action::ToolEnter));
        let online = state.search_summary.clone().unwrap_or_default();
        assert!(
            online.to_ascii_lowercase().contains("off")
                || online.to_ascii_lowercase().contains("disabled"),
            "{online}"
        );

        state.search_tab = SearchTab::Find;
        state.search_query = "pUC".into();
        state.library.plasmids.push(persist::LibraryEntry {
            id: "pUC19".into(),
            name: "pUC19".into(),
            size: 2686,
            gb_text: String::new(),
            source: String::new(),
            alignments: Vec::new(),
            history_xml: String::new(),
        });
        assert!(state.reduce(Action::ToolEnter));
        assert!(
            state.search_lines.iter().any(|l| l.contains("pUC19")),
            "{:?}",
            state.search_lines
        );
    }

    #[test]
    fn search_overlay_tab_bar_is_navigable() {
        let mut state = AppState::new();
        assert!(state.reduce(Action::OpenSearch));
        let text = draw_text(90, 26, &state);
        for chip in [
            "Local BLAST",
            "Find ORFs",
            "Online",
            "HMM DBs",
            "Find plasmid",
        ] {
            assert!(text.contains(chip), "missing {chip} in:\n{text}");
        }
        assert!(
            text.contains("← →") || text.contains("switch tool"),
            "{text}"
        );
        assert_eq!(state.search_tab, SearchTab::Local);
        assert!(apply_key(
            &mut state,
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)
        ));
        assert_eq!(state.search_tab, SearchTab::Orf);
        assert!(apply_key(
            &mut state,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)
        ));
        assert_eq!(state.search_tab, SearchTab::Local);
        assert!(state.reduce(Action::ToolTabPrev));
        assert_eq!(state.search_tab, SearchTab::Find);
        let find = draw_text(90, 26, &state);
        assert!(find.contains("Find plasmid"), "{find}");
        assert!(find.contains("Fuzzy plasmid"), "{find}");
    }

    #[test]
    fn tool_overlays_share_blast_tab_chrome() {
        fn assert_chips(text: &str, chips: &[&str]) {
            for chip in chips {
                assert!(text.contains(chip), "missing {chip} in:\n{text}");
            }
            assert!(
                text.contains("← →") && text.contains("switch"),
                "tab footer missing in:\n{text}"
            );
        }

        let mut ctor = AppState::new();
        assert!(ctor.reduce(Action::OpenConstructor));
        assert_chips(
            &draw_text(90, 26, &ctor),
            &["Traditional", "Gibson", "Domesticator", "Parts", "Syn-frag"],
        );
        assert_eq!(ctor.ctor_tab, ConstructorTab::Traditional);
        assert!(apply_key(
            &mut ctor,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)
        ));
        assert_eq!(ctor.ctor_tab, ConstructorTab::SynFrag);

        let mut primers = AppState::new();
        assert!(primers.reduce(Action::OpenPrimerDesign));
        assert_chips(
            &draw_text(90, 26, &primers),
            &["Generic", "Cloning", "Detection", "Golden Braid"],
        );
        assert_eq!(primers.design_kind, DesignKind::Generic);
        assert!(primers.reduce(Action::ToolTabPrev));
        assert_eq!(primers.design_kind, DesignKind::GoldenBraid);

        let mut mutato = AppState::new();
        assert!(mutato.reduce(Action::OpenMutato));
        assert_chips(
            &draw_text(90, 26, &mutato),
            &["SDM", "QuikChange", "Golden Braid"],
        );

        let mut synth = AppState::new();
        assert!(synth.reduce(Action::OpenSynthesis));
        assert_chips(&draw_text(90, 26, &synth), &["DNA", "Protein", "Operon"]);

        let mut sim = AppState::new();
        assert!(sim.reduce(Action::OpenSimulator));
        assert_chips(&draw_text(90, 26, &sim), &["PCR", "Gel"]);

        let mut seq = AppState::new();
        assert!(seq.reduce(Action::OpenSequencing));
        assert_chips(
            &draw_text(90, 26, &seq),
            &["Zip", "Align", "Sanger", "Report"],
        );

        let mut exp = AppState::new();
        assert!(exp.reduce(Action::OpenExperiments));
        assert_chips(&draw_text(90, 26, &exp), &["List", "Compose", "Attach"]);

        let mut hist = AppState::new();
        assert!(hist.reduce(Action::OpenHistory));
        assert_chips(&draw_text(90, 26, &hist), &["Protocol", "Tree", "Detail"]);

        let mut settings = AppState::new();
        assert!(settings.reduce(Action::OpenSettings));
        let settings_text = draw_text(80, 24, &settings);
        assert!(
            settings_text.contains("allow_online_search"),
            "{settings_text}"
        );
        assert!(
            settings_text.contains("allow_online_lookups"),
            "{settings_text}"
        );
        assert!(settings_text.contains("toggle"), "{settings_text}");
        assert!(
            !settings_text.contains("← →"),
            "settings is not tabbed:\n{settings_text}"
        );

        let mut autolab = AppState::new();
        assert!(autolab.reduce(Action::OpenAutolab));
        let autolab_text = draw_text(80, 24, &autolab);
        assert!(autolab_text.contains("disarmed"), "{autolab_text}");
        assert!(autolab_text.contains("Tab"), "{autolab_text}");
        assert!(
            !autolab_text.contains("← →"),
            "autolab Tab arms motion, not tabs:\n{autolab_text}"
        );
        assert!(!autolab.autolab_motion_armed);
        assert!(apply_key(&mut autolab, key(KeyCode::Tab)));
        assert!(autolab.autolab_motion_armed);
        let _ = apply_key(
            &mut autolab,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        );
        assert!(autolab.autolab_motion_armed);
        assert_eq!(autolab.overlay, Overlay::Autolab);

        for (open, needle) in [
            (Action::OpenEnzymes, "Enzymes"),
            (Action::OpenParts, "Parts"),
            (Action::OpenBabs, "BABS"),
        ] {
            let mut state = AppState::new();
            assert!(state.reduce(open));
            let text = draw_text(80, 24, &state);
            assert!(text.contains(needle), "missing {needle} in:\n{text}");
            assert!(text.contains("Esc"), "{text}");
            assert!(
                !text.contains("← →"),
                "{needle} should not grow fake tabs:\n{text}"
            );
        }
    }

    #[test]
    fn settings_overlay_toggles_online_in_memory() {
        let mut state = AppState::new();
        let cmd = state
            .visible_commands()
            .into_iter()
            .find(|c| c.title == "Settings")
            .expect("Settings");
        assert_eq!(cmd.action, Action::OpenSettings);
        assert!(state.reduce(Action::OpenSettings));
        assert_eq!(state.overlay, Overlay::Settings);
        assert!(!state.allow_online_search);
        assert!(state.reduce(Action::ToolEnter));
        assert!(state.allow_online_search);
        let text = draw_text(80, 24, &state);
        assert!(text.to_ascii_lowercase().contains("settings"), "{text}");
        assert!(text.contains("allow_online_search"), "{text}");
    }

    #[test]
    fn master_delete_is_palette_only_and_defaults_to_no() {
        let titles: Vec<_> = palette_commands().iter().map(|c| c.title).collect();
        assert!(
            titles.iter().any(|t| t.contains("Master Delete")),
            "{titles:?}"
        );
        assert!(
            !KEY_TABLE
                .iter()
                .any(|e| e.description.to_ascii_lowercase().contains("master delete")),
            "Master Delete must not have a keybinding"
        );
        let mut state = AppState::new();
        assert!(state.reduce(Action::OpenMasterDelete));
        assert_eq!(state.overlay, Overlay::MasterDelete);
        assert!(!state.master_delete_yes);
        assert!(state.reduce(Action::ToolEnter));
        assert_eq!(state.overlay, Overlay::None);
        let text = {
            let mut s = AppState::new();
            s.reduce(Action::OpenMasterDelete);
            draw_text(80, 24, &s)
        };
        assert!(
            text.to_ascii_lowercase().contains("master delete"),
            "{text}"
        );
        assert!(text.contains("[No]"), "{text}");
    }

    #[test]
    fn autolab_overlay_compiles_fixture() {
        let mut state = AppState::new();
        assert!(state.reduce(Action::OpenAutolab));
        assert_eq!(state.overlay, Overlay::Autolab);
        assert!(state.reduce(Action::ToolEnter));
        let proto = state.autolab_protocol.clone().unwrap_or_default();
        assert!(
            proto.contains("from opentrons import protocol_api"),
            "{proto}"
        );
        assert!(confirm_motion(false).is_err());
    }

    #[test]
    fn babs_overlay_and_public_url_refused() {
        let mut state = AppState::new();
        assert!(state.reduce(Action::OpenBabs));
        assert_eq!(state.overlay, Overlay::Babs);
        assert!(assert_ollama_loopback("https://example.com").is_err());
        assert!(assert_ollama_loopback("http://8.8.8.8:11434").is_err());
        let text = draw_text(80, 24, &state);
        assert!(text.contains("BABS") || text.contains("Ollama"), "{text}");
    }
}
