//! Two-strand sequence panel. CDS letters sit on codon midpoints.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use splicecraft_bio::translate_cds;
use splicecraft_core::Record;

use crate::theme::{AA_GREEN, CARET, TEXT, feature_paint_color};

/// Sequence-panel view.
#[derive(Clone, Debug)]
pub struct SeqView {
    /// Columns of bases.
    pub width: usize,
    /// First visible base.
    pub window_start: usize,
    /// Cursor (0-based).
    pub cursor: usize,
}

/// Render top strand, column-aligned complement, feature lane, and CDS AAs.
#[must_use]
pub fn render_sequence(record: &Record, view: &SeqView) -> Vec<String> {
    render_sequence_styled(record, view)
        .into_iter()
        .map(|line| {
            line.spans
                .into_iter()
                .map(|s| s.content.into_owned())
                .collect()
        })
        .collect()
}

/// Styled sequence panel matching upstream density: arrows, AA, strands.
#[must_use]
pub fn render_sequence_styled(record: &Record, view: &SeqView) -> Vec<Line<'static>> {
    let n = record.len();
    if n == 0 {
        return vec![Line::from("(empty)")];
    }
    let w = view.width.max(8).min(n.max(8));
    let start = view.window_start.min(n.saturating_sub(1));

    let mut top_chars = Vec::with_capacity(w);
    for i in 0..w {
        let bp = start + i;
        if bp >= n {
            top_chars.push(' ');
        } else {
            top_chars.push((record.sequence.as_bytes()[bp] as char).to_ascii_uppercase());
        }
    }

    let ruler = format!("{start}..{}", (start + w).min(n));
    let arrows = feature_arrows_styled(record, start, w);
    let labels = feature_lane_styled(record, start, w);
    let aa = aa_lane_styled(record, start, w);
    let top = bases_styled(record, start, &top_chars);
    let bot_chars: Vec<char> = top_chars
        .iter()
        .map(|&c| if c == ' ' { ' ' } else { complement_base(c) })
        .collect();
    let bot = bases_styled(record, start, &bot_chars);
    let caret = caret_line(view.cursor, start, w);

    vec![
        Line::from(Span::styled(ruler, Style::default().fg(TEXT))),
        labels,
        arrows,
        aa,
        top,
        bot,
        caret,
    ]
}

fn bases_styled(record: &Record, start: usize, chars: &[char]) -> Line<'static> {
    let n = record.len();
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut run_fg = TEXT;
    for (i, &ch) in chars.iter().enumerate() {
        let bp = start + i;
        let fg = if ch == ' ' || bp >= n {
            TEXT
        } else {
            enclosing_feature_color(record, bp).unwrap_or(Color::White)
        };
        if run.is_empty() {
            run.push(ch);
            run_fg = fg;
        } else if fg == run_fg {
            run.push(ch);
        } else {
            spans.push(Span::styled(
                std::mem::take(&mut run),
                Style::default().fg(run_fg),
            ));
            run.push(ch);
            run_fg = fg;
        }
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, Style::default().fg(run_fg)));
    }
    Line::from(spans)
}

fn enclosing_feature_color(record: &Record, bp: usize) -> Option<Color> {
    record
        .features
        .iter()
        .filter(|f| f.kind != "source" && f.contains_bp(bp))
        .min_by_key(|f| f.len_on(record.len()))
        .map(feature_paint_color)
}

/// Thick colored feature bars with strand arrowheads (upstream sequence chrome).
fn feature_arrows_styled(record: &Record, start: usize, w: usize) -> Line<'static> {
    let n = record.len();
    let mut cells: Vec<(char, Color, bool)> = vec![(' ', TEXT, false); w];
    for feat in record.features.iter().filter(|f| f.kind != "source") {
        let color = feature_paint_color(feat);
        let mut first = None;
        let mut last = None;
        for (i, cell) in cells.iter_mut().enumerate() {
            let bp = start + i;
            if bp < n && feat.contains_bp(bp) {
                *cell = ('█', color, true);
                if first.is_none() {
                    first = Some(i);
                }
                last = Some(i);
            }
        }
        match (first, last, feat.strand) {
            (Some(a), Some(b), 1) if a <= b => cells[b] = ('▶', color, true),
            (Some(a), Some(_), -1) => cells[a] = ('◀', color, true),
            _ => {}
        }
    }
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut run_style = Style::default().fg(TEXT);
    for &(ch, fg, filled) in &cells {
        let style = if filled {
            Style::default().fg(fg).bg(fg)
        } else {
            Style::default().fg(TEXT)
        };
        // Arrowheads: bright fg on feature bg.
        let style = if ch == '▶' || ch == '◀' {
            Style::default()
                .fg(Color::Black)
                .bg(fg)
                .add_modifier(Modifier::BOLD)
        } else if filled {
            Style::default().fg(fg).bg(fg)
        } else {
            style
        };
        let paint_ch = if filled && ch == '█' { ' ' } else { ch };
        if run.is_empty() {
            run.push(paint_ch);
            run_style = style;
        } else if style == run_style {
            run.push(paint_ch);
        } else {
            spans.push(Span::styled(std::mem::take(&mut run), run_style));
            run.push(paint_ch);
            run_style = style;
        }
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, run_style));
    }
    Line::from(spans)
}

fn feature_lane_styled(record: &Record, start: usize, w: usize) -> Line<'static> {
    let n = record.len();
    let mut cells: Vec<(char, Color)> = vec![(' ', TEXT); w];
    for feat in record.features.iter().filter(|f| f.kind != "source") {
        let color = feature_paint_color(feat);
        if !feat.label.is_empty() {
            let mid = splicecraft_core::wrap_midpoint(feat.start, feat.end, n);
            if mid >= start && mid < start + w {
                for (k, ch) in feat.label.chars().take(10).enumerate() {
                    let idx = mid - start + k;
                    if idx < w {
                        cells[idx] = (ch, color);
                    }
                }
            }
        }
    }
    spans_from_cells(&cells, false)
}

fn aa_lane_styled(record: &Record, start: usize, w: usize) -> Line<'static> {
    let n = record.len();
    let mut cells: Vec<(char, Color)> = vec![(' ', TEXT); w];
    for feat in record
        .features
        .iter()
        .filter(|f| f.kind.eq_ignore_ascii_case("CDS"))
    {
        let cs = feat
            .qualifiers
            .get("codon_start")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(1);
        let aa = translate_cds(&record.sequence, feat.start, feat.end, feat.strand, cs);
        for (i, ch) in aa.chars().enumerate() {
            let codon_mid = codon_midpoint(feat, n, i);
            if let Some(bp) = codon_mid
                && bp >= start
                && bp < start + w
            {
                cells[bp - start] = (ch, AA_GREEN);
            }
        }
    }
    spans_from_cells(&cells, true)
}

fn caret_line(cursor: usize, start: usize, w: usize) -> Line<'static> {
    let mut cells: Vec<(char, Color)> = vec![(' ', TEXT); w];
    if cursor >= start && cursor < start + w {
        cells[cursor - start] = ('^', CARET);
    }
    spans_from_cells(&cells, false)
}

fn spans_from_cells(cells: &[(char, Color)], bold_green: bool) -> Line<'static> {
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut run_fg = TEXT;
    for &(ch, fg) in cells {
        if run.is_empty() {
            run.push(ch);
            run_fg = fg;
        } else if fg == run_fg {
            run.push(ch);
        } else {
            let style = cell_style(run_fg, bold_green);
            spans.push(Span::styled(std::mem::take(&mut run), style));
            run.push(ch);
            run_fg = fg;
        }
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, cell_style(run_fg, bold_green)));
    }
    Line::from(spans)
}

fn cell_style(fg: Color, bold_green: bool) -> Style {
    if bold_green && fg == AA_GREEN {
        Style::default().fg(AA_GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg)
    }
}

fn codon_midpoint(feat: &splicecraft_core::Feature, total: usize, codon_i: usize) -> Option<usize> {
    let flen = feat.len_on(total);
    let mid_off = codon_i * 3 + 1;
    if mid_off >= flen {
        return None;
    }
    Some((feat.start + mid_off) % total)
}

fn complement_base(c: char) -> char {
    match c.to_ascii_uppercase() {
        'A' => 'T',
        'T' | 'U' => 'A',
        'C' => 'G',
        'G' => 'C',
        'R' => 'Y',
        'Y' => 'R',
        'W' => 'W',
        'S' => 'S',
        'M' => 'K',
        'K' => 'M',
        'B' => 'V',
        'V' => 'B',
        'D' => 'H',
        'H' => 'D',
        'N' => 'N',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_core::{Feature, Record};

    #[test]
    fn aa_lane_uses_green_style() {
        let mut rec = Record::new("t", "ATGAAATAGAAA", true);
        rec.features.push(Feature::new("CDS", 0, 9, 1, "orf"));
        let lines = render_sequence_styled(
            &rec,
            &SeqView {
                width: 12,
                window_start: 0,
                cursor: 0,
            },
        );
        let aa = &lines[3];
        assert!(
            aa.spans.iter().any(|s| {
                s.style.fg == Some(AA_GREEN) && s.content.chars().any(|c| c.is_alphabetic())
            }),
            "expected green AA span, got {aa:?}"
        );
    }

    #[test]
    fn feature_arrows_paint_filled_span() {
        let mut rec = Record::new("t", "ATGAAATAGAAA", true);
        rec.features.push(Feature::new("promoter", 0, 6, 1, "p"));
        let lines = render_sequence_styled(
            &rec,
            &SeqView {
                width: 12,
                window_start: 0,
                cursor: 0,
            },
        );
        let arrows = &lines[2];
        assert!(
            arrows.spans.iter().any(|s| s.style.bg.is_some()),
            "expected colored feature bar background, got {arrows:?}"
        );
    }
}
