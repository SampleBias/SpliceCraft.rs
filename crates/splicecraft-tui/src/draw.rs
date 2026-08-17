//! Pure draw: `AppState` → Ratatui. No persist, no sequence logging.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::action::{FocusMode, Overlay, Pane};
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
        Pane::Library => "No collections loaded.\nOpen stays in memory until stage 06.".into(),
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
                },
            )
            .join("\n"),
        },
        Pane::Features => match &state.record {
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
        },
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
    let box_area = centered(area, 56, 16);
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
