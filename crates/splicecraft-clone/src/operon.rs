//! Native operon SOE domestication primers.

use splicecraft_bio::{CustomEnzyme, rc};
use splicecraft_codon::UsageTable;
use splicecraft_core::Feature;
use splicecraft_primer::{
    SCRUB_PRIMER_FOOTPRINT, ScrubEdit, binding_max_len, pick_binding_region, primer_tm,
    resolve_sites, scrub_design,
};

use crate::domesticator::find_forbidden_hits;
use crate::error::CloneError;
use crate::grammar::Grammar;

const MIN_OPERON: usize = 40;
const MIN_BIND: usize = 18;
const SOE_PRIMER_WARN_LEN: usize = 100;
const OVERLAP_ARM: usize = 18;

/// One oligo in an operon SOE set.
#[derive(Clone, Debug, PartialEq)]
pub struct OperonPrimer {
    /// `{part}-DOM-{idx}-F/R`.
    pub name: String,
    /// Full oligo.
    pub seq: String,
    /// `flank-fwd` / `flank-rev` / `soe-fwd` / `soe-rev`.
    pub kind: String,
    /// Wallace Tm (1 dp).
    pub tm: f64,
    /// Covered `[start, end)` on the cured operon.
    pub covers: (usize, usize),
}

/// Result of [`design_operon_soe_primers`].
#[derive(Clone, Debug, PartialEq)]
pub struct OperonSoE {
    /// True when primers are ready to order.
    pub ok: bool,
    /// True when non-coding (or uncurable) sites need manual edits.
    pub needs_manual: bool,
    /// Fatal refusal.
    pub error: Option<String>,
    /// Cured operon (same length as input).
    pub cured_seq: String,
    /// Applied substitutions (CDS auto + manual).
    pub edits: Vec<ScrubEdit>,
    /// Flank + mutagenic pairs.
    pub primers: Vec<OperonPrimer>,
    /// Mutagenic cluster count.
    pub n_clusters: usize,
    /// TU / OPERON overhangs.
    pub overhangs: (String, String),
    /// ATG-fusion skip on the forward flank.
    pub fwd_skip: usize,
    /// Predicted amplicon length.
    pub amplicon_len: usize,
    /// Non-fatal notes.
    pub warnings: Vec<String>,
    /// Remaining forbidden hits when `needs_manual`.
    pub sites_skipped: Vec<(String, String, usize, bool)>,
}

fn pair_names(part: &str, idx: usize) -> (String, String) {
    let base = if part.trim().is_empty() {
        "part"
    } else {
        part.trim()
    };
    (format!("{base}-DOM-{idx}-F"), format!("{base}-DOM-{idx}-R"))
}

/// Design SOE primers that lift a native operon and cure grammar-forbidden sites.
#[allow(clippy::too_many_arguments)]
pub fn design_operon_soe_primers(
    operon_seq: &str,
    feats: &[Feature],
    grammar: &Grammar,
    manual_edits: &[(usize, char)],
    extra_enzymes: &[&str],
    codon_raw: Option<&UsageTable>,
    extra_catalog: &[CustomEnzyme],
    target_tm: f64,
) -> Result<OperonSoE, CloneError> {
    let operon_seq = operon_seq.to_ascii_uppercase();
    let n = operon_seq.len();
    if n < MIN_OPERON {
        return Err(CloneError::assembly(format!(
            "Operon is too short ({n} bp) for SOE domestication."
        )));
    }
    let mut forbidden = grammar.forbidden_sites.clone();
    if !extra_enzymes.is_empty() {
        forbidden.extend(resolve_sites(extra_enzymes, extra_catalog));
    }
    let enzymes: Vec<&str> = forbidden.keys().map(String::as_str).collect();
    let scrub = scrub_design(
        &operon_seq,
        feats,
        Some(&enzymes),
        false,
        codon_raw,
        extra_catalog,
    );
    if !scrub.ok {
        return Err(CloneError::assembly(
            scrub
                .warnings
                .first()
                .cloned()
                .unwrap_or_else(|| "cure failed".into()),
        ));
    }
    let cds_spans: Vec<(usize, usize)> = feats
        .iter()
        .filter(|f| f.kind.eq_ignore_ascii_case("CDS"))
        .map(|f| (f.start, f.end))
        .collect();
    let in_cds = |p: usize| {
        cds_spans.iter().any(|&(s, e)| {
            if e >= s {
                s <= p && p < e
            } else {
                p >= s || p < e
            }
        })
    };
    let mut working: Vec<u8> = operon_seq.as_bytes().to_vec();
    let mut edits = Vec::new();
    for e in &scrub.edits {
        if !in_cds(e.pos) {
            continue;
        }
        working[e.pos] = e.to as u8;
        edits.push(ScrubEdit {
            pos: e.pos,
            to: e.to,
            frm: operon_seq.as_bytes()[e.pos] as char,
            region: "CDS".into(),
            enzyme: e.enzyme.clone(),
        });
    }
    for &(p, b) in manual_edits {
        let b = b.to_ascii_uppercase();
        if p < n && matches!(b, 'A' | 'C' | 'G' | 'T') {
            working[p] = b as u8;
            edits.push(ScrubEdit {
                pos: p,
                to: b,
                frm: operon_seq.as_bytes()[p] as char,
                region: "manual".into(),
                enzyme: String::new(),
            });
        }
    }
    let cured = String::from_utf8_lossy(&working).into_owned();
    if cured.len() != n {
        return Err(CloneError::assembly(
            "internal: cure changed the sequence length",
        ));
    }
    let remaining = find_forbidden_hits(&cured, &forbidden);
    if !remaining.is_empty() {
        let flagged = remaining
            .iter()
            .map(|(nm, st, ps)| (nm.clone(), st.clone(), *ps, in_cds(*ps)))
            .collect();
        return Ok(OperonSoE {
            ok: false,
            needs_manual: true,
            error: None,
            cured_seq: cured,
            edits,
            primers: Vec::new(),
            n_clusters: 0,
            overhangs: (String::new(), String::new()),
            fwd_skip: 0,
            amplicon_len: 0,
            warnings: scrub.warnings,
            sites_skipped: flagged,
        });
    }
    let mut positions: Vec<usize> = edits.iter().map(|e| e.pos).collect();
    positions.sort_unstable();
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    if !positions.is_empty() {
        clusters.push(vec![positions[0]]);
        for &p in &positions[1..] {
            if p - *clusters.last().unwrap().last().unwrap() <= SCRUB_PRIMER_FOOTPRINT {
                clusters.last_mut().unwrap().push(p);
            } else {
                clusters.push(vec![p]);
            }
        }
    }
    let (op_oh5, op_oh3) = grammar
        .position_overhangs(&["OPERON", "CDS"])
        .unwrap_or_else(|| {
            let tu = grammar.tu_overhangs();
            if tu.0.is_empty() || tu.1.is_empty() {
                ("AATG".into(), "GCTT".into())
            } else {
                tu
            }
        });
    let fwd_tail = format!("{}{}{}{op_oh5}", grammar.pad, grammar.site, grammar.spacer);
    let rev_tail = format!(
        "{}{}{}{}",
        grammar.pad,
        grammar.site,
        grammar.spacer,
        rc(&op_oh3)
    );
    let fwd_skip = if cured[..3.min(cured.len())] == *"ATG" {
        grammar.atg_offset(&op_oh5, "OPERON")
    } else {
        0
    };
    let fwd_max = binding_max_len(fwd_tail.len(), MIN_BIND);
    let rev_max = binding_max_len(rev_tail.len(), MIN_BIND);
    let (fwd_bind, fwd_tm) = pick_binding_region(
        &cured[fwd_skip.min(cured.len())..],
        target_tm,
        MIN_BIND,
        fwd_max,
    );
    let (rev_bind, rev_tm) = pick_binding_region(&rc(&cured), target_tm, MIN_BIND, rev_max);
    let mut primers = Vec::new();
    let (fwd_name, rev_name) = pair_names("operon", 1);
    primers.push(OperonPrimer {
        name: fwd_name,
        seq: format!("{fwd_tail}{fwd_bind}"),
        kind: "flank-fwd".into(),
        tm: (fwd_tm * 10.0).round() / 10.0,
        covers: (fwd_skip, fwd_skip + fwd_bind.len()),
    });
    primers.push(OperonPrimer {
        name: rev_name,
        seq: format!("{rev_tail}{rev_bind}"),
        kind: "flank-rev".into(),
        tm: (rev_tm * 10.0).round() / 10.0,
        covers: (n.saturating_sub(rev_bind.len()), n),
    });
    let mut cover = vec![
        (fwd_skip, fwd_skip + fwd_bind.len()),
        (n.saturating_sub(rev_bind.len()), n),
    ];
    let mut warnings = scrub.warnings;
    for (j, cl) in clusters.iter().enumerate() {
        let mut ws = cl[0].saturating_sub(OVERLAP_ARM);
        let mut we = (cl.last().copied().unwrap_or(0) + 1 + OVERLAP_ARM).min(n);
        if we - ws < 2 * OVERLAP_ARM {
            if ws == 0 {
                we = (2 * OVERLAP_ARM).min(n);
            } else {
                ws = if we == n {
                    n.saturating_sub(2 * OVERLAP_ARM)
                } else {
                    we.saturating_sub(2 * OVERLAP_ARM)
                };
            }
        }
        let win = &cured[ws..we];
        if win.len() > SOE_PRIMER_WARN_LEN {
            warnings.push(format!(
                "cluster {}: SOE mutagenic primer is {} nt (> {SOE_PRIMER_WARN_LEN}) — near/over the standard oligo-synthesis limit; order as an ultramer or split the edits.",
                j + 1,
                win.len()
            ));
        }
        let (fj, rj) = pair_names("operon", j + 2);
        let tm = primer_tm(win).unwrap_or(0.0);
        primers.push(OperonPrimer {
            name: fj,
            seq: win.to_owned(),
            kind: "soe-fwd".into(),
            tm: (tm * 10.0).round() / 10.0,
            covers: (ws, we),
        });
        primers.push(OperonPrimer {
            name: rj,
            seq: rc(win),
            kind: "soe-rev".into(),
            tm: (primer_tm(&rc(win)).unwrap_or(0.0) * 10.0).round() / 10.0,
            covers: (ws, we),
        });
        cover.push((ws, we));
    }
    for e in &edits {
        if !cover.iter().any(|&(s, q)| s <= e.pos && e.pos < q) {
            return Err(CloneError::assembly(format!(
                "cure at +{} falls outside every primer — the SOE set would not introduce it. Aborting (catastrophic-class primer safety).",
                e.pos + 1
            )));
        }
    }
    if !find_forbidden_hits(&cured, &forbidden).is_empty() {
        return Err(CloneError::assembly(
            "internal: forbidden site survived the cure",
        ));
    }
    Ok(OperonSoE {
        ok: true,
        needs_manual: false,
        error: None,
        cured_seq: cured,
        edits,
        primers,
        n_clusters: clusters.len(),
        overhangs: (op_oh5, op_oh3),
        fwd_skip,
        amplicon_len: fwd_tail.len() + (n - fwd_skip) + rev_tail.len(),
        warnings,
        sites_skipped: Vec::new(),
    })
}
