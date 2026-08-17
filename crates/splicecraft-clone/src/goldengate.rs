//! Golden Gate / MoClo one-pot: digest + overhang chain + close.

use crate::domesticator::simulate_primed_amplicon;
use crate::error::CloneError;
use crate::fragment::{
    Fragment, close_circular, digest_to_fragments, enzyme_is_type_iis, ligate_fragments,
};
use crate::grammar::Grammar;
use crate::parts::PartRecord;
use crate::synfrag::stub_entry_vector;

/// Golden Gate simulator result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenGateResult {
    /// Closed circle assembled.
    pub ok: bool,
    /// Product top strand.
    pub product_seq: String,
    /// Always circular on success.
    pub circular: bool,
    /// Enzyme used.
    pub enzyme: String,
    /// Junction overhangs in chain order.
    pub junctions: Vec<String>,
    /// Residual enzyme sites in the product.
    pub n_residual_sites: usize,
    /// Soft notes.
    pub warnings: Vec<String>,
    /// Hard failures.
    pub errors: Vec<String>,
}

/// Fragments cut on both ends.
#[must_use]
pub fn gg_released_bodies(seq: &str, enzyme: &str, circular: bool) -> Vec<Fragment> {
    digest_to_fragments(seq, &[enzyme], circular, &[], "")
        .into_iter()
        .filter(|f| f.left.kind != "linear" && f.right.kind != "linear")
        .collect()
}

/// Order `parts` after `start` by overhang match. `None` if a part is left over.
#[must_use]
pub fn gg_greedy_chain<'a>(
    start: &'a Fragment,
    parts: &'a [Fragment],
) -> Option<Vec<&'a Fragment>> {
    let mut chain = vec![start];
    let mut pool: Vec<&Fragment> = parts.iter().collect();
    while !pool.is_empty() {
        let cur = chain[chain.len() - 1];
        let idx = pool
            .iter()
            .position(|f| ligate_fragments(cur, f).is_some())?;
        chain.push(pool.remove(idx));
    }
    Some(chain)
}

/// One-pot Type IIS assembly of part sequences + a destination vector.
#[must_use]
pub fn simulate_golden_gate(
    part_seqs: &[&str],
    vector_seq: &str,
    enzyme: &str,
) -> GoldenGateResult {
    if !enzyme_is_type_iis(enzyme) {
        return GoldenGateResult {
            ok: false,
            product_seq: String::new(),
            circular: false,
            enzyme: enzyme.into(),
            junctions: Vec::new(),
            n_residual_sites: 0,
            warnings: Vec::new(),
            errors: vec![format!(
                "'{enzyme}' is not a Type IIS enzyme — Golden Gate / MoClo needs one that cuts \
                 OUTSIDE its recognition site (BsaI / BsmBI / BbsI / SapI / Esp3I)."
            )],
        };
    }
    if part_seqs.is_empty() {
        return GoldenGateResult {
            ok: false,
            product_seq: String::new(),
            circular: false,
            enzyme: enzyme.into(),
            junctions: Vec::new(),
            n_residual_sites: 0,
            warnings: Vec::new(),
            errors: vec!["no parts supplied".into()],
        };
    }
    let mut part_frags = Vec::new();
    for (i, ps) in part_seqs.iter().enumerate() {
        let bodies = gg_released_bodies(&ps.to_ascii_uppercase(), enzyme, false);
        if bodies.len() != 1 {
            return GoldenGateResult {
                ok: false,
                product_seq: String::new(),
                circular: false,
                enzyme: enzyme.into(),
                junctions: Vec::new(),
                n_residual_sites: 0,
                warnings: Vec::new(),
                errors: vec![format!(
                    "part {} released {} fragment(s) when cut with {enzyme} (expected exactly 1).",
                    i + 1,
                    bodies.len()
                )],
            };
        }
        part_frags.push(bodies.into_iter().next().expect("len==1"));
    }
    let vec_bodies = gg_released_bodies(&vector_seq.to_ascii_uppercase(), enzyme, true);
    if vec_bodies.is_empty() {
        return GoldenGateResult {
            ok: false,
            product_seq: String::new(),
            circular: false,
            enzyme: enzyme.into(),
            junctions: Vec::new(),
            n_residual_sites: 0,
            warnings: Vec::new(),
            errors: vec![format!(
                "vector released no fragment when cut with {enzyme} — it needs two {enzyme} sites flanking the dropout."
            )],
        };
    }
    let mut chosen: Option<(Vec<Fragment>, String)> = None;
    for vstart in &vec_bodies {
        let Some(chain) = gg_greedy_chain(vstart, &part_frags) else {
            continue;
        };
        let mut ligated = chain[0].clone();
        let mut good = true;
        for f in chain.iter().skip(1) {
            match ligate_fragments(&ligated, f) {
                Some(m) => ligated = m,
                None => {
                    good = false;
                    break;
                }
            }
        }
        if !good {
            continue;
        }
        if let Some(closed) = close_circular(&ligated) {
            chosen = Some((chain.into_iter().cloned().collect(), closed.top_seq));
            break;
        }
    }
    let Some((chain, product)) = chosen else {
        return GoldenGateResult {
            ok: false,
            product_seq: String::new(),
            circular: false,
            enzyme: enzyme.into(),
            junctions: Vec::new(),
            n_residual_sites: 0,
            warnings: Vec::new(),
            errors: vec![
                "the overhangs don't chain every part + the vector into a closed circle — \
                 check that adjacent parts share a 4 nt overhang and the vector's two \
                 overhangs match the assembly's ends."
                    .into(),
            ],
        };
    };
    let junctions: Vec<String> = chain.iter().map(|f| f.right.overhang_seq.clone()).collect();
    let mut warnings = Vec::new();
    let unique: std::collections::HashSet<&str> = junctions.iter().map(String::as_str).collect();
    if unique.len() != junctions.len() {
        warnings.push(
            "non-unique junction overhang(s) — the assembly is ambiguous and can mis-assemble; \
             redesign to all-distinct 4 nt overhangs."
                .into(),
        );
    }
    let residual = splicecraft_bio::enzyme_cuts(&product, &[enzyme], true);
    if !residual.is_empty() {
        warnings.push(format!(
            "{} residual {enzyme} site(s) in the product — it would be re-cut during the one-pot reaction; \
             domesticate them out of the parts first.",
            residual.len()
        ));
    }
    GoldenGateResult {
        ok: true,
        product_seq: product,
        circular: true,
        enzyme: enzyme.into(),
        junctions,
        n_residual_sites: residual.len(),
        warnings,
        errors: Vec::new(),
    }
}

/// Wrap each part as a primed amplicon and Golden-Gate into a stub (or given) vector.
pub fn assemble_parts(
    parts: &[PartRecord],
    grammar: &Grammar,
    vector_seq: Option<&str>,
) -> Result<GoldenGateResult, CloneError> {
    if parts.is_empty() {
        return Err(CloneError::assembly("no parts supplied"));
    }
    for p in parts {
        if p.sequence.is_empty() {
            return Err(CloneError::assembly(format!(
                "refusing to assemble part '{}' — empty body cannot assemble",
                p.name
            )));
        }
    }
    let amplicons: Vec<String> = parts
        .iter()
        .map(|p| simulate_primed_amplicon(&p.sequence, &p.oh5, &p.oh3, grammar, &p.type_name, None))
        .collect();
    let refs: Vec<&str> = amplicons.iter().map(String::as_str).collect();
    let (tu5, tu3) = grammar.tu_overhangs();
    let stub = stub_entry_vector(grammar, &tu5, &tu3);
    let vector = vector_seq.unwrap_or(&stub);
    let result = simulate_golden_gate(&refs, vector, &grammar.enzyme);
    if !result.ok {
        return Err(CloneError::assembly(
            result
                .errors
                .first()
                .cloned()
                .unwrap_or_else(|| "Golden Gate assembly failed".into()),
        ));
    }
    Ok(result)
}
