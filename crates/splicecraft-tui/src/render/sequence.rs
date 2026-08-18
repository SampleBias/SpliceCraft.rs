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

/// Styled sequence panel (feature-colored bases, green AA lane).
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
    let lane = feature_lane_styled(record, start, w);
    let top = bases_styled(record, start, &top_chars, false);
    let aa = aa_lane_styled(record, start, w);
    let bot_chars: Vec<char> = top_chars
        .iter()
        .map(|&c| if c == ' ' { ' ' } else { complement_base(c) })
        .collect();
    let bot = bases_styled(record, start, &bot_chars, false);
    let caret = caret_line(view.cursor, start, w);

    vec![
        Line::from(Span::styled(ruler, Style::default().fg(TEXT))),
        lane,
        top,
        aa,
        bot,
        caret,
    ]
}

fn bases_styled(record: &Record, start: usize, chars: &[char], _rev: bool) -> Line<'static> {
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

fn feature_lane_styled(record: &Record, start: usize, w: usize) -> Line<'static> {
    let n = record.len();
    let mut cells: Vec<(char, Color)> = vec![(' ', TEXT); w];
    for feat in record.features.iter().filter(|f| f.kind != "source") {
        let color = feature_paint_color(feat);
        for (i, cell) in cells.iter_mut().enumerate() {
            let bp = start + i;
            if bp < n && feat.contains_bp(bp) {
                *cell = (
                    if feat.kind.eq_ignore_ascii_case("CDS") {
                        '='
                    } else {
                        '-'
                    },
                    color,
                );
            }
        }
        if !feat.label.is_empty() {
            let mid = splicecraft_core::wrap_midpoint(feat.start, feat.end, n);
            if mid >= start && mid < start + w {
                for (k, ch) in feat.label.chars().take(6).enumerate() {
                    let idx = mid - start + k;
                    if idx < w {
                        cells[idx] = (ch, color);
                    }
                }
            }
        }
    }
    spans_from_cells(&cells)
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
    spans_from_cells(&cells)
}

fn caret_line(cursor: usize, start: usize, w: usize) -> Line<'static> {
    let mut cells: Vec<(char, Color)> = vec![(' ', TEXT); w];
    if cursor >= start && cursor < start + w {
        cells[cursor - start] = ('^', CARET);
    }
    spans_from_cells(&cells)
}

fn spans_from_cells(cells: &[(char, Color)]) -> Line<'static> {
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
            spans.push(Span::styled(
                std::mem::take(&mut run),
                Style::default().fg(run_fg),
            ));
            run.push(ch);
            run_fg = fg;
        }
    }
    if !run.is_empty() {
        let style = if run_fg == AA_GREEN {
            Style::default().fg(AA_GREEN).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(run_fg)
        };
        spans.push(Span::styled(run, style));
    }
    Line::from(spans)
}

fn codon_midpoint(
    feat: &splicecraft_core::Feature,
    total: usize,
    aa_index: usize,
) -> Option<usize> {
    let flen = feat.len_on(total);
    let base = aa_index * 3 + 1;
    if base >= flen {
        return None;
    }
    Some((feat.start + base) % total.max(1))
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
            aa.spans
                .iter()
                .any(|s| s.style.fg == Some(AA_GREEN)
                    && s.content.chars().any(|c| c.is_alphabetic())),
            "expected green AA span, got {aa:?}"
        );
    }
}
