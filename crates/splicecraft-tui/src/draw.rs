//! Pure draw: `AppState` → Ratatui. No persist, no sequence logging.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::WELCOME_TITLE;
use crate::action::{
    ConstructorTab, DesignKind, ExperimentsTab, FocusMode, HistoryTab, MutatoTab, Overlay, Pane,
    PathKind, SearchTab, SequencingTab, SimulatorTab, SynthTab,
};
use crate::keys::KEY_TABLE;
use crate::menu::{FILE_ITEMS, MENUS};
use crate::render::{MapOptions, SeqView, render_map_styled, render_sequence_styled};
use crate::state::AppState;
use crate::theme::{
    BORDER_DIM, DANGER, ENZYME_ACCENT, FOCUS_BG, FOOTER_BG, FOOTER_FG, FOOTER_SHORTCUTS, MENU_FG,
    PANEL_BG, PRIMARY, PRIMARY_DARK, SEQUENCE_ROWS, SIDE_PANE_COLS, TEXT, TEXT_ON_PRIMARY, WARN,
    darken, feature_paint_color,
};

/// Paint the workbench (and any overlay) into `frame`.
pub fn draw_workbench(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_menu(frame, chunks[0], state);
    draw_body(frame, chunks[1], state);
    draw_status(frame, chunks[2], state);

    match state.overlay {
        Overlay::Help => draw_help(frame, area),
        Overlay::Palette => draw_palette(frame, area, state),
        Overlay::Collision => draw_collision(frame, area, state),
        Overlay::Path => draw_path(frame, area, state),
        Overlay::PrimerDesign => draw_primer_design(frame, area, state),
        Overlay::PrimerCheck => draw_primer_check(frame, area, state),
        Overlay::Enzymes => draw_enzymes(frame, area, state),
        Overlay::Constructor => draw_constructor(frame, area, state),
        Overlay::Mutato => draw_mutato(frame, area, state),
        Overlay::Synthesis => draw_synthesis(frame, area, state),
        Overlay::Simulator => draw_simulator(frame, area, state),
        Overlay::Sequencing => draw_sequencing(frame, area, state),
        Overlay::Experiments => draw_experiments(frame, area, state),
        Overlay::History => draw_history(frame, area, state),
        Overlay::Search => draw_search(frame, area, state),
        Overlay::Parts => draw_parts(frame, area, state),
        Overlay::Settings => draw_settings(frame, area, state),
        Overlay::Babs => draw_babs(frame, area, state),
        Overlay::Autolab => draw_autolab(frame, area, state),
        Overlay::MasterDelete => draw_master_delete(frame, area, state),
        Overlay::FileMenu => draw_file_menu(frame, area, state),
        Overlay::None => {}
    }
}

fn draw_menu(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let mut spans = vec![
        Span::styled(
            format!(" {WELCOME_TITLE} "),
            Style::default()
                .fg(PRIMARY)
                .bg(PRIMARY_DARK)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    for (i, name) in MENUS.iter().enumerate() {
        let focused = state.menu_focus && i == state.menu_selected;
        let style = if focused {
            Style::default()
                .fg(TEXT_ON_PRIMARY)
                .bg(PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MENU_FG)
        };
        spans.push(Span::styled(format!(" {name} "), style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(PRIMARY_DARK)),
        area,
    );
}

fn draw_file_menu(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let height = (FILE_ITEMS.len() as u16)
        .saturating_add(2)
        .min(area.height.saturating_sub(1));
    let width = 28u16.min(area.width.saturating_sub(2)).max(16);
    let box_area = Rect {
        x: area.x.saturating_add(16),
        y: area.y.saturating_add(1),
        width,
        height,
    };
    frame.render_widget(Clear, box_area);
    let mut lines = Vec::new();
    for (i, (label, _)) in FILE_ITEMS.iter().enumerate() {
        let mark = if i == state.tool_selected { ">" } else { " " };
        let style = if i == state.tool_selected {
            Style::default()
                .fg(TEXT_ON_PRIMARY)
                .bg(PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        lines.push(Line::from(Span::styled(format!(" {mark} {label}"), style)));
    }
    frame.render_widget(
        Paragraph::new(lines).block(dialog_block("File", PRIMARY)),
        box_area,
    );
}

fn draw_body(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.focus_mode {
        FocusMode::Single(pane) => draw_pane(frame, area, pane, state, true),
        FocusMode::All => {
            // Upstream density: side panes ~32 cols, sequence ~14 rows when room.
            let side = if area.width >= 100 {
                SIDE_PANE_COLS
            } else {
                (area.width / 5).max(12)
            };
            let seq_h = if area.height >= 24 {
                SEQUENCE_ROWS.min(area.height.saturating_sub(8)).max(6)
            } else {
                (area.height / 3).max(4)
            };
            let rows =
                Layout::vertical([Constraint::Min(1), Constraint::Length(seq_h)]).split(area);
            let cols = Layout::horizontal([
                Constraint::Length(side),
                Constraint::Min(1),
                Constraint::Length(side),
            ])
            .split(rows[0]);
            draw_pane(
                frame,
                cols[0],
                Pane::Library,
                state,
                state.focus == Pane::Library,
            );
            draw_pane(frame, cols[1], Pane::Map, state, state.focus == Pane::Map);
            draw_pane(
                frame,
                cols[2],
                Pane::Features,
                state,
                state.focus == Pane::Features,
            );
            draw_pane(
                frame,
                rows[1],
                Pane::Sequence,
                state,
                state.focus == Pane::Sequence,
            );
        }
    }
}

fn draw_pane(frame: &mut Frame<'_>, area: Rect, pane: Pane, state: &AppState, focused: bool) {
    let title = match pane {
        Pane::Library => "Library",
        Pane::Map => {
            if state.map_circular {
                "Map ⊙"
            } else {
                "Map ─"
            }
        }
        Pane::Features => "Features",
        Pane::Sequence => "Sequence",
    };
    let border = if focused { PRIMARY } else { BORDER_DIM };
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(if focused { FOCUS_BG } else { PANEL_BG }));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let body = pane_body(pane, state, inner.width as usize, inner.height as usize);
    frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }), inner);
}

fn pane_body(pane: Pane, state: &AppState, width: usize, height: usize) -> Vec<Line<'static>> {
    match pane {
        Pane::Library => library_pane(state),
        Pane::Map => match &state.record {
            None => vec![Line::from(Span::styled(
                "Empty canvas — no plasmid loaded.  Ctrl+K · Load demo (basic / advanced)",
                Style::default().fg(TEXT),
            ))],
            Some(rec) => render_map_styled(
                rec,
                &MapOptions {
                    width,
                    height,
                    circular: state.map_circular,
                    origin: state.view_origin,
                    show_restr: state.show_restr,
                    show_labels: state.show_labels,
                    ascii: false,
                    unique_only: state.restr_unique,
                    min_recognition_len: if state.restr_min_six { 6 } else { 4 },
                    allowed_enzymes: state.enzymes.allowed_enzymes(),
                    extra_enzymes: state.custom_for_scan(),
                    align_segments: state.seq_segments.clone(),
                },
            ),
        },
        Pane::Features => features_pane(state),
        Pane::Sequence => match &state.record {
            None => vec![Line::from(Span::styled(
                "Sequence panel — empty.",
                Style::default().fg(TEXT),
            ))],
            Some(rec) => {
                let w = width.max(8);
                let start = state.cursor.saturating_sub(w / 4);
                render_sequence_styled(
                    rec,
                    &SeqView {
                        width: w,
                        window_start: start,
                        cursor: state.cursor,
                    },
                )
            }
        },
    }
}

fn library_pane(state: &AppState) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {:<28}", state.library.active),
            Style::default()
                .fg(TEXT_ON_PRIMARY)
                .bg(PRIMARY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " Search…",
            Style::default().fg(TEXT).bg(Color::Rgb(30, 30, 35)),
        )),
    ];
    if state.library.plasmids.is_empty() {
        lines.push(Line::from(Span::styled(
            " Empty — Alt+K keeps loaded record",
            Style::default().fg(TEXT),
        )));
    } else {
        for (i, e) in state.library.plasmids.iter().enumerate() {
            let seq = crate::io::library_entry_alignment_summary(&e.alignments)
                .map(|s| format!("  {}", s.glyph()))
                .unwrap_or_default();
            let label = format!(" {}  {} bp{seq} ", e.name, e.size);
            if state.selected_lib == i {
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default()
                        .fg(TEXT_ON_PRIMARY)
                        .bg(PRIMARY)
                        .add_modifier(Modifier::BOLD),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(Color::White),
                )));
            }
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" [+] ", Style::default().fg(Color::White).bg(Color::Blue)),
        Span::styled(" [-] ", Style::default().fg(Color::White).bg(DANGER)),
        Span::styled(" [□] ", Style::default().fg(Color::White).bg(PRIMARY_DARK)),
        Span::styled(" [✎] ", Style::default().fg(Color::Black).bg(Color::Gray)),
    ]));
    lines
}

fn features_pane(state: &AppState) -> Vec<Line<'static>> {
    match &state.record {
        None => vec![Line::from(Span::styled(
            "No features.",
            Style::default().fg(TEXT),
        ))],
        Some(rec) => {
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("{:<14}", "Type"),
                    Style::default()
                        .fg(Color::White)
                        .bg(PRIMARY_DARK)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " Label",
                    Style::default()
                        .fg(Color::White)
                        .bg(PRIMARY_DARK)
                        .add_modifier(Modifier::BOLD),
                ),
            ])];
            for (i, f) in rec.features.iter().enumerate() {
                if f.kind == "source" {
                    continue;
                }
                let color = feature_paint_color(f);
                let selected = state.selected_feat == Some(i);
                let type_txt = format!(" {:<12}", truncate_str(&f.kind, 11));
                let label_txt = format!(" {} ", f.label);
                let type_style = if selected {
                    Style::default()
                        .fg(TEXT_ON_PRIMARY)
                        .bg(color)
                        .add_modifier(Modifier::BOLD)
                } else {
                    // Upstream FeatureSidebar: type cell sits on a tinted chip.
                    Style::default()
                        .fg(Color::Black)
                        .bg(darken(color, 0.85))
                        .add_modifier(Modifier::BOLD)
                };
                let label_style = if selected {
                    Style::default()
                        .fg(TEXT_ON_PRIMARY)
                        .bg(color)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(color)
                };
                lines.push(Line::from(vec![
                    Span::styled(type_txt, type_style),
                    Span::styled(label_txt, label_style),
                ]));
            }
            if lines.len() == 1 {
                lines.push(Line::from(Span::styled(
                    "No features.",
                    Style::default().fg(TEXT),
                )));
            }
            lines
        }
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    s.chars().take(max.max(1)).collect()
}

fn draw_status(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    // Toast temporarily replaces the cheat-sheet (upstream Footer toast).
    let line = if let Some(toast) = state.toast.as_deref() {
        format!(" {toast} ")
    } else {
        format!(" {FOOTER_SHORTCUTS} ")
    };
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(FOOTER_FG).bg(FOOTER_BG)),
        area,
    );
}

/// Shared overlay dialog chrome (stage 18).
fn dialog_block(title: &str, border: Color) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(FOCUS_BG))
}

fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let box_area = centered(area, 62, 22);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            "SpliceCraft.rs — keyboard shortcuts",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for row in KEY_TABLE {
        lines.push(Line::from(format!(
            "  {:<12}  {}",
            row.keys, row.description
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "F10 then ←/→ Enter opens menus (File is a dropdown). q / Esc / ? close Help.",
    ));
    let block = dialog_block("Help", PRIMARY);
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_palette(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 52, 14);
    frame.render_widget(Clear, box_area);
    let cmds = state.visible_commands();
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Ctrl+K  {}", state.palette_query),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    if cmds.is_empty() {
        lines.push(Line::from("  (no matches)"));
    } else {
        for (i, cmd) in cmds.iter().enumerate() {
            let mark = if i == state.palette_selected {
                ">"
            } else {
                " "
            };
            lines.push(Line::from(format!(" {mark} {}", cmd.title)));
        }
    }
    let block = dialog_block("Command palette", PRIMARY);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_collision(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 58, 10);
    frame.render_widget(Clear, box_area);
    let name = match &state.pending_collision {
        Some(crate::state::PendingCollision::Keep(e)) => e.name.as_str(),
        Some(crate::state::PendingCollision::Feature(f)) => f.name.as_str(),
        None => "(unknown)",
    };
    let lines = vec![
        Line::from(Span::styled(
            "Name collision — choose explicitly",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("  {name}")),
        Line::from(""),
        Line::from("  s  skip (keep original)   c  copy   o  overwrite"),
        Line::from("  Esc cancel — overwrite is never implied"),
    ];
    let block = Block::default()
        .title(" Collision ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(WARN));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_path(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 60, 8);
    frame.render_widget(Clear, box_area);
    let title = match state.path_kind {
        PathKind::OpenFile => "Open file",
        PathKind::BulkImport => "Bulk import folder",
        PathKind::BulkExport => "Bulk export folder",
        PathKind::BulkAlign => "Bulk-align folder",
        PathKind::MapExport => "Export plasmid map (svg/png path)",
        PathKind::MigrateExport => "Export migrate archive (.zip)",
        PathKind::MigrateImport => "Import migrate archive (.zip)",
        PathKind::FetchNcbi => "NCBI accession",
        PathKind::FindDna => "Find DNA (both strands)",
        PathKind::NewPlasmid => "New plasmid — paste DNA",
        PathKind::AddFeature => "Add feature — label [type]",
    };
    let lines = vec![
        Line::from(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("  {}", state.path_query)),
        Line::from(""),
        Line::from("  Enter submit · Esc cancel"),
    ];
    let block = Block::default()
        .title(" Path ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_primer_design(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 78, 18);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        chip_tab_bar(
            DesignKind::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.design_kind)),
        ),
        Line::from(""),
        blurb_line(state.design_kind.blurb()),
        Line::from(""),
    ];
    if let Some(summary) = &state.design_summary {
        for row in summary.lines().take(8) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(true, &[("s", "save")]));
    let block = Block::default()
        .title(format!(" Primers — {} ", state.design_kind.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_primer_check(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 72, 16);
    frame.render_widget(Clear, box_area);
    let empty = state.tool_query.is_empty();
    let mut lines = vec![
        blurb_line("One oligo: binding sites. Two oligos (space or /): amplicon length."),
        Line::from(""),
        field_line(
            "Oligos",
            if empty {
                "FWD  or  FWD/REV"
            } else {
                &state.tool_query
            },
            empty,
        ),
        Line::from(""),
    ];
    if let Some(summary) = &state.check_summary {
        for row in summary.lines().take(8) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(false, &[]));
    let block = Block::default()
        .title(" Primer check ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_enzymes(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 64, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        blurb_line("Activate a collection for the map overlay. [ ] also cycles on the map."),
        Line::from(""),
        field_line(
            "Active",
            state
                .enzymes
                .active
                .as_deref()
                .unwrap_or("full NEB catalog"),
            false,
        ),
        field_line(
            "Custom",
            &format!("{} enzymes", state.enzymes.custom.len()),
            false,
        ),
        Line::from(""),
    ];
    if state.enzymes.collections.is_empty() {
        lines.push(blurb_line(
            "No saved collections — the full NEB catalog is in use.",
        ));
    } else {
        for (i, c) in state.enzymes.collections.iter().enumerate() {
            let selected = i == state.enzyme_selected;
            let star = if state.enzymes.active.as_deref() == Some(c.name.as_str()) {
                "* "
            } else {
                "  "
            };
            let style = if selected {
                Style::default()
                    .fg(TEXT_ON_PRIMARY)
                    .bg(PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            lines.push(Line::from(Span::styled(
                format!(" {star}{}  ({} enzymes)", c.name, c.enzymes.len()),
                style,
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(false, &[("↑↓", "pick")]));
    let block = Block::default()
        .title(" Enzymes ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_constructor(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 78, 20);
    frame.render_widget(Clear, box_area);
    let empty_q = state.tool_query.is_empty();
    let mut lines = vec![
        chip_tab_bar(
            ConstructorTab::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.ctor_tab)),
        ),
        Line::from(""),
        blurb_line(state.ctor_tab.blurb()),
        Line::from(""),
        field_line("Source", &format!("{}/4", state.ctor_source + 1), false),
        field_line("Grammar", &state.grammar_id, false),
        field_line(
            "Enzymes",
            if empty_q {
                "EcoRI BamHI"
            } else {
                &state.tool_query
            },
            empty_q,
        ),
        Line::from(""),
    ];
    if let Some(summary) = &state.ctor_summary {
        for row in summary.lines().take(8) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(true, &[("s", "save"), ("a", "arms")]));
    let block = Block::default()
        .title(format!(" Constructor — {} ", state.ctor_tab.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_mutato(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 76, 18);
    frame.render_widget(Clear, box_area);
    let empty = state.mutato_query.is_empty();
    let mut lines = vec![
        chip_tab_bar(
            MutatoTab::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.mutato_tab)),
        ),
        Line::from(""),
        blurb_line(state.mutato_tab.blurb()),
        Line::from(""),
        field_line(
            "Mutation",
            if empty { "V40F" } else { &state.mutato_query },
            empty,
        ),
        Line::from(""),
    ];
    if let Some(summary) = &state.mutato_summary {
        for row in summary.lines().take(8) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(true, &[]));
    let block = Block::default()
        .title(format!(" Mutato — {} ", state.mutato_tab.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_synthesis(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 76, 18);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        chip_tab_bar(
            SynthTab::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.synth_tab)),
        ),
        Line::from(""),
        blurb_line(state.synth_tab.blurb()),
        Line::from(""),
        field_line("DNA", &format!("{} bp", state.dna_buf.seq.len()), false),
        field_line(
            "Protein",
            &format!("{} aa", state.protein_buf.aa.chars().count()),
            false,
        ),
        field_line("Motifs", &format!("{}", state.motifs.merged().len()), false),
        Line::from(""),
    ];
    if let Some(summary) = &state.synth_summary {
        for row in summary.lines().take(6) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(true, &[]));
    let block = Block::default()
        .title(format!(" Synthesis — {} ", state.synth_tab.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_simulator(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 76, 20);
    frame.render_widget(Clear, box_area);
    let empty = state.sim_query.is_empty();
    let mut lines = vec![
        chip_tab_bar(
            SimulatorTab::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.sim_tab)),
        ),
        Line::from(""),
        blurb_line(state.sim_tab.blurb()),
        Line::from(""),
        field_line(
            "Primers",
            if empty { "FWD/REV" } else { &state.sim_query },
            empty,
        ),
        field_line("Agarose", &format!("{}%", state.sim_agarose), false),
        field_line("Lanes", &format!("{}", state.sim_lanes.len()), false),
        Line::from(""),
    ];
    if let Some(summary) = &state.sim_summary {
        for row in summary.lines().take(3) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    if state.sim_tab == SimulatorTab::Gel
        && let Some(img) = &state.sim_gel_image
    {
        for row in img.lines().take(6) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(true, &[("g", "gel"), ("s", "save")]));
    let block = Block::default()
        .title(format!(" Simulator — {} ", state.sim_tab.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_sequencing(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 78, 20);
    frame.render_widget(Clear, box_area);
    let empty = state.seq_query.is_empty();
    let mut lines = vec![
        chip_tab_bar(
            SequencingTab::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.seq_tab)),
        ),
        Line::from(""),
        blurb_line(state.seq_tab.blurb()),
        Line::from(""),
        field_line(
            "Query",
            if empty {
                state.seq_tab.query_hint()
            } else {
                &state.seq_query
            },
            empty,
        ),
        field_line(
            "Hits",
            &format!("{} variant(s)", state.seq_variants.len()),
            false,
        ),
        Line::from(""),
    ];
    if let Some(summary) = &state.seq_summary {
        for row in summary.lines().take(8) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    if !state.seq_variants.is_empty() {
        let idx = state.seq_variant_idx.min(state.seq_variants.len() - 1);
        let v = &state.seq_variants[idx];
        lines.push(field_line(
            "Variant",
            &format!(
                "{}/{}  {} @{}",
                idx + 1,
                state.seq_variants.len(),
                v.kind,
                v.target_pos
            ),
            false,
        ));
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(true, &[("j", "jump")]));
    let block = Block::default()
        .title(format!(" Sequencing — {} ", state.seq_tab.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_experiments(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 78, 20);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        chip_tab_bar(
            ExperimentsTab::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.exp_tab)),
        ),
        Line::from(""),
        blurb_line(state.exp_tab.blurb()),
        Line::from(""),
        field_line("Project", &state.experiments.active, false),
        Line::from(""),
    ];
    match state.exp_tab {
        ExperimentsTab::List => {
            if state.experiments.entries.is_empty() {
                lines.push(blurb_line("Empty notebook — Enter opens compose."));
            } else {
                for (i, e) in state.experiments.entries.iter().enumerate().take(8) {
                    lines.push(selected_row(
                        format!(" {}  {}", e.id, e.title),
                        i == state.exp_selected,
                    ));
                }
            }
        }
        ExperimentsTab::Compose => {
            let empty_title = state.exp_title.is_empty();
            lines.push(field_line(
                "Title",
                if empty_title {
                    "from first line"
                } else {
                    &state.exp_title
                },
                empty_title,
            ));
            if state.exp_body.is_empty() {
                lines.push(field_line("Body", "@plasmid !action &gel", true));
            } else {
                for row in state
                    .exp_body
                    .lines()
                    .rev()
                    .take(6)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                {
                    lines.push(Line::from(format!("  {row}")));
                }
            }
        }
        ExperimentsTab::Attach => {
            let empty = state.tool_query.is_empty();
            lines.push(field_line(
                "Path",
                if empty {
                    "image path"
                } else {
                    &state.tool_query
                },
                empty,
            ));
        }
    }
    if let Some(summary) = &state.exp_summary {
        lines.push(Line::from(""));
        for row in summary.lines().take(4) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer_ex(
        true,
        "save",
        &[("Ctrl+G", "jump"), ("F7", "spell")],
    ));
    let block = Block::default()
        .title(format!(" Experiments — {} ", state.exp_tab.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_history(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 78, 20);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        chip_tab_bar(
            HistoryTab::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.hist_tab)),
        ),
        Line::from(""),
        blurb_line(state.hist_tab.blurb()),
        Line::from(""),
    ];
    if !state.hist_warnings.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "  ⚠ {} recorded detail(s) the sequence doesn't support",
                state.hist_warnings.len()
            ),
            Style::default().fg(WARN),
        )));
        for w in state.hist_warnings.iter().take(3) {
            lines.push(Line::from(format!("  {w}")));
        }
        lines.push(Line::from(""));
    }
    for row in state.hist_lines.iter().take(8) {
        lines.push(Line::from(format!("  {row}")));
    }
    if let Some(summary) = &state.hist_summary {
        for row in summary.lines().take(3) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer_ex(true, "recover", &[]));
    let block = Block::default()
        .title(format!(" History — {} ", state.hist_tab.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_search(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 78, 22);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        chip_tab_bar(
            SearchTab::ALL
                .iter()
                .map(|t| (t.chip(), *t == state.search_tab)),
        ),
        Line::from(""),
        blurb_line(state.search_tab.blurb()),
        Line::from(""),
        field_line(
            "Query",
            if state.search_query.is_empty() {
                state.search_tab.query_hint()
            } else {
                &state.search_query
            },
            state.search_query.is_empty(),
        ),
        field_line("Program", state.search_program.as_str(), false),
        field_line(
            "Online",
            if state.allow_online_search {
                "armed — Settings"
            } else {
                "off — Settings to enable"
            },
            false,
        ),
        Line::from(""),
    ];
    if let Some(summary) = &state.search_summary {
        for row in summary.lines().take(3) {
            lines.push(Line::from(Span::styled(
                row.to_owned(),
                Style::default().fg(WARN),
            )));
        }
    }
    for (i, row) in state.search_lines.iter().enumerate().take(8) {
        let selected = i == state.search_selected;
        let mark = if selected { "▸" } else { " " };
        let style = if selected {
            Style::default()
                .fg(TEXT_ON_PRIMARY)
                .bg(PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        lines.push(Line::from(Span::styled(format!(" {mark} {row}"), style)));
    }
    if state.search_lines.is_empty() && state.search_summary.is_none() {
        lines.push(Line::from(Span::styled(
            "  Type a query, then Enter. ↑↓ move a hit.",
            Style::default().fg(TEXT),
        )));
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer(true, &[]));
    let block = Block::default()
        .title(format!(" BLAST — {} ", state.search_tab.chip()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn chip_tab_bar<'a>(chips: impl IntoIterator<Item = (&'a str, bool)>) -> Line<'a> {
    let mut spans = Vec::new();
    for (chip, on) in chips {
        spans.push(Span::styled(
            format!(" {chip} "),
            if on {
                Style::default()
                    .fg(TEXT_ON_PRIMARY)
                    .bg(PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            },
        ));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

fn field_line(name: &str, value: &str, placeholder: bool) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {name:<8} "),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            value.to_owned(),
            if placeholder {
                Style::default().fg(BORDER_DIM)
            } else {
                Style::default().fg(MENU_FG)
            },
        ),
    ])
}

fn overlay_footer(tabbed: bool, extras: &[(&'static str, &'static str)]) -> Line<'static> {
    overlay_footer_ex(tabbed, "run", extras)
}

fn overlay_footer_ex(
    tabbed: bool,
    enter: &'static str,
    extras: &[(&'static str, &'static str)],
) -> Line<'static> {
    let mut spans = Vec::new();
    if tabbed {
        spans.push(Span::styled(" ← → ", Style::default().fg(FOOTER_FG)));
        spans.push(Span::styled("switch   ", Style::default().fg(TEXT)));
        spans.push(Span::styled("Tab", Style::default().fg(FOOTER_FG)));
        spans.push(Span::styled(" next   ", Style::default().fg(TEXT)));
    }
    spans.push(Span::styled("Enter", Style::default().fg(FOOTER_FG)));
    spans.push(Span::styled(
        format!(" {enter}   "),
        Style::default().fg(TEXT),
    ));
    for (key, desc) in extras {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default().fg(FOOTER_FG),
        ));
        spans.push(Span::styled(
            format!("{desc}   "),
            Style::default().fg(TEXT),
        ));
    }
    spans.push(Span::styled("Esc", Style::default().fg(FOOTER_FG)));
    spans.push(Span::styled(" close", Style::default().fg(TEXT)));
    Line::from(spans)
}

fn blurb_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_owned(), Style::default().fg(TEXT)))
}

fn selected_row(text: String, selected: bool) -> Line<'static> {
    let style = if selected {
        Style::default()
            .fg(TEXT_ON_PRIMARY)
            .bg(PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    };
    Line::from(Span::styled(text, style))
}

fn draw_parts(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 72, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        blurb_line("Classify the loaded plasmid into the parts bin, or file a syn-frag."),
        Line::from(""),
        field_line("Count", &format!("{}", state.parts.parts.len()), false),
        Line::from(""),
    ];
    if state.parts.parts.is_empty() {
        lines.push(blurb_line(
            "Empty — Enter classifies a digest, or use Constructor → Syn-frag.",
        ));
    } else {
        for (i, p) in state.parts.parts.iter().enumerate().take(8) {
            lines.push(selected_row(
                format!(
                    " {}  {}  {}/{}  {} bp",
                    p.name,
                    p.type_name,
                    p.oh5,
                    p.oh3,
                    p.sequence.len()
                ),
                i == state.tool_selected,
            ));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer_ex(false, "classify", &[]));
    let block = Block::default()
        .title(" Parts ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_settings(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 64, 14);
    frame.render_widget(Clear, box_area);
    let tile = |i: usize, name: &str, on: bool| {
        selected_row(
            format!(" {name:<22}  {}", if on { "ON " } else { "off" }),
            state.settings_selected == i,
        )
    };
    let lines = vec![
        blurb_line("Online stays off until you tick a setting. The agent cannot arm search."),
        Line::from(""),
        tile(0, "allow_online_search", state.allow_online_search),
        tile(1, "allow_online_lookups", state.allow_online_lookups),
        Line::from(""),
        overlay_footer_ex(false, "toggle", &[("↑↓", "pick")]),
    ];
    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_babs(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 72, 18);
    frame.render_widget(Clear, box_area);
    let empty = state.babs_query.is_empty();
    let mut lines = vec![
        blurb_line("Local Ollama only (127.0.0.1). No public URLs."),
        Line::from(""),
        field_line("Model", &state.babs_model, false),
        Line::from(""),
    ];
    for row in state.babs_lines.iter().rev().take(6).rev() {
        lines.push(Line::from(row.as_str()));
    }
    lines.push(Line::from(""));
    lines.push(field_line(
        "Query",
        if empty {
            "/help  or a prompt"
        } else {
            &state.babs_query
        },
        empty,
    ));
    lines.push(Line::from(""));
    lines.push(overlay_footer_ex(
        false,
        "send",
        &[("/help", ""), ("/clear", ""), ("/model", "")],
    ));
    let block = Block::default()
        .title(format!(" BABS — {} ", state.babs_model))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ENZYME_ACCENT));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_autolab(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 72, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        blurb_line("OT-2 compiler. Tab arms motion; live robots stay off in this build."),
        Line::from(""),
        field_line(
            "Motion",
            if state.autolab_motion_armed {
                "ARMED — confirm still required; no robot"
            } else {
                "disarmed"
            },
            false,
        ),
        Line::from(""),
    ];
    if let Some(s) = &state.autolab_summary {
        lines.push(Line::from(s.as_str()));
    }
    if let Some(p) = &state.autolab_protocol {
        for row in p.lines().take(6) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    lines.push(Line::from(""));
    lines.push(overlay_footer_ex(false, "compile", &[("Tab", "arm")]));
    let block = Block::default()
        .title(" AUTOLAB ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(WARN));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_master_delete(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 64, 14);
    frame.render_widget(Clear, box_area);
    let remain = state.master_delete_cooldown_remaining();
    let no = if !state.master_delete_yes {
        "[No]"
    } else {
        " No "
    };
    let yes = if state.master_delete_yes {
        "[Yes]"
    } else {
        " Yes "
    };
    let lines = vec![
        Line::from(Span::styled(
            "Master Delete — wipe SpliceCraft.rs data",
            Style::default().fg(DANGER).add_modifier(Modifier::BOLD),
        )),
        Line::from("  This does not touch the Python ~/.local/share/splicecraft/ dir."),
        Line::from("  Default focus is No. There is no keyboard shortcut to open this."),
        Line::from(""),
        Line::from(format!(
            "  step {}/3   {no}  {yes}",
            state.master_delete_step + 1
        )),
        Line::from(format!("  type DELETE: {}", state.master_delete_typed)),
        Line::from(format!(
            "  cooldown: {}",
            if remain.is_zero() {
                "ready".into()
            } else {
                format!("{:.1}s", remain.as_secs_f32())
            }
        )),
        Line::from("  ←→ choose · Enter · n / Esc keep data"),
    ];
    let block = Block::default()
        .title(" Master Delete ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DANGER));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let pad_x = 2.min(area.width);
    let pad_y = 2.min(area.height);
    let max_w = area.width.saturating_sub(pad_x).max(1);
    let max_h = area.height.saturating_sub(pad_y).max(1);
    let width = width.min(max_w);
    let height = height.min(max_h);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}
