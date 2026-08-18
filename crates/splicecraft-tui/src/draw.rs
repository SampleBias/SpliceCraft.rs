//! Pure draw: `AppState` → Ratatui. No persist, no sequence logging.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::WELCOME_TITLE;
use crate::action::{FocusMode, Overlay, Pane, PathKind};
use crate::keys::KEY_TABLE;
use crate::render::{MapOptions, SeqView, render_map_styled, render_sequence_styled};
use crate::state::AppState;
use crate::theme::{
    BORDER_DIM, DANGER, ENZYME_ACCENT, FOCUS_BG, FOOTER_BG, FOOTER_FG, FOOTER_SHORTCUTS, MENU_FG,
    PANEL_BG, PRIMARY, PRIMARY_DARK, SEQUENCE_ROWS, SIDE_PANE_COLS, TEXT, TEXT_ON_PRIMARY, WARN,
    darken, feature_paint_color,
};

/// Menu labels matching upstream `MenuBar.MENUS`.
const MENUS: &[&str] = &[
    "File",
    "Settings",
    "BLAST",
    "Enzymes",
    "Features",
    "Primers",
    "Mutato",
    "Synthesis",
    "Parts",
    "Constructor",
    "Simulator",
    "Sequencing",
    "Experiments",
    "History",
    "AUTOLAB",
    "BABS",
];

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
        Overlay::None => {}
    }
}

fn draw_menu(frame: &mut Frame<'_>, area: Rect, _state: &AppState) {
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
    for name in MENUS {
        spans.push(Span::styled(
            format!(" {name} "),
            Style::default().fg(MENU_FG),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(PRIMARY_DARK)),
        area,
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
                "Empty canvas — no plasmid loaded.  Ctrl+K · Load demo plasmid",
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
    let box_area = centered(area, 56, 18);
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
        "q / Esc / ? close this overlay. Main-view q / Esc quits.",
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
    let box_area = centered(area, 62, 14);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Primer design — {}", state.design_kind.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab cycle mode · Enter design · s save to library · Esc close"),
        Line::from(""),
    ];
    if let Some(summary) = &state.design_summary {
        for row in summary.lines() {
            lines.push(Line::from(row.to_owned()));
        }
    } else {
        lines.push(Line::from(
            "  Uses the selected feature, or the whole record if none.",
        ));
    }
    let block = Block::default()
        .title(" Primers ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_primer_check(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 64, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            "Primer check",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  One oligo: sites. Two oligos (space or /): amplicon length."),
        Line::from(format!("  {}", state.tool_query)),
        Line::from(""),
    ];
    if let Some(summary) = &state.check_summary {
        for row in summary.lines().take(8) {
            lines.push(Line::from(row.to_owned()));
        }
    }
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
    let box_area = centered(area, 56, 14);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            "Enzyme collections",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Enter activate · Esc close · [ ] also cycle on the map"),
        Line::from(""),
    ];
    if state.enzymes.collections.is_empty() {
        lines.push(Line::from("  (no collections — full NEB catalog)"));
    } else {
        for (i, c) in state.enzymes.collections.iter().enumerate() {
            let mark = if i == state.enzyme_selected { ">" } else { " " };
            let on = if state.enzymes.active.as_deref() == Some(c.name.as_str()) {
                "*"
            } else {
                " "
            };
            lines.push(Line::from(format!(
                " {mark}{on} {}  ({} enzymes)",
                c.name,
                c.enzymes.len()
            )));
        }
    }
    lines.push(Line::from(format!(
        "  custom enzymes: {}",
        state.enzymes.custom.len()
    )));
    let block = Block::default()
        .title(" Enzymes ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_constructor(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 70, 18);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Constructor — {}", state.ctor_tab.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab cycle · Enter run · s save product · a Gibson arms · Esc close"),
        Line::from(format!(
            "  source {}/4  ·  grammar {}  ·  {}",
            state.ctor_source + 1,
            state.grammar_id,
            if state.tool_query.is_empty() {
                "EcoRI BamHI"
            } else {
                &state.tool_query
            }
        )),
        Line::from(""),
    ];
    if let Some(summary) = &state.ctor_summary {
        for row in summary.lines().take(10) {
            lines.push(Line::from(row.to_owned()));
        }
    } else {
        lines.push(Line::from(
            "  Traditional: digest the loaded plasmid with two enzymes.",
        ));
        lines.push(Line::from(
            "  Gibson: current record + highlighted library plasmid.",
        ));
        lines.push(Line::from(
            "  Domesticator / syn-frag: 4-source picker (↑↓) + part type.",
        ));
    }
    let block = Block::default()
        .title(" Constructor ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_mutato(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 70, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Mutato — {}", state.mutato_tab.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab cycle · type mutation (V40F) · Enter design · Esc close"),
        Line::from(format!(
            "  query {}",
            if state.mutato_query.is_empty() {
                "(V40F)"
            } else {
                &state.mutato_query
            }
        )),
        Line::from(""),
    ];
    if let Some(summary) = &state.mutato_summary {
        for row in summary.lines().take(8) {
            lines.push(Line::from(row.to_owned()));
        }
    } else {
        lines.push(Line::from(
            "  SDM: SOE 4-primer or near-end 2-primer shortcut.",
        ));
        lines.push(Line::from(
            "  Scrub QC: clone-free QuikChange.  Scrub GB: recirc must match.",
        ));
    }
    let block = Block::default()
        .title(" Mutato ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_synthesis(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 70, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Synthesis — {}", state.synth_tab.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab cycle · type DNA/AA · Enter compose · Esc close"),
        Line::from(format!(
            "  DNA {} bp  ·  protein {} aa  ·  motifs {}",
            state.dna_buf.seq.len(),
            state.protein_buf.aa.chars().count(),
            state.motifs.merged().len()
        )),
        Line::from(""),
    ];
    if let Some(summary) = &state.synth_summary {
        for row in summary.lines().take(8) {
            lines.push(Line::from(row.to_owned()));
        }
    } else {
        lines.push(Line::from(
            "  DNA: linear IUPAC buffer.  Protein: fill from K12 table.",
        ));
        lines.push(Line::from(
            "  Operon: SOE domestication of CDS features on the loaded record.",
        ));
    }
    let block = Block::default()
        .title(" Synthesis ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_simulator(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 72, 18);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Simulator — {}", state.sim_tab.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab PCR/gel · Enter run · g send to gel · s save · Esc close"),
        Line::from(format!(
            "  primers {}  ·  {}% agarose  ·  {} lanes",
            if state.sim_query.is_empty() {
                "(FWD/REV)"
            } else {
                &state.sim_query
            },
            state.sim_agarose,
            state.sim_lanes.len()
        )),
        Line::from(""),
    ];
    if let Some(summary) = &state.sim_summary {
        for row in summary.lines().take(3) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    if state.sim_tab == crate::action::SimulatorTab::Gel {
        if let Some(img) = &state.sim_gel_image {
            for row in img.lines().take(10) {
                lines.push(Line::from(row.to_owned()));
            }
        } else {
            lines.push(Line::from(
                "  Enter renders HGB mobility (well → dye front).",
            ));
        }
    } else if state.sim_summary.is_none() {
        lines.push(Line::from(
            "  Exact-match PCR. Wrap amplicons are legal on circular templates.",
        ));
        lines.push(Line::from(
            "  Save writes a linear library entry with primer_bind features.",
        ));
    }
    let block = Block::default()
        .title(" Simulator ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_sequencing(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 72, 18);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Sequencing — {}", state.seq_tab.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab zip/align/Sanger/report · Enter run · j jump variant · Esc close"),
        Line::from(format!(
            "  {}  ·  {} variant(s)",
            if state.seq_query.is_empty() {
                match state.seq_tab {
                    crate::action::SequencingTab::Zip => "(zip path)",
                    crate::action::SequencingTab::Align => "(read DNA or path)",
                    crate::action::SequencingTab::Sanger => "(AB1 path)",
                    crate::action::SequencingTab::Report => "(reads folder)",
                }
            } else {
                &state.seq_query
            },
            state.seq_variants.len()
        )),
        Line::from(""),
    ];
    if let Some(summary) = &state.seq_summary {
        for row in summary.lines().take(10) {
            lines.push(Line::from(row.to_owned()));
        }
    } else {
        match state.seq_tab {
            crate::action::SequencingTab::Zip => {
                lines.push(Line::from(
                    "  Import a Plasmidsaurus zip. Tagged plasmidsaurus:run:sample; never clobbers.",
                ));
            }
            crate::action::SequencingTab::Align => {
                lines.push(Line::from(
                    "  Pairwise overlay vs the loaded plasmid. Identity <100 never shows as 100%.",
                ));
            }
            crate::action::SequencingTab::Sanger => {
                lines.push(Line::from(
                    "  Load an AB1/ABIF trace (Phred). Aligns against the loaded plasmid if any.",
                ));
            }
            crate::action::SequencingTab::Report => {
                lines.push(Line::from(
                    "  Bulk-align a folder of reads. Grades: verified / near / partial / divergent.",
                ));
            }
        }
    }
    if !state.seq_variants.is_empty() {
        let idx = state.seq_variant_idx.min(state.seq_variants.len() - 1);
        let v = &state.seq_variants[idx];
        lines.push(Line::from(format!(
            "  variant {}/{}  {} @{}",
            idx + 1,
            state.seq_variants.len(),
            v.kind,
            v.target_pos
        )));
    }
    let block = Block::default()
        .title(" Sequencing ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_experiments(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 72, 20);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Experiments — {}", state.exp_tab.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(
            "  Tab list/compose/attach · Enter save/open · Ctrl+G jump · F7 spellcheck · Esc",
        ),
        Line::from(format!("  project: {}", state.experiments.active)),
        Line::from(""),
    ];
    match state.exp_tab {
        crate::action::ExperimentsTab::List => {
            if state.experiments.entries.is_empty() {
                lines.push(Line::from("  (empty notebook — Enter to compose)"));
            } else {
                for (i, e) in state.experiments.entries.iter().enumerate().take(10) {
                    let mark = if i == state.exp_selected { ">" } else { " " };
                    lines.push(Line::from(format!(" {mark} {}  {}", e.id, e.title)));
                }
            }
        }
        crate::action::ExperimentsTab::Compose => {
            let title = if state.exp_title.is_empty() {
                "(title from first line)"
            } else {
                &state.exp_title
            };
            lines.push(Line::from(format!("  title: {title}")));
            if state.exp_body.is_empty() {
                lines.push(Line::from("  (type markdown: @plasmid !action &gel)"));
            } else {
                for row in state
                    .exp_body
                    .lines()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                {
                    lines.push(Line::from(format!("  {row}")));
                }
            }
        }
        crate::action::ExperimentsTab::Attach => {
            lines.push(Line::from(format!(
                "  path: {}",
                if state.tool_query.is_empty() {
                    "(image path)"
                } else {
                    &state.tool_query
                }
            )));
            lines.push(Line::from("  Enter writes via the persist chokepoint."));
        }
    }
    if let Some(summary) = &state.exp_summary {
        lines.push(Line::from(""));
        for row in summary.lines().take(6) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    let block = Block::default()
        .title(" Experiments ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_history(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 74, 20);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("History — {}", state.hist_tab.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab protocol/tree/detail · Enter recover (type apply to write) · Esc close"),
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
    }
    for row in state.hist_lines.iter().take(10) {
        lines.push(Line::from(format!("  {row}")));
    }
    if let Some(summary) = &state.hist_summary {
        for row in summary.lines().take(4) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    let block = Block::default()
        .title(" History ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_search(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 74, 20);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Search — {}", state.search_tab.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab local/ORF/online/HMM-DB/find · Enter run · Esc close"),
        Line::from(format!(
            "  program {}  ·  online {}  ·  {}",
            state.search_program.as_str(),
            if state.allow_online_search {
                "armed"
            } else {
                "off"
            },
            if state.search_query.is_empty() {
                match state.search_tab {
                    crate::action::SearchTab::Local => "(query DNA/protein)",
                    crate::action::SearchTab::Orf => "(Enter scans the loaded record)",
                    crate::action::SearchTab::Online => "(refused unless setting is ticked)",
                    crate::action::SearchTab::HmmDb => "(catalog — no Pfam download)",
                    crate::action::SearchTab::Find => "(plasmid name)",
                }
            } else {
                &state.search_query
            }
        )),
        Line::from(""),
    ];
    if let Some(summary) = &state.search_summary {
        for row in summary.lines().take(3) {
            lines.push(Line::from(row.to_owned()));
        }
    }
    for (i, row) in state.search_lines.iter().enumerate().take(10) {
        let mark = if i == state.search_selected { ">" } else { " " };
        lines.push(Line::from(format!(" {mark} {row}")));
    }
    if state.search_lines.is_empty() && state.search_summary.is_none() {
        lines.push(Line::from(
            "  Local BLAST is ungapped (HMMER is not in the default build).",
        ));
        lines.push(Line::from(
            "  ORF length is length_aa / nt_len — never (end − start) on a wrap.",
        ));
    }
    let block = Block::default()
        .title(" BLAST ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_parts(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 64, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            "Parts Bin",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Enter classify loaded plasmid · Esc close"),
        Line::from(""),
    ];
    if state.parts.parts.is_empty() {
        lines.push(Line::from(
            "  (empty — file a syn-frag or classify a digest)",
        ));
    } else {
        for (i, p) in state.parts.parts.iter().enumerate().take(8) {
            let mark = if i == state.tool_selected { ">" } else { " " };
            lines.push(Line::from(format!(
                " {mark} {}  {}  {}/{}  {} bp",
                p.name,
                p.type_name,
                p.oh5,
                p.oh3,
                p.sequence.len()
            )));
        }
    }
    let block = Block::default()
        .title(" Parts ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_settings(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 62, 12);
    frame.render_widget(Clear, box_area);
    let mark = |i: usize| {
        if state.settings_selected == i {
            ">"
        } else {
            " "
        }
    };
    let on = |b: bool| if b { "ON" } else { "off" };
    let lines = vec![
        Line::from(Span::styled(
            "Settings",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  ↑↓ select · Enter toggle · Esc close"),
        Line::from("  Agent cannot enable online search (stage 14)."),
        Line::from(""),
        Line::from(format!(
            " {} allow_online_search     [{}]",
            mark(0),
            on(state.allow_online_search)
        )),
        Line::from(format!(
            " {} allow_online_lookups    [{}]",
            mark(1),
            on(state.allow_online_lookups)
        )),
    ];
    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_babs(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 68, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("BABS — {}", state.babs_model),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Local Ollama only (127.0.0.1). /help /clear /model · Enter send · Esc"),
        Line::from(""),
    ];
    for row in state.babs_lines.iter().rev().take(8).rev() {
        lines.push(Line::from(row.as_str()));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(format!("  > {}", state.babs_query)));
    let block = Block::default()
        .title(" BABS ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ENZYME_ACCENT));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        box_area,
    );
}

fn draw_autolab(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let box_area = centered(area, 68, 16);
    frame.render_widget(Clear, box_area);
    let mut lines = vec![
        Line::from(Span::styled(
            "AUTOLAB — OT-2 compiler",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Enter compile fixture · Tab arm motion (still no robot) · Esc"),
        Line::from(format!(
            "  motion armed: {}",
            if state.autolab_motion_armed {
                "yes (confirm required to run)"
            } else {
                "no"
            }
        )),
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
