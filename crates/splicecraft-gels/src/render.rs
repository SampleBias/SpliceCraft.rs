//! Text gel image: well-at-top, dye-front-at-bottom.

use crate::lanes::{GelLane, gel_bands_for_lane};
use crate::mobility::agarose_mobility;

/// Default body rows (wells + dye front are extra).
pub const GEL_HEIGHT_DEFAULT: usize = 22;
const GEL_HEIGHT_MIN: usize = 4;
const GEL_HEIGHT_MAX: usize = 200;
const GEL_LANE_WIDTH_MIN: usize = 1;
const GEL_LANE_WIDTH_MAX: usize = 32;
const FAINT_FRAC_THRESHOLD: f64 = 0.25;

/// Render knobs matching `_render_gel_image`.
#[derive(Clone, Debug)]
pub struct GelRenderOpts<'a> {
    /// Template used by plasmid / digest lanes.
    pub template_seq: &'a str,
    /// Circular uncut shows SC + nicked.
    pub template_circular: bool,
    /// Selected PCR length for unfrozen `pcr` lanes.
    pub pcr_length: Option<usize>,
    /// Agarose % (snapped internally).
    pub agarose_pct: f64,
    /// Body height.
    pub height: usize,
    /// Glyphs per lane column.
    pub lane_width: usize,
    /// Left tick-label column.
    pub label_col: usize,
}

impl Default for GelRenderOpts<'_> {
    fn default() -> Self {
        Self {
            template_seq: "",
            template_circular: false,
            pcr_length: None,
            agarose_pct: 1.0,
            height: GEL_HEIGHT_DEFAULT,
            lane_width: 7,
            label_col: 7,
        }
    }
}

/// Paint the gel as plain text (Unicode band glyphs).
#[must_use]
pub fn render_gel_image(lane_specs: &[GelLane], opt: &GelRenderOpts<'_>) -> String {
    let n_lanes = lane_specs.len();
    if n_lanes == 0 {
        return "(no lanes — add at least one to render a gel)\n".into();
    }
    let height = opt.height.clamp(GEL_HEIGHT_MIN, GEL_HEIGHT_MAX);
    let lane_width = opt.lane_width.clamp(GEL_LANE_WIDTH_MIN, GEL_LANE_WIDTH_MAX);
    let label_col = opt.label_col.max(1);

    let mut lane_bands = Vec::new();
    let mut ladder_lane_idx = None;
    for (li, lane) in lane_specs.iter().enumerate() {
        let bands = gel_bands_for_lane(
            lane,
            opt.template_seq,
            opt.template_circular,
            opt.pcr_length,
        );
        if ladder_lane_idx.is_none() && lane.source.eq_ignore_ascii_case("ladder") {
            ladder_lane_idx = Some(li);
        }
        lane_bands.push(bands);
    }

    let mut band_grid: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::new();
    let mut band_faint: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();
    let mut ladder_rows: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (li, bands) in lane_bands.iter().enumerate() {
        for (bp, form) in bands {
            let mob = agarose_mobility(*bp as i64, opt.agarose_pct, *form);
            let row_float = mob * (height.saturating_sub(1) as f64);
            let row = row_float
                .round()
                .clamp(0.0, (height.saturating_sub(1)) as f64) as usize;
            *band_grid.entry((row, li)).or_insert(0) += 1;
            if Some(li) == ladder_lane_idx {
                let e = ladder_rows.entry(row).or_insert(0);
                *e = (*e).max(*bp);
            }
            let frac = row_float - row as f64;
            if frac.abs() > FAINT_FRAC_THRESHOLD {
                let row_sec = if frac > 0.0 {
                    row.saturating_add(1)
                } else {
                    row.saturating_sub(1)
                };
                if row_sec < height && row_sec != row {
                    band_faint.insert((row_sec, li));
                }
            }
        }
    }

    let mut out = String::new();
    let mut head = " ".repeat(label_col);
    for li in 0..n_lanes {
        head.push_str(&format!("{:^width$} ", li + 1, width = lane_width));
    }
    out.push_str(head.trim_end());
    out.push('\n');

    let mut names = " ".repeat(label_col);
    for lane in lane_specs {
        let label: String = lane.name.chars().take(lane_width).collect();
        names.push_str(&format!("{label:^lane_width$} "));
    }
    out.push_str(names.trim_end());
    out.push('\n');

    let mut wells = " ".repeat(label_col);
    for _ in 0..n_lanes {
        wells.push_str(&"█".repeat(lane_width));
        wells.push(' ');
    }
    out.push_str(wells.trim_end());
    out.push('\n');

    for row in 0..height {
        let line_left = if let Some(&bp) = ladder_rows.get(&row) {
            let label = if bp >= 1000 {
                format!("{:>4.1}k", bp as f64 / 1000.0)
            } else {
                format!("{bp:>5}")
            };
            format!("{label} ")
                .to_string()
                .chars()
                .chain(std::iter::repeat(' '))
                .take(label_col)
                .collect::<String>()
        } else {
            " ".repeat(label_col)
        };
        let mut line = line_left.clone();
        for li in 0..n_lanes {
            let count = band_grid.get(&(row, li)).copied().unwrap_or(0);
            if count == 0 {
                if band_faint.contains(&(row, li)) {
                    line.push_str(&"─".repeat(lane_width));
                } else {
                    line.push_str(&" ".repeat(lane_width));
                }
                line.push(' ');
            } else if count == 1 {
                line.push_str(&"━".repeat(lane_width));
                line.push(' ');
            } else if count == 2 {
                line.push_str(&"▆".repeat(lane_width));
                line.push(' ');
            } else {
                line.push_str(&"█".repeat(lane_width));
                line.push(' ');
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }

    let mut front = " ".repeat(label_col);
    for _ in 0..n_lanes {
        front.push_str(&"░".repeat(lane_width));
        front.push(' ');
    }
    out.push_str(front.trim_end());

    let pcr_empty: Vec<usize> = lane_specs
        .iter()
        .enumerate()
        .filter(|(i, lane)| lane.source.eq_ignore_ascii_case("pcr") && lane_bands[*i].is_empty())
        .map(|(i, _)| i + 1)
        .collect();
    if !pcr_empty.is_empty() {
        let nums = pcr_empty
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let label = if pcr_empty.len() > 1 { "lanes" } else { "lane" };
        out.push_str(&format!(
            "\n  PCR {label} {nums}: no amplicon — run a PCR first to populate."
        ));
    }
    out
}
