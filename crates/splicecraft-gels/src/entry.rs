//! Gel-entry normalisation, id minting, and `&gel` xref extraction.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::lanes::{GEL_LANES_MAX, GelLane};
use splicecraft_util::{now_iso, sanitize_label};

/// Display-name cap.
pub const GEL_NAME_MAX_LEN: usize = 200;
/// Notes cap.
pub const GEL_NOTES_MAX_LEN: usize = 2000;
/// Lane name cap.
pub const GEL_LANE_NAME_MAX_LEN: usize = 60;
/// Lane detail cap.
pub const GEL_LANE_DETAIL_MAX_LEN: usize = 200;
/// Lane source cap.
pub const GEL_LANE_SOURCE_MAX_LEN: usize = 64;
/// Agarose clamp low.
pub const GEL_AGAROSE_MIN: f64 = 0.3;
/// Agarose clamp high.
pub const GEL_AGAROSE_MAX: f64 = 5.0;
/// Chip colour for `&gel` tokens (stage 12).
pub const GEL_CHIP_COLOR: &str = "#FFB347";

/// One saved gel snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GelEntry {
    /// Filesystem-safe id (`gel-<hex>` or a sanitised custom token).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional notes (controls stripped, newlines kept).
    #[serde(default)]
    pub notes: String,
    /// Agarose % (clamped).
    #[serde(default = "default_agarose")]
    pub agarose_pct: f64,
    /// Lanes (capped).
    #[serde(default)]
    pub lanes: Vec<GelLaneJson>,
    /// ISO created stamp.
    #[serde(default)]
    pub created_at: String,
    /// ISO updated stamp.
    #[serde(default)]
    pub updated_at: String,
    /// Forward-compat keys (`_plugin_data`, …).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_agarose() -> f64 {
    1.0
}

/// On-disk lane row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GelLaneJson {
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Source kind.
    #[serde(default = "default_source")]
    pub source: String,
    /// Ladder / enzyme / empty.
    #[serde(default)]
    pub detail: String,
    /// Frozen PCR bp.
    #[serde(default, rename = "_pcr_bp", skip_serializing_if = "Option::is_none")]
    pub pcr_bp: Option<usize>,
}

fn default_source() -> String {
    "empty".into()
}

impl GelLaneJson {
    /// Live lane.
    #[must_use]
    pub fn to_lane(&self) -> GelLane {
        GelLane {
            name: self.name.clone(),
            source: self.source.clone(),
            detail: self.detail.clone(),
            pcr_bp: self.pcr_bp,
        }
    }

    /// From a live lane.
    #[must_use]
    pub fn from_lane(lane: &GelLane) -> Self {
        Self {
            name: lane.name.clone(),
            source: lane.source.clone(),
            detail: lane.detail.clone(),
            pcr_bp: lane.pcr_bp,
        }
    }
}

/// Filesystem-safe gel id, or `None`.
#[must_use]
pub fn sanitize_gel_id(raw: &str) -> Option<String> {
    if raw.is_empty()
        || raw.contains('\0')
        || raw.contains("..")
        || raw.contains('/')
        || raw.contains('\\')
    {
        return None;
    }
    let mut chars = raw.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
    if !rest_ok || raw.len() > 64 {
        return None;
    }
    Some(raw.to_owned())
}

/// Mint `gel-<8 hex>` not in `existing`.
#[must_use]
pub fn new_gel_id(existing: &HashSet<String>) -> String {
    let stamp = now_iso();
    let mut n = 0u32;
    loop {
        n += 1;
        let mixed = format!("{stamp}{n}");
        let mut hex = String::new();
        for b in mixed.bytes() {
            hex.push_str(&format!("{b:02x}"));
            if hex.len() >= 8 {
                break;
            }
        }
        hex.truncate(8);
        let gid = format!("gel-{hex}");
        if !existing.contains(&gid) {
            return gid;
        }
        if n > 64 {
            return format!("gel-{stamp}-{n}");
        }
    }
}

/// Unique `&<id>` tokens in first-appearance order.
#[must_use]
pub fn extract_gel_refs(body_md: &str) -> Vec<String> {
    if !body_md.contains('&') {
        return Vec::new();
    }
    let chars: Vec<char> = body_md.chars().collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' {
            let prev_ok = i == 0 || {
                let p = chars[i - 1];
                !p.is_ascii_alphanumeric() && p != '_' && p != '&'
            };
            if prev_ok && i + 1 < chars.len() && chars[i + 1].is_ascii_alphabetic() {
                let mut j = i + 1;
                while j < chars.len() {
                    let c = chars[j];
                    if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' {
                        if j - (i + 1) >= 63 {
                            break;
                        }
                        j += 1;
                    } else {
                        break;
                    }
                }
                let next_bad = j < chars.len() && (chars[j] == ';' || chars[j] == '=');
                if !next_bad {
                    let id: String = chars[i + 1..j].iter().collect();
                    if seen.insert(id.clone()) {
                        out.push(id);
                    }
                    i = j;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn sanitize_note(s: &str, max_len: usize) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| *c == '\t' || *c == '\n' || !c.is_control())
        .collect();
    cleaned.trim_end().chars().take(max_len).collect()
}

fn clamp_agarose(raw: f64) -> f64 {
    if !raw.is_finite() {
        return 1.0;
    }
    raw.clamp(GEL_AGAROSE_MIN, GEL_AGAROSE_MAX)
}

/// Cap name / notes / lanes, stamp times, sanitise id.
#[must_use]
pub fn normalise_gel_entry(entry: GelEntry, fresh: bool) -> GelEntry {
    let mut existing = HashSet::new();
    existing.insert(entry.id.clone());
    let id = sanitize_gel_id(&entry.id).unwrap_or_else(|| {
        existing.remove(&entry.id);
        new_gel_id(&existing)
    });
    let name = {
        let n = sanitize_label(&entry.name, GEL_NAME_MAX_LEN);
        if n.is_empty() {
            "Untitled gel".into()
        } else {
            n
        }
    };
    let notes = sanitize_note(&entry.notes, GEL_NOTES_MAX_LEN);
    let agarose_pct = clamp_agarose(entry.agarose_pct);
    let lanes: Vec<GelLaneJson> = entry
        .lanes
        .into_iter()
        .take(GEL_LANES_MAX)
        .map(|ln| GelLaneJson {
            name: sanitize_label(&ln.name, GEL_LANE_NAME_MAX_LEN),
            source: {
                let s = sanitize_label(&ln.source, GEL_LANE_SOURCE_MAX_LEN);
                if s.is_empty() { "empty".into() } else { s }
            },
            detail: sanitize_label(&ln.detail, GEL_LANE_DETAIL_MAX_LEN),
            pcr_bp: ln.pcr_bp,
        })
        .collect();
    let now = now_iso();
    let created_at = if fresh || entry.created_at.is_empty() {
        now.clone()
    } else {
        entry.created_at
    };
    GelEntry {
        id,
        name,
        notes,
        agarose_pct,
        lanes,
        created_at,
        updated_at: now,
        extra: entry.extra,
    }
}

/// Case-sensitive name collision (stripped).
#[must_use]
pub fn gel_name_taken(name: &str, entries: &[GelEntry]) -> bool {
    let n = name.trim();
    if n.is_empty() {
        return false;
    }
    entries.iter().any(|e| e.name.trim() == n)
}

/// Look up by sanitised id.
#[must_use]
pub fn find_gel<'a>(gel_id: &str, entries: &'a [GelEntry]) -> Option<&'a GelEntry> {
    let id = sanitize_gel_id(gel_id)?;
    entries.iter().find(|e| e.id == id)
}
