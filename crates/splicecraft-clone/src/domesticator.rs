//! Type IIS domestication primers, with optional synonymous codon repair.

use splicecraft_bio::rc;
use splicecraft_codon::{UsageTable, fix_sites};
use splicecraft_core::slice_circular;
use splicecraft_primer::{binding_max_len, mut_translate, pick_binding_region};

use crate::error::CloneError;
use crate::grammar::Grammar;

const MIN_BIND: usize = 18;

/// One designed domestication pair.
#[derive(Clone, Debug, PartialEq)]
pub struct DomesticationPrimers {
    /// Part type that was designed.
    pub part_type: String,
    /// Position label from the grammar.
    pub position: String,
    /// Category 5′ overhang.
    pub oh5: String,
    /// Category 3′ overhang.
    pub oh3: String,
    /// Overhangs the Type IIS cut actually presents (entry pair when nested).
    pub entry_oh5: String,
    /// See [`Self::entry_oh5`].
    pub entry_oh3: String,
    /// Insert that was primed (may be codon-repaired in stage 09).
    pub insert_seq: String,
    /// Full forward oligo.
    pub fwd_full: String,
    /// Full reverse oligo.
    pub rev_full: String,
    /// Forward annealing region.
    pub fwd_binding: String,
    /// Reverse annealing region.
    pub rev_binding: String,
    /// Binding Tm, 1 dp.
    pub fwd_tm: f64,
    /// Binding Tm, 1 dp.
    pub rev_tm: f64,
    /// Predicted amplicon length.
    pub amplicon_len: usize,
    /// Grammar pad.
    pub enzyme_pad: String,
    /// Grammar recognition site.
    pub enzyme_site: String,
    /// Grammar spacer.
    pub enzyme_spacer: String,
    /// Silent mutations applied to the insert (empty when none).
    pub mutations: Vec<String>,
}

/// Join a 5′ fusion overhang to a coding body, collapsing a duplicated ATG.
#[must_use]
pub fn fuse_overhang_body(oh5: &str, body: &str, part_type: &str, grammar: &Grammar) -> String {
    let off = grammar.atg_offset(oh5, part_type);
    let body = if off > 0 && body.len() >= 3 && body[..3].eq_ignore_ascii_case("ATG") {
        &body[off..]
    } else {
        body
    };
    format!("{oh5}{body}")
}

/// PCR amplicon top strand: pad+site+spacer+[ent5]+oh5+insert+oh3+[ent3]+rc tails.
#[must_use]
pub fn simulate_primed_amplicon(
    insert: &str,
    oh5: &str,
    oh3: &str,
    grammar: &Grammar,
    part_type: &str,
    entry_overhangs: Option<(&str, &str)>,
) -> String {
    let left_tail = format!("{}{}{}", grammar.pad, grammar.site, grammar.spacer);
    let right_tail = format!(
        "{}{}{}",
        rc(&grammar.spacer),
        rc(&grammar.site),
        rc(&grammar.pad)
    );
    let (ent5, ent3) = nested_entry(oh5, oh3, entry_overhangs);
    format!(
        "{left_tail}{ent5}{}{oh3}{ent3}{right_tail}",
        fuse_overhang_body(oh5, insert, part_type, grammar)
    )
}

/// Design GB / MoClo domestication primers. Coding parts may be codon-repaired
/// when `codon_raw` is supplied.
#[allow(clippy::too_many_arguments)]
pub fn design_gb_primers(
    template_seq: &str,
    start: usize,
    end: usize,
    part_type: &str,
    grammar: &Grammar,
    target_tm: f64,
    entry_overhangs: Option<(&str, &str)>,
    codon_raw: Option<&UsageTable>,
) -> Result<DomesticationPrimers, CloneError> {
    let pos = grammar.position_for_type(part_type).ok_or_else(|| {
        let known: Vec<_> = grammar
            .positions
            .iter()
            .map(|p| p.type_name.as_str())
            .collect();
        CloneError::grammar(format!(
            "Part type '{part_type}' is not defined in grammar '{}'. Available types: {}.",
            grammar.name,
            known.join(", ")
        ))
    })?;
    let oh5 = pos.oh5.to_ascii_uppercase();
    let oh3 = pos.oh3.to_ascii_uppercase();
    let (ent5, ent3) = nested_entry(&oh5, &oh3, entry_overhangs);
    let (entry_oh5, entry_oh3) = if ent5.is_empty() {
        (oh5.clone(), oh3.clone())
    } else {
        (ent5.clone(), ent3.clone())
    };

    let total = template_seq.len();
    let mut insert = slice_circular(&template_seq.to_ascii_uppercase(), start, end);
    if insert.len() < MIN_BIND {
        return Err(CloneError::assembly(format!(
            "Cloning region is too short ({} bp). Select at least {MIN_BIND} bp.",
            insert.len()
        )));
    }

    let mut mutations = Vec::new();
    let hits = find_forbidden_hits(&insert, &grammar.forbidden_sites);
    if !hits.is_empty() {
        let hit_str = hits
            .iter()
            .map(|(n, s, p)| format!("{n} {s} at +{}", p + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let can_attempt_fix =
            grammar.is_coding(part_type) && codon_raw.is_some() && insert.len().is_multiple_of(3);
        if can_attempt_fix {
            let Some(raw) = codon_raw else {
                unreachable!("can_attempt_fix implies codon_raw");
            };
            let protein = mut_translate(&insert);
            let expected_codons = insert.len() / 3;
            if expected_codons > 0 && protein.len() < (expected_codons as f64 * 0.9) as usize {
                return Err(CloneError::assembly(format!(
                    "CDS reading-frame validation failed: translated protein is {} aa but the {} bp insert should encode ~{expected_codons} aa. The selection is likely off-frame (check codon_start, partial CDS, or adjust selection boundaries).",
                    protein.len(),
                    insert.len()
                )));
            }
            if protein.is_empty() {
                return Err(CloneError::assembly(format!(
                    "Internal Type IIS site(s) found ({hit_str}) but the insert could not be translated for silent mutation — pick a different region."
                )));
            }
            let (fixed, fixes) = fix_sites(
                &insert,
                &protein,
                raw,
                Some(&grammar.forbidden_sites),
                false,
                1,
            );
            mutations = fixes;
            let remaining = find_forbidden_hits(&fixed, &grammar.forbidden_sites);
            if !remaining.is_empty() {
                let remain_str = remaining
                    .iter()
                    .map(|(n, s, p)| format!("{n} {s} at +{}", p + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(CloneError::assembly(format!(
                    "Internal Type IIS site(s) remain after silent-mutation attempt ({remain_str}). The sites overlap codons with no synonymous alternative in this codon table — pick a different region or redesign."
                )));
            }
            insert = fixed;
        } else {
            let mut reasons = Vec::new();
            if !grammar.is_coding(part_type) {
                reasons.push(format!("{part_type} is non-coding"));
            } else {
                if codon_raw.is_none() {
                    reasons.push("no codon table selected".into());
                }
                if !insert.len().is_multiple_of(3) {
                    reasons.push(format!(
                        "insert length {} bp is not a multiple of 3",
                        insert.len()
                    ));
                }
            }
            let extra = if reasons.is_empty() {
                String::new()
            } else {
                format!(" ({})", reasons.join("; "))
            };
            return Err(CloneError::assembly(format!(
                "Internal Type IIS site(s) found: {hit_str}.{extra} Silent-mutation repair needs a coding part, a codon table, and an in-frame insert."
            )));
        }
    }

    let fused = simulate_primed_amplicon(&insert, &oh5, &oh3, grammar, part_type, entry_overhangs);
    let tail_len = grammar.pad.len() + grammar.site.len() + grammar.spacer.len();
    let region = if fused.len() > 2 * tail_len {
        &fused[tail_len..fused.len() - tail_len]
    } else {
        fused.as_str()
    };
    let junction_hits = find_forbidden_hits(region, &grammar.forbidden_sites);
    if !junction_hits.is_empty() {
        let jstr = junction_hits
            .iter()
            .map(|(n, s, p)| format!("{n} {s} at +{}", p + 1))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CloneError::assembly(format!(
            "A Type IIS site forms across a fusion junction in the cloned part ({jstr}) — \
             it would self-cut during domestication / L1 even though the insert body is clean."
        )));
    }

    let fwd_skip = grammar.atg_offset(&oh5, part_type);
    let fwd_insert = if fwd_skip > 0 && fwd_skip <= insert.len() {
        &insert[fwd_skip..]
    } else {
        insert.as_str()
    };
    let fwd_tail = format!(
        "{}{}{}{ent5}{oh5}",
        grammar.pad, grammar.site, grammar.spacer
    );
    let rev_tail = format!(
        "{}{}{}{}{}",
        grammar.pad,
        grammar.site,
        grammar.spacer,
        rc(&ent3),
        rc(&oh3)
    );
    let fwd_max = binding_max_len(fwd_tail.len(), MIN_BIND);
    let rev_max = binding_max_len(rev_tail.len(), MIN_BIND);
    let (fwd_bind, fwd_tm) = pick_binding_region(fwd_insert, target_tm, MIN_BIND, fwd_max);
    let (rev_bind, rev_tm) = pick_binding_region(&rc(&insert), target_tm, MIN_BIND, rev_max);
    let fwd_full = format!("{fwd_tail}{fwd_bind}");
    let rev_full = format!("{rev_tail}{rev_bind}");
    let mut amplicon_len = fwd_tail.len() + insert.len() + rev_tail.len();
    if fwd_skip > 0 && insert.len() >= 3 && insert[..3].eq_ignore_ascii_case("ATG") {
        amplicon_len = amplicon_len.saturating_sub(fwd_skip);
    }
    let _ = total;
    Ok(DomesticationPrimers {
        part_type: part_type.into(),
        position: pos.name.clone(),
        oh5,
        oh3,
        entry_oh5,
        entry_oh3,
        insert_seq: insert,
        fwd_full,
        rev_full,
        fwd_binding: fwd_bind,
        rev_binding: rev_bind,
        fwd_tm: (fwd_tm * 10.0).round() / 10.0,
        rev_tm: (rev_tm * 10.0).round() / 10.0,
        amplicon_len,
        enzyme_pad: grammar.pad.clone(),
        enzyme_site: grammar.site.clone(),
        enzyme_spacer: grammar.spacer.clone(),
        mutations,
    })
}

/// Every forbidden-site hit on both strands (exact string match, like upstream).
#[must_use]
pub fn find_forbidden_hits(
    seq: &str,
    sites: &std::collections::BTreeMap<String, String>,
) -> Vec<(String, String, usize)> {
    let mut out = Vec::new();
    for (name, site) in sites {
        if site.is_empty() {
            continue;
        }
        let rc_site = rc(site);
        let needles = if rc_site == *site {
            vec![site.clone()]
        } else {
            vec![site.clone(), rc_site]
        };
        for needle in needles {
            let mut start = 0;
            while let Some(pos) = seq[start..].find(&needle) {
                let abs = start + pos;
                out.push((name.clone(), needle.clone(), abs));
                start = abs + 1;
            }
        }
    }
    out.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    out
}

fn nested_entry(oh5: &str, oh3: &str, entry: Option<(&str, &str)>) -> (String, String) {
    let Some((e5, e3)) = entry else {
        return (String::new(), String::new());
    };
    let e5 = e5.to_ascii_uppercase();
    let e3 = e3.to_ascii_uppercase();
    if e5.len() == 4
        && e3.len() == 4
        && (e5 != oh5.to_ascii_uppercase() || e3 != oh3.to_ascii_uppercase())
    {
        (e5, e3)
    } else {
        (String::new(), String::new())
    }
}
