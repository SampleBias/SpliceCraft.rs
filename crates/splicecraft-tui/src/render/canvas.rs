//! Braille / ASCII sub-cell canvas. Port of upstream `_BrailleCanvas`.

const DOT_BITS: [[u8; 2]; 4] = [[0, 3], [1, 4], [2, 5], [6, 7]];
const ASCII_RAMP: &[u8] = b" .:-=+*#@";

/// Character overlay (labels, centre text) on top of braille dots.
#[derive(Clone, Debug)]
pub struct CharCanvas {
    width: usize,
    height: usize,
    chars: Vec<Vec<char>>,
}

impl CharCanvas {
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            chars: vec![vec![' '; width]; height],
        }
    }

    pub fn put(&mut self, col: i32, row: i32, ch: char) {
        if col >= 0 && row >= 0 {
            let (c, r) = (col as usize, row as usize);
            if c < self.width && r < self.height {
                self.chars[r][c] = ch;
            }
        }
    }

    pub fn put_text(&mut self, col: i32, row: i32, text: &str) {
        for (i, ch) in text.chars().enumerate() {
            self.put(col + i as i32, row, ch);
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
    pub fn to_lines(&self, text: &CharCanvas, ascii: bool) -> Vec<String> {
        let rows = self.rows.min(text.height);
        let cols = self.cols.min(text.width);
        let mut lines = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut s = String::with_capacity(cols);
            for col in 0..cols {
                let tc = text.chars[row][col];
                if tc != ' ' {
                    s.push(tc);
                } else {
                    let bits = self.bits[row][col];
                    if bits == 0 {
                        s.push(' ');
                    } else if ascii {
                        let pop = bits.count_ones() as usize;
                        s.push(ASCII_RAMP[pop.min(8)] as char);
                    } else {
                        s.push(char::from_u32(0x2800 + u32::from(bits)).unwrap_or(' '));
                    }
                }
            }
            lines.push(s);
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
