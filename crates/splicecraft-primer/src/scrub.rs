//! Clone-free restriction-site scrub (synonymous / silent substitutions).

use std::collections::{BTreeMap, HashSet};

use splicecraft_bio::{
    CustomEnzyme, ScanOptions, enzyme, enzyme_lookup, forbidden_hit_set, iupac_pattern, rc,
    scan_restriction_sites, translate_cds_table,
};
use splicecraft_codon::{UsageTable, build_aa_map};
use splicecraft_core::{Feature, bp_in};

use crate::tm::wallace_tm;

/// Default Type IIS set (Esp3I covers BsmBI).
pub const SCRUB_DEFAULT_ENZYMES: &[&str] = &["BsaI", "Esp3I", "BbsI"];
/// Edits within this many bp share one QuikChange round.
pub const SCRUB_PRIMER_FOOTPRINT: usize = 30;
const SCRUB_MAX_CHANGES: usize = 3;
const SCRUB_QC_LEN_MIN: usize = 25;
const SCRUB_QC_LEN_MAX: usize = 48;
const SCRUB_QC_MIN_TEMPLATE: usize = 60;
const SCRUB_QC_TARGET_TM: f64 = 72.0;

/// One substitution the planner chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrubEdit {
    /// 0-based genome coordinate.
    pub pos: usize,
    /// Original base.
    pub frm: char,
    /// Replacement base.
    pub to: char,
    /// CDS / non-coding label.
    pub region: String,
    /// Enzyme whose site this edit killed.
    pub enzyme: String,
}

/// One site instance the planner handled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrubSite {
    /// Enzyme name.
    pub enzyme: String,
    /// Recognition start.
    pub pos: usize,
    /// `1` / `-1`.
    pub strand: i8,
    /// Human region label.
    pub region: String,
    /// Skip reason, when skipped.
    pub reason: String,
}

/// Clone-free scrub plan (no primers).
#[derive(Clone, Debug, PartialEq)]
pub struct ScrubPlan {
    /// False only on length-drift abort.
    pub ok: bool,
    /// Input (uppercase).
    pub orig_seq: String,
    /// Same length as `orig_seq`.
    pub cured_seq: String,
    /// Resolved enzyme names.
    pub enzymes: Vec<String>,
    /// Applied substitutions.
    pub edits: Vec<ScrubEdit>,
    /// Sites successfully removed.
    pub sites_removed: Vec<ScrubSite>,
    /// Sites that could not be cured silently.
    pub sites_skipped: Vec<ScrubSite>,
    /// QuikChange round clusters.
    pub clusters: Vec<Vec<usize>>,
    /// `clusters.len()`.
    pub n_rounds: usize,
    /// Non-fatal notes.
    pub warnings: Vec<String>,
}

/// Improved-QuikChange pair for one cluster.
#[derive(Clone, Debug, PartialEq)]
pub struct QcPrimers {
    /// 1-based round index.
    pub round: usize,
    /// Cure positions in this round.
    pub positions: Vec<usize>,
    /// Forward oligo.
    pub fwd_seq: String,
    /// Reverse oligo.
    pub rev_seq: String,
    /// Forward template start (mod n).
    pub fwd_start: usize,
    /// Forward length.
    pub fwd_len: usize,
    /// Reverse template start (mod n).
    pub rev_start: usize,
    /// Reverse length.
    pub rev_len: usize,
    /// Wallace Tm (1 dp).
    pub fwd_tm: f64,
    /// Wallace Tm (1 dp).
    pub rev_tm: f64,
    /// Stratagene QuikChange Tm (1 dp).
    pub fwd_tm_qc: f64,
    /// Stratagene QuikChange Tm (1 dp).
    pub rev_tm_qc: f64,
    /// GC%.
    pub fwd_gc: f64,
    /// GC%.
    pub rev_gc: f64,
    /// Shared overlap length.
    pub overlap_len: usize,
    /// Mismatch count in the forward footprint.
    pub n_mismatch: usize,
    /// `improved` or `classic`.
    pub overlap_style: String,
    /// Design warnings.
    pub warnings: Vec<String>,
    /// Set when no pair fitted.
    pub error: Option<String>,
}

/// Resolve enzyme names → forward sites. Unknown / non-IUPAC names are skipped.
#[must_use]
pub fn resolve_sites(enzymes: &[&str], extra: &[CustomEnzyme]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for nm in enzymes {
        let site = extra
            .iter()
            .find(|e| e.name == *nm)
            .map(|e| e.site.as_str())
            .or_else(|| enzyme(nm).map(|(s, _, _)| s))
            .or_else(|| enzyme_lookup(nm, extra).map(|(s, _, _)| s));
        let Some(site) = site else {
            continue;
        };
        let site = site.to_ascii_uppercase();
        if site.is_empty() || iupac_pattern(&site).is_err() {
            continue;
        }
        out.insert((*nm).to_owned(), site);
    }
    out
}

fn expand_forbidden(forward: &BTreeMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    for site in forward.values() {
        if !out.iter().any(|s| s == site) {
            out.push(site.clone());
        }
        let rc_site = rc(site);
        if !out.iter().any(|s| s == &rc_site) {
            out.push(rc_site);
        }
    }
    out
}

fn circ_window(seq: &str, start: usize, length: usize, n: usize) -> String {
    if n == 0 || length == 0 {
        return String::new();
    }
    let end = start + length;
    if end <= n {
        seq[start..end].to_owned()
    } else {
        format!("{}{}", &seq[start..], &seq[..end - n])
    }
}

/// Extract `length` bases from `start` (mod n), wrapping the origin.
#[must_use]
pub fn circ_extract(seq: &str, start: i64, length: usize, n: usize) -> String {
    if n == 0 || length == 0 {
        return String::new();
    }
    let start = ((start % n as i64) + n as i64) as usize % n;
    if start + length <= n {
        seq[start..start + length].to_owned()
    } else {
        (0..length)
            .map(|i| seq.as_bytes()[(start + i) % n] as char)
            .collect()
    }
}

fn pos_in_feat(g: usize, s: usize, e: usize) -> bool {
    bp_in(g, s, e)
}

fn is_transition(a: u8, b: u8) -> bool {
    matches!(
        (a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (b'A', b'G') | (b'G', b'A') | (b'C', b'T') | (b'T', b'C')
    )
}

fn gc_pct(seq: &str) -> f64 {
    if seq.is_empty() {
        return 0.0;
    }
    let gc = seq
        .bytes()
        .filter(|b| matches!(b, b'G' | b'C' | b'g' | b'c'))
        .count();
    gc as f64 / seq.len() as f64 * 100.0
}

fn ends_gc(seq: &str) -> bool {
    seq.as_bytes()
        .last()
        .is_some_and(|b| matches!(b, b'G' | b'C' | b'g' | b'c'))
}

fn fullmatch(pat: &regex::Regex, s: &str) -> bool {
    pat.find(s)
        .is_some_and(|m| m.start() == 0 && m.end() == s.len())
}

#[derive(Clone)]
struct Target {
    enzyme: String,
    strand: i8,
    rec_start: usize,
    positions: Vec<usize>,
}

fn scan_targets(seq: &str, allowed: &HashSet<String>, circular: bool) -> Vec<Target> {
    let n = seq.len();
    let hits = scan_restriction_sites(
        seq,
        &ScanOptions {
            min_recognition_len: 1,
            unique_only: false,
            circular,
            allowed_enzymes: Some(allowed.iter().cloned().collect()),
            extra_enzymes: Vec::new(),
        },
    );
    let mut out = Vec::new();
    for h in hits {
        if !h.is_resite() || h.label.is_empty() {
            continue;
        }
        let rs = h.rec_start.unwrap_or(h.start);
        let re_ = h.rec_end.unwrap_or(h.end);
        let positions = if re_ < rs {
            let mut p: Vec<usize> = (rs..n).collect();
            p.extend(0..re_);
            p
        } else {
            (rs..re_).collect()
        };
        if positions.is_empty() {
            continue;
        }
        out.push(Target {
            enzyme: h.label,
            strand: h.strand,
            rec_start: rs,
            positions,
        });
    }
    out.sort_by(|a, b| {
        a.rec_start
            .cmp(&b.rec_start)
            .then(a.enzyme.cmp(&b.enzyme))
            .then(a.strand.cmp(&b.strand))
    });
    out
}

fn overlapping_feats<'a>(
    target: &Target,
    feats: &'a [Feature],
) -> (Vec<&'a Feature>, Vec<&'a Feature>) {
    let posset: HashSet<usize> = target.positions.iter().copied().collect();
    let mut cds = Vec::new();
    let mut other = Vec::new();
    for f in feats {
        if !posset.iter().any(|g| pos_in_feat(*g, f.start, f.end)) {
            continue;
        }
        if f.kind.eq_ignore_ascii_case("CDS") {
            cds.push(f);
        } else {
            other.push(f);
        }
    }
    (cds, other)
}

fn region_label(cds: &[&Feature], other: &[&Feature]) -> String {
    if !cds.is_empty() {
        let names: Vec<_> = cds
            .iter()
            .map(|f| {
                if f.label.is_empty() {
                    "CDS".to_owned()
                } else {
                    f.label.clone()
                }
            })
            .collect();
        return format!("CDS: {}", names.join(", "));
    }
    if !other.is_empty() {
        let names: Vec<_> = other
            .iter()
            .map(|f| {
                if !f.label.is_empty() {
                    f.label.clone()
                } else if !f.kind.is_empty() {
                    f.kind.clone()
                } else {
                    "?".into()
                }
            })
            .collect();
        return format!("non-coding (in {})", names.join(", "));
    }
    "non-coding".into()
}

fn feat_i32(f: &Feature, key: &str, default: i32) -> i32 {
    f.qualifiers
        .get(key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn cds_protein(seq: &str, f: &Feature) -> String {
    let n = seq.len();
    let (gpos, st) = cds_reading_positions(f, n);
    let mut dna = String::new();
    for g in gpos {
        if g >= n {
            continue;
        }
        let b = (seq.as_bytes()[g] as char).to_string();
        if st < 0 {
            dna.push_str(&rc(&b));
        } else {
            dna.push_str(&b);
        }
    }
    let table = feat_i32(f, "transl_table", 1);
    translate_cds_table(&dna, 0, dna.len(), 1, 1, table)
}

fn cds_reading_positions(f: &Feature, n: usize) -> (Vec<usize>, i8) {
    let mut gpos = Vec::new();
    if !f.parts.is_empty() {
        for p in &f.parts {
            gpos.extend(p.start..p.end.min(n));
        }
    } else if f.end < f.start {
        gpos.extend(f.start..n);
        gpos.extend(0..f.end);
    } else {
        gpos.extend(f.start..f.end.min(n));
    }
    if f.strand < 0 {
        gpos.reverse();
    }
    let cs = (feat_i32(f, "codon_start", 1).clamp(1, 3) - 1) as usize;
    (gpos.into_iter().skip(cs).collect(), f.strand)
}

fn introduces_site(
    orig: &str,
    test: &str,
    target: &Target,
    all_forbidden: &[&str],
    n: usize,
) -> bool {
    let maxlen = all_forbidden.iter().map(|s| s.len()).max().unwrap_or(8);
    let pad = maxlen.saturating_sub(1).max(1);
    let win_len = target.positions.len() + 2 * pad;
    let (a, b) = if win_len >= n {
        let wrap = maxlen.saturating_sub(1);
        (
            format!("{}{}", orig, &orig[..wrap.min(orig.len())]),
            format!("{}{}", test, &test[..wrap.min(test.len())]),
        )
    } else {
        let start = (target.rec_start + n - pad) % n;
        (
            circ_window(orig, start, win_len, n),
            circ_window(test, start, win_len, n),
        )
    };
    let after = forbidden_hit_set(&b, all_forbidden);
    let before = forbidden_hit_set(&a, all_forbidden);
    after.difference(&before).next().is_some()
}

fn combinations(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    fn rec(
        items: &[usize],
        k: usize,
        start: usize,
        cur: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if cur.len() == k {
            out.push(cur.clone());
            return;
        }
        for i in start..items.len() {
            cur.push(items[i]);
            rec(items, k, i + 1, cur, out);
            cur.pop();
        }
    }
    rec(items, k, 0, &mut cur, &mut out);
    out
}

fn cartesian(alts: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut out = vec![Vec::new()];
    for row in alts {
        let mut next = Vec::new();
        for prefix in &out {
            for &b in row {
                let mut p = prefix.clone();
                p.push(b);
                next.push(p);
            }
        }
        out = next;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn one_site(
    seq: &str,
    target: &Target,
    feats: &[Feature],
    forward: &BTreeMap<String, String>,
    all_forbidden: &[&str],
    n: usize,
    codon_frac: Option<&std::collections::HashMap<String, f64>>,
    max_changes: usize,
) -> (Option<BTreeMap<usize, char>>, String, Option<String>) {
    let positions = &target.positions;
    let site_len = positions.len();
    let (cds_feats, other_feats) = overlapping_feats(target, feats);
    let region = region_label(&cds_feats, &other_feats);
    let orig_aa: Vec<String> = cds_feats.iter().map(|f| cds_protein(seq, f)).collect();
    let mut annotated = HashSet::new();
    for f in &other_feats {
        for g in positions {
            if pos_in_feat(*g, f.start, f.end) {
                annotated.insert(*g);
            }
        }
    }
    let mut cds_frames = Vec::new();
    if codon_frac.is_some() {
        for f in &cds_feats {
            let (gpos, st) = cds_reading_positions(f, n);
            let idx: std::collections::HashMap<usize, usize> =
                gpos.iter().enumerate().map(|(j, g)| (*g, j)).collect();
            cds_frames.push((gpos, st, idx));
        }
    }
    let Some(fwd_site) = forward.get(&target.enzyme) else {
        return (
            None,
            region,
            Some("no silent substitution removes this site without creating another".into()),
        );
    };
    let Ok(pat_f) = iupac_pattern(fwd_site) else {
        return (
            None,
            region,
            Some("no silent substitution removes this site without creating another".into()),
        );
    };
    let Ok(pat_r) = iupac_pattern(&rc(fwd_site)) else {
        return (
            None,
            region,
            Some("no silent substitution removes this site without creating another".into()),
        );
    };
    let seq_bytes = seq.as_bytes();
    let mut best: Option<(ScoreKey, BTreeMap<usize, char>)> = None;
    for k in 1..=max_changes {
        for combo in combinations(positions, k) {
            let alts: Vec<Vec<u8>> = combo
                .iter()
                .map(|&g| {
                    b"ACGT"
                        .iter()
                        .copied()
                        .filter(|b| *b != seq_bytes[g].to_ascii_uppercase())
                        .collect()
                })
                .collect();
            for repl in cartesian(&alts) {
                let mut tl = seq_bytes.to_vec();
                for (j, &g) in combo.iter().enumerate() {
                    tl[g] = repl[j];
                }
                let test = String::from_utf8_lossy(&tl).into_owned();
                let win = circ_window(&test, target.rec_start, site_len, n);
                if fullmatch(&pat_f, &win) || fullmatch(&pat_r, &win) {
                    continue;
                }
                if cds_feats
                    .iter()
                    .enumerate()
                    .any(|(i, f)| cds_protein(&test, f) != orig_aa[i])
                {
                    continue;
                }
                if introduces_site(seq, &test, target, all_forbidden, n) {
                    continue;
                }
                let mut freq = 0.0;
                if let Some(frac) = codon_frac {
                    for (gpos, st, idx) in &cds_frames {
                        for &g in &combo {
                            let Some(&jj) = idx.get(&g) else {
                                continue;
                            };
                            let trip_lo = (jj / 3) * 3;
                            if trip_lo + 3 > gpos.len() {
                                continue;
                            }
                            let trip = &gpos[trip_lo..trip_lo + 3];
                            let codon: String = if *st < 0 {
                                trip.iter()
                                    .map(|p| rc(&test[*p..*p + 1]).chars().next().unwrap_or('N'))
                                    .collect()
                            } else {
                                trip.iter().map(|p| test.as_bytes()[*p] as char).collect()
                            };
                            freq += frac
                                .get(&codon.to_ascii_uppercase())
                                .copied()
                                .unwrap_or(0.0);
                        }
                    }
                }
                let key = ScoreKey {
                    k,
                    annotated: combo.iter().filter(|g| annotated.contains(g)).count(),
                    neg_freq: -((freq * 1_000_000.0).round() as i64),
                    transversions: (0..k)
                        .filter(|&j| !is_transition(seq_bytes[combo[j]], repl[j]))
                        .count(),
                    gc_changes: (0..k)
                        .filter(|&j| {
                            let a = seq_bytes[combo[j]].to_ascii_uppercase();
                            let b = repl[j].to_ascii_uppercase();
                            matches!(a, b'G' | b'C') != matches!(b, b'G' | b'C')
                        })
                        .count(),
                    repl: repl.clone(),
                };
                let changes: BTreeMap<usize, char> = combo
                    .iter()
                    .enumerate()
                    .map(|(j, g)| (*g, repl[j] as char))
                    .collect();
                if best.as_ref().is_none_or(|(s, _)| key < *s) {
                    best = Some((key, changes));
                }
            }
        }
        if best.is_some() {
            break;
        }
    }
    match best {
        Some((_, changes)) => (Some(changes), region, None),
        None => (
            None,
            region,
            Some("no silent substitution removes this site without creating another".into()),
        ),
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ScoreKey {
    k: usize,
    annotated: usize,
    neg_freq: i64,
    transversions: usize,
    gc_changes: usize,
    repl: Vec<u8>,
}

/// Group edit positions into QuikChange rounds.
#[must_use]
pub fn cluster_edits(positions: &[usize], n: usize, footprint: usize) -> Vec<Vec<usize>> {
    if positions.is_empty() {
        return Vec::new();
    }
    let mut ps = positions.to_vec();
    ps.sort_unstable();
    ps.dedup();
    let mut clusters: Vec<Vec<usize>> = vec![vec![ps[0]]];
    for &p in &ps[1..] {
        if p - clusters.last().unwrap().last().copied().unwrap_or(p) <= footprint {
            clusters.last_mut().unwrap().push(p);
        } else {
            clusters.push(vec![p]);
        }
    }
    if clusters.len() > 1 {
        let first = clusters[0][0];
        let last = *clusters.last().unwrap().last().unwrap();
        if (first + n).saturating_sub(last) <= footprint {
            let mut merged = clusters.pop().unwrap();
            merged.append(&mut clusters[0]);
            clusters[0] = merged;
        }
    }
    clusters
}

/// Plan a clone-free scrub. Substitution-only: `cured_seq` length equals `seq`.
#[must_use]
pub fn scrub_design(
    seq: &str,
    feats: &[Feature],
    enzymes: Option<&[&str]>,
    circular: bool,
    codon_raw: Option<&UsageTable>,
    extra: &[CustomEnzyme],
) -> ScrubPlan {
    let seq = seq.to_ascii_uppercase();
    let n = seq.len();
    let names: Vec<&str> = match enzymes {
        Some(e) => e.to_vec(),
        None => SCRUB_DEFAULT_ENZYMES.to_vec(),
    };
    let forward = resolve_sites(&names, extra);
    let codon_frac = codon_raw.map(|raw| build_aa_map(raw, 1).1);
    let mut result = ScrubPlan {
        ok: true,
        orig_seq: seq.clone(),
        cured_seq: seq.clone(),
        enzymes: forward.keys().cloned().collect(),
        edits: Vec::new(),
        sites_removed: Vec::new(),
        sites_skipped: Vec::new(),
        clusters: Vec::new(),
        n_rounds: 0,
        warnings: Vec::new(),
    };
    result.enzymes.sort();
    if seq.is_empty() {
        result.warnings.push("No sequence loaded.".into());
        return result;
    }
    if forward.is_empty() {
        result
            .warnings
            .push("No valid enzymes selected to scrub.".into());
        return result;
    }
    let allowed: HashSet<String> = forward.keys().cloned().collect();
    let all_owned = expand_forbidden(&forward);
    let all_forbidden: Vec<&str> = all_owned.iter().map(String::as_str).collect();
    let feats: Vec<Feature> = feats.to_vec();

    let mut working: Vec<u8> = seq.into_bytes();
    let mut failed: HashSet<(String, usize, i8)> = HashSet::new();
    let initial = scan_targets(
        std::str::from_utf8(&working).unwrap_or(""),
        &allowed,
        circular,
    );
    let max_iter = 2 * initial.len() + 8;
    let mut it = 0usize;
    let mut targets: Option<Vec<Target>> = None;
    let mut cur = String::from_utf8_lossy(&working).into_owned();
    while it < max_iter {
        it += 1;
        if targets.is_none() {
            cur = String::from_utf8_lossy(&working).into_owned();
            targets = Some(
                scan_targets(&cur, &allowed, circular)
                    .into_iter()
                    .filter(|t| !failed.contains(&(t.enzyme.clone(), t.rec_start, t.strand)))
                    .collect(),
            );
        }
        let Some(list) = targets.as_mut() else {
            break;
        };
        if list.is_empty() {
            break;
        }
        let t = list[0].clone();
        let ident = (t.enzyme.clone(), t.rec_start, t.strand);
        let (changes, region, reason) = one_site(
            &cur,
            &t,
            &feats,
            &forward,
            &all_forbidden,
            n,
            codon_frac.as_ref(),
            SCRUB_MAX_CHANGES,
        );
        if let Some(changes) = changes {
            for (g, nb) in &changes {
                result.edits.push(ScrubEdit {
                    pos: *g,
                    frm: cur.as_bytes()[*g] as char,
                    to: *nb,
                    region: region.clone(),
                    enzyme: t.enzyme.clone(),
                });
                working[*g] = *nb as u8;
            }
            result.sites_removed.push(ScrubSite {
                enzyme: t.enzyme.clone(),
                pos: t.rec_start,
                strand: t.strand,
                region,
                reason: String::new(),
            });
            targets = None;
        } else {
            result.sites_skipped.push(ScrubSite {
                enzyme: t.enzyme.clone(),
                pos: t.rec_start,
                strand: t.strand,
                region,
                reason: reason.unwrap_or_default(),
            });
            failed.insert(ident);
            list.remove(0);
        }
    }
    let cured = String::from_utf8_lossy(&working).into_owned();
    if cured.len() != n {
        result.ok = false;
        result.cured_seq = result.orig_seq.clone();
        result.edits.clear();
        result.sites_removed.clear();
        result
            .warnings
            .push("Internal error: aborted to protect sequence integrity.".into());
        return result;
    }
    result.cured_seq = cured.clone();
    for t in scan_targets(&cured, &allowed, circular) {
        let ident = (t.enzyme.clone(), t.rec_start, t.strand);
        if failed.contains(&ident) {
            continue;
        }
        result
            .sites_removed
            .retain(|r| (r.enzyme.clone(), r.pos, r.strand) != ident);
        result.sites_skipped.push(ScrubSite {
            enzyme: t.enzyme,
            pos: t.rec_start,
            strand: t.strand,
            region: "?".into(),
            reason: "could not be removed without side effects".into(),
        });
    }
    result.clusters = cluster_edits(
        &result.edits.iter().map(|e| e.pos).collect::<Vec<_>>(),
        n,
        SCRUB_PRIMER_FOOTPRINT,
    );
    result.n_rounds = result.clusters.len();
    if n > 8000 {
        result.warnings.push(format!(
            "Plasmid is {n} bp — whole-plasmid QuikChange amplifies linearly and less efficiently above ~8 kb; use a long extension time (~1 min/kb) and more template."
        ));
    }
    result
}

/// Smallest circular arc containing `positions`: `(start, end)` (`end < start` wraps).
#[must_use]
pub fn cluster_span(positions: &[usize], n: usize) -> (usize, usize) {
    let mut ps = positions.to_vec();
    ps.sort_unstable();
    ps.dedup();
    if ps.len() == 1 {
        return (ps[0], ps[0]);
    }
    let mut best_gap = 0usize;
    let mut best_i = 0usize;
    for i in 0..ps.len() {
        let gap = (ps[(i + 1) % ps.len()] + n - ps[i]) % n;
        if gap > best_gap {
            best_gap = gap;
            best_i = i;
        }
    }
    (ps[(best_i + 1) % ps.len()], ps[best_i])
}

fn qc_tm(primer: &str, n_mismatch: usize) -> f64 {
    let n = primer.len();
    if n == 0 {
        return 0.0;
    }
    81.5 + 0.41 * gc_pct(primer) - 675.0 / n as f64 - (n_mismatch as f64 / n as f64) * 100.0
}

/// Design the improved-QuikChange primer pair for one cluster.
#[must_use]
pub fn qc_primers(
    cured_seq: &str,
    positions: &[usize],
    circular: bool,
    overlap: &str,
    round_no: usize,
) -> QcPrimers {
    let _ = circular;
    let n = cured_seq.len();
    let mut res = QcPrimers {
        round: round_no,
        positions: {
            let mut p = positions.to_vec();
            p.sort_unstable();
            p
        },
        fwd_seq: String::new(),
        rev_seq: String::new(),
        fwd_start: 0,
        fwd_len: 0,
        rev_start: 0,
        rev_len: 0,
        fwd_tm: 0.0,
        rev_tm: 0.0,
        fwd_tm_qc: 0.0,
        rev_tm_qc: 0.0,
        fwd_gc: 0.0,
        rev_gc: 0.0,
        overlap_len: 0,
        n_mismatch: 0,
        overlap_style: if overlap == "classic" {
            "classic".into()
        } else {
            "improved".into()
        },
        warnings: Vec::new(),
        error: None,
    };
    if n < SCRUB_QC_MIN_TEMPLATE {
        res.error = Some(format!(
            "Plasmid is only {n} bp — too small for whole-plasmid QuikChange."
        ));
        return res;
    }
    if positions.is_empty() {
        res.error = Some("No cure positions in this round.".into());
        return res;
    }
    let (start, end) = cluster_span(positions, n);
    let width = (end + n - start) % n;
    let ext_choices: Vec<usize> = if overlap == "classic" {
        vec![0]
    } else {
        (8..17).collect()
    };
    let posset: HashSet<usize> = positions.iter().copied().collect();
    let count_mm = |fp_start: usize, fp_len: usize| {
        (0..fp_len)
            .filter(|i| posset.contains(&((fp_start + i) % n)))
            .count()
    };
    struct QcPairBest {
        fwd: String,
        rev: String,
        fwd_start: usize,
        fwd_len: usize,
        rev_start: usize,
        rev_len: usize,
        tm_f: f64,
        tm_r: f64,
        gc_f: f64,
        gc_r: f64,
        ov_len: usize,
    }
    let mut best_score = f64::INFINITY;
    let mut best: Option<QcPairBest> = None;
    for flank in 10..19 {
        let ov_start = (start + n - flank) % n;
        let ov_len = width + 1 + 2 * flank;
        for ext_f in &ext_choices {
            let fwd_len = ov_len + ext_f;
            if !(SCRUB_QC_LEN_MIN..=SCRUB_QC_LEN_MAX).contains(&fwd_len) || fwd_len >= n {
                continue;
            }
            let fwd = circ_extract(cured_seq, ov_start as i64, fwd_len, n);
            let gc_f = gc_pct(&fwd);
            if !(35.0..=68.0).contains(&gc_f) {
                continue;
            }
            let tm_f = wallace_tm(&fwd);
            for ext_r in &ext_choices {
                let rev_start = (start + n - flank - ext_r) % n;
                let rev_len = ov_len + ext_r;
                if !(SCRUB_QC_LEN_MIN..=SCRUB_QC_LEN_MAX).contains(&rev_len) || rev_len >= n {
                    continue;
                }
                let rev = rc(&circ_extract(cured_seq, rev_start as i64, rev_len, n));
                let gc_r = gc_pct(&rev);
                if !(35.0..=68.0).contains(&gc_r) {
                    continue;
                }
                let tm_r = wallace_tm(&rev);
                let score = (tm_f - SCRUB_QC_TARGET_TM).abs()
                    + (tm_r - SCRUB_QC_TARGET_TM).abs()
                    + if ends_gc(&fwd) { 0.0 } else { 6.0 }
                    + if ends_gc(&rev) { 0.0 } else { 6.0 }
                    + (gc_f - 50.0).abs() * 0.1
                    + (gc_r - 50.0).abs() * 0.1
                    + (ov_len as f64 - 21.0).abs() * 0.5
                    + (fwd_len + rev_len) as f64 * 0.02
                    + (tm_f - tm_r).abs() * 0.5;
                if score < best_score {
                    best_score = score;
                    best = Some(QcPairBest {
                        fwd: fwd.clone(),
                        rev,
                        fwd_start: ov_start,
                        fwd_len,
                        rev_start,
                        rev_len,
                        tm_f,
                        tm_r,
                        gc_f,
                        gc_r,
                        ov_len,
                    });
                }
            }
        }
    }
    let Some(QcPairBest {
        fwd,
        rev,
        fwd_start,
        fwd_len,
        rev_start,
        rev_len,
        tm_f,
        tm_r,
        gc_f,
        gc_r,
        ov_len,
    }) = best
    else {
        res.error =
            Some("No QuikChange primer pair met the length/GC constraints for this locus.".into());
        return res;
    };
    let mm_f = count_mm(fwd_start, fwd_len);
    let mm_r = count_mm(rev_start, rev_len);
    res.fwd_seq = fwd.clone();
    res.rev_seq = rev.clone();
    res.fwd_start = fwd_start;
    res.fwd_len = fwd_len;
    res.rev_start = rev_start;
    res.rev_len = rev_len;
    res.fwd_tm = (tm_f * 10.0).round() / 10.0;
    res.rev_tm = (tm_r * 10.0).round() / 10.0;
    res.fwd_tm_qc = (qc_tm(&fwd, mm_f) * 10.0).round() / 10.0;
    res.rev_tm_qc = (qc_tm(&rev, mm_r) * 10.0).round() / 10.0;
    res.fwd_gc = (gc_f * 10.0).round() / 10.0;
    res.rev_gc = (gc_r * 10.0).round() / 10.0;
    res.overlap_len = ov_len;
    res.n_mismatch = mm_f;
    if !ends_gc(&fwd) || !ends_gc(&rev) {
        res.warnings.push("No 3' G/C clamp on a primer.".into());
    }
    if res.fwd_tm_qc.min(res.rev_tm_qc) < 78.0 {
        res.warnings.push(format!(
            "QuikChange Tm below the 78 °C guideline (min {} °C).",
            res.fwd_tm_qc.min(res.rev_tm_qc)
        ));
    }
    let _ = mm_r;
    res
}

/// Prove QuikChange primers reconstitute the cured plasmid.
#[must_use]
pub fn qc_verify(orig: &str, cured: &str, rounds: &[QcPrimers], n: usize) -> (bool, Vec<String>) {
    if orig.is_empty() || orig.len() != cured.len() || n == 0 {
        return (false, vec!["sequence / length mismatch".into()]);
    }
    let mut product: Vec<u8> = orig.as_bytes().to_vec();
    let mut any_primer = false;
    for r in rounds {
        if r.error.is_some() {
            continue;
        }
        if r.fwd_seq.is_empty() || r.rev_seq.is_empty() {
            continue;
        }
        any_primer = true;
        for (i, b) in r.fwd_seq.bytes().enumerate() {
            product[(r.fwd_start + i) % n] = b;
        }
        let top = rc(&r.rev_seq);
        for (i, b) in top.bytes().enumerate() {
            product[(r.rev_start + i) % n] = b;
        }
    }
    if !any_primer {
        return (false, vec!["no primers to verify".into()]);
    }
    let got = String::from_utf8_lossy(&product);
    if got.as_ref() != cured {
        return (
            false,
            vec![
                "product does not equal the cured plasmid — a cure fell outside primer reach"
                    .into(),
            ],
        );
    }
    (true, Vec::new())
}
