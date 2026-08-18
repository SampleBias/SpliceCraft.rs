//! `Record` → map lines. Geometry is unit-tested without a tty.

use ratatui::style::Color;
use ratatui::text::Line;
use splicecraft_bio::{
    CustomEnzyme, RestrictionHit, ScanOptions, feat_decorated_label, scan_restriction_sites,
};
use splicecraft_core::{Feature, Record, wrap_midpoint};
use splicecraft_io::{AlignState, render_alignment_bar};

use crate::theme::{BRAILLE_FG, ENZYME_ACCENT, TEXT, feature_paint_color};

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

/// Render `record` to terminal rows (plain text; geometry tests).
#[must_use]
pub fn render_map(record: &Record, opt: &MapOptions) -> Vec<String> {
    render_map_styled(record, opt)
        .into_iter()
        .map(|line| {
            line.spans
                .into_iter()
                .map(|s| s.content.into_owned())
                .collect()
        })
        .collect()
}

/// Same geometry as [`render_map`], with feature / enzyme label colors.
#[must_use]
pub fn render_map_styled(record: &Record, opt: &MapOptions) -> Vec<Line<'static>> {
    let w = opt.width.max(8);
    let h = opt.height.max(4);
    if opt.circular && record.circular {
        render_circular(record, opt, w, h)
    } else {
        render_linear(record, opt, w, h)
    }
}

/// Terminal cell aspect: cells are ~2× wider than tall (upstream `_aspect`).
const MAP_ASPECT: f64 = 2.0;
/// Feature band in cell-radial units: +strand `1..3`, −strand `-3..-1`.
const BAND_DR: f64 = 3.0;

fn render_circular(record: &Record, opt: &MapOptions, w: usize, h: usize) -> Vec<Line<'static>> {
    let mut dots = BrailleCanvas::new(w, h);
    let mut text = CharCanvas::new(w, h);
    let n = record.len().max(1);
    let geom = CircleGeom::new(w, h);
    let scale = geom.scale();
    // Keep a packed 2–3 cell band on large maps; on short panes shrink
    // just enough that the centre hole still fits name / bp / origin.
    let band = BAND_DR.min(geom.ry * 0.55).max(2.0);

    // Raster-fill every braille dot in the thick annulus (upstream paints
    // a 2-cell feature band + backbone; sampling a 1-px stroke looked empty).
    let px_max = (w * 2) as i32;
    let py_max = (h * 4) as i32;
    for py in 0..py_max {
        for px in 0..px_max {
            let dx = f64::from(px) / 2.0 - geom.cx;
            let dy = f64::from(py) / 4.0 - geom.cy;
            let r_ell = dx.hypot(dy / scale);
            let dr = r_ell - geom.rx;
            if dr.abs() > band {
                continue;
            }
            let bp = bp_at_angle(ellipse_angle(dx, dy, scale), n, opt.origin);
            let color = annulus_color(record, bp, dr >= 0.0);
            dots.set_pixel_colored(px, py, Some(color));
        }
    }

    // Inner ┼ ticks + bp scale (upstream `TICK_DR_MARK = -2`, label at -5).
    if n >= 1 && w >= 24 && h >= 8 {
        let tick = nice_tick(n);
        let mut bp_i = 0usize;
        let label_ticks = h >= 14;
        while bp_i < n {
            let ang = bp_to_angle(bp_i, n, opt.origin);
            let (mx, my) = geom.xy(ang, -2.0);
            text.put_colored(mx, my, '┼', Some(Color::White));
            if label_ticks {
                let label = format_bp(bp_i);
                let (lx, ly) = geom.xy(ang, -5.0);
                let lx = if ang.cos() >= 0.0 {
                    lx - (label.chars().count() as i32) + 1
                } else {
                    lx
                };
                text.put_text_colored(lx, ly, &label, Some(TEXT));
            }
            bp_i += tick;
        }
    }

    for feat in record.features.iter().filter(|f| f.kind != "source") {
        let tip = if feat.strand < 0 {
            feat.start
        } else if feat.end == 0 {
            n.saturating_sub(1)
        } else {
            (feat.end + n - 1) % n
        };
        let ang = bp_to_angle(tip, n, opt.origin);
        let dr = if feat.strand < 0 { -2.0 } else { 2.0 };
        let (col, row) = geom.xy(ang, dr);
        text.put_colored(
            col,
            row,
            arrow_char(ang, feat.strand),
            Some(feature_paint_color(feat)),
        );
    }

    place_outer_labels(&mut text, record, opt, n, &geom, band);

    if w >= 40 && h >= 12 {
        let hint = "[ v = linear ]";
        text.put_text_colored(
            (w.saturating_sub(hint.len() + 1)) as i32,
            0,
            hint,
            Some(TEXT),
        );
    }

    let name_cap = (w / 3).max(1);
    let name = if record.name.chars().count() > name_cap {
        let mut s: String = record
            .name
            .chars()
            .take(name_cap.saturating_sub(1))
            .collect();
        s.push('…');
        s
    } else {
        record.name.clone()
    };
    let size_txt = format!("{} bp", comma_int(record.len()));
    let orig_txt = format!("▲ {}", comma_int(opt.origin));
    let cx_i = geom.cx.round() as i32;
    let cy_i = geom.cy.round() as i32;
    text.put_text_colored(
        cx_i - (name.chars().count() as i32) / 2,
        cy_i - 1,
        &name,
        Some(Color::White),
    );
    text.put_text_colored(
        cx_i - (size_txt.chars().count() as i32) / 2,
        cy_i,
        &size_txt,
        Some(TEXT),
    );
    text.put_text_colored(
        cx_i - (orig_txt.chars().count() as i32) / 2,
        cy_i + 1,
        &orig_txt,
        Some(crate::theme::PRIMARY),
    );
    dots.to_styled_lines(&text, opt.ascii, BRAILLE_FG)
}

fn render_linear(record: &Record, opt: &MapOptions, w: usize, h: usize) -> Vec<Line<'static>> {
    let mut dots = BrailleCanvas::new(w, h);
    let mut text = CharCanvas::new(w, h);
    let n = record.len().max(1);
    let y = ((h * 4) / 2) as i32;
    let x0 = 2;
    let x1 = (w * 2).saturating_sub(2) as i32;
    for px in x0..x1 {
        let frac = (px - x0) as f64 / (x1 - x0).max(1) as f64;
        let bp = ((frac * n as f64) as usize) % n;
        let color = backbone_color(record, bp);
        for dy in -2..=2 {
            dots.set_pixel_colored(px, y + dy, Some(color));
        }
    }
    for feat in record.features.iter().filter(|f| f.kind != "source") {
        let (a, b) = linear_span(feat.start, feat.end, n, x0, x1, opt.origin);
        let color = feature_paint_color(feat);
        for px in a..b {
            dots.set_pixel_colored(px, y - 5, Some(color));
            dots.set_pixel_colored(px, y - 6, Some(color));
        }
        if opt.show_labels && !feat.label.is_empty() {
            let mid = feature_label_bp(feat, n);
            let col = linear_x(mid, n, x0, x1, opt.origin) / 2;
            text.put_text_colored(col, (y / 4) - 2, &truncate(&feat.label, 8), Some(color));
        }
    }
    if opt.show_restr {
        for hit in labeled_resites(record, opt) {
            let px = linear_x(hit.start, n, x0, x1, opt.origin);
            dots.set_pixel_colored(px, y + 3, Some(ENZYME_ACCENT));
            text.put_text_colored(
                px / 2,
                (y / 4) + 1,
                &truncate(&feat_decorated_label(&hit.label, hit.cut_count), 8),
                Some(ENZYME_ACCENT),
            );
        }
    }
    text.put_text_colored(
        1,
        0,
        &truncate(&record.name, w.saturating_sub(8)),
        Some(Color::White),
    );
    text.put_text_colored(
        (w.saturating_sub(8)) as i32,
        0,
        &format!("{} bp", record.len()),
        Some(TEXT),
    );
    if !opt.align_segments.is_empty() {
        let bar = render_alignment_bar(&opt.align_segments, n, w);
        text.put_text(0, h.saturating_sub(1) as i32, &bar);
    }
    dots.to_styled_lines(&text, opt.ascii, BRAILLE_FG)
}

struct CircleGeom {
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    h: usize,
}

impl CircleGeom {
    fn new(w: usize, h: usize) -> Self {
        let cx = (w / 2) as f64;
        let cy = (h / 2) as f64;
        let rx_from_w = cx - 16.0;
        let rx_from_h = (cy - 3.0) * MAP_ASPECT;
        let rx = rx_from_w.min(rx_from_h).max(8.0);
        let ry = (rx / MAP_ASPECT).round().max(4.0);
        Self { cx, cy, rx, ry, h }
    }

    fn scale(&self) -> f64 {
        if self.rx == 0.0 {
            0.5
        } else {
            self.ry / self.rx
        }
    }

    fn xy(&self, angle: f64, dr: f64) -> (i32, i32) {
        let scale = self.scale();
        let x = (self.cx + (self.rx + dr) * angle.cos()).round() as i32;
        let y = (self.cy + (self.ry + dr * scale) * angle.sin()).round() as i32;
        (x, y)
    }
}

fn backbone_color(record: &Record, bp: usize) -> Color {
    record
        .features
        .iter()
        .filter(|f| f.kind != "source" && f.contains_bp(bp))
        .min_by_key(|f| f.len_on(record.len()))
        .map(feature_paint_color)
        .unwrap_or(BRAILLE_FG)
}

fn annulus_color(record: &Record, bp: usize, outer: bool) -> Color {
    let n = record.len();
    let mut best_pref: Option<&Feature> = None;
    let mut best_any: Option<&Feature> = None;
    for f in &record.features {
        if f.kind == "source" || !f.contains_bp(bp) {
            continue;
        }
        if best_any.is_none_or(|c| f.len_on(n) < c.len_on(n)) {
            best_any = Some(f);
        }
        let prefer = if outer { f.strand >= 0 } else { f.strand < 0 };
        if prefer && best_pref.is_none_or(|c| f.len_on(n) < c.len_on(n)) {
            best_pref = Some(f);
        }
    }
    best_pref
        .or(best_any)
        .map(feature_paint_color)
        .unwrap_or(BRAILLE_FG)
}

fn ellipse_angle(dx: f64, dy: f64, scale: f64) -> f64 {
    (dy / scale).atan2(dx)
}

fn bp_to_angle(bp: usize, total: usize, origin: usize) -> f64 {
    let frac = ((bp + total - origin % total) % total) as f64 / total as f64;
    frac * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2
}

fn bp_at_angle(ang: f64, total: usize, origin: usize) -> usize {
    let frac = ((ang + std::f64::consts::FRAC_PI_2) / std::f64::consts::TAU).rem_euclid(1.0);
    let bp = ((frac * total as f64).floor() as usize).min(total.saturating_sub(1));
    (bp + origin) % total
}

fn arrow_char(angle: f64, strand: i8) -> char {
    let tan = if strand < 0 {
        angle - std::f64::consts::FRAC_PI_2
    } else {
        angle + std::f64::consts::FRAC_PI_2
    };
    let t = tan.rem_euclid(std::f64::consts::TAU);
    let sector = ((t + std::f64::consts::PI / 4.0) / std::f64::consts::FRAC_PI_2).floor() as i32;
    const CHARS: [char; 4] = ['▶', '▼', '◀', '▲'];
    CHARS[sector.rem_euclid(4) as usize]
}

fn nice_tick(total: usize) -> usize {
    for t in [
        50, 100, 200, 250, 500, 1000, 2000, 2500, 5000, 10000, 25000, 50000,
    ] {
        if (4..=14).contains(&(total / t)) {
            return t;
        }
    }
    (total / 8).max(1)
}

fn format_bp(bp: usize) -> String {
    if bp < 1000 {
        return bp.to_string();
    }
    if bp.is_multiple_of(1000) {
        format!("{}k", bp / 1000)
    } else if bp.is_multiple_of(100) {
        format!("{:.1}k", bp as f64 / 1000.0)
    } else {
        format!("{:.2}k", bp as f64 / 1000.0)
    }
}

fn comma_int(n: usize) -> String {
    let raw = n.to_string();
    let len = raw.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in raw.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn place_outer_labels(
    text: &mut CharCanvas,
    record: &Record,
    opt: &MapOptions,
    n: usize,
    geom: &CircleGeom,
    band: f64,
) {
    let mut slots: Vec<(f64, String, Color)> = Vec::new();
    if opt.show_labels {
        let mut feats: Vec<&Feature> = record
            .features
            .iter()
            .filter(|f| f.kind != "source" && !f.label.is_empty())
            .collect();
        feats.sort_by_key(|f| std::cmp::Reverse(f.len_on(n)));
        for feat in feats {
            let ang = bp_to_angle(feature_label_bp(feat, n), n, opt.origin);
            slots.push((ang, feat.label.clone(), feature_paint_color(feat)));
        }
    }
    if opt.show_restr {
        for hit in labeled_resites(record, opt) {
            let mid = (hit.start + hit.end) / 2;
            let ang = bp_to_angle(mid, n, opt.origin);
            slots.push((
                ang,
                feat_decorated_label(&hit.label, hit.cut_count),
                ENZYME_ACCENT,
            ));
        }
    }
    if slots.is_empty() {
        return;
    }

    let dr_min = (band.ceil() as i32).max(2);
    let dr_max = (geom.rx as i32 / 2 + 6).max(dr_min + 10);
    let h_i = geom.h as i32;
    let mut placed_by_row: Vec<Vec<(i32, i32)>> = vec![Vec::new(); geom.h];

    for (angle, lbl, color) in slots {
        let on_right = angle.cos() >= 0.0;
        let len = lbl.chars().count() as i32;
        let mut chosen: Option<(i32, i32, i32, i32)> = None; // lx, ly, x0, x1
        for dr in dr_min..=dr_max {
            let (lx, ly) = geom.xy(angle, f64::from(dr));
            if ly < 0 || ly >= h_i {
                continue;
            }
            let (x0, x1) = if on_right {
                (lx, lx + len - 1)
            } else {
                (lx - len + 1, lx)
            };
            let x0 = x0.max(0);
            let row = ly as usize;
            let ok = placed_by_row[row]
                .iter()
                .all(|&(bx0, bx1)| x1 < bx0 || x0 > bx1);
            if ok {
                chosen = Some((lx, ly, x0, x1));
                break;
            }
        }
        let (lx, ly, x0, x1) = chosen.unwrap_or_else(|| {
            let (lx, ly) = geom.xy(angle, f64::from(dr_max));
            let (x0, x1) = if on_right {
                (lx, lx + len - 1)
            } else {
                (lx - len + 1, lx)
            };
            (lx, ly, x0.max(0), x1)
        });
        if ly >= 0 && ly < h_i {
            placed_by_row[ly as usize].push((x0, x1));
        }
        let (dot_x, dot_y) = geom.xy(angle, band);
        text.put_colored(dot_x, dot_y, '·', Some(color));
        let put_x = if on_right { lx } else { x0 };
        text.put_text_colored(put_x, ly, &lbl, Some(color));
    }
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
