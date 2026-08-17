//! HMM-DB catalog (`hmm_db_catalog.json`). Builtins are re-injected if missing.
//!
//! Download bytes live under `hmm_databases/<id>/` and go through the write
//! chokepoint. Default CI never fetches Pfam.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{load_hmm_db_catalog, save_hmm_db_catalog};
use crate::error::PersistError;
use crate::paths::DataLayout;

/// Catalog id / directory-name cap.
pub const HMM_DB_ID_MAX: usize = 64;
/// Display name cap.
pub const HMM_DB_NAME_MAX: usize = 200;
/// Description cap.
pub const HMM_DB_DESC_MAX: usize = 500;
/// URL cap.
pub const HMM_DB_URL_MAX: usize = 2048;

/// Builtin Pfam-A download (never fetched by default tests).
pub const PFAM_A_URL: &str =
    "https://ftp.ebi.ac.uk/pub/databases/Pfam/current_release/Pfam-A.hmm.gz";
/// Pfam version file.
pub const PFAM_A_VERSION_URL: &str =
    "https://ftp.ebi.ac.uk/pub/databases/Pfam/current_release/Pfam.version.gz";
/// Builtin NCBIfam download.
pub const NCBIFAM_URL: &str = "https://ftp.ncbi.nlm.nih.gov/hmm/current/NCBIfam.HMM.gz";

/// One catalog row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HmmDbEntry {
    /// Filesystem-safe id (`pfam-a`, `ncbifam`, or a user slug).
    pub id: String,
    /// Display name.
    pub name: String,
    /// HTTPS (or http) source URL.
    pub url: String,
    /// Optional tiny version URL.
    #[serde(default)]
    pub version_url: String,
    /// `hmm-gz` or `hmm`.
    #[serde(default = "default_format")]
    pub format: String,
    /// Built-in rows cannot be deleted from the UI.
    #[serde(default)]
    pub builtin: bool,
    /// One-line description.
    #[serde(default)]
    pub description: String,
}

fn default_format() -> String {
    "hmm-gz".into()
}

/// Built-in catalog rows. Re-injected on every load if missing.
#[must_use]
pub fn builtin_hmm_db_catalog() -> [HmmDbEntry; 2] {
    [
        HmmDbEntry {
            id: "pfam-a".into(),
            name: "Pfam-A".into(),
            url: PFAM_A_URL.into(),
            version_url: PFAM_A_VERSION_URL.into(),
            format: "hmm-gz".into(),
            builtin: true,
            description: "Pfam-A: curated protein family HMMs from EBI \
                          (~300 MB download, ~3 GB on disk after hmmpress)."
                .into(),
        },
        HmmDbEntry {
            id: "ncbifam".into(),
            name: "NCBIfam".into(),
            url: NCBIFAM_URL.into(),
            version_url: String::new(),
            format: "hmm-gz".into(),
            builtin: true,
            description: "NCBIfam: HMMs for prokaryotic protein families \
                          (~600 MB download, ~4 GB on disk)."
                .into(),
        },
    ]
}

/// ASCII `[A-Za-z0-9_-]`, no `..`, no path separators, ≤ 64 chars.
#[must_use]
pub fn sanitize_hmm_db_id(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > HMM_DB_ID_MAX {
        return None;
    }
    if s.contains('\0') || s.contains("..") || s.contains('/') || s.contains('\\') {
        return None;
    }
    if s.chars()
        .all(|ch| matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-'))
    {
        Some(s.to_owned())
    } else {
        None
    }
}

/// Reject whitespace / controls anywhere (including edges) before strip.
#[must_use]
pub fn sanitize_hmm_db_url(raw: &str) -> Option<String> {
    if raw
        .chars()
        .any(|ch| ch.is_whitespace() || (ch as u32) < 0x20)
    {
        return None;
    }
    if raw.is_empty() || raw.len() > HMM_DB_URL_MAX {
        return None;
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        Some(raw.to_owned())
    } else {
        None
    }
}

/// Shape a catalog row or drop it.
#[must_use]
pub fn normalise_hmm_db_entry(entry: &HmmDbEntry, builtin_default: bool) -> Option<HmmDbEntry> {
    let id = sanitize_hmm_db_id(&entry.id)?;
    let url = sanitize_hmm_db_url(&entry.url)?;
    let name = {
        let n = entry.name.trim();
        if n.is_empty() {
            id.clone()
        } else {
            n.chars().take(HMM_DB_NAME_MAX).collect()
        }
    };
    let version_url = if entry.version_url.trim().is_empty() {
        String::new()
    } else {
        sanitize_hmm_db_url(entry.version_url.trim()).unwrap_or_default()
    };
    let format = match entry.format.as_str() {
        "hmm" => "hmm",
        _ => "hmm-gz",
    };
    let description = entry
        .description
        .trim()
        .chars()
        .take(HMM_DB_DESC_MAX)
        .collect();
    Some(HmmDbEntry {
        id,
        name,
        url,
        version_url,
        format: format.into(),
        builtin: entry.builtin || builtin_default,
        description,
    })
}

fn from_value(v: &Value) -> Option<HmmDbEntry> {
    let obj = v.as_object()?;
    let entry = HmmDbEntry {
        id: obj.get("id")?.as_str()?.to_owned(),
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        url: obj.get("url")?.as_str()?.to_owned(),
        version_url: obj
            .get("version_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        format: obj
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("hmm-gz")
            .to_owned(),
        builtin: obj.get("builtin").and_then(Value::as_bool).unwrap_or(false),
        description: obj
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
    };
    normalise_hmm_db_entry(&entry, false)
}

/// Load the catalog and re-inject `pfam-a` / `ncbifam` if a hand-edit dropped them.
#[must_use]
pub fn load_hmm_catalog(layout: &DataLayout) -> Vec<HmmDbEntry> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for v in load_hmm_db_catalog(layout).entries {
        let Some(e) = from_value(&v) else {
            continue;
        };
        if !seen.insert(e.id.clone()) {
            continue;
        }
        out.push(e);
    }
    for builtin in builtin_hmm_db_catalog() {
        if !seen.contains(&builtin.id)
            && let Some(e) = normalise_hmm_db_entry(&builtin, true)
        {
            seen.insert(e.id.clone());
            out.push(e);
        }
    }
    out
}

/// Persist a cleaned catalog (builtins may be omitted; the next load re-injects).
pub fn save_hmm_catalog(layout: &DataLayout, entries: &[HmmDbEntry]) -> Result<(), PersistError> {
    let mut cleaned = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in entries {
        let Some(n) = normalise_hmm_db_entry(e, false) else {
            continue;
        };
        if !seen.insert(n.id.clone()) {
            continue;
        }
        cleaned.push(n);
    }
    let values: Vec<Value> = cleaned
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
        .filter(|v| !v.is_null())
        .collect();
    save_hmm_db_catalog(layout, &values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::authorize_writes_for_sandbox;
    use crate::save::safe_save_json;

    fn sandbox() -> (tempfile::TempDir, DataLayout) {
        let tmp = tempfile::tempdir().expect("tempdir");
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        (tmp, layout)
    }

    #[test]
    fn builtins_present_on_empty_and_after_hand_delete() {
        let (tmp, layout) = sandbox();
        assert!(layout.root.starts_with(tmp.path()));
        let loaded = load_hmm_catalog(&layout);
        let ids: Vec<_> = loaded.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"pfam-a"), "{ids:?}");
        assert!(ids.contains(&"ncbifam"), "{ids:?}");

        safe_save_json(&layout.hmm_db_catalog_file(), &[], "HMM database catalog").unwrap();
        let again = load_hmm_catalog(&layout);
        assert!(again.iter().any(|e| e.id == "pfam-a"));
        assert!(again.iter().any(|e| e.id == "ncbifam"));
    }

    #[test]
    fn user_entry_roundtrips_and_corrupt_rows_drop() {
        let (_tmp, layout) = sandbox();
        let mut cat = load_hmm_catalog(&layout);
        cat.push(HmmDbEntry {
            id: "lab-hmm".into(),
            name: "Lab".into(),
            url: "https://example.org/lab.hmm.gz".into(),
            version_url: String::new(),
            format: "hmm-gz".into(),
            builtin: false,
            description: "tiny".into(),
        });
        save_hmm_catalog(&layout, &cat).unwrap();
        let reloaded = load_hmm_catalog(&layout);
        assert!(
            reloaded
                .iter()
                .any(|e| e.id == "lab-hmm" && e.name == "Lab")
        );
    }

    #[test]
    fn sanitize_rejects_traversal_unicode_and_url_whitespace() {
        assert!(sanitize_hmm_db_id("..").is_none());
        assert!(sanitize_hmm_db_id("a/b").is_none());
        assert!(sanitize_hmm_db_id("pfamé").is_none());
        assert!(sanitize_hmm_db_id("pfam-a").is_some());
        assert!(sanitize_hmm_db_url("https://ftp.ebi.ac.uk/x\n").is_none());
        assert!(sanitize_hmm_db_url(" https://ftp.ebi.ac.uk/x").is_none());
        assert!(sanitize_hmm_db_url("https://ftp.ebi.ac.uk/x").is_some());
        assert!(sanitize_hmm_db_url("ftp://ftp.ebi.ac.uk/x").is_none());
    }
}
