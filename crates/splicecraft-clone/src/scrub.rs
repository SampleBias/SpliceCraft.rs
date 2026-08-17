//! Golden Braid recirc of a scrubbed plasmid (real digest + ligate).

use splicecraft_bio::{CustomEnzyme, forbidden_hit_set, iupac_pattern, rc};
use splicecraft_codon::UsageTable;
use splicecraft_core::Feature;
use splicecraft_primer::{
    PRIMER_MAX_OLIGO_LEN, ScrubEdit, ScrubPlan, ScrubSite, circ_extract, cluster_edits,
    cluster_span, pick_binding_region, scrub_design,
};

use crate::fragment::{close_circular, digest_to_fragments, ligate_fragments};
use crate::grammar::{GB_PAD, GB_SPACER};

/// Assembly enzyme for Golden Braid scrub.
pub const SCRUB_GB_ENZYME: &str = "BsaI";
/// BsaI recognition.
pub const SCRUB_GB_SITE: &str = "GGTCTC";
const SCRUB_GB_TARGET_TM: f64 = 60.0;
const SCRUB_GB_BIND_MIN: usize = 18;
const SCRUB_GB_MIN_FRAG: usize = 50;
const SCRUB_GB_OH_SLIDE: usize = 10;
const SCRUB_GB_CLUSTER_FOOTPRINT: usize = 18;

fn bind_max() -> usize {
    PRIMER_MAX_OLIGO_LEN.saturating_sub(GB_PAD.len() + SCRUB_GB_SITE.len() + GB_SPACER.len())
}

/// A 4 nt Golden Gate overhang is usable iff it is ACGT, not palindromic, not a homopolymer.
#[must_use]
pub fn overhang_ok(oh: &str) -> bool {
    if oh.len() != 4 || oh.bytes().any(|c| !matches!(c, b'A' | b'C' | b'G' | b'T')) {
        return false;
    }
    if oh == rc(oh) {
        return false;
    }
    let b = oh.as_bytes();
    if b.iter().all(|c| *c == b[0]) {
        return false;
    }
    true
}

fn cluster_center(positions: &[usize], n: usize) -> usize {
    let (start, end) = cluster_span(positions, n);
    let span = (end + n - start) % n;
    (start + span / 2) % n
}

fn pick_cuts(cured: &str, clusters: &[Vec<usize>], n: usize) -> Result<Vec<usize>, String> {
    let mut used = std::collections::HashSet::new();
    let mut raw = Vec::new();
    for c in clusters {
        let center = cluster_center(c, n);
        let mut chosen = None;
        for d in 0..=SCRUB_GB_OH_SLIDE {
            let mut seen = std::collections::HashSet::new();
            for p in [(center + d) % n, (center + n - d % n) % n] {
                if !seen.insert(p) {
                    continue;
                }
                let oh = circ_extract(cured, p as i64, 4, n);
                if !overhang_ok(&oh) {
                    continue;
                }
                if used.contains(&oh) || used.contains(&rc(&oh)) {
                    continue;
                }
                chosen = Some(p);
                used.insert(oh.clone());
                used.insert(rc(&oh));
                break;
            }
            if chosen.is_some() {
                break;
            }
        }
        let Some(p) = chosen else {
            return Err(format!(
                "no unique non-palindromic BsaI overhang within {SCRUB_GB_OH_SLIDE} bp of the cure near {center} bp — the QuikChange method has no overhang constraint"
            ));
        };
        raw.push(p);
    }
    raw.sort_unstable();
    raw.dedup();
    Ok(raw)
}

/// One Golden Braid PCR fragment + BsaI-tailed primers.
#[derive(Clone, Debug, PartialEq)]
pub struct GbFragment {
    /// 0-based fragment index.
    pub index: usize,
    /// Left junction cut.
    pub cut_l: usize,
    /// Right junction cut.
    pub cut_r: usize,
    /// Body length including the right overhang's 4 nt.
    pub span: usize,
    /// Native 4 nt left overhang.
    pub oh_left: String,
    /// Native 4 nt right overhang.
    pub oh_right: String,
    /// Full forward oligo.
    pub fwd_seq: String,
    /// Full reverse oligo.
    pub rev_seq: String,
    /// Forward binding length.
    pub fwd_bind_len: usize,
    /// Reverse binding length.
    pub rev_bind_len: usize,
    /// Binding Tm (1 dp).
    pub fwd_tm: f64,
    /// Binding Tm (1 dp).
    pub rev_tm: f64,
}

fn gb_fragment(
    cured: &str,
    cut_l: usize,
    cut_r: usize,
    n: usize,
    single: bool,
    idx: usize,
) -> GbFragment {
    let span = if single { n } else { (cut_r + n - cut_l) % n };
    let body_len = span + 4;
    let tail = format!("{GB_PAD}{SCRUB_GB_SITE}{GB_SPACER}");
    let max_bind = bind_max();
    let fwd_full_bind = circ_extract(cured, cut_l as i64, max_bind, n);
    let (fwd_bind, fwd_tm) = pick_binding_region(
        &fwd_full_bind,
        SCRUB_GB_TARGET_TM,
        SCRUB_GB_BIND_MIN,
        max_bind,
    );
    let rev_win = circ_extract(cured, (cut_r + 4 + n - max_bind) as i64, max_bind, n);
    let (rev_bind, rev_tm) = pick_binding_region(
        &rc(&rev_win),
        SCRUB_GB_TARGET_TM,
        SCRUB_GB_BIND_MIN,
        max_bind,
    );
    GbFragment {
        index: idx,
        cut_l,
        cut_r,
        span: body_len,
        oh_left: circ_extract(cured, cut_l as i64, 4, n),
        oh_right: circ_extract(cured, cut_r as i64, 4, n),
        fwd_seq: format!("{tail}{fwd_bind}"),
        rev_seq: format!("{tail}{rev_bind}"),
        fwd_bind_len: fwd_bind.len(),
        rev_bind_len: rev_bind.len(),
        fwd_tm: (fwd_tm * 10.0).round() / 10.0,
        rev_tm: (rev_tm * 10.0).round() / 10.0,
    }
}

/// Reconstructed PCR product for one fragment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GbAmplicon {
    /// Fragment index.
    pub index: usize,
    /// Left cut (orders the circle).
    pub cut_l: usize,
    /// Insert body between the two BsaI cuts.
    pub body: String,
    /// Full tailed amplicon.
    pub amplicon: String,
}

/// Reconstruct each Golden Braid fragment's real PCR product.
#[must_use]
pub fn build_amplicons(
    orig: &str,
    cured: &str,
    frags: &[GbFragment],
    n: usize,
    single: bool,
) -> Vec<GbAmplicon> {
    let tail = format!("{GB_PAD}{SCRUB_GB_SITE}{GB_SPACER}");
    let mut out = Vec::new();
    for fr in frags {
        let span = if single {
            n
        } else {
            (fr.cut_r + n - fr.cut_l) % n
        };
        let body_len = span + 4;
        let mut chars = String::new();
        for i in 0..body_len {
            let g = (fr.cut_l + i) % n;
            if i < fr.fwd_bind_len || i >= body_len - fr.rev_bind_len {
                chars.push(cured.as_bytes()[g] as char);
            } else {
                chars.push(orig.as_bytes()[g] as char);
            }
        }
        out.push(GbAmplicon {
            index: fr.index,
            cut_l: fr.cut_l,
            amplicon: format!("{tail}{chars}{}", rc(&tail)),
            body: chars,
        });
    }
    out
}

/// Design-time proof: amplicons digest/ligate back to the cured circle (string-chain).
#[must_use]
pub fn verify(
    orig: &str,
    cured: &str,
    frags: &[GbFragment],
    n: usize,
    single: bool,
) -> (bool, Vec<String>) {
    let mut errors = Vec::new();
    let Ok(site_pat) = iupac_pattern(SCRUB_GB_SITE) else {
        return (false, vec!["invalid BsaI site".into()]);
    };
    let Ok(rc_pat) = iupac_pattern(&rc(SCRUB_GB_SITE)) else {
        return (false, vec!["invalid BsaI site".into()]);
    };
    let amps = build_amplicons(orig, cured, frags, n, single);
    let mut bodies = Vec::new();
    for a in &amps {
        let n_sites =
            site_pat.find_iter(&a.amplicon).count() + rc_pat.find_iter(&a.amplicon).count();
        if n_sites != 2 {
            errors.push(format!(
                "fragment {}: amplicon carries {n_sites} BsaI site(s) (expected exactly 2 — the two tails); an internal site slipped through and would be cut mid-assembly",
                a.index + 1
            ));
        }
        bodies.push((a.cut_l, a.body.clone()));
    }
    if bodies.is_empty() {
        return (false, vec!["no fragments to verify".into()]);
    }
    bodies.sort_by_key(|t| t.0);
    let product: String = bodies
        .iter()
        .map(|(_, b)| b[..b.len().saturating_sub(4)].to_owned())
        .collect();
    let expect = circ_extract(cured, bodies[0].0 as i64, n, n);
    if product.len() != n || product != expect {
        errors.push(
            "re-assembled product does not equal the cured plasmid — a cure fell outside primer reach of its junction, or a junction is mis-placed".into(),
        );
    }
    let wrap = format!(
        "{}{}",
        product,
        &product[..SCRUB_GB_SITE.len().saturating_sub(1).min(product.len())]
    );
    let asm = [SCRUB_GB_SITE, "GAGACC"];
    if !forbidden_hit_set(&wrap, &asm).is_empty() {
        errors.push("re-assembled product still carries a BsaI site".into());
    }
    (errors.is_empty(), errors)
}

/// Real BsaI digest + ligate of the designed amplicons. None → abort the save.
#[must_use]
pub fn assemble_amplicons_real(amplicon_specs: &[GbAmplicon]) -> Option<String> {
    let mut specs: Vec<&GbAmplicon> = amplicon_specs.iter().collect();
    specs.sort_by_key(|s| s.cut_l);
    let mut inserts = Vec::new();
    for spec in specs {
        let frags = digest_to_fragments(&spec.amplicon, &[SCRUB_GB_ENZYME], false, &[], "");
        let body: Vec<_> = frags
            .into_iter()
            .filter(|f| f.left.kind != "linear" && f.right.kind != "linear")
            .collect();
        if body.len() != 1 {
            return None;
        }
        inserts.push(body.into_iter().next().unwrap());
    }
    if inserts.is_empty() {
        return None;
    }
    let mut chained = inserts.remove(0);
    for nxt in inserts {
        chained = ligate_fragments(&chained, &nxt)?;
    }
    let closed = close_circular(&chained)?;
    if closed.top_seq.is_empty() {
        None
    } else {
        Some(closed.top_seq)
    }
}

/// Golden Braid scrub design.
#[derive(Clone, Debug, PartialEq)]
pub struct GbScrubPlan {
    /// False when curing / verification failed.
    pub ok: bool,
    /// Original sequence.
    pub orig_seq: String,
    /// Cured sequence (same length).
    pub cured_seq: String,
    /// Substitutions.
    pub edits: Vec<ScrubEdit>,
    /// Removed sites.
    pub sites_removed: Vec<ScrubSite>,
    /// Skipped sites.
    pub sites_skipped: Vec<ScrubSite>,
    /// PCR fragments.
    pub fragments: Vec<GbFragment>,
    /// Design-time recirc proof.
    pub verified: bool,
    /// Non-fatal notes.
    pub warnings: Vec<String>,
    /// Fatal notes (fail-closed).
    pub errors: Vec<String>,
}

impl GbScrubPlan {
    /// Fragment count.
    #[must_use]
    pub fn n_fragments(&self) -> usize {
        self.fragments.len()
    }
}

/// Plan a Golden Braid fragment cure. Uncurable BsaI is fatal.
#[must_use]
pub fn design(
    seq: &str,
    feats: &[Feature],
    enzymes: Option<&[&str]>,
    circular: bool,
    codon_raw: Option<&UsageTable>,
    extra: &[CustomEnzyme],
) -> GbScrubPlan {
    let seq = seq.to_ascii_uppercase();
    let n = seq.len();
    let mut base: Vec<&str> = match enzymes {
        Some(e) => e.to_vec(),
        None => splicecraft_primer::SCRUB_DEFAULT_ENZYMES.to_vec(),
    };
    if !base.contains(&SCRUB_GB_ENZYME) {
        base.push(SCRUB_GB_ENZYME);
    }
    let plan: ScrubPlan = scrub_design(&seq, feats, Some(&base), circular, codon_raw, extra);
    let mut result = GbScrubPlan {
        ok: true,
        orig_seq: seq.clone(),
        cured_seq: plan.cured_seq.clone(),
        edits: plan.edits.clone(),
        sites_removed: plan.sites_removed.clone(),
        sites_skipped: plan.sites_skipped.clone(),
        fragments: Vec::new(),
        verified: false,
        warnings: plan.warnings.clone(),
        errors: Vec::new(),
    };
    if seq.is_empty() {
        result.ok = false;
        result.errors.push("No sequence loaded.".into());
        return result;
    }
    if !plan.ok {
        result.ok = false;
        result
            .errors
            .push("Curing failed; nothing to assemble.".into());
        return result;
    }
    let bsai_skipped: Vec<_> = result
        .sites_skipped
        .iter()
        .filter(|s| s.enzyme == SCRUB_GB_ENZYME)
        .collect();
    if !bsai_skipped.is_empty() {
        result.ok = false;
        let spots: Vec<_> = bsai_skipped.iter().map(|s| s.pos.to_string()).collect();
        result.errors.push(format!(
            "{} BsaI site(s) (at {}) can't be silently removed, but BsaI is the assembly enzyme — it would cut the fragments mid-reaction. Golden Braid curing is impossible here; use QuikChange, or supply a codon table so the coding site has a synonymous alternative.",
            bsai_skipped.len(),
            spots.join(", ")
        ));
        return result;
    }
    if result.edits.is_empty() {
        result
            .warnings
            .push("No sites needed curing — nothing to fragment.".into());
        return result;
    }
    let gb_clusters = cluster_edits(
        &result.edits.iter().map(|e| e.pos).collect::<Vec<_>>(),
        n,
        SCRUB_GB_CLUSTER_FOOTPRINT,
    );
    let cuts = match pick_cuts(&result.cured_seq, &gb_clusters, n) {
        Ok(c) if !c.is_empty() => c,
        Ok(_) => {
            result.ok = false;
            result.errors.push("could not place any junction".into());
            return result;
        }
        Err(reason) => {
            result.ok = false;
            result.errors.push(reason);
            return result;
        }
    };
    let single = cuts.len() == 1;
    let mut frags = Vec::new();
    for k in 0..cuts.len() {
        let cut_l = cuts[k];
        let cut_r = if single {
            cuts[0]
        } else {
            cuts[(k + 1) % cuts.len()]
        };
        frags.push(gb_fragment(&result.cured_seq, cut_l, cut_r, n, single, k));
    }
    let short = frags
        .iter()
        .filter(|fr| fr.span < SCRUB_GB_MIN_FRAG)
        .count();
    if short > 0 {
        result.ok = false;
        result.errors.push(format!(
            "{short} fragment(s) shorter than {SCRUB_GB_MIN_FRAG} bp — the junctions are too close to PCR + gel-purify reliably; use QuikChange or cure fewer sites."
        ));
        result.fragments = frags;
        return result;
    }
    let (ok, errors) = verify(&seq, &result.cured_seq, &frags, n, single);
    result.fragments = frags;
    result.verified = ok;
    if !ok {
        result.ok = false;
        result.errors.extend(errors);
    }
    result
}

/// True when `a` is a circular rotation of `b`.
#[must_use]
pub fn rotations_equal(a: &str, b: &str) -> bool {
    a.len() == b.len() && !a.is_empty() && (a.repeat(2)).contains(b)
}
