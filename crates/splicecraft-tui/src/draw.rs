//! Pure draw: `AppState` → Ratatui. No persist, no sequence logging.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::action::{FocusMode, Overlay, Pane, PathKind};
use crate::keys::KEY_TABLE;
use crate::render::{MapOptions, SeqView, render_map, render_sequence};
use crate::state::AppState;
use crate::{IMPLEMENTATION_STAGE, WELCOME_TITLE};

/// Menu labels matching upstream `MenuBar.MENUS` (tools are stubs).
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

    draw_menu(frame, chunks[0]);
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
        Overlay::Parts => draw_parts(frame, area, state),
        Overlay::None => {}
    }
}

fn draw_menu(frame: &mut Frame<'_>, area: Rect) {
    let items = MENUS.join("  ");
    let line = Line::from(vec![
        Span::styled(
            format!(" {WELCOME_TITLE} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(items, Style::default().fg(Color::Gray)),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::Black)),
        area,
    );
}

fn draw_body(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    match state.focus_mode {
        FocusMode::Single(pane) => draw_pane(frame, area, pane, state, true),
        FocusMode::All => {
            let rows = Layout::vertical([Constraint::Percentage(62), Constraint::Percentage(38)])
                .split(area);
            let cols = Layout::horizontal([
                Constraint::Percentage(22),
                Constraint::Percentage(50),
                Constraint::Percentage(28),
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
    let border = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let body = pane_body(pane, state, inner.width as usize, inner.height as usize);
    frame.render_widget(
        Paragraph::new(body)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn pane_body(pane: Pane, state: &AppState, width: usize, height: usize) -> String {
    match pane {
        Pane::Library => library_pane(state),
        Pane::Map => match &state.record {
            None => "Empty canvas — no plasmid loaded.\nCtrl+K · Load demo plasmid".into(),
            Some(rec) => render_map(
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
            )
            .join("\n"),
        },
        Pane::Features => features_pane(state),
        Pane::Sequence => match &state.record {
            None => "Sequence panel — empty.".into(),
            Some(rec) => {
                let w = width.max(8);
                let start = state.cursor.saturating_sub(w / 4);
                render_sequence(
                    rec,
                    &SeqView {
                        width: w,
                        window_start: start,
                        cursor: state.cursor,
                    },
                )
                .join("\n")
            }
        },
    }
}

fn library_pane(state: &AppState) -> String {
    let mut lines = vec![format!("[{}]", state.library.active)];
    if state.library.plasmids.is_empty() {
        lines.push("Empty collection — Alt+K keeps the loaded record.".into());
    } else {
        for (i, e) in state.library.plasmids.iter().enumerate() {
            let mark = if state.selected_lib == i { ">" } else { " " };
            let seq = crate::io::library_entry_alignment_summary(&e.alignments)
                .map(|s| format!("  {}", s.glyph()))
                .unwrap_or_default();
            lines.push(format!("{mark} {}  {} bp{seq}", e.name, e.size));
        }
    }
    lines.join("\n")
}

fn features_pane(state: &AppState) -> String {
    match &state.record {
        None => "No features.".into(),
        Some(rec) => {
            let mut lines = Vec::new();
            for (i, f) in rec.features.iter().enumerate() {
                let mark = if state.selected_feat == Some(i) {
                    ">"
                } else {
                    " "
                };
                lines.push(format!(
                    "{mark} {}  {}..{}  {} bp",
                    f.label,
                    f.start,
                    f.end,
                    f.len_on(rec.len())
                ));
            }
            if lines.is_empty() {
                "No features.".into()
            } else {
                lines.join("\n")
            }
        }
    }
}

fn draw_status(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let (topo, bp) = match &state.record {
        Some(rec) => (
            if rec.circular { "circular" } else { "linear" },
            format!("{} bp", rec.len()),
        ),
        None => ("—", "— bp".into()),
    };
    let hint = state
        .toast
        .as_deref()
        .unwrap_or("? help  ^K palette  q quit");
    let text = format!(
        " {}  ·  {topo}  ·  {bp}  ·  stage {IMPLEMENTATION_STAGE:02}  ·  {hint} ",
        state.source_label
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        area,
    );
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
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
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
    let block = Block::default()
        .title(" Command palette ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Yellow));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
        .border_style(Style::default().fg(Color::Cyan));
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
