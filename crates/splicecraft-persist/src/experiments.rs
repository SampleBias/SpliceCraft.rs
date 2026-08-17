//! Lab-notebook entries, projects, cross-refs, and image blobs. [INV-07]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use splicecraft_util::{now_iso, sanitize_label};

use crate::atomic::{atomic_write_bytes, refuse_symlink_chain};
use crate::auth::refuse_unauthorized_write;
use crate::domain::{
    load_experiment_projects, load_experiments, save_experiment_projects, save_experiments,
};
use crate::error::PersistError;
use crate::event::log_event;
use crate::paths::DataLayout;

/// Upstream `_DEFAULT_PROJECT_NAME`.
pub const DEFAULT_PROJECT_NAME: &str = "Main Project";
/// Per-entry body cap.
pub const EXPERIMENT_BODY_MAX_BYTES: usize = 1_000_000;
/// Title cap.
pub const EXPERIMENT_TITLE_MAX_LEN: usize = 200;
/// Tag string cap.
pub const EXPERIMENT_TAG_MAX_LEN: usize = 60;
/// Tag count cap.
pub const EXPERIMENT_TAGS_MAX: usize = 20;
/// Per-image byte cap.
pub const EXPERIMENT_IMAGE_MAX_BYTES: usize = 10_000_000;
/// Per-entry attach-dir cap.
pub const EXPERIMENT_DIR_MAX_BYTES: usize = 100_000_000;
/// Allowed image suffixes.
pub const IMAGE_EXTS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp"];

/// One notebook entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentEntry {
    /// `exp-<8 hex>` (or a sanitised custom id).
    pub id: String,
    /// List title.
    #[serde(default)]
    pub title: String,
    /// Markdown body.
    #[serde(default)]
    pub body_md: String,
    /// ISO-ish stamp.
    #[serde(default)]
    pub created_at: String,
    /// ISO-ish stamp.
    #[serde(default)]
    pub updated_at: String,
    /// Free-form tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Denormalised `@id` tokens.
    #[serde(default)]
    pub attached_plasmid_ids: Vec<String>,
    /// Denormalised `!id` tokens.
    #[serde(default)]
    pub attached_actions: Vec<String>,
    /// Denormalised `&id` tokens.
    #[serde(default)]
    pub attached_gel_ids: Vec<String>,
    /// Relative image names under the attach dir.
    #[serde(default)]
    pub image_paths: Vec<String>,
}

/// Named project wrapping a list of entries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentProject {
    /// Display name.
    pub name: String,
    /// Optional prose.
    #[serde(default)]
    pub description: String,
    /// Entries in this project.
    #[serde(default)]
    pub experiments: Vec<ExperimentEntry>,
    /// Optional saved stamp.
    #[serde(default)]
    pub saved: String,
}

/// Unique `@` / `!` / `&` tokens in first-appearance order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExperimentJumpTable {
    /// `@plasmid` ids.
    pub plasmids: Vec<String>,
    /// `!action` ids.
    pub actions: Vec<String>,
    /// `&gel` ids.
    pub gels: Vec<String>,
}

/// In-memory notebook. Writes go through `safe_save_json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentStore {
    /// All projects.
    pub projects: Vec<ExperimentProject>,
    /// Active project name.
    pub active: String,
    /// Live entries (mirrors the active project).
    pub entries: Vec<ExperimentEntry>,
}

impl Default for ExperimentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ExperimentStore {
    /// Empty Main Project.
    #[must_use]
    pub fn new() -> Self {
        Self {
            projects: vec![ExperimentProject {
                name: DEFAULT_PROJECT_NAME.into(),
                ..ExperimentProject::default()
            }],
            active: DEFAULT_PROJECT_NAME.into(),
            entries: Vec::new(),
        }
    }

    /// Load both JSON files and ensure a default project.
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let raw_entries = decode_list::<ExperimentEntry>(&load_experiments(layout).entries);
        let projects = decode_list::<ExperimentProject>(&load_experiment_projects(layout).entries);
        let mut store = if projects.is_empty() {
            let mut s = Self::new();
            s.entries = raw_entries
                .into_iter()
                .map(|e| normalise_experiment_entry(e, false))
                .collect();
            s.sync_active();
            s
        } else {
            let active = projects
                .first()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| DEFAULT_PROJECT_NAME.into());
            let entries = projects
                .iter()
                .find(|p| p.name == active)
                .map(|p| p.experiments.clone())
                .unwrap_or(raw_entries);
            Self {
                projects,
                active,
                entries: entries
                    .into_iter()
                    .map(|e| normalise_experiment_entry(e, false))
                    .collect(),
            }
        };
        store.ensure_default_project();
        store.sync_active();
        store
    }

    /// First-run wrap + repair a missing/orphan active pointer.
    pub fn ensure_default_project(&mut self) {
        if self.projects.is_empty() {
            self.projects.push(ExperimentProject {
                name: DEFAULT_PROJECT_NAME.into(),
                experiments: self.entries.clone(),
                ..ExperimentProject::default()
            });
            self.active = DEFAULT_PROJECT_NAME.into();
            return;
        }
        if !self.projects.iter().any(|p| p.name == self.active) {
            self.active = self.projects[0].name.clone();
            self.entries = self.projects[0].experiments.clone();
        }
    }

    /// Persist `experiments.json` then mirror into the active project.
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        let entries: Vec<Value> = self
            .entries
            .iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect();
        save_experiments(layout, &entries)?;
        let mut projects = self.projects.clone();
        if let Some(p) = projects.iter_mut().find(|p| p.name == self.active) {
            p.experiments = self.entries.clone();
            p.saved = now_iso();
        }
        let proj_vals: Vec<Value> = projects
            .iter()
            .filter_map(|p| serde_json::to_value(p).ok())
            .collect();
        save_experiment_projects(layout, &proj_vals)?;
        log_event(
            "experiments.saved",
            &[
                ("project", &self.active),
                ("n", &self.entries.len().to_string()),
            ],
        );
        Ok(())
    }

    fn sync_active(&mut self) {
        if let Some(p) = self.projects.iter_mut().find(|p| p.name == self.active) {
            p.experiments = self.entries.clone();
        } else {
            self.projects.push(ExperimentProject {
                name: self.active.clone(),
                experiments: self.entries.clone(),
                ..ExperimentProject::default()
            });
        }
    }

    /// Insert or replace by id.
    pub fn upsert(&mut self, entry: ExperimentEntry) {
        let e = normalise_experiment_entry(entry, false);
        if let Some(slot) = self.entries.iter_mut().find(|x| x.id == e.id) {
            *slot = e;
        } else {
            self.entries.push(e);
        }
        self.sync_active();
    }

    /// Switch the live list to `name` if the project exists.
    pub fn set_active(&mut self, name: &str) -> bool {
        let Some(p) = self.projects.iter().find(|p| p.name == name) else {
            return false;
        };
        self.active = p.name.clone();
        self.entries = p.experiments.clone();
        true
    }
}

fn decode_list<T: for<'de> Deserialize<'de>>(entries: &[Value]) -> Vec<T> {
    entries
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect()
}

/// Filesystem-safe experiment id, or `None`.
#[must_use]
pub fn sanitize_experiment_id(raw: &str) -> Option<String> {
    if raw.is_empty()
        || raw.contains('\0')
        || raw.contains("..")
        || raw.contains('/')
        || raw.contains('\\')
    {
        return None;
    }
    let b = raw.as_bytes();
    if b.len() > 64 {
        return None;
    }
    let first = *b.first()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    if !b[1..]
        .iter()
        .all(|c| c.is_ascii_alphanumeric() || *c == b'.' || *c == b'_' || *c == b'-')
    {
        return None;
    }
    Some(raw.to_owned())
}

/// Fresh `exp-<8 hex>` avoiding `existing`.
#[must_use]
pub fn new_experiment_id(existing: &HashSet<String>) -> String {
    let mut n = splicecraft_util::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    for _ in 0..64 {
        let id = format!("exp-{:08x}", n as u32);
        if !existing.contains(&id) {
            return id;
        }
        n = n.wrapping_add(0x9E37_79B9);
    }
    format!("exp-{:08x}", n.wrapping_add(1) as u32)
}

/// Rewrite legacy `@plasmid:` / `@actions:` prefixes.
#[must_use]
pub fn migrate_legacy_tag_format(body: &str) -> String {
    body.replace("@plasmid:", "@").replace("@actions:", "!")
}

/// Unique `@id` tokens, first-appearance order.
#[must_use]
pub fn extract_plasmid_refs(body_md: &str) -> Vec<String> {
    extract_sigil_refs(body_md, '@')
}

/// Unique `!id` tokens.
#[must_use]
pub fn extract_action_refs(body_md: &str) -> Vec<String> {
    extract_sigil_refs(body_md, '!')
}

/// Unique `&id` tokens (same rule as gels).
#[must_use]
pub fn extract_experiment_gel_refs(body_md: &str) -> Vec<String> {
    extract_sigil_refs(body_md, '&')
}

/// All three sigils.
#[must_use]
pub fn experiment_jump_table(body_md: &str) -> ExperimentJumpTable {
    let body = migrate_legacy_tag_format(body_md);
    ExperimentJumpTable {
        plasmids: extract_plasmid_refs(&body),
        actions: extract_action_refs(&body),
        gels: extract_experiment_gel_refs(&body),
    }
}

/// Resolve `@id` against library rows by id or display name.
#[must_use]
pub fn resolve_plasmid_jump<'a, T>(
    id: &str,
    plasmids: &'a [T],
    id_of: impl Fn(&T) -> &str,
    name_of: impl Fn(&T) -> &str,
) -> Option<&'a T> {
    plasmids
        .iter()
        .find(|e| id_of(e) == id)
        .or_else(|| plasmids.iter().find(|e| name_of(e) == id))
}

fn extract_sigil_refs(body_md: &str, sigil: char) -> Vec<String> {
    if !body_md.contains(sigil) {
        return Vec::new();
    }
    let chars: Vec<char> = body_md.chars().collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == sigil {
            let prev_ok = i == 0 || {
                let p = chars[i - 1];
                !p.is_ascii_alphanumeric() && p != '_' && p != sigil
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

/// Cap title/body/tags and rebuild xrefs.
#[must_use]
pub fn normalise_experiment_entry(entry: ExperimentEntry, fresh: bool) -> ExperimentEntry {
    let mut out = entry;
    let existing = HashSet::new();
    if sanitize_experiment_id(&out.id).is_none() {
        out.id = new_experiment_id(&existing);
    }
    out.title = sanitize_label(&out.title, EXPERIMENT_TITLE_MAX_LEN);
    let mut body = migrate_legacy_tag_format(&strip_note_ctrl(&out.body_md));
    let encoded = body.as_bytes();
    if encoded.len() > EXPERIMENT_BODY_MAX_BYTES {
        body = String::from_utf8_lossy(&encoded[..EXPERIMENT_BODY_MAX_BYTES]).into_owned();
    }
    out.body_md = body;
    let mut tags = Vec::new();
    for t in out.tags.drain(..) {
        let t = sanitize_label(&t, EXPERIMENT_TAG_MAX_LEN);
        if t.is_empty() {
            continue;
        }
        tags.push(t);
        if tags.len() >= EXPERIMENT_TAGS_MAX {
            break;
        }
    }
    out.tags = tags;
    out.attached_plasmid_ids = extract_plasmid_refs(&out.body_md);
    out.attached_actions = extract_action_refs(&out.body_md);
    out.attached_gel_ids = extract_experiment_gel_refs(&out.body_md);
    out.image_paths.retain(|p| !p.is_empty());
    let now = now_iso();
    if fresh || out.created_at.is_empty() {
        out.created_at = now.clone();
    }
    out.updated_at = now;
    out
}

fn strip_note_ctrl(s: &str) -> String {
    s.chars()
        .filter(|c| *c == '\t' || *c == '\n' || !c.is_control())
        .collect()
}

/// Attach dir `_EXPERIMENTS_DIR/<id>/`.
pub fn experiment_attach_dir(
    layout: &DataLayout,
    entry_id: &str,
    create: bool,
) -> Result<PathBuf, PersistError> {
    let id = sanitize_experiment_id(entry_id).ok_or_else(|| {
        PersistError::Commit(format!(
            "refusing experiment attach dir for id {entry_id:?}"
        ))
    })?;
    let dir = layout.experiments_dir().join(id);
    refuse_symlink_chain(&dir, "experiment attach dir")?;
    if create {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Atomic image write under the entry dir. Filename `img-<ts>-<rand>.<ext>`.
pub fn save_experiment_image(
    layout: &DataLayout,
    entry_id: &str,
    data: &[u8],
    suggested_name: Option<&str>,
) -> Result<PathBuf, PersistError> {
    if data.len() > EXPERIMENT_IMAGE_MAX_BYTES {
        return Err(PersistError::Commit(format!(
            "refusing image attach: {} bytes > {EXPERIMENT_IMAGE_MAX_BYTES} cap",
            data.len()
        )));
    }
    let dir = experiment_attach_dir(layout, entry_id, true)?;
    let existing = dir_size_bytes(&dir);
    if existing.saturating_add(data.len() as u64) > EXPERIMENT_DIR_MAX_BYTES as u64 {
        return Err(PersistError::Commit(format!(
            "refusing image attach: entry dir would exceed {EXPERIMENT_DIR_MAX_BYTES}"
        )));
    }
    let suffix = image_suffix(suggested_name);
    let stamp: String = now_iso()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(14)
        .collect();
    let n = (data.len() as u32).wrapping_mul(2_654_435_761);
    let name = format!("img-{stamp}-{n:06x}{suffix}");
    let out = dir.join(&name);
    refuse_unauthorized_write(&out, "experiment image")?;
    refuse_symlink_chain(&out, "experiment image")?;
    atomic_write_bytes(&out, data)?;
    log_event(
        "experiments.attach.image",
        &[("entry", entry_id), ("bytes", &data.len().to_string())],
    );
    Ok(out)
}

fn image_suffix(suggested: Option<&str>) -> &'static str {
    let Some(name) = suggested else {
        return ".png";
    };
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        ext if IMAGE_EXTS.iter().any(|e| e.trim_start_matches('.') == ext) => IMAGE_EXTS
            .iter()
            .copied()
            .find(|e| e.trim_start_matches('.') == ext)
            .unwrap_or(".png"),
        _ => ".png",
    }
}

fn dir_size_bytes(dir: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else {
        return 0;
    };
    rd.filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// Half-block mosaic from bytes (no image decoder; not a photograph).
#[must_use]
pub fn halfblock_preview(data: &[u8], width: usize, height: usize) -> String {
    let w = width.clamp(4, 40);
    let h = height.clamp(2, 12);
    let mut lines = Vec::new();
    if data.is_empty() {
        return "(empty)".into();
    }
    for row in 0..h {
        let mut line = String::new();
        for col in 0..w {
            let i = (row * w + col) % data.len();
            let v = data[i];
            line.push(match v {
                0..=63 => ' ',
                64..=127 => '▄',
                128..=191 => '▀',
                _ => '█',
            });
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Masked spellcheck. Returns unique lowercase misses, first-appearance order.
#[must_use]
pub fn spellcheck_body(body_md: &str) -> Vec<String> {
    let masked = mask_non_prose(body_md);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = String::new();
    for c in masked.chars() {
        if c.is_ascii_alphabetic() || c == '\'' || c == '-' {
            cur.push(c);
        } else {
            push_word(&mut out, &mut seen, &cur);
            cur.clear();
        }
    }
    push_word(&mut out, &mut seen, &cur);
    out
}

fn push_word(out: &mut Vec<String>, seen: &mut HashSet<String>, raw: &str) {
    let w = raw.trim_matches(|c| c == '\'' || c == '-');
    if w.chars().count() < 2 {
        return;
    }
    if w.chars().all(|c| "ACGTacgtUunN".contains(c)) && w.len() >= 4 {
        return;
    }
    let lower = w.to_ascii_lowercase();
    if WORDLIST.contains(&lower.as_str()) {
        return;
    }
    if seen.insert(lower.clone()) {
        out.push(lower);
    }
}

fn mask_non_prose(body: &str) -> String {
    let mut s = migrate_legacy_tag_format(body);
    // URLs
    s = mask_urls(&s);
    // inline code
    s = mask_backticks(&s);
    // refs
    for (sigil, refs) in [
        ('@', extract_plasmid_refs(&s)),
        ('!', extract_action_refs(&s)),
        ('&', extract_experiment_gel_refs(&s)),
    ] {
        for r in refs {
            let tok = format!("{sigil}{r}");
            s = s.replace(&tok, " ");
        }
    }
    s
}

fn mask_urls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i..].starts_with(&['h', 't', 't', 'p']) {
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn mask_backticks(s: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for c in s.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
            continue;
        }
        out.push(if in_code { ' ' } else { c });
    }
    out
}

/// Modest English list plus lab tokens. Unknown words are reported.
const WORDLIST: &[&str] = &[
    "a",
    "about",
    "after",
    "again",
    "all",
    "also",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "been",
    "before",
    "both",
    "but",
    "by",
    "can",
    "check",
    "clone",
    "cloning",
    "colony",
    "cut",
    "day",
    "design",
    "did",
    "digest",
    "dna",
    "do",
    "each",
    "extract",
    "first",
    "for",
    "from",
    "gel",
    "gibson",
    "had",
    "has",
    "have",
    "here",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "lab",
    "left",
    "ligate",
    "load",
    "min",
    "next",
    "no",
    "not",
    "note",
    "of",
    "on",
    "or",
    "pcr",
    "plasmid",
    "primer",
    "primers",
    "round",
    "run",
    "save",
    "see",
    "seq",
    "sequence",
    "site",
    "so",
    "than",
    "that",
    "the",
    "then",
    "this",
    "to",
    "today",
    "transform",
    "two",
    "used",
    "vector",
    "was",
    "we",
    "were",
    "with",
    "work",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{authorize_writes_for_sandbox, revoke_thread_writes};
    use crate::library::LibraryEntry;

    fn sandbox() -> (tempfile::TempDir, DataLayout) {
        let tmp = tempfile::tempdir().expect("tempdir");
        authorize_writes_for_sandbox(tmp.path()).expect("sandbox");
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        (tmp, layout)
    }

    #[test]
    fn parse_plasmid_action_gel_from_fixture_body() {
        let body = "Today: @pUC19 then !digest and &runA, Email user@example.com, \
                    ![img](x), &amp; leftover @plasmid:pACYC";
        let migrated = migrate_legacy_tag_format(body);
        let table = experiment_jump_table(&migrated);
        assert_eq!(table.plasmids, vec!["pUC19", "pACYC"]);
        assert_eq!(table.actions, vec!["digest"]);
        assert_eq!(table.gels, vec!["runA"]);
        assert!(extract_plasmid_refs("user@example.com").is_empty());
        assert!(extract_plasmid_refs("@../etc").is_empty());
        assert!(extract_experiment_gel_refs("&amp;").is_empty());
    }

    #[test]
    fn jump_table_resolves_known_plasmid_id() {
        let table = experiment_jump_table("See @pUC19 and @missing");
        assert_eq!(table.plasmids, vec!["pUC19", "missing"]);
        let lib = vec![LibraryEntry {
            id: "pUC19".into(),
            name: "pUC19".into(),
            size: 4,
            gb_text: String::new(),
            source: String::new(),
            alignments: Vec::new(),
            history_xml: String::new(),
        }];
        let hit = resolve_plasmid_jump("pUC19", &lib, |e| &e.id, |e| &e.name);
        assert_eq!(hit.map(|e| e.id.as_str()), Some("pUC19"));
        assert!(resolve_plasmid_jump("missing", &lib, |e| &e.id, |e| &e.name).is_none());
    }

    #[test]
    fn attachment_write_uses_persist_chokepoint() {
        let (_tmp, layout) = sandbox();
        let png = b"\x89PNG\r\n\x1a\n tiny";
        let path = save_experiment_image(&layout, "exp-abc12345", png, Some("gel.png")).unwrap();
        assert!(path.starts_with(layout.experiments_dir()));
        assert!(path.extension().and_then(|s| s.to_str()) == Some("png"));
        assert_eq!(fs::read(&path).unwrap(), png);
        revoke_thread_writes();
        let err = save_experiment_image(&layout, "exp-abc12345", png, None).unwrap_err();
        assert!(matches!(err, PersistError::Unauthorized { .. }));
    }

    #[test]
    fn experiments_round_trip_rebuilds_xrefs() {
        let (_tmp, layout) = sandbox();
        let mut store = ExperimentStore::load(&layout);
        store.upsert(normalise_experiment_entry(
            ExperimentEntry {
                id: "exp-test1234".into(),
                title: "Cloning round 1".into(),
                body_md: "Today: cut with HindIII.\n@pUC19".into(),
                tags: vec!["cloning".into()],
                ..ExperimentEntry::default()
            },
            true,
        ));
        store.persist(&layout).unwrap();
        let again = ExperimentStore::load(&layout);
        assert_eq!(again.entries[0].attached_plasmid_ids, vec!["pUC19"]);
        assert_eq!(again.active, DEFAULT_PROJECT_NAME);
    }

    #[test]
    fn spellcheck_masks_refs_and_flags_typos() {
        let hits = spellcheck_body("Today @pUC19 we xyzzyqq the `code` http://x.test digest");
        assert!(hits.contains(&"xyzzyqq".into()), "{hits:?}");
        assert!(!hits.iter().any(|h| h.contains("puc19")));
    }
}
