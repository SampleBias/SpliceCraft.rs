//! L0-from-synthetic-fragment: real digest + ligate + close. Mismatch refused.

use splicecraft_bio::enzyme;
use splicecraft_core::Feature;

use crate::domesticator::{find_forbidden_hits, simulate_primed_amplicon};
use crate::error::CloneError;
use crate::fragment::{Fragment, close_circular, digest_to_fragments, ligate_fragments};
use crate::grammar::Grammar;

/// Built synthesis fragment plus metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SynFragment {
    /// Full linear fragment (pad+site+spacer+overhangs+body+rc tails).
    pub fragment: String,
    /// Category 5′ overhang.
    pub oh5: String,
    /// Category 3′ overhang.
    pub oh3: String,
    /// Overhangs the Type IIS cut presents.
    pub entry_oh5: String,
    /// See [`Self::entry_oh5`].
    pub entry_oh3: String,
    /// Part type used to fuse ATG.
    pub part_type: String,
    /// Grammar enzyme.
    pub enzyme: String,
    /// Two-tier nesting.
    pub nested: bool,
    /// Forbidden sites found in the cloned region (name, site, pos).
    pub internal_sites: Vec<(String, String, usize)>,
}

/// Filed L0 part plus the cloned plasmid (INV-127).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct L0Part {
    /// Display name.
    pub name: String,
    /// Grammar part type.
    pub type_name: String,
    /// Position label.
    pub position: String,
    /// Category overhangs from the grammar table — never inferred.
    pub oh5: String,
    /// See [`Self::oh5`].
    pub oh3: String,
    /// Body BETWEEN the category overhangs (neither overhang stored inline).
    pub sequence: String,
    /// Two-tier fragment.
    pub nested: bool,
    /// Grammar id.
    pub grammar: String,
    /// Always 0 for this path.
    pub level: u8,
    /// Type IIS enzyme.
    pub enzyme: String,
    /// Real cloned circle.
    pub cloned_seq: String,
    /// Features that rode through the digest+ligate.
    pub cloned_features: Vec<crate::fragment::FragFeature>,
    /// Released insert length.
    pub insert_len: usize,
}

/// Wrap `sequence` in the grammar's Type IIS ends + fusion overhangs.
pub fn build_synthesis_l0_fragment(
    sequence: &str,
    oh5: &str,
    oh3: &str,
    grammar: &Grammar,
    part_type: &str,
    entry_overhangs: Option<(&str, &str)>,
) -> SynFragment {
    let seq = clean_iupac(sequence);
    let cat5 = oh5.to_ascii_uppercase();
    let cat3 = oh3.to_ascii_uppercase();
    let ext = match entry_overhangs {
        Some((e5, e3)) => {
            let e5 = e5.to_ascii_uppercase();
            let e3 = e3.to_ascii_uppercase();
            if e5.len() == 4 && e3.len() == 4 && (e5 != cat5 || e3 != cat3) {
                Some((e5, e3))
            } else {
                None
            }
        }
        None => None,
    };
    let fragment = simulate_primed_amplicon(
        &seq,
        &cat5,
        &cat3,
        grammar,
        part_type,
        ext.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
    );
    let tail = grammar.pad.len() + grammar.site.len() + grammar.spacer.len();
    let region = if fragment.len() > 2 * tail {
        &fragment[tail..fragment.len() - tail]
    } else {
        fragment.as_str()
    };
    let hits = find_forbidden_hits(region, &grammar.forbidden_sites)
        .into_iter()
        .map(|(n, s, p)| (n, s, p + tail))
        .collect();
    SynFragment {
        fragment,
        oh5: cat5.clone(),
        oh3: cat3.clone(),
        entry_oh5: ext.as_ref().map(|(a, _)| a.clone()).unwrap_or(cat5),
        entry_oh3: ext.as_ref().map(|(_, b)| b.clone()).unwrap_or(cat3),
        part_type: part_type.into(),
        enzyme: grammar.enzyme.clone(),
        nested: ext.is_some(),
        internal_sites: hits,
    }
}

/// Digest a linear synthesised fragment; return the piece cut on both ends.
pub fn released_insert_from_fragment(
    frag_seq: &str,
    enzyme_name: &str,
) -> Result<Fragment, CloneError> {
    let seq = clean_iupac(frag_seq);
    if seq.is_empty() {
        return Err(CloneError::assembly("the fragment has no sequence"));
    }
    if enzyme_name.is_empty() {
        return Err(CloneError::assembly(
            "this grammar has no Type IIS enzyme configured",
        ));
    }
    if enzyme(enzyme_name).is_none() {
        return Err(CloneError::assembly(format!(
            "this grammar's enzyme '{enzyme_name}' isn't in the enzyme catalog — \
             fix the grammar (or add the enzyme under Settings → Enzymes); \
             the fragment is not the problem."
        )));
    }
    let pieces = digest_to_fragments(&seq, &[enzyme_name], false, &[], "");
    let cut_both: Vec<_> = pieces
        .iter()
        .filter(|p| p.left.kind != "linear" && p.right.kind != "linear")
        .cloned()
        .collect();
    if cut_both.is_empty() {
        let n_cuts = pieces.iter().filter(|p| p.right.kind != "linear").count();
        if n_cuts == 0 {
            return Err(CloneError::assembly(format!(
                "no {enzyme_name} site found — this is a plain fragment, not a wrapped L0 fragment. \
                 Build one with Synthesis → L0 Fragment."
            )));
        }
        return Err(CloneError::assembly(format!(
            "only one {enzyme_name} cut — an L0 fragment needs the enzyme on BOTH ends so the insert can be released."
        )));
    }
    if cut_both.len() > 1 {
        return Err(CloneError::assembly(format!(
            "{} {enzyme_name} cuts — the insert carries an extra internal site, so the digest would fragment it. \
             Scrub the site (Optimize) and rebuild the fragment.",
            cut_both.len() + 1
        )));
    }
    Ok(cut_both.into_iter().next().expect("len==1"))
}

/// Digest fragment + vector, ligate insert into backbone, close the circle.
pub fn clone_syn_fragment_into_entry_vector(
    frag_seq: &str,
    vector_seq: &str,
    grammar: &Grammar,
    frag_features: &[Feature],
    vector_features: &[Feature],
) -> Result<ClosedClone, CloneError> {
    let enzyme_name = grammar.enzyme.as_str();
    let mut insert = released_insert_from_fragment(frag_seq, enzyme_name)?;
    if !frag_features.is_empty() {
        let seq = clean_iupac(frag_seq);
        for p in digest_to_fragments(&seq, &[enzyme_name], false, frag_features, "") {
            if p.left.kind != "linear" && p.right.kind != "linear" {
                insert = p;
                break;
            }
        }
    }
    let vseq = clean_iupac(vector_seq);
    if vseq.is_empty() {
        return Err(CloneError::assembly("the entry vector has no sequence"));
    }
    let vpieces = digest_to_fragments(&vseq, &[enzyme_name], true, vector_features, "");
    if vpieces.len() < 2 {
        return Err(CloneError::assembly(format!(
            "the entry vector has no {enzyme_name} dropout to replace — it must be cut twice to open a slot for the insert."
        )));
    }
    let want = (
        insert.left.overhang_seq.as_str(),
        insert.right.overhang_seq.as_str(),
    );
    let keep: Vec<_> = vpieces
        .iter()
        .filter(|p| (p.left.overhang_seq.as_str(), p.right.overhang_seq.as_str()) != want)
        .cloned()
        .collect();
    if keep.len() == vpieces.len() {
        return Err(CloneError::assembly(format!(
            "the insert's {}/{} overhangs match nothing the vector releases — \
             wrong entry vector for this grammar, or the fragment was built against a different one.",
            want.0, want.1
        )));
    }
    if keep.is_empty() {
        return Err(CloneError::assembly(format!(
            "the entry vector leaves {}/{} at both cuts, so nothing survives as a backbone — \
             it can't take a directional insert.",
            want.0, want.1
        )));
    }
    let mut chain = insert.clone();
    for p in &keep {
        chain = ligate_fragments(&chain, p).ok_or_else(|| {
            CloneError::assembly(
                "the vector backbone pieces don't ligate onto the insert — \
                 check that the fragment and vector share a grammar.",
            )
        })?;
    }
    let closed = close_circular(&chain).ok_or_else(|| {
        CloneError::assembly("the assembly won't close into a circle — the free ends don't match.")
    })?;
    Ok(ClosedClone {
        sequence: closed.top_seq,
        features: closed.features,
        insert_len: insert.top_seq.len(),
        enzyme: enzyme_name.into(),
        vector_len: vseq.len(),
    })
}

/// Successful syn-frag clone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedClone {
    /// Circular product.
    pub sequence: String,
    /// Features that survived digest+ligate.
    pub features: Vec<crate::fragment::FragFeature>,
    /// Released insert length.
    pub insert_len: usize,
    /// Enzyme used.
    pub enzyme: String,
    /// Input vector length.
    pub vector_len: usize,
}

/// File a synthesised L0 fragment as a part. Overhangs come from the grammar.
pub fn l0_part_from_syn_fragment(
    frag_seq: &str,
    vector_seq: &str,
    grammar: &Grammar,
    part_type: &str,
    name: &str,
    frag_features: &[Feature],
    vector_features: &[Feature],
) -> Result<L0Part, CloneError> {
    let pos = grammar.position_for_type(part_type).ok_or_else(|| {
        let known: Vec<_> = grammar
            .positions
            .iter()
            .map(|p| p.type_name.as_str())
            .collect();
        CloneError::grammar(format!(
            "{} has no '{part_type}' position — it defines: {}.",
            if grammar.name.is_empty() {
                grammar.id.as_str()
            } else {
                grammar.name.as_str()
            },
            if known.is_empty() {
                "none".into()
            } else {
                known.join(", ")
            }
        ))
    })?;
    let oh5 = pos.oh5.to_ascii_uppercase();
    let oh3 = pos.oh3.to_ascii_uppercase();
    if oh5.len() != 4 || oh3.len() != 4 {
        return Err(CloneError::grammar(format!(
            "the '{part_type}' position has no usable 4-nt overhang pair."
        )));
    }
    let insert = released_insert_from_fragment(frag_seq, &grammar.enzyme)?;
    let region = format!("{}{}", insert.top_seq, insert.right.overhang_seq);
    let off5 = if region.len() >= 4 && region[..4] == oh5 {
        0
    } else if region.len() >= 8 && region[4..8] == oh5 {
        4
    } else {
        return Err(CloneError::assembly(format!(
            "this fragment doesn't start with the {oh5} overhang that '{part_type}' requires — \
             it was built as a different part type."
        )));
    };
    let off3 = if region.len() >= 4 && region[region.len() - 4..] == oh3 {
        0
    } else if region.len() >= 8 && region[region.len() - 8..region.len() - 4] == oh3 {
        4
    } else {
        return Err(CloneError::assembly(format!(
            "this fragment doesn't end with the {oh3} overhang that '{part_type}' requires — \
             it was built as a different part type."
        )));
    };
    let body_start = off5 + 4;
    let body_end = region.len().saturating_sub(off3 + 4);
    if body_start >= body_end {
        return Err(CloneError::assembly(format!(
            "there is nothing between the {oh5} and {oh3} overhangs — this fragment carries no insert."
        )));
    }
    let body = region[body_start..body_end].to_owned();
    let cloned = clone_syn_fragment_into_entry_vector(
        frag_seq,
        vector_seq,
        grammar,
        frag_features,
        vector_features,
    )?;
    Ok(L0Part {
        name: {
            let n = name.trim();
            if n.is_empty() {
                "part".into()
            } else {
                n.into()
            }
        },
        type_name: if pos.type_name.is_empty() {
            part_type.into()
        } else {
            pos.type_name.clone()
        },
        position: if pos.name.is_empty() {
            pos.type_name.clone()
        } else {
            pos.name.clone()
        },
        oh5,
        oh3,
        sequence: body,
        nested: off5 > 0 || off3 > 0,
        grammar: grammar.id.clone(),
        level: 0,
        enzyme: grammar.enzyme.clone(),
        cloned_seq: cloned.sequence,
        cloned_features: cloned.features,
        insert_len: cloned.insert_len,
    })
}

/// Deterministic pUPD2-shaped backbone free of BsaI/Esp3I/BsmBI on both strands.
#[must_use]
pub fn pupd2_backbone_stub(length: usize) -> String {
    let mut rng = 0xBACDBAC0u32;
    let mut bases = Vec::with_capacity(length);
    for _ in 0..length {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        bases.push(b"ACGT"[(rng >> 30) as usize]);
    }
    let forbidden: &[&[u8]] = &[b"GGTCTC", b"GAGACC", b"CGTCTC", b"GAGACG"];
    let mut i = 0;
    while i + 6 <= bases.len() {
        if forbidden.iter().any(|f| &bases[i..i + 6] == *f) {
            let middle = i + 3;
            let current = bases[middle];
            for &rep in b"ACGT" {
                if rep != current {
                    bases[middle] = rep;
                    break;
                }
            }
            i = i.saturating_sub(5);
            continue;
        }
        i += 1;
    }
    String::from_utf8(bases).expect("ACGT")
}

/// Circular acceptor that releases `e5`/`e3` when cut with the grammar enzyme.
#[must_use]
pub fn stub_entry_vector(grammar: &Grammar, e5: &str, e3: &str) -> String {
    let pad = grammar.pad.as_str();
    let site = grammar.site.as_str();
    let spacer = grammar.spacer.as_str();
    format!(
        "{}{pad}{site}{spacer}{e5}{}{e3}{spacer}{}{pad}{}",
        "GGCGCGTTAACCGGTTAACCGG".repeat(4),
        "TTTTGGGGCCCCAAAA".repeat(2),
        splicecraft_bio::rc(site),
        "ACGTACGTTGCA".repeat(4),
    )
}

fn clean_iupac(s: &str) -> String {
    s.chars()
        .map(|c| c.to_ascii_uppercase())
        .map(|c| if c == 'U' { 'T' } else { c })
        .filter(|c| "ACGTRYSWKMBDHVN".contains(*c))
        .collect()
}
