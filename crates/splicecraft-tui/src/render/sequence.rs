//! Two-strand sequence panel. CDS letters sit on codon midpoints.

use splicecraft_bio::translate_cds;
use splicecraft_core::Record;

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
    let n = record.len();
    if n == 0 {
        return vec!["(empty)".into()];
    }
    let w = view.width.max(8).min(n.max(8));
    let start = view.window_start.min(n.saturating_sub(1));
    let mut top = String::with_capacity(w);
    for i in 0..w {
        let bp = start + i;
        if bp >= n {
            top.push(' ');
        } else {
            top.push(record.sequence.as_bytes()[bp] as char);
        }
    }
    let top_u = top.to_ascii_uppercase();
    // Column-aligned complement (upstream `_DNA_COMP_PRESERVE_CASE`), not `rc`
    // of the window — reversing would put the last base under the first.
    let bot_line: String = top_u
        .chars()
        .map(|c| if c == ' ' { ' ' } else { complement_base(c) })
        .collect();
    let mut caret = " ".repeat(w);
    if view.cursor >= start && view.cursor < start + w {
        caret.replace_range(view.cursor - start..view.cursor - start + 1, "^");
    }
    let lane = feature_lane(record, start, w);
    let aa = aa_lane(record, start, w);
    let ruler = format!("{start}..{}", (start + w).min(n));
    vec![ruler, lane, top_u, aa, bot_line, caret]
}

fn feature_lane(record: &Record, start: usize, w: usize) -> String {
    let n = record.len();
    let mut lane = vec![' '; w];
    for feat in record.features.iter().filter(|f| f.kind != "source") {
        for (i, cell) in lane.iter_mut().enumerate() {
            let bp = start + i;
            if bp < n && feat.contains_bp(bp) {
                *cell = if feat.kind.eq_ignore_ascii_case("CDS") {
                    '='
                } else {
                    '-'
                };
            }
        }
        if !feat.label.is_empty() {
            let mid = splicecraft_core::wrap_midpoint(feat.start, feat.end, n);
            if mid >= start && mid < start + w {
                for (k, ch) in feat.label.chars().take(6).enumerate() {
                    let idx = mid - start + k;
                    if idx < w {
                        lane[idx] = ch;
                    }
                }
            }
        }
    }
    lane.into_iter().collect()
}

fn aa_lane(record: &Record, start: usize, w: usize) -> String {
    let n = record.len();
    let mut lane = vec![' '; w];
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
                lane[bp - start] = ch;
            }
        }
    }
    lane.into_iter().collect()
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
        'D' => 'H',
        'H' => 'D',
        'V' => 'B',
        'N' => 'N',
        other => other,
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
