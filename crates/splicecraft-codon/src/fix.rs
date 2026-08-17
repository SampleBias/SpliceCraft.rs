//! Synonymous forbidden-site, GC-window, and repeat-diversification scrubs.

use std::collections::{BTreeMap, HashSet};

use splicecraft_bio::{forbidden_hit_set, iupac_pattern, rc};

use crate::error::CodonError;
use crate::table::{UsageTable, build_aa_map, default_forbidden};

/// Default GC-window width.
pub const GC_WINDOW_DEFAULT: usize = 50;
/// Default shared-run length for diversification.
pub const REPEAT_RUN_DEFAULT: usize = 25;

/// Expand `{name: site}` with reverse complements for non-palindromes.
#[must_use]
pub fn expand_sites(sites: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut expanded = BTreeMap::new();
    for (name, site) in sites {
        let site = site.to_ascii_uppercase();
        if site.is_empty() || iupac_pattern(&site).is_err() {
            continue;
        }
        expanded.insert(name.clone(), site.clone());
        let rc_site = rc(&site);
        if rc_site != site {
            expanded.insert(format!("{name}_rc"), rc_site);
        }
    }
    expanded
}

/// Whether swapping `alt` at `codon_start` removes `(site, idx)` and adds no new hit.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn swap_ok(
    seq: &str,
    codon_start: usize,
    alt: &str,
    site: &str,
    idx: usize,
    all_forbidden: &[&str],
    before_hits: &HashSet<(String, usize)>,
    maxlen: usize,
    window: bool,
) -> bool {
    if window {
        let a_lo = codon_start.saturating_sub(maxlen.saturating_sub(1));
        let a_hi = codon_start + 3;
        let s_hi = (a_hi + maxlen.saturating_sub(1)).min(seq.len());
        let sub = format!(
            "{}{}{}",
            &seq[a_lo..codon_start],
            alt,
            &seq[codon_start + 3..s_hi]
        );
        let sub_hits = forbidden_hit_set(&sub, all_forbidden);
        let after_win: HashSet<(String, usize)> = sub_hits
            .into_iter()
            .filter_map(|(p, off)| {
                let q = a_lo + off;
                (q < a_hi).then_some((p, q))
            })
            .collect();
        if !(a_lo <= idx && idx < a_hi) || after_win.contains(&(site.to_owned(), idx)) {
            return false;
        }
        let before_win: HashSet<(String, usize)> = before_hits
            .iter()
            .filter(|(_, q)| a_lo <= *q && *q < a_hi)
            .cloned()
            .collect();
        after_win.difference(&before_win).next().is_none()
    } else {
        let cand = format!("{}{}{}", &seq[..codon_start], alt, &seq[codon_start + 3..]);
        let after_hits = forbidden_hit_set(&cand, all_forbidden);
        if after_hits.contains(&(site.to_owned(), idx)) {
            return false;
        }
        after_hits.difference(before_hits).next().is_none()
    }
}

/// Substitute synonymous codons to remove internal restriction sites.
pub fn fix_sites(
    dna: &str,
    protein: &str,
    raw: &UsageTable,
    sites: Option<&BTreeMap<String, String>>,
    has_appended_stop: bool,
    transl_table: i32,
) -> (String, Vec<String>) {
    let owned;
    let sites = match sites {
        Some(s) => s,
        None => {
            owned = default_forbidden();
            &owned
        }
    };
    let expanded = expand_sites(sites);
    let all_owned: Vec<String> = expanded.values().cloned().collect();
    let all_forbidden: Vec<&str> = all_owned.iter().map(String::as_str).collect();
    let maxlen = all_forbidden.iter().map(|s| s.len()).max().unwrap_or(0);
    let (aa_codons, _) = build_aa_map(raw, transl_table);
    let mut dna_list: Vec<u8> = dna.as_bytes().to_vec();
    let mut fixes = Vec::new();
    let protein_u: Vec<char> = protein.chars().map(|c| c.to_ascii_uppercase()).collect();

    for (enzyme, site) in &expanded {
        let Ok(pat) = iupac_pattern(site) else {
            continue;
        };
        let mut pos = 0usize;
        loop {
            let seq = String::from_utf8_lossy(&dna_list).into_owned();
            let hit = pat.find_at(&seq, pos);
            let Some(m) = hit else {
                break;
            };
            let idx = m.start();
            let mut fixed = false;
            let lo_codon = (idx / 3).saturating_sub(1);
            let hi_codon = (idx + site.len()) / 3 + 2;
            let before_hits = forbidden_hit_set(&seq, &all_forbidden);
            let last_safe = if has_appended_stop {
                dna_list.len().saturating_sub(3)
            } else {
                dna_list.len()
            };
            for codon_idx in lo_codon..hi_codon {
                let codon_start = codon_idx * 3;
                if codon_start + 3 > last_safe {
                    break;
                }
                if codon_idx >= protein_u.len() {
                    break;
                }
                let aa = protein_u[codon_idx];
                let current =
                    String::from_utf8_lossy(&dna_list[codon_start..codon_start + 3]).into_owned();
                let Some(alts) = aa_codons.get(&aa) else {
                    continue;
                };
                for (alt, frac) in alts {
                    if alt == &current {
                        continue;
                    }
                    if !swap_ok(
                        &seq,
                        codon_start,
                        alt,
                        site,
                        idx,
                        &all_forbidden,
                        &before_hits,
                        maxlen,
                        true,
                    ) {
                        continue;
                    }
                    dna_list[codon_start..codon_start + 3].copy_from_slice(alt.as_bytes());
                    let strand = if enzyme.ends_with("_rc") { " (rc)" } else { "" };
                    let enz = enzyme.trim_end_matches("_rc");
                    fixes.push(format!(
                        "{enz}{strand} at nt {}: {current}→{alt} (codon {} {aa}, freq={frac:.3})",
                        idx + 1,
                        codon_idx + 1
                    ));
                    fixed = true;
                    break;
                }
                if fixed {
                    break;
                }
            }
            if !fixed {
                pos = idx + 1;
            }
        }
    }
    (String::from_utf8_lossy(&dna_list).into_owned(), fixes)
}

/// `(min_pct, max_pct)` GC over every `window`-base window.
#[must_use]
pub fn gc_window_range(dna: &str, window: usize) -> (f64, f64) {
    let n = dna.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let w = if window == 0 { n } else { window.min(n) };
    let bytes = dna.as_bytes();
    let is_gc = |b: u8| matches!(b, b'G' | b'C' | b'g' | b'c');
    let mut run = bytes[..w].iter().filter(|b| is_gc(**b)).count();
    let mut lo = run as f64 / w as f64 * 100.0;
    let mut hi = lo;
    for i in 1..=n - w {
        if is_gc(bytes[i - 1]) {
            run -= 1;
        }
        if is_gc(bytes[i + w - 1]) {
            run += 1;
        }
        let pct = run as f64 / w as f64 * 100.0;
        lo = lo.min(pct);
        hi = hi.max(pct);
    }
    (lo, hi)
}

/// Pull every window into `[min_gc, max_gc]` by synonymous swaps.
#[allow(clippy::too_many_arguments)]
pub fn fix_gc_window(
    dna: &str,
    protein: &str,
    raw: &UsageTable,
    window: usize,
    min_gc: Option<f64>,
    max_gc: Option<f64>,
    sites: Option<&BTreeMap<String, String>>,
    has_appended_stop: bool,
    transl_table: i32,
    avoid_kmers: Option<&HashSet<String>>,
    kmer_len: usize,
) -> Result<(String, Vec<String>), CodonError> {
    if min_gc.is_none() && max_gc.is_none() {
        return Ok((dna.to_owned(), Vec::new()));
    }
    if window == 0 {
        return Err(CodonError::parse(
            "'window' must be a positive number of bases",
        ));
    }
    if let (Some(lo), Some(hi)) = (min_gc, max_gc)
        && lo > hi
    {
        return Err(CodonError::InvertedGcBand { min: lo, max: hi });
    }
    let (aa_codons, _) = build_aa_map(raw, transl_table);
    let mut all_forbidden = Vec::new();
    if let Some(sites) = sites {
        for site in sites.values() {
            let s = site.to_ascii_uppercase();
            if s.is_empty() || iupac_pattern(&s).is_err() {
                continue;
            }
            let rc_s = rc(&s);
            all_forbidden.push(s.clone());
            if rc_s != s {
                all_forbidden.push(rc_s);
            }
        }
    }
    let forbidden_refs: Vec<&str> = all_forbidden.iter().map(String::as_str).collect();
    let mut dna_list: Vec<u8> = dna.as_bytes().to_vec();
    let last_safe = if has_appended_stop {
        dna_list.len().saturating_sub(3)
    } else {
        dna_list.len()
    };
    let protein_u: Vec<char> = protein.chars().map(|c| c.to_ascii_uppercase()).collect();
    let mut fixes = Vec::new();
    let mut stuck: HashSet<(usize, usize, bool)> = HashSet::new();
    let kmer_guard = avoid_kmers;
    let check_kmers = kmer_guard.map(|s| !s.is_empty()).unwrap_or(false) && kmer_len > 0;

    for _ in 0..dna_list.len().max(3) / 3 {
        let seq = String::from_utf8_lossy(&dna_list).into_owned();
        let Some((lo, hi, need_more)) = worst_window(&seq, window, min_gc, max_gc, &stuck) else {
            break;
        };
        let before_hits = if forbidden_refs.is_empty() {
            HashSet::new()
        } else {
            forbidden_hit_set(&seq, &forbidden_refs)
        };
        let mut cands = Vec::new();
        let c_hi = (hi / 3 + 1).min(protein_u.len());
        #[allow(clippy::needless_range_loop)]
        for codon_idx in (lo / 3)..c_hi {
            let codon_start = codon_idx * 3;
            if codon_start + 3 > last_safe {
                continue;
            }
            let aa = protein_u[codon_idx];
            let current =
                String::from_utf8_lossy(&dna_list[codon_start..codon_start + 3]).into_owned();
            let cur_gc = current
                .bytes()
                .filter(|b| matches!(b, b'G' | b'C' | b'g' | b'c'))
                .count() as i32;
            if let Some(alts) = aa_codons.get(&aa) {
                for (alt, afrac) in alts {
                    if alt == &current {
                        continue;
                    }
                    let alt_gc = alt
                        .bytes()
                        .filter(|b| matches!(b, b'G' | b'C' | b'g' | b'c'))
                        .count() as i32;
                    let delta = alt_gc - cur_gc;
                    if (need_more && delta > 0) || (!need_more && delta < 0) {
                        cands.push((
                            -delta.abs(),
                            -afrac,
                            codon_idx,
                            codon_start,
                            aa,
                            current.clone(),
                            alt.clone(),
                        ));
                    }
                }
            }
        }
        cands.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        });
        let mut applied = false;
        for (_d, _f, codon_idx, codon_start, aa, current, alt) in &cands {
            dna_list[*codon_start..*codon_start + 3].copy_from_slice(alt.as_bytes());
            if !forbidden_refs.is_empty() {
                let after = forbidden_hit_set(&String::from_utf8_lossy(&dna_list), &forbidden_refs);
                if !after.is_subset(&before_hits) {
                    dna_list[*codon_start..*codon_start + 3].copy_from_slice(current.as_bytes());
                    continue;
                }
            }
            if check_kmers {
                let guard = kmer_guard.unwrap();
                let lo_k = codon_start.saturating_sub(kmer_len.saturating_sub(1));
                let hi_k = (codon_start + 2).min(dna_list.len().saturating_sub(kmer_len));
                let mut hit = false;
                if lo_k <= hi_k {
                    for i in lo_k..=hi_k {
                        let kmer = String::from_utf8_lossy(&dna_list[i..i + kmer_len]).into_owned();
                        if guard.contains(&kmer) {
                            hit = true;
                            break;
                        }
                    }
                }
                if hit {
                    dna_list[*codon_start..*codon_start + 3].copy_from_slice(current.as_bytes());
                    continue;
                }
            }
            let direction = if need_more { "raise" } else { "lower" };
            fixes.push(format!(
                "GC {direction} at nt {}-{}: {current}→{alt} (codon {} {aa})",
                lo + 1,
                hi,
                codon_idx + 1
            ));
            applied = true;
            break;
        }
        if !applied {
            stuck.insert((lo, hi, need_more));
        }
    }
    Ok((String::from_utf8_lossy(&dna_list).into_owned(), fixes))
}

fn worst_window(
    seq: &str,
    window: usize,
    min_gc: Option<f64>,
    max_gc: Option<f64>,
    skip: &HashSet<(usize, usize, bool)>,
) -> Option<(usize, usize, bool)> {
    let n = seq.len();
    if n == 0 {
        return None;
    }
    let w = window.min(n);
    let bytes = seq.as_bytes();
    let is_gc = |b: u8| matches!(b, b'G' | b'C' | b'g' | b'c');
    let mut run = bytes[..w].iter().filter(|b| is_gc(**b)).count();
    let mut worst: Option<(f64, usize, usize, bool)> = None;
    for lo in 0..=n - w {
        if lo > 0 {
            if is_gc(bytes[lo - 1]) {
                run -= 1;
            }
            if is_gc(bytes[lo + w - 1]) {
                run += 1;
            }
        }
        let pct = run as f64 / w as f64 * 100.0;
        let (need_more, gap) = if let Some(min) = min_gc
            && pct < min
        {
            (true, min - pct)
        } else if let Some(max) = max_gc
            && pct > max
        {
            (false, pct - max)
        } else {
            continue;
        };
        if skip.contains(&(lo, lo + w, need_more)) {
            continue;
        }
        if worst.as_ref().is_none_or(|w| gap > w.0) {
            worst = Some((gap, lo, lo + w, need_more));
        }
    }
    worst.map(|(_, lo, hi, more)| (lo, hi, more))
}

/// Every k-mer across `references`.
#[must_use]
pub fn kmer_set(references: &[&str], k: usize) -> HashSet<String> {
    let mut out = HashSet::new();
    if k == 0 {
        return out;
    }
    for r in references {
        let text = r.to_ascii_uppercase();
        if text.len() < k {
            continue;
        }
        for i in 0..=text.len() - k {
            out.insert(text[i..i + k].to_owned());
        }
    }
    out
}

/// Start offsets of every `min_run` window shared with a reference.
#[must_use]
pub fn shared_runs(dna: &str, references: &[&str], min_run: usize) -> Vec<usize> {
    if min_run == 0 || dna.is_empty() {
        return Vec::new();
    }
    let kmers = kmer_set(references, min_run);
    if kmers.is_empty() {
        return Vec::new();
    }
    let up = dna.to_ascii_uppercase();
    (0..=up.len().saturating_sub(min_run))
        .filter(|i| kmers.contains(&up[*i..*i + min_run]))
        .collect()
}

/// Break shared runs of `min_run`+ bases via synonymous swaps.
#[allow(clippy::too_many_arguments)]
pub fn diversify(
    dna: &str,
    protein: &str,
    raw: &UsageTable,
    references: &[&str],
    min_run: usize,
    sites: Option<&BTreeMap<String, String>>,
    has_appended_stop: bool,
    transl_table: i32,
) -> (String, Vec<String>) {
    let refs: Vec<String> = references
        .iter()
        .map(|r| r.to_ascii_uppercase())
        .filter(|r| r.len() >= min_run)
        .collect();
    if refs.is_empty() || min_run == 0 {
        return (dna.to_owned(), Vec::new());
    }
    let ref_strs: Vec<&str> = refs.iter().map(String::as_str).collect();
    let (aa_codons, _) = build_aa_map(raw, transl_table);
    let mut all_forbidden = Vec::new();
    if let Some(sites) = sites {
        for site in sites.values() {
            let s = site.to_ascii_uppercase();
            if s.is_empty() || iupac_pattern(&s).is_err() {
                continue;
            }
            let rc_s = rc(&s);
            all_forbidden.push(s.clone());
            if rc_s != s {
                all_forbidden.push(rc_s);
            }
        }
    }
    let forbidden_refs: Vec<&str> = all_forbidden.iter().map(String::as_str).collect();
    let kmers = kmer_set(&ref_strs, min_run);
    let mut dna_list: Vec<u8> = dna.as_bytes().to_vec();
    let last_safe = if has_appended_stop {
        dna_list.len().saturating_sub(3)
    } else {
        dna_list.len()
    };
    let protein_u: Vec<char> = protein.chars().map(|c| c.to_ascii_uppercase()).collect();
    let mut fixes = Vec::new();
    let mut stuck: HashSet<usize> = HashSet::new();

    for _ in 0..dna_list.len().max(3) / 3 {
        let seq = String::from_utf8_lossy(&dna_list).into_owned();
        let hit = (0..=seq.len().saturating_sub(min_run))
            .find(|i| !stuck.contains(i) && kmers.contains(&seq[*i..*i + min_run]));
        let Some(hit) = hit else {
            break;
        };
        let before_hits = if forbidden_refs.is_empty() {
            HashSet::new()
        } else {
            forbidden_hit_set(&seq, &forbidden_refs)
        };
        let mid = hit + min_run / 2;
        let mut order: Vec<usize> =
            (hit / 3..((hit + min_run) / 3 + 1).min(protein_u.len())).collect();
        order.sort_by_key(|ci| (*ci * 3 + 1).abs_diff(mid));
        let mut applied = false;
        for codon_idx in order {
            let codon_start = codon_idx * 3;
            if codon_start + 3 > last_safe {
                continue;
            }
            let aa = protein_u[codon_idx];
            let current =
                String::from_utf8_lossy(&dna_list[codon_start..codon_start + 3]).into_owned();
            let Some(alts) = aa_codons.get(&aa) else {
                continue;
            };
            for (alt, afrac) in alts {
                if alt == &current {
                    continue;
                }
                dna_list[codon_start..codon_start + 3].copy_from_slice(alt.as_bytes());
                let window = String::from_utf8_lossy(&dna_list[hit..hit + min_run]).into_owned();
                if kmers.contains(&window) {
                    dna_list[codon_start..codon_start + 3].copy_from_slice(current.as_bytes());
                    continue;
                }
                if !forbidden_refs.is_empty() {
                    let after =
                        forbidden_hit_set(&String::from_utf8_lossy(&dna_list), &forbidden_refs);
                    if !after.is_subset(&before_hits) {
                        dna_list[codon_start..codon_start + 3].copy_from_slice(current.as_bytes());
                        continue;
                    }
                }
                fixes.push(format!(
                    "repeat at nt {}-{}: {current}→{alt} (codon {} {aa}, freq={afrac:.3})",
                    hit + 1,
                    hit + min_run,
                    codon_idx + 1
                ));
                applied = true;
                break;
            }
            if applied {
                break;
            }
        }
        if !applied {
            stuck.insert(hit);
        }
    }
    (String::from_utf8_lossy(&dna_list).into_owned(), fixes)
}

/// 0-based nucleotide starts of `_codon_fix_sites` mutation strings.
#[must_use]
pub fn fix_mutation_positions(mutations: &[String]) -> Vec<i32> {
    mutations
        .iter()
        .map(|m| {
            m.find("codon ")
                .and_then(|i| {
                    m[i + 6..]
                        .split(|c: char| !c.is_ascii_digit())
                        .next()
                        .and_then(|n| n.parse::<i32>().ok())
                })
                .map(|n| (n - 1) * 3)
                .unwrap_or(-1)
        })
        .collect()
}
