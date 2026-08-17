//! `Record` → map lines. Geometry is unit-tested without a tty.

use splicecraft_bio::{
    CustomEnzyme, RestrictionHit, ScanOptions, feat_decorated_label, scan_restriction_sites,
};
use splicecraft_core::{Feature, Record, wrap_midpoint};
use splicecraft_io::{AlignState, render_alignment_bar};

use super::canvas::{BrailleCanvas, CharCanvas};

/// How to paint a plasmid map.
#[derive(Clone, Debug)]
pub struct MapOptions {
    /// Terminal columns.
    pub width: usize,
    /// Terminal rows.
    pub height: usize,
    /// Circular ring vs linear backbone.
    pub circular: bool,
    /// Display rotation (bp that sits at 12 o'clock / left).
    pub origin: usize,
    /// Draw restriction ticks + labels.
    pub show_restr: bool,
    /// Draw feature names at [INV-05] midpoints.
    pub show_labels: bool,
    /// 7-bit density ramp instead of braille.
    pub ascii: bool,
    /// Keep only enzymes that cut once.
    pub unique_only: bool,
    /// Skip enzymes shorter than this (6+ filter).
    pub min_recognition_len: usize,
    /// Active collection; `None` scans the full catalog.
    pub allowed_enzymes: Option<Vec<String>>,
    /// User-defined enzymes merged into the scan.
    pub extra_enzymes: Vec<CustomEnzyme>,
    /// Pairwise overlay in target coordinates (linear map only).
    pub align_segments: Vec<(usize, usize, AlignState)>,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self {
            width: 48,
            height: 16,
            circular: true,
            origin: 0,
            show_restr: false,
            show_labels: true,
            ascii: false,
            unique_only: false,
            min_recognition_len: 6,
            allowed_enzymes: None,
            extra_enzymes: Vec::new(),
            align_segments: Vec::new(),
        }
    }
}

/// Label anchor in record coordinates. Always [`wrap_midpoint`], never
/// `(start + end) / 2`.
#[must_use]
pub fn feature_label_bp(feat: &Feature, total: usize) -> usize {
    wrap_midpoint(feat.start, feat.end, total)
}

/// Render `record` to terminal rows.
#[must_use]
pub fn render_map(record: &Record, opt: &MapOptions) -> Vec<String> {
    let w = opt.width.max(8);
    let h = opt.height.max(4);
    if opt.circular && record.circular {
        render_circular(record, opt, w, h)
    } else {
        render_linear(record, opt, w, h)
    }
}

fn render_circular(record: &Record, opt: &MapOptions, w: usize, h: usize) -> Vec<String> {
    let mut dots = BrailleCanvas::new(w, h);
    let mut text = CharCanvas::new(w, h);
    let n = record.len().max(1);
    let cx = (w * 2) as f64 / 2.0;
    let cy = (h * 4) as f64 / 2.0;
    let r_back = (cx.min(cy) * 0.72).max(4.0);
    let samples = (n.max(64) * 4).min(800);
    for i in 0..samples {
        let bp = (i * n) / samples;
        let (px, py) = polar(bp, n, opt.origin, cx, cy, r_back);
        dots.set_pixel(px, py);
    }
    let mut ring = 0.0;
    for feat in record.features.iter().filter(|f| f.kind != "source") {
        let r = r_back - 3.0 - ring;
        ring = (ring + 2.0) % 6.0;
        if r < 3.0 {
            continue;
        }
        let flen = feat.len_on(n).max(1);
        let steps = flen.min(400);
        for k in 0..steps {
            let bp = step_along(feat.start, feat.end, n, k, steps);
            let (px, py) = polar(bp, n, opt.origin, cx, cy, r);
            dots.set_pixel(px, py);
        }
        if opt.show_labels && !feat.label.is_empty() {
            let mid = feature_label_bp(feat, n);
            let (px, py) = polar(mid, n, opt.origin, cx, cy, r_back + 3.0);
            let col = px / 2;
            let row = py / 4;
            text.put_text(col, row, &truncate(&feat.label, 10));
        }
    }
    if opt.show_restr {
        for hit in labeled_resites(record, opt) {
            let (px, py) = polar(hit.start, n, opt.origin, cx, cy, r_back + 1.5);
            dots.set_pixel(px, py);
            text.put_text(
                px / 2,
                py / 4,
                &truncate(&feat_decorated_label(&hit.label, hit.cut_count), 7),
            );
        }
    }
    let name = truncate(&record.name, w.saturating_sub(2));
    let bp = format!("{} bp", record.len());
    let name_col = ((w.saturating_sub(name.len())) / 2) as i32;
    let bp_col = ((w.saturating_sub(bp.len())) / 2) as i32;
    let mid_row = (h / 2) as i32;
    text.put_text(name_col, mid_row.saturating_sub(1), &name);
    text.put_text(bp_col, mid_row, &bp);
    dots.to_lines(&text, opt.ascii)
}

fn render_linear(record: &Record, opt: &MapOptions, w: usize, h: usize) -> Vec<String> {
    let mut dots = BrailleCanvas::new(w, h);
    let mut text = CharCanvas::new(w, h);
    let n = record.len().max(1);
    let y = ((h * 4) / 2) as i32;
    let x0 = 2;
    let x1 = (w * 2).saturating_sub(2) as i32;
    for px in x0..x1 {
        dots.set_pixel(px, y);
    }
    for feat in record.features.iter().filter(|f| f.kind != "source") {
        let (a, b) = linear_span(feat.start, feat.end, n, x0, x1, opt.origin);
        for px in a..b {
            dots.set_pixel(px, y - 3);
        }
        if opt.show_labels && !feat.label.is_empty() {
            let mid = feature_label_bp(feat, n);
            let col = linear_x(mid, n, x0, x1, opt.origin) / 2;
            text.put_text(col, (y / 4) - 2, &truncate(&feat.label, 8));
        }
    }
    if opt.show_restr {
        for hit in labeled_resites(record, opt) {
            let px = linear_x(hit.start, n, x0, x1, opt.origin);
            dots.set_pixel(px, y + 2);
            text.put_text(
                px / 2,
                (y / 4) + 1,
                &truncate(&feat_decorated_label(&hit.label, hit.cut_count), 8),
            );
        }
    }
    text.put_text(1, 0, &truncate(&record.name, w.saturating_sub(8)));
    text.put_text(
        (w.saturating_sub(8)) as i32,
        0,
        &format!("{} bp", record.len()),
    );
    if !opt.align_segments.is_empty() {
        let bar = render_alignment_bar(&opt.align_segments, n, w);
        text.put_text(0, h.saturating_sub(1) as i32, &bar);
    }
    dots.to_lines(&text, opt.ascii)
}

fn labeled_resites(record: &Record, opt: &MapOptions) -> impl Iterator<Item = RestrictionHit> {
    scan_restriction_sites(
        &record.sequence,
        &ScanOptions {
            min_recognition_len: opt.min_recognition_len,
            unique_only: opt.unique_only,
            circular: record.circular,
            allowed_enzymes: opt.allowed_enzymes.clone(),
            extra_enzymes: opt.extra_enzymes.clone(),
        },
    )
    .into_iter()
    .filter(|h| h.is_resite() && !h.label.is_empty())
}

fn polar(bp: usize, total: usize, origin: usize, cx: f64, cy: f64, r: f64) -> (i32, i32) {
    let frac = ((bp + total - origin % total) % total) as f64 / total as f64;
    let ang = frac * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
    (
        (cx + r * ang.cos()).round() as i32,
        (cy + r * ang.sin()).round() as i32,
    )
}

fn step_along(start: usize, end: usize, total: usize, k: usize, steps: usize) -> usize {
    let flen = if end < start {
        (total - start) + end
    } else {
        end.saturating_sub(start)
    };
    let off = if steps <= 1 { 0 } else { (k * flen) / steps };
    (start + off) % total
}

fn linear_x(bp: usize, total: usize, x0: i32, x1: i32, origin: usize) -> i32 {
    let span = (x1 - x0).max(1) as f64;
    let frac = ((bp + total - origin % total) % total) as f64 / total as f64;
    x0 + (frac * span).round() as i32
}

fn linear_span(
    start: usize,
    end: usize,
    total: usize,
    x0: i32,
    x1: i32,
    origin: usize,
) -> (i32, i32) {
    if end < start {
        (linear_x(start, total, x0, x1, origin), x1)
    } else {
        let a = linear_x(start, total, x0, x1, origin);
        let b = linear_x(end, total, x0, x1, origin);
        (a.min(b), a.max(b).max(a + 1))
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max.max(1)).collect()
}
