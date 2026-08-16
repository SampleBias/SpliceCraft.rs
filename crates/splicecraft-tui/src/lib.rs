//! Ratatui shell for SpliceCraft.rs.
//!
//! Stage 00 ships a welcome frame. Stages 04–05 fill in the real workbench.

#![forbid(unsafe_code)]

pub use splicecraft_bio as bio;
pub use splicecraft_clone as clone;
pub use splicecraft_core as core;
pub use splicecraft_gels as gels;
pub use splicecraft_persist as persist;

use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

/// Stage this crate currently satisfies (welcome frame only).
pub const IMPLEMENTATION_STAGE: u8 = 0;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Title painted on the stage-00 welcome frame.
pub const WELCOME_TITLE: &str = "SpliceCraft.rs";

/// Run the interactive TUI until the user presses `q` or Esc.
pub fn run() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut DefaultTerminal) -> io::Result<()> {
    loop {
        terminal.draw(draw_welcome)?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => break,
            _ => {}
        }
    }
    Ok(())
}

/// Draw the stage-00 welcome screen. Pure with respect to app state so it can
/// be unit-tested on a `TestBackend` without a real tty.
pub fn draw_welcome(frame: &mut Frame<'_>) {
    let area = frame.area();
    let block = Block::default()
        .title(" SpliceCraft.rs ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(7),
        Constraint::Min(1),
    ])
    .split(inner);

    let body = Paragraph::new(vec![
        Line::from(Span::styled(
            WELCOME_TITLE,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("plasmid workbench — stage 00 bootstrap"),
        Line::from("q / Esc  quit"),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });

    frame.render_widget(body, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-tui");
    }

    #[test]
    fn welcome_frame_paints_title() {
        let backend = TestBackend::new(60, 16);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(draw_welcome).expect("draw");
        let text = buffer_text(terminal.backend());
        assert!(
            text.contains(WELCOME_TITLE),
            "welcome frame missing title, got:\n{text}"
        );
        assert!(
            text.contains("stage 00"),
            "welcome frame missing stage label, got:\n{text}"
        );
    }

    #[test]
    fn persist_data_dir_is_isolated_from_python() {
        assert_eq!(persist::XDG_DATA_DIR_LEAF, "splicecraft-rs");
    }
}
