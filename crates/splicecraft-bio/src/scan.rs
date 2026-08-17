//! Restriction-site scanner. Sacred [INV-01], [INV-02], [INV-06].

use std::collections::{HashMap, HashSet};

use crate::enzymes::{CustomEnzyme, all_enzymes, enzyme_color};
use crate::iupac::{iter_match_starts, iupac_pattern, rc};

/// One overlay hit (`resite` bar or `recut` tick).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestrictionHit {
    /// `"resite"` or `"recut"`.
    pub kind: HitKind,
    /// Inclusive start (0-based).
    pub start: usize,
    /// Exclusive end.
    pub end: usize,
    /// `1` forward, `-1` reverse.
    pub strand: i8,
    /// Overlay color (stable per enzyme).
    pub color: String,
    /// Enzyme name; empty on unlabeled wrap-head pieces.
    pub label: String,
    /// Cut column inside the recognition bar, if in-bar.
    pub cut_col: Option<i32>,
    /// Absolute cut when the cut falls outside the recognition (Type IIS).
    pub ext_cut_bp: Option<usize>,
    /// Full recognition start (wrap-encoded when `rec_end < rec_start`).
    pub rec_start: Option<usize>,
    /// Full recognition end.
    pub rec_end: Option<usize>,
    /// Top-strand cut in forward coordinates.
    pub top_cut_bp: Option<usize>,
    /// Bottom-strand cut in forward coordinates.
    pub bottom_cut_bp: Option<usize>,
    /// Labeled-site count when `> 1`.
    pub cut_count: Option<u32>,
}

/// Overlay hit class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    /// Recognition-sequence span.
    Resite,
    /// Single-bp cut marker.
    Recut,
}

impl RestrictionHit {
    /// True for recognition bars.
    #[must_use]
    pub fn is_resite(&self) -> bool {
        self.kind == HitKind::Resite
    }

    /// True for cut ticks.
    #[must_use]
    pub fn is_recut(&self) -> bool {
        self.kind == HitKind::Recut
    }
}

/// Options for [`scan_restriction_sites`].
#[derive(Clone, Debug)]
pub struct ScanOptions {
    /// Skip enzymes shorter than this (default 6).
    pub min_recognition_len: usize,
    /// Keep only enzymes that cut exactly once (default true).
    pub unique_only: bool,
    /// Scan across the origin (default true).
    pub circular: bool,
    /// When set, only these names are scanned (bypasses min length).
    pub allowed_enzymes: Option<Vec<String>>,
    /// User-defined enzymes merged into the NEB catalog for this scan.
    pub extra_enzymes: Vec<CustomEnzyme>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            min_recognition_len: 6,
            unique_only: true,
            circular: true,
            allowed_enzymes: None,
            extra_enzymes: Vec::new(),
        }
    }
}

/// Scan both strands; return resite + recut hits.
pub fn scan_restriction_sites(seq: &str, opts: &ScanOptions) -> Vec<RestrictionHit> {
    let seq_u = seq.to_ascii_uppercase();
    let n = seq_u.len();
    let mut by_enzyme: Vec<(String, Vec<RestrictionHit>)> = Vec::new();
    let mut seen: HashSet<(String, usize, i8)> = HashSet::new();

    let catalog = scan_catalog(&opts.extra_enzymes);
    let max_site_len = catalog.iter().map(|e| e.site_len).max().unwrap_or(0);
    let scan_seq = if opts.circular && n > 0 && max_site_len > 1 {
        let mut s = seq_u.clone();
        s.push_str(&seq_u[..max_site_len.saturating_sub(1).min(n)]);
        s
    } else {
        seq_u.clone()
    };

    let allowed: Option<HashSet<&str>> = opts
        .allowed_enzymes
        .as_ref()
        .map(|v| v.iter().map(String::as_str).collect());

    for entry in &catalog {
        if let Some(allow) = &allowed {
            if !allow.contains(entry.name.as_str()) {
                continue;
            }
        } else if entry.site_len < opts.min_recognition_len {
            continue;
        }

        let mut hits = Vec::new();
        for p in iter_match_starts(&entry.pat, &scan_seq) {
            if p >= n {
                continue;
            }
            let key = (entry.name.clone(), p, 1);
            if !seen.insert(key) {
                continue;
            }
            if !opts.circular
                && (out_of_molecule(p, entry.fwd_cut, n) || out_of_molecule(p, entry.rev_cut, n))
            {
                continue;
            }
            let ext = if entry.fwd_cut <= 0 || entry.fwd_cut >= entry.site_len as i32 {
                Some(mod_cut(p, entry.fwd_cut, n))
            } else {
                None
            };
            let cc = if entry.fwd_cut > 0 && entry.fwd_cut < entry.site_len as i32 {
                Some(entry.fwd_cut)
            } else {
                None
            };
            let top_cut = mod_cut(p, entry.fwd_cut, n);
            let bot_cut = mod_cut(p, entry.rev_cut, n);
            emit_resite(
                &mut hits,
                ResiteEmit {
                    p,
                    site_len: entry.site_len,
                    strand: 1,
                    color: entry.color.as_str(),
                    name: entry.name.as_str(),
                    cut_col: cc,
                    ext_cut_bp: ext,
                    top_cut_bp: top_cut,
                    bottom_cut_bp: bot_cut,
                    n,
                },
            );
            hits.push(recut(top_cut, 1, entry.color.as_str(), entry.name.as_str()));
        }

        if !entry.is_palindrome {
            for p in iter_match_starts(&entry.rc_pat, &scan_seq) {
                if p >= n {
                    continue;
                }
                let key = (entry.name.clone(), p, -1);
                if !seen.insert(key) {
                    continue;
                }
                let rev_cut_col = entry.site_len as i32 - entry.fwd_cut;
                let top_raw = p as i32 + entry.site_len as i32 - entry.rev_cut;
                let bot_raw = p as i32 + entry.site_len as i32 - entry.fwd_cut;
                if !opts.circular && (raw_out(top_raw, n) || raw_out(bot_raw, n)) {
                    continue;
                }
                let top_cut = wrap_raw(top_raw, n);
                let bot_cut = wrap_raw(bot_raw, n);
                let top_outside = n == 0 || ((top_cut + n - p) % n) >= entry.site_len;
                let cc = if (0..entry.site_len as i32).contains(&rev_cut_col) {
                    Some(rev_cut_col)
                } else {
                    None
                };
                let ext = if top_outside { Some(top_cut) } else { None };
                emit_resite(
                    &mut hits,
                    ResiteEmit {
                        p,
                        site_len: entry.site_len,
                        strand: -1,
                        color: entry.color.as_str(),
                        name: entry.name.as_str(),
                        cut_col: cc,
                        ext_cut_bp: ext,
                        top_cut_bp: top_cut,
                        bottom_cut_bp: bot_cut,
                        n,
                    },
                );
                hits.push(recut(
                    bot_cut,
                    -1,
                    entry.color.as_str(),
                    entry.name.as_str(),
                ));
            }
        }

        if !hits.is_empty() {
            by_enzyme.push((entry.name.clone(), hits));
        }
    }

    let site_of: HashMap<&str, &str> = catalog
        .iter()
        .map(|e| (e.name.as_str(), e.site.as_str()))
        .collect();
    let effective_unique = opts.unique_only && allowed.is_none();
    let mut placed: HashSet<(usize, usize, String)> = HashSet::new();
    let mut feats = Vec::new();

    for (name, mut hits) in by_enzyme {
        let n_sites = hits
            .iter()
            .filter(|h| h.is_resite() && !h.label.is_empty())
            .count();
        if effective_unique && n_sites != 1 {
            continue;
        }
        let site_key = site_of.get(name.as_str()).copied().unwrap_or("");
        let positions: HashSet<(usize, usize, String)> = hits
            .iter()
            .filter(|h| h.is_resite() && !h.label.is_empty())
            .map(|h| (h.start, h.end, site_key.to_owned()))
            .collect();
        if !positions.is_disjoint(&placed) {
            continue;
        }
        placed.extend(positions);
        if n_sites > 1 {
            for h in &mut hits {
                if h.is_resite() && !h.label.is_empty() {
                    h.cut_count = Some(n_sites as u32);
                }
            }
        }
        feats.extend(hits);
    }
    feats
}

struct CatalogEntry {
    name: String,
    site: String,
    site_len: usize,
    fwd_cut: i32,
    rev_cut: i32,
    color: String,
    is_palindrome: bool,
    pat: std::sync::Arc<regex::Regex>,
    rc_pat: std::sync::Arc<regex::Regex>,
}

fn scan_catalog(extra: &[CustomEnzyme]) -> Vec<CatalogEntry> {
    let mut out = Vec::new();
    for (name, site, fwd, rev) in all_enzymes(extra) {
        let Ok(pat) = iupac_pattern(&site) else {
            continue;
        };
        let rc_site = rc(&site);
        let Ok(rc_pat) = iupac_pattern(&rc_site) else {
            continue;
        };
        out.push(CatalogEntry {
            name: name.clone(),
            site: site.clone(),
            site_len: site.len(),
            fwd_cut: fwd,
            rev_cut: rev,
            color: enzyme_color(&name).to_owned(),
            is_palindrome: rc_site == site,
            pat,
            rc_pat,
        });
    }
    out
}

struct ResiteEmit<'a> {
    p: usize,
    site_len: usize,
    strand: i8,
    color: &'a str,
    name: &'a str,
    cut_col: Option<i32>,
    ext_cut_bp: Option<usize>,
    top_cut_bp: usize,
    bottom_cut_bp: usize,
    n: usize,
}

fn emit_resite(hits: &mut Vec<RestrictionHit>, e: ResiteEmit<'_>) {
    let ResiteEmit {
        p,
        site_len,
        strand,
        color,
        name,
        cut_col,
        ext_cut_bp,
        top_cut_bp,
        bottom_cut_bp,
        n,
    } = e;
    if p + site_len <= n {
        hits.push(RestrictionHit {
            kind: HitKind::Resite,
            start: p,
            end: p + site_len,
            strand,
            color: color.into(),
            label: name.into(),
            cut_col,
            ext_cut_bp,
            rec_start: Some(p),
            rec_end: Some(p + site_len),
            top_cut_bp: Some(top_cut_bp),
            bottom_cut_bp: Some(bottom_cut_bp),
            cut_count: None,
        });
        return;
    }
    let tail_len = n - p;
    let head_len = (p + site_len) - n;
    let tail_cut_col = cut_col.filter(|&c| c < tail_len as i32);
    let head_cut_col = cut_col.and_then(|c| {
        if c >= tail_len as i32 {
            Some(c - tail_len as i32)
        } else {
            None
        }
    });
    hits.push(RestrictionHit {
        kind: HitKind::Resite,
        start: p,
        end: n,
        strand,
        color: color.into(),
        label: name.into(),
        cut_col: tail_cut_col,
        ext_cut_bp,
        rec_start: Some(p),
        rec_end: Some(head_len),
        top_cut_bp: Some(top_cut_bp),
        bottom_cut_bp: Some(bottom_cut_bp),
        cut_count: None,
    });
    hits.push(RestrictionHit {
        kind: HitKind::Resite,
        start: 0,
        end: head_len,
        strand,
        color: color.into(),
        label: String::new(),
        cut_col: head_cut_col,
        ext_cut_bp,
        rec_start: Some(p),
        rec_end: Some(head_len),
        top_cut_bp: Some(top_cut_bp),
        bottom_cut_bp: Some(bottom_cut_bp),
        cut_count: None,
    });
}

fn recut(start: usize, strand: i8, color: &str, name: &str) -> RestrictionHit {
    RestrictionHit {
        kind: HitKind::Recut,
        start,
        end: start + 1,
        strand,
        color: color.into(),
        label: name.into(),
        cut_col: None,
        ext_cut_bp: None,
        rec_start: None,
        rec_end: None,
        top_cut_bp: None,
        bottom_cut_bp: None,
        cut_count: None,
    }
}

fn out_of_molecule(p: usize, cut: i32, n: usize) -> bool {
    let raw = p as i32 + cut;
    raw < 0 || raw > n as i32
}

fn raw_out(raw: i32, n: usize) -> bool {
    raw < 0 || raw > n as i32
}

fn mod_cut(p: usize, cut: i32, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (p as i32 + cut).rem_euclid(n as i32) as usize
    }
}

fn wrap_raw(raw: i32, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        raw.rem_euclid(n as i32) as usize
    }
}

/// Convenience matching the Python defaults.
pub fn scan_restriction_sites_default(seq: &str) -> Vec<RestrictionHit> {
    scan_restriction_sites(seq, &ScanOptions::default())
}
