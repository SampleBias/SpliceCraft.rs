//! Per-lane band resolution (ladder / uncut / digest / PCR).

use splicecraft_bio::digest_with_enzymes;

use crate::ladders::ladder_bands;
use crate::mobility::DnaForm;

/// UI cap on Simulator gel lanes.
pub const GEL_UI_MAX_LANES: usize = 8;
/// Persist cap (mirrors upstream `_GEL_LANES_MAX`).
pub const GEL_LANES_MAX: usize = 20;

/// One gel lane configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GelLane {
    /// Short display name.
    pub name: String,
    /// `ladder` / `plasmid` / `digest` / `pcr` / `empty`.
    pub source: String,
    /// Ladder name, enzyme list, or empty.
    pub detail: String,
    /// Frozen PCR size (`Send to Gel lane`); wins over the selected amplicon.
    pub pcr_bp: Option<usize>,
}

impl GelLane {
    /// Empty lane.
    #[must_use]
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: "empty".into(),
            detail: String::new(),
            pcr_bp: None,
        }
    }

    /// Ladder lane.
    #[must_use]
    pub fn ladder(name: impl Into<String>, ladder: &str) -> Self {
        Self {
            name: name.into(),
            source: "ladder".into(),
            detail: ladder.into(),
            pcr_bp: None,
        }
    }
}

/// Resolve `(bp, form)` bands for one lane.
#[must_use]
pub fn gel_bands_for_lane(
    lane: &GelLane,
    template_seq: &str,
    template_circular: bool,
    pcr_length: Option<usize>,
) -> Vec<(usize, DnaForm)> {
    let src = lane.source.to_ascii_lowercase();
    let detail = lane.detail.trim();
    match src.as_str() {
        "ladder" => ladder_bands(detail)
            .iter()
            .map(|bp| (*bp as usize, DnaForm::Linear))
            .collect(),
        "plasmid" => {
            let seq_len = template_seq.len();
            if seq_len == 0 {
                return Vec::new();
            }
            if template_circular {
                vec![(seq_len, DnaForm::Supercoiled), (seq_len, DnaForm::Nicked)]
            } else {
                vec![(seq_len, DnaForm::Linear)]
            }
        }
        "digest" => {
            if template_seq.is_empty() {
                return Vec::new();
            }
            let enz: Vec<&str> = detail
                .split(',')
                .map(str::trim)
                .filter(|e| !e.is_empty())
                .collect();
            if enz.is_empty() {
                return Vec::new();
            }
            digest_with_enzymes(template_seq, &enz, template_circular)
                .into_iter()
                .filter_map(|f| {
                    let bp = f.len();
                    (bp > 0).then_some((bp, DnaForm::Linear))
                })
                .collect()
        }
        "pcr" => {
            let bp = lane
                .pcr_bp
                .filter(|n| *n > 0)
                .or(pcr_length.filter(|n| *n > 0));
            match bp {
                Some(n) => vec![(n, DnaForm::Linear)],
                None => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Append a PCR lane pinned to `amp_bp`. Returns `(idx, at_cap)`.
pub fn append_pcr_gel_lane(
    lanes: &mut Vec<GelLane>,
    lane_name: impl Into<String>,
    amp_bp: usize,
    max_lanes: usize,
) -> (isize, bool) {
    if lanes.len() >= max_lanes {
        return (-1, true);
    }
    lanes.push(GelLane {
        name: lane_name.into(),
        source: "pcr".into(),
        detail: String::new(),
        pcr_bp: Some(amp_bp),
    });
    ((lanes.len() - 1) as isize, false)
}
