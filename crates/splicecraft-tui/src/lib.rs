//! Ratatui workbench: map, sequence editor, help, palette, cloning, Mutato, Simulator, Sequencing.
//!
//! Stage 11. Event → [`Action`] → [`AppState::reduce`] → draw.
//! Library writes go through `safe_save_json`. Crash-recovery autosave
//! uses the persist chokepoint only.

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
mod commands;
mod draw;
mod editor;
mod keys;
mod render;
mod state;

pub use action::{
    Action, CollisionChoice, ConstructorTab, DesignKind, FocusMode, MutatoTab, Overlay, Pane,
    PathKind, SequencingTab, SimulatorTab, SynthTab,
};
pub use commands::{Command, filter_commands, palette_commands};
pub use draw::draw_workbench;
pub use editor::{UNDO_LIMIT, UndoStack};
pub use keys::{KEY_TABLE, KeyEntry, action_from_key};
pub use render::{
    MapOptions, SeqView, feature_label_bp, lines_contain_braille, render_map, render_sequence,
};
pub use state::{AppState, demo_record};

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};

/// Stage this crate currently satisfies (Sequencing).
pub const IMPLEMENTATION_STAGE: u8 = 11;

/// Title painted on the menu bar and help overlay.
pub const WELCOME_TITLE: &str = "SpliceCraft.rs";

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Run the interactive TUI until Quit.
pub fn run() -> std::io::Result<()> {
    persist::authorize_writes("splicecraft-tui");
    let mut state = AppState::new();
    if let Ok(layout) = persist::DataLayout::resolve() {
        state.attach_layout(layout);
    }
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut state);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut DefaultTerminal, state: &mut AppState) -> std::io::Result<()> {
    let mut dirty_since: Option<std::time::Instant> = None;
    loop {
        terminal.draw(|frame| draw_workbench(frame, state))?;
        maybe_autosave(state, &mut dirty_since);
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if !apply_key(state, key) {
                    break;
                }
                if state.dirty {
                    dirty_since.get_or_insert_with(std::time::Instant::now);
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
            text.contains("stage 11"),
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
        state.palette_selected = titles
            .iter()
            .position(|t| t.contains("demo"))
            .expect("demo");
        assert!(state.reduce(Action::PaletteExecute));
        assert!(state.record.is_some());
        assert_eq!(state.source_label, "pDemo (memory)");
        assert_eq!(state.record.as_ref().map(|r| r.len()), Some(120));
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
    fn edit_undo_is_deep_clone() {
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

    fn alt(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT)
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
}
