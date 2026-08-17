//! Publication plasmid-map export (SVG + PNG). Ports `splicecraft_mapimage`.
//!
//! Writes go to a **user-chosen path** via [`splicecraft_persist::atomic_write_bytes`]
//! — not the data-dir JSON chokepoint.

use std::io::Cursor;
use std::path::Path;

use image::{ImageFormat, Rgba, RgbaImage};
use splicecraft_bio::{ScanOptions, scan_restriction_sites};
use splicecraft_core::{Record, feat_len};
use splicecraft_persist::atomic_write_bytes;

/// Default square canvas (px / SVG viewBox).
pub const MAP_IMAGE_DEFAULT_SIZE: u32 = 1400;
/// Minimum export size.
pub const MAP_IMAGE_MIN_SIZE: u32 = 300;
/// Maximum export size (runaway-allocation guard).
pub const MAP_IMAGE_MAX_SIZE: u32 = 6000;
const MAP_MAX_SITE_TICKS: usize = 60;
const MAP_MAX_LANES: usize = 14;
const R_BACKBONE: f64 = 0.230;
const BAND: f64 = 0.046;
const LANE_STEP: f64 = 0.056;
const R_MIN_LANE: f64 = 0.088;
const ARROW_HEAD: f64 = 0.020;
const SITE_TICK: f64 = 0.018;
const BACKBONE_COLOR: (u8, u8, u8) = (0x5A, 0x64, 0x72);
const SITE_COLOR: (u8, u8, u8) = (0xFF, 0x69, 0xB4);
const TITLE_COLOR: (u8, u8, u8) = (0x1A, 0x1D, 0x24);
const PALETTE: &[(u8, u8, u8)] = &[
    (0x00, 0x87, 0xFF),
    (0x87, 0xD7, 0x00),
    (0xFF, 0x87, 0x00),
    (0xFF, 0x5F, 0xD7),
    (0x00, 0xD7, 0xD7),
    (0xD7, 0xAF, 0x00),
    (0xFF, 0x00, 0x00),
    (0x00, 0xD7, 0x00),
];

/// Export options.
#[derive(Clone, Debug)]
pub struct MapImageOpts {
    /// Square size in px (clamped 300–6000).
    pub size: u32,
    /// Transparent canvas (SVG omits the backing rect; PNG uses alpha 0).
    pub transparent: bool,
    /// Draw feature names.
    pub show_labels: bool,
    /// Draw restriction ticks.
    pub show_sites: bool,
    /// Title override (defaults to the record name).
    pub title: String,
    /// Display origin in bp.
    pub origin_bp: usize,
}

impl Default for MapImageOpts {
    fn default() -> Self {
        Self {
            size: MAP_IMAGE_DEFAULT_SIZE,
            transparent: false,
            show_labels: true,
            show_sites: true,
            title: String::new(),
            origin_bp: 0,
        }
    }
}

/// Summary returned by [`export_plasmid_map`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapExportReport {
    /// Destination path.
    pub path: std::path::PathBuf,
    /// `png` or `svg`.
    pub fmt: String,
    /// Backbone length.
    pub bp: usize,
    /// Feature count drawn.
    pub features: usize,
    /// Bytes written.
    pub bytes: usize,
}

/// Clamp size to the publication range.
#[must_use]
pub fn clamp_size(size: u32) -> u32 {
    size.clamp(MAP_IMAGE_MIN_SIZE, MAP_IMAGE_MAX_SIZE)
}

/// Render SVG (UTF-8). Always includes an XML declaration and the plasmid name.
#[must_use]
pub fn render_plasmid_map_svg(record: &Record, opts: &MapImageOpts) -> String {
    let prim = build_primitives(record, opts);
    emit_svg(&prim)
}

/// Render PNG bytes.
pub fn render_plasmid_map_png(record: &Record, opts: &MapImageOpts) -> Result<Vec<u8>, String> {
    let prim = build_primitives(record, opts);
    emit_png(&prim)
}

/// Render encoded bytes (`png` or `svg`).
pub fn render_map_bytes(
    record: &Record,
    fmt: &str,
    opts: &MapImageOpts,
) -> Result<Vec<u8>, String> {
    match fmt.to_ascii_lowercase().as_str() {
        "svg" => Ok(render_plasmid_map_svg(record, opts).into_bytes()),
        "png" => render_plasmid_map_png(record, opts),
        other => Err(format!(
            "unknown map image format {other:?}; choose png or svg"
        )),
    }
}

/// Atomically write a map to a user-chosen path (not the data-dir chokepoint).
pub fn export_plasmid_map(
    record: &Record,
    dest: &Path,
    fmt: &str,
    opts: &MapImageOpts,
) -> Result<MapExportReport, String> {
    let fmt = fmt.to_ascii_lowercase();
    let data = render_map_bytes(record, &fmt, opts)?;
    atomic_write_bytes(dest, &data).map_err(|e| e.to_string())?;
    Ok(MapExportReport {
        path: dest.to_path_buf(),
        fmt,
        bp: record.len(),
        features: record.features.len(),
        bytes: data.len(),
    })
}

/// Bulk-export each record as `{stem}.{fmt}` under `dest_dir`.
pub fn export_plasmid_maps(
    records: &[Record],
    dest_dir: &Path,
    fmt: &str,
    opts: &MapImageOpts,
) -> Result<Vec<MapExportReport>, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for rec in records {
        let stem = sanitize_stem(&rec.name);
        let path = dest_dir.join(format!("{stem}.{fmt}"));
        out.push(export_plasmid_map(rec, &path, fmt, opts)?);
    }
    Ok(out)
}

/// True when `svg` is a well-formed XML document with a single `svg` root.
#[must_use]
pub fn svg_is_well_formed(svg: &str) -> bool {
    let trimmed = svg.trim_start();
    if !trimmed.starts_with("<?xml") && !trimmed.starts_with("<svg") {
        return false;
    }
    xml_well_formed(svg)
}

fn sanitize_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "plasmid".into() } else { s }
}

struct MapFeat {
    start: usize,
    end: usize,
    strand: i8,
    label: String,
    color: (u8, u8, u8),
}

struct MapSite {
    start: usize,
    label: String,
}

struct Circle {
    cx: f64,
    cy: f64,
    r: f64,
    color: (u8, u8, u8),
    width: f64,
}

struct Poly {
    pts: Vec<(f64, f64)>,
    fill: (u8, u8, u8),
}

struct Seg {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    color: (u8, u8, u8),
    width: f64,
}

struct Label {
    x: f64,
    y: f64,
    text: String,
    color: (u8, u8, u8),
    size: f64,
}

struct Primitives {
    size: u32,
    title: String,
    transparent: bool,
    circles: Vec<Circle>,
    polys: Vec<Poly>,
    lines: Vec<Seg>,
    texts: Vec<Label>,
}

fn map_feats_from_record(record: &Record) -> Vec<MapFeat> {
    record
        .features
        .iter()
        .enumerate()
        .filter(|(_, f)| f.kind != "resite" && f.kind != "recut")
        .map(|(i, f)| MapFeat {
            start: f.start,
            end: f.end,
            strand: f.strand,
            label: if f.label.is_empty() {
                f.kind.clone()
            } else {
                f.label.clone()
            },
            color: PALETTE[i % PALETTE.len()],
        })
        .collect()
}

fn map_sites_from_record(record: &Record) -> Vec<MapSite> {
    let hits = scan_restriction_sites(&record.sequence, &ScanOptions::default());
    let mut sites = Vec::new();
    for h in hits {
        if h.label.is_empty() {
            continue;
        }
        if sites.len() >= MAP_MAX_SITE_TICKS {
            break;
        }
        sites.push(MapSite {
            start: h.start,
            label: h.label,
        });
    }
    sites
}

fn angle(bp: f64, total: usize, origin_bp: usize) -> f64 {
    if total == 0 {
        return -std::f64::consts::FRAC_PI_2;
    }
    let t = total as f64;
    let rel = (bp - origin_bp as f64).rem_euclid(t);
    2.0 * std::f64::consts::PI * (rel / t) - std::f64::consts::FRAC_PI_2
}

fn in_arc(x: usize, start: usize, length: usize, total: usize) -> bool {
    if total == 0 || length == 0 {
        return false;
    }
    (x + total - start) % total < length
}

fn assign_lanes(feats: &[MapFeat], total: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..feats.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(feat_len(feats[i].start, feats[i].end, total)));
    let mut lanes: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut result = vec![0usize; feats.len()];
    for i in order {
        let s = feats[i].start;
        let ln = feat_len(s, feats[i].end, total);
        let mut placed = false;
        for (lane_idx, occ) in lanes.iter_mut().enumerate() {
            if occ
                .iter()
                .all(|&(os, ol)| !in_arc(s, os, ol, total) && !in_arc(os, s, ln, total))
            {
                occ.push((s, ln));
                result[i] = lane_idx;
                placed = true;
                break;
            }
        }
        if !placed {
            if lanes.len() >= MAP_MAX_LANES {
                if let Some(last) = lanes.last_mut() {
                    last.push((s, ln));
                    result[i] = lanes.len() - 1;
                }
            } else {
                lanes.push(vec![(s, ln)]);
                result[i] = lanes.len() - 1;
            }
        }
    }
    result
}

fn lane_radius(lane: usize, size: f64) -> f64 {
    (R_BACKBONE - lane as f64 * LANE_STEP).max(R_MIN_LANE) * size
}

fn arc_points(cx: f64, cy: f64, r: f64, a0: f64, a1: f64, steps: usize) -> Vec<(f64, f64)> {
    let steps = steps.max(1);
    (0..=steps)
        .map(|k| {
            let a = a0 + (a1 - a0) * (k as f64 / steps as f64);
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn band_polygon(
    cx: f64,
    cy: f64,
    r_mid: f64,
    band: f64,
    a0: f64,
    a1: f64,
    strand: i8,
) -> Vec<(f64, f64)> {
    let r_out = r_mid + band / 2.0;
    let r_in = r_mid - band / 2.0;
    let span = a1 - a0;
    let steps = ((span / (2.0 * std::f64::consts::PI) * 240.0) as usize).max(2);
    let head = if strand != 0 {
        ARROW_HEAD.min(span * 0.5)
    } else {
        0.0
    };
    if strand < 0 {
        let tip = (cx + r_mid * a0.cos(), cy + r_mid * a0.sin());
        let mut pts = vec![tip];
        pts.extend(arc_points(cx, cy, r_out, a0 + head, a1, steps));
        pts.extend(
            arc_points(cx, cy, r_in, a0 + head, a1, steps)
                .into_iter()
                .rev(),
        );
        pts
    } else if strand > 0 {
        let tip = (cx + r_mid * a1.cos(), cy + r_mid * a1.sin());
        let mut pts = arc_points(cx, cy, r_out, a0, a1 - head, steps);
        pts.push(tip);
        pts.extend(
            arc_points(cx, cy, r_in, a0, a1 - head, steps)
                .into_iter()
                .rev(),
        );
        pts
    } else {
        let mut pts = arc_points(cx, cy, r_out, a0, a1, steps);
        pts.extend(arc_points(cx, cy, r_in, a0, a1, steps).into_iter().rev());
        pts
    }
}

fn build_primitives(record: &Record, opts: &MapImageOpts) -> Primitives {
    let size = clamp_size(opts.size);
    let s = f64::from(size);
    let cx = s / 2.0;
    let cy = s / 2.0;
    let total = record.len();
    let title = if opts.title.is_empty() {
        record.name.clone()
    } else {
        opts.title.clone()
    };
    let feats = map_feats_from_record(record);
    let sites = if opts.show_sites {
        map_sites_from_record(record)
    } else {
        Vec::new()
    };

    let mut prim = Primitives {
        size,
        title,
        transparent: opts.transparent,
        circles: vec![Circle {
            cx,
            cy,
            r: R_BACKBONE * s,
            color: BACKBONE_COLOR,
            width: (s * 0.0022).max(1.5),
        }],
        polys: Vec::new(),
        lines: Vec::new(),
        texts: Vec::new(),
    };

    if total > 0 && !feats.is_empty() {
        let lanes = assign_lanes(&feats, total);
        for (i, f) in feats.iter().enumerate() {
            let arclen = feat_len(f.start, f.end, total);
            if arclen == 0 {
                continue;
            }
            let a0 = angle(f.start as f64, total, opts.origin_bp);
            let a1 = a0 + 2.0 * std::f64::consts::PI * (arclen as f64 / total as f64);
            let r_mid = lane_radius(lanes[i], s);
            prim.polys.push(Poly {
                pts: band_polygon(cx, cy, r_mid, BAND * s, a0, a1, f.strand),
                fill: f.color,
            });
            if opts.show_labels && !f.label.is_empty() {
                let a_mid = a0 + (a1 - a0) / 2.0;
                let r_lab = r_mid;
                prim.texts.push(Label {
                    x: cx + r_lab * a_mid.cos(),
                    y: cy + r_lab * a_mid.sin(),
                    text: truncate_label(&f.label, 28),
                    color: TITLE_COLOR,
                    size: s * 0.014,
                });
            }
        }
    }

    for site in &sites {
        let a = angle(site.start as f64, total, opts.origin_bp);
        let r0 = R_BACKBONE * s;
        let r1 = r0 + SITE_TICK * s;
        prim.lines.push(Seg {
            x1: cx + r0 * a.cos(),
            y1: cy + r0 * a.sin(),
            x2: cx + r1 * a.cos(),
            y2: cy + r1 * a.sin(),
            color: SITE_COLOR,
            width: (s * 0.0016).max(1.0),
        });
        if opts.show_labels {
            prim.texts.push(Label {
                x: cx + (r1 + s * 0.012) * a.cos(),
                y: cy + (r1 + s * 0.012) * a.sin(),
                text: site.label.clone(),
                color: SITE_COLOR,
                size: s * 0.011,
            });
        }
    }

    prim.texts.push(Label {
        x: cx,
        y: cy - s * 0.012,
        text: prim.title.clone(),
        color: TITLE_COLOR,
        size: s * 0.028,
    });
    prim.texts.push(Label {
        x: cx,
        y: cy + s * 0.022,
        text: format!("{} bp", total),
        color: BACKBONE_COLOR,
        size: s * 0.016,
    });
    prim
}

fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn hex(rgb: (u8, u8, u8)) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb.0, rgb.1, rgb.2)
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn emit_svg(prim: &Primitives) -> String {
    let s = prim.size;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{s}\" height=\"{s}\" viewBox=\"0 0 {s} {s}\">\n"
    ));
    if !prim.transparent {
        out.push_str(&format!(
            "  <rect width=\"{s}\" height=\"{s}\" fill=\"#FFFFFF\"/>\n"
        ));
    }
    for c in &prim.circles {
        out.push_str(&format!(
            "  <circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
            c.cx, c.cy, c.r, hex(c.color), c.width
        ));
    }
    for p in &prim.polys {
        let pts: String = p
            .pts
            .iter()
            .map(|(x, y)| format!("{x:.2},{y:.2}"))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "  <polygon points=\"{pts}\" fill=\"{}\" stroke=\"#00000022\" stroke-width=\"0.6\"/>\n",
            hex(p.fill)
        ));
    }
    for l in &prim.lines {
        out.push_str(&format!(
            "  <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
            l.x1, l.y1, l.x2, l.y2, hex(l.color), l.width
        ));
    }
    for t in &prim.texts {
        out.push_str(&format!(
            "  <text x=\"{:.2}\" y=\"{:.2}\" fill=\"{}\" font-size=\"{:.2}\" text-anchor=\"middle\" font-family=\"sans-serif\">{}</text>\n",
            t.x, t.y, hex(t.color), t.size, xml_escape(&t.text)
        ));
    }
    out.push_str("</svg>\n");
    out
}

fn emit_png(prim: &Primitives) -> Result<Vec<u8>, String> {
    let s = prim.size;
    let mut img = RgbaImage::new(s, s);
    let bg = if prim.transparent {
        Rgba([0, 0, 0, 0])
    } else {
        Rgba([255, 255, 255, 255])
    };
    for px in img.pixels_mut() {
        *px = bg;
    }
    for p in &prim.polys {
        fill_polygon(&mut img, &p.pts, Rgba([p.fill.0, p.fill.1, p.fill.2, 255]));
    }
    for c in &prim.circles {
        stroke_circle(
            &mut img,
            c.cx,
            c.cy,
            c.r,
            c.width,
            Rgba([c.color.0, c.color.1, c.color.2, 255]),
        );
    }
    for l in &prim.lines {
        stroke_line(
            &mut img,
            l.x1,
            l.y1,
            l.x2,
            l.y2,
            l.width,
            Rgba([l.color.0, l.color.1, l.color.2, 255]),
        );
    }
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn put(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, c);
    }
}

fn stroke_circle(img: &mut RgbaImage, cx: f64, cy: f64, r: f64, width: f64, c: Rgba<u8>) {
    let steps = ((2.0 * std::f64::consts::PI * r) as usize).max(64);
    for k in 0..steps {
        let a = 2.0 * std::f64::consts::PI * (k as f64 / steps as f64);
        let x = cx + r * a.cos();
        let y = cy + r * a.sin();
        let rad = (width / 2.0).max(0.5);
        let ir = rad.ceil() as i32;
        for dy in -ir..=ir {
            for dx in -ir..=ir {
                if f64::from(dx * dx + dy * dy) <= rad * rad {
                    put(img, x as i32 + dx, y as i32 + dy, c);
                }
            }
        }
    }
}

fn stroke_line(img: &mut RgbaImage, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, c: Rgba<u8>) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let n = ((dx.hypot(dy)) as usize).max(1);
    let rad = (width / 2.0).max(0.5);
    let ir = rad.ceil() as i32;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let x = x1 + dx * t;
        let y = y1 + dy * t;
        for oy in -ir..=ir {
            for ox in -ir..=ir {
                if f64::from(ox * ox + oy * oy) <= rad * rad {
                    put(img, x as i32 + ox, y as i32 + oy, c);
                }
            }
        }
    }
}

fn fill_polygon(img: &mut RgbaImage, pts: &[(f64, f64)], c: Rgba<u8>) {
    if pts.len() < 3 {
        return;
    }
    let min_y = pts
        .iter()
        .map(|p| p.1)
        .fold(f64::INFINITY, f64::min)
        .floor() as i32;
    let max_y = pts
        .iter()
        .map(|p| p.1)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil() as i32;
    let h = img.height() as i32;
    for y in min_y.max(0)..=max_y.min(h - 1) {
        let mut xs = Vec::new();
        let yf = f64::from(y) + 0.5;
        for i in 0..pts.len() {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % pts.len()];
            if (y1 <= yf && y2 > yf) || (y2 <= yf && y1 > yf) {
                let t = (yf - y1) / (y2 - y1);
                xs.push(x1 + t * (x2 - x1));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks(2) {
            if pair.len() < 2 {
                continue;
            }
            let x0 = pair[0].floor() as i32;
            let x1 = pair[1].ceil() as i32;
            for x in x0..=x1 {
                put(img, x, y, c);
            }
        }
    }
}

fn xml_well_formed(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut stack: Vec<String> = Vec::new();
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"<?") {
            if let Some(end) = find(bytes, i + 2, b"?>") {
                i = end + 2;
                continue;
            }
            return false;
        }
        if bytes[i..].starts_with(b"<!--") {
            if let Some(end) = find(bytes, i + 4, b"-->") {
                i = end + 3;
                continue;
            }
            return false;
        }
        if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let end = match find(bytes, i + 2, b">") {
                Some(e) => e,
                None => return false,
            };
            let name = tag_name(&s[i + 2..end]);
            match stack.pop() {
                Some(open) if open == name => {}
                _ => return false,
            }
            i = end + 1;
            continue;
        }
        let end = match find(bytes, i + 1, b">") {
            Some(e) => e,
            None => return false,
        };
        let inner = &s[i + 1..end];
        let self_close = inner.trim_end().ends_with('/');
        let name = tag_name(inner);
        if name.is_empty() {
            return false;
        }
        if !self_close {
            stack.push(name);
        }
        i = end + 1;
    }
    stack.is_empty()
}

fn find(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

fn tag_name(inner: &str) -> String {
    inner
        .trim()
        .trim_end_matches('/')
        .split(|c: char| c.is_whitespace())
        .next()
        .unwrap_or("")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_core::{Feature, Record};
    use splicecraft_persist::{
        authorize_writes_for_sandbox, revoke_thread_writes, stage_bytes_tempfile,
    };

    fn demo() -> Record {
        let mut rec = Record::new("pUC19-map", "A".repeat(200) + &"GATC".repeat(50), true);
        rec.features.push(Feature::new("CDS", 10, 80, 1, "lacZ"));
        rec
    }

    #[test]
    fn svg_contains_name_and_is_well_formed() {
        let rec = demo();
        let svg = render_plasmid_map_svg(&rec, &MapImageOpts::default());
        assert!(svg.contains("pUC19-map"), "{svg}");
        assert!(svg.contains("<?xml"), "{svg}");
        assert!(svg_is_well_formed(&svg), "{svg}");
        let amp = {
            let mut r = rec.clone();
            r.name = "pA&B<C>".into();
            render_plasmid_map_svg(&r, &MapImageOpts::default())
        };
        assert!(amp.contains("pA&amp;B&lt;C&gt;"), "{amp}");
        assert!(svg_is_well_formed(&amp), "{amp}");
    }

    #[test]
    fn size_clamps_to_publication_range() {
        assert_eq!(clamp_size(10), 300);
        assert_eq!(clamp_size(9000), 6000);
        assert_eq!(clamp_size(1400), 1400);
    }

    #[test]
    fn png_write_is_atomic_to_user_path_not_chokepoint() {
        revoke_thread_writes();
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("figure.png");
        let rec = demo();
        let report = export_plasmid_map(&rec, &dest, "png", &MapImageOpts::default()).unwrap();
        assert!(dest.exists());
        assert!(!dest.display().to_string().contains("splicecraft-rs"));
        let bytes = std::fs::read(&dest).unwrap();
        assert!(bytes.starts_with(b"\x89PNG"), "not a PNG");
        assert_eq!(report.fmt, "png");
        assert_eq!(report.bp, rec.len());

        let staged = stage_bytes_tempfile(&dest, b"not-a-png").unwrap();
        assert!(staged.exists());
        let live = std::fs::read(&dest).unwrap();
        assert!(
            live.starts_with(b"\x89PNG"),
            "previous PNG must stay intact"
        );
        let _ = std::fs::remove_file(staged);

        authorize_writes_for_sandbox(tmp.path()).ok();
    }

    #[test]
    fn bulk_export_writes_named_svgs() {
        let tmp = tempfile::tempdir().unwrap();
        let rec = demo();
        let reports = export_plasmid_maps(
            &[rec],
            tmp.path(),
            "svg",
            &MapImageOpts {
                size: 300,
                ..MapImageOpts::default()
            },
        )
        .unwrap();
        assert_eq!(reports.len(), 1);
        let svg = std::fs::read_to_string(&reports[0].path).unwrap();
        assert!(svg.contains("pUC19-map"));
        assert!(svg_is_well_formed(&svg));
    }
}
