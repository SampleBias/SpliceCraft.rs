//! Braille / ASCII sub-cell canvas. Port of upstream `_BrailleCanvas`.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

const DOT_BITS: [[u8; 2]; 4] = [[0, 3], [1, 4], [2, 5], [6, 7]];
const ASCII_RAMP: &[u8] = b" .:-=+*#@";

/// Character overlay (labels, centre text) on top of braille dots.
#[derive(Clone, Debug)]
pub struct CharCanvas {
    width: usize,
    height: usize,
    chars: Vec<Vec<char>>,
    colors: Vec<Vec<Option<Color>>>,
}

impl CharCanvas {
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            chars: vec![vec![' '; width]; height],
            colors: vec![vec![None; width]; height],
        }
    }

    #[allow(dead_code)] // plain put kept for mapimage / future ASCII helpers
    pub fn put(&mut self, col: i32, row: i32, ch: char) {
        self.put_colored(col, row, ch, None);
    }

    pub fn put_colored(&mut self, col: i32, row: i32, ch: char, color: Option<Color>) {
        if col >= 0 && row >= 0 {
            let (c, r) = (col as usize, row as usize);
            if c < self.width && r < self.height {
                self.chars[r][c] = ch;
                self.colors[r][c] = color;
            }
        }
    }

    pub fn put_text(&mut self, col: i32, row: i32, text: &str) {
        self.put_text_colored(col, row, text, None);
    }

    pub fn put_text_colored(&mut self, col: i32, row: i32, text: &str, color: Option<Color>) {
        for (i, ch) in text.chars().enumerate() {
            self.put_colored(col + i as i32, row, ch, color);
        }
    }
}

/// 2×4 dots per terminal cell (U+2800–U+28FF).
#[derive(Clone, Debug)]
pub struct BrailleCanvas {
    cols: usize,
    rows: usize,
    bits: Vec<Vec<u8>>,
}

impl BrailleCanvas {
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            bits: vec![vec![0; cols]; rows],
        }
    }

    pub fn set_pixel(&mut self, px: i32, py: i32) {
        if px < 0 || py < 0 {
            return;
        }
        let col = px as usize / 2;
        let row = py as usize / 4;
        if col >= self.cols || row >= self.rows {
            return;
        }
        let bit = DOT_BITS[py as usize % 4][px as usize % 2];
        self.bits[row][col] |= 1 << bit;
    }

    /// Overlay `text` on braille (or the ASCII density ramp).
    #[must_use]
    #[allow(dead_code)] // plain-string path; styled maps call `to_styled_lines`
    pub fn to_lines(&self, text: &CharCanvas, ascii: bool) -> Vec<String> {
        self.to_styled_lines(text, ascii, Color::DarkGray)
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|s| s.content.into_owned())
                    .collect::<String>()
            })
            .collect()
    }

    /// Same geometry as [`Self::to_lines`], with per-cell label colors.
    #[must_use]
    pub fn to_styled_lines(
        &self,
        text: &CharCanvas,
        ascii: bool,
        braille_fg: Color,
    ) -> Vec<Line<'static>> {
        let rows = self.rows.min(text.height);
        let cols = self.cols.min(text.width);
        let mut lines = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut run = String::new();
            let mut run_style = Style::default().fg(braille_fg);
            let flush = |spans: &mut Vec<Span<'static>>, run: &mut String, style: Style| {
                if !run.is_empty() {
                    spans.push(Span::styled(std::mem::take(run), style));
                }
            };
            for col in 0..cols {
                let tc = text.chars[row][col];
                let (ch, style) = if tc != ' ' {
                    let fg = text.colors[row][col].unwrap_or(Color::White);
                    (tc, Style::default().fg(fg))
                } else {
                    let bits = self.bits[row][col];
                    let ch = if bits == 0 {
                        ' '
                    } else if ascii {
                        let pop = bits.count_ones() as usize;
                        ASCII_RAMP[pop.min(8)] as char
                    } else {
                        char::from_u32(0x2800 + u32::from(bits)).unwrap_or(' ')
                    };
                    (ch, Style::default().fg(braille_fg))
                };
                if run.is_empty() {
                    run.push(ch);
                    run_style = style;
                } else if style == run_style {
                    run.push(ch);
                } else {
                    flush(&mut spans, &mut run, run_style);
                    run.push(ch);
                    run_style = style;
                }
            }
            flush(&mut spans, &mut run, run_style);
            lines.push(Line::from(spans));
        }
        lines
    }
}

/// True if any cell is a Unicode braille glyph.
#[must_use]
pub fn lines_contain_braille(lines: &[String]) -> bool {
    lines
        .iter()
        .any(|l| l.chars().any(|c| ('\u{2800}'..='\u{28FF}').contains(&c)))
}
