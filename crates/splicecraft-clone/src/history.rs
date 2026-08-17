//! Construction history: product stamps, lineage view, and read-only warnings.
//!
//! Warnings report a disagreement between recorded history and the molecule.
//! They never rewrite sequence or XML to silence a lie.

use serde::{Deserialize, Serialize};
use splicecraft_bio::{enzyme, iupac_pattern, rc};

/// One construction step stamped onto a product. Parent names only — never sequence bases.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryNode {
    /// `ligateFwd`, `ligateRev`, `gibson`, `goldenGate`, `l0FromSynFrag`.
    pub operation: String,
    /// Product display name.
    pub name: String,
    /// Product length (bp).
    pub seq_len: usize,
    /// Product topology.
    pub circular: bool,
    /// Parent record / part names (no DNA).
    pub parents: Vec<String>,
    /// Human note (enzyme pair, grammar id, …).
    pub note: String,
}

impl HistoryNode {
    /// Build a node. `parents` must not contain sequence.
    #[must_use]
    pub fn new(
        operation: impl Into<String>,
        name: impl Into<String>,
        seq_len: usize,
        circular: bool,
        parents: Vec<String>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            operation: operation.into(),
            name: name.into(),
            seq_len,
            circular,
            parents,
            note: note.into(),
        }
    }

    /// One-line comment stamped onto the product record (no bases).
    #[must_use]
    pub fn comment_line(&self) -> String {
        let parents = if self.parents.is_empty() {
            String::new()
        } else {
            format!(" parents={}", self.parents.join(","))
        };
        format!(
            "history: {} {} {}bp{}{}",
            self.operation,
            if self.circular { "circular" } else { "linear" },
            self.seq_len,
            parents,
            if self.note.is_empty() {
                String::new()
            } else {
                format!(" ({})", self.note)
            }
        )
    }
}

/// Cap on reported history warnings (upstream `_HISTORY_WARN_MAX`).
pub const HISTORY_WARN_MAX: usize = 50;
/// Cap on claims inspected (upstream `_HISTORY_CHECK_MAX_ITEMS`).
pub const HISTORY_CHECK_MAX_ITEMS: usize = 200;
/// Skip DNA scans above this length (upstream `_HISTORY_CHECK_MAX_BP`).
pub const HISTORY_CHECK_MAX_BP: usize = 2_000_000;
/// Hostile-XML parse cap.
const HISTORY_PARSE_MAX_NODES: usize = 10_000;
/// Hostile-XML depth cap.
const HISTORY_PARSE_MAX_DEPTH: usize = 64;

const HISTORY_MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// Regenerated-site claim on a history node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegeneratedSiteClaim {
    /// Enzyme name (`EcoRI`).
    pub name: String,
    /// Recorded offset. `0` is an assembly marker, not a surviving-site claim.
    pub pos: i64,
}

/// One recorded primer-binding site.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryBindingSite {
    /// `start-end` location string.
    pub location: String,
    /// Non-zero → reverse strand (annealed bases are reverse-complemented).
    pub strand: i64,
    /// Bases claimed to anneal.
    pub annealed_bases: String,
    /// Optional Tm string from the file.
    pub tm: String,
}

/// Primer block used by the warning checker and the detail pane.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryPrimerDetail {
    /// Primer name.
    pub name: String,
    /// Recorded oligo (never logged).
    pub sequence: String,
    /// Binding sites.
    pub binding_sites: Vec<HistoryBindingSite>,
}

/// Lineage node for the History viewer. Distinct from [`HistoryNode`] (product stamp).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryCheckNode {
    /// Display name (often `foo.dna`).
    pub name: String,
    /// Operation string (`goldenGateAssembly`, `createDocument`, …).
    pub operation: String,
    /// Recorded length.
    pub seq_len: usize,
    /// Recorded topology.
    pub circular: bool,
    /// Storage stamp (`YYYY-MM-DDTHH:MM`) or Notes `YYYY.MM.DD`.
    pub date: String,
    /// Direct parents (nested `<Node>` children).
    pub parents: Vec<HistoryCheckNode>,
    /// Regenerated-site claims.
    pub regenerated_sites: Vec<RegeneratedSiteClaim>,
    /// Primer annealing claims.
    pub primer_details: Vec<HistoryPrimerDetail>,
    /// Free-form `<Parameter name= val=>`.
    pub parameters: Vec<(String, String)>,
    /// 5′ / 3′ chemistry labels.
    pub end_chemistry: Vec<(String, String)>,
    /// Input-summary rows `(manipulation, name1, name2, val1, val2)`.
    pub input_summaries: Vec<HistoryInputSummary>,
}

/// One `<InputSummary>` row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryInputSummary {
    /// Manipulation name.
    pub manipulation: String,
    /// First named input.
    pub name1: String,
    /// Second named input.
    pub name2: String,
    /// Coordinate 1.
    pub val1: i64,
    /// Coordinate 2.
    pub val2: i64,
}

/// One numbered protocol step (earliest first).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryProtocolStep {
    /// 1-based recipe number.
    pub number: usize,
    /// Friendly operation.
    pub operation: String,
    /// Product name.
    pub product: String,
    /// Parent / ingredient names.
    pub inputs: Vec<String>,
    /// Regenerated enzyme names (pos > 0).
    pub enzymes: Vec<String>,
    /// Product length.
    pub seq_len: usize,
}

/// Claims this node makes that the molecule contradicts. **Read-only.**
///
/// `seq` is not mutated. Empty `seq` runs only length-arithmetic checks.
#[must_use]
pub fn history_node_warnings(node: &HistoryCheckNode, seq: &str) -> Vec<String> {
    let mut out = Vec::new();
    history_node_warnings_impl(node, seq, &mut out);
    out
}

fn history_node_warnings_impl(node: &HistoryCheckNode, seq: &str, out: &mut Vec<String>) {
    let op = node.operation.trim();
    if let Some(parent) = node.parents.first() {
        let plen = parent.seq_len;
        let nlen = node.seq_len;
        if matches!(
            op,
            "setOrigin" | "flip" | "circularize" | "linearize" | "changeMethylation"
        ) && plen > 0
            && nlen > 0
            && plen != nlen
        {
            out.push(format!(
                "{} should not change length, but the parent is {} bp and this is {} bp",
                history_op_label(op).unwrap_or(op),
                fmt_int(plen as i64),
                fmt_int(nlen as i64)
            ));
        }
        if op == "remove" && plen > 0 && nlen > 0 {
            for sm in &node.input_summaries {
                if sm.manipulation != "remove" {
                    continue;
                }
                let span = sm.val2 - sm.val1 + 1;
                if span > 0 && plen as i64 - span != nlen as i64 {
                    out.push(format!(
                        "deletion of {} bp from {} bp should leave {} bp, but this is {} bp",
                        fmt_int(span),
                        fmt_int(plen as i64),
                        fmt_int(plen as i64 - span),
                        fmt_int(nlen as i64)
                    ));
                }
                break;
            }
        }
    }

    let n = seq.len();
    if seq.is_empty() || out.len() >= HISTORY_WARN_MAX || n > HISTORY_CHECK_MAX_BP {
        return;
    }

    let circ = node.circular;
    let mut seen = 0usize;
    let mut hay_cache: Option<String> = None;

    for s in &node.regenerated_sites {
        if out.len() >= HISTORY_WARN_MAX || seen >= HISTORY_CHECK_MAX_ITEMS {
            break;
        }
        seen += 1;
        let nm: String = s.name.trim().chars().take(32).collect();
        let pos = s.pos;
        if nm.is_empty() || nm.starts_with('(') || pos == 0 {
            continue;
        }
        let Some((site, _, _)) = enzyme(&nm) else {
            continue;
        };
        let site = site.to_ascii_uppercase();
        if site.is_empty() || site.len() > n {
            continue;
        }
        let window = if circ {
            let wrap = site.len().saturating_sub(1).min(seq.len());
            format!("{seq}{}", &seq[..wrap])
        } else {
            seq.to_owned()
        };
        let Ok(fwd) = iupac_pattern(&site) else {
            continue;
        };
        let rc_site = rc(&site);
        let Ok(rev) = iupac_pattern(&rc_site) else {
            continue;
        };
        if fwd.find(&window).is_some() || rev.find(&window).is_some() {
            continue;
        }
        out.push(format!(
            "records a regenerated {nm} site at {}, but {nm} ({site}) does not occur anywhere in this molecule",
            fmt_int(pos)
        ));
    }

    for pr in &node.primer_details {
        if out.len() >= HISTORY_WARN_MAX || seen >= HISTORY_CHECK_MAX_ITEMS {
            break;
        }
        let pname: String = if pr.name.is_empty() {
            "primer".into()
        } else {
            pr.name.chars().take(40).collect()
        };
        for bs in &pr.binding_sites {
            if out.len() >= HISTORY_WARN_MAX || seen >= HISTORY_CHECK_MAX_ITEMS {
                break;
            }
            seen += 1;
            let ab = bs.annealed_bases.to_ascii_uppercase();
            let loc = &bs.location;
            if ab.is_empty() || !loc.contains('-') || ab.len() > n {
                continue;
            }
            let Ok(start) = loc
                .split_once('-')
                .map(|(a, _)| a)
                .unwrap_or("")
                .parse::<i64>()
            else {
                continue;
            };
            if start < 0 || start > 2 * n as i64 {
                continue;
            }
            let want = if bs.strand == 0 { ab } else { rc(&ab) };
            let hay = hay_cache.get_or_insert_with(|| {
                if circ {
                    format!("{seq}{seq}")
                } else {
                    seq.to_owned()
                }
            });
            let start_us = start as usize;
            let slice = hay.get(start_us..start_us.saturating_add(want.len()));
            if slice == Some(want.as_str()) {
                continue;
            }
            if hay.contains(&want) {
                out.push(format!(
                    "{pname} is recorded at {} but anneals elsewhere — the molecule was re-origined after this was written, so the position is stale (the primer still binds)",
                    fmt_int(start)
                ));
            } else {
                out.push(format!(
                    "{pname} does not bind this molecule at all — the sequence has changed since this step was recorded"
                ));
            }
        }
    }
}

/// Render a stored history timestamp as `JUN 9 2026 14:30`. Empty on garbage.
#[must_use]
pub fn history_human_dt(stamp: &str) -> String {
    let s: String = stamp.trim().chars().take(40).collect();
    if s.is_empty() {
        return String::new();
    }
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let Some(year) = take_digits(bytes, &mut i, 4, 4) else {
        return String::new();
    };
    if i >= bytes.len() || !matches!(bytes[i], b'-' | b'.' | b'/') {
        return String::new();
    }
    i += 1;
    let Some(mon) = take_digits(bytes, &mut i, 1, 2) else {
        return String::new();
    };
    if i >= bytes.len() || !matches!(bytes[i], b'-' | b'.' | b'/') {
        return String::new();
    }
    i += 1;
    let Some(day) = take_digits(bytes, &mut i, 1, 2) else {
        return String::new();
    };
    if !(1..=12).contains(&mon) || !(1..=31).contains(&day) {
        return String::new();
    }
    let mut out = format!("{} {day} {year}", HISTORY_MONTHS[mon as usize - 1]);
    if i < bytes.len() && (bytes[i] == b'T' || bytes[i] == b' ') {
        i += 1;
        if let Some(hh) = take_digits(bytes, &mut i, 1, 2)
            && i < bytes.len()
            && bytes[i] == b':'
        {
            i += 1;
            if let Some(mm) = take_digits(bytes, &mut i, 2, 2)
                && (0..=23).contains(&hh)
                && (0..=59).contains(&mm)
            {
                out.push_str(&format!(" {hh:02}:{mm:02}"));
            }
        }
    }
    out
}

fn take_digits(bytes: &[u8], i: &mut usize, min: usize, max: usize) -> Option<u32> {
    let start = *i;
    while *i < bytes.len() && bytes[*i].is_ascii_digit() && *i - start < max {
        *i += 1;
    }
    let n = *i - start;
    if n < min {
        return None;
    }
    std::str::from_utf8(&bytes[start..*i]).ok()?.parse().ok()
}

/// `<Node` element count; 0 on empty. Does not require an XML crate.
#[must_use]
pub fn history_node_count_of_xml(xml: &str) -> usize {
    if xml.is_empty() {
        return 0;
    }
    let bytes = xml.as_bytes();
    let mut n = 0usize;
    let mut i = 0usize;
    while i + 5 <= bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1..].starts_with(b"Node") {
            let after = i + 5;
            if after == bytes.len()
                || matches!(bytes[after], b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>')
            {
                n += 1;
            }
        }
        i += 1;
    }
    n
}

/// Friendly verb for an operation; `None` for empty / sentinel ops.
#[must_use]
pub fn history_op_label(op: &str) -> Option<&str> {
    let raw = op.trim();
    if raw.is_empty() {
        return None;
    }
    match raw.to_ascii_lowercase().as_str() {
        "invalid" | "unknown" | "none" | "unspecified" => return None,
        _ => {}
    }
    Some(match raw {
        "insertFragment" | "insertFragments" => "assemble",
        "goldenGateAssembly" => "Golden Gate",
        "gibsonAssembly" => "Gibson",
        "hifiAssembly" => "HiFi assembly",
        "insert" => "insert",
        "replace" => "replace",
        "amplifyFragment" => "PCR",
        "editSequence" => "edit",
        "primerDirectedMutagenesis" => "mutagenesis",
        "insertCodons" => "insert codons",
        "insertRestrictionSite" => "insert site",
        "remove" => "delete",
        "editDNAEnds" => "edit ends",
        "changeMethylation" => "methylation",
        "setOrigin" => "set origin",
        "flip" => "reverse-complement",
        "circularize" => "circularize",
        "linearize" => "linearize",
        "newFileFromSelection" => "extract region",
        "importFile" => "import",
        "createDocument" => "create",
        other => other,
    })
}

/// Strip a cosmetic `.dna` suffix.
#[must_use]
pub fn history_clean_name(name: &str) -> String {
    let n = name.trim();
    let out = if n.len() >= 4 && n[n.len() - 4..].eq_ignore_ascii_case(".dna") {
        n[..n.len() - 4].trim()
    } else {
        n
    };
    if out.is_empty() {
        "(unnamed)".into()
    } else {
        out.to_owned()
    }
}

/// Compact size for a tree row.
#[must_use]
pub fn history_size_label(bp: usize) -> String {
    if bp < 10_000 {
        format!("{bp} bp")
    } else if bp < 1_000_000 {
        format!("{:.1} kb", bp as f64 / 1_000.0)
    } else {
        format!("{:.2} Mb", bp as f64 / 1_000_000.0)
    }
}

/// One-line tree label (no markup).
#[must_use]
pub fn history_tree_label(node: &HistoryCheckNode) -> String {
    let mut name = history_clean_name(&node.name);
    if name.chars().count() > 40 {
        name = format!("{}…", name.chars().take(39).collect::<String>());
    }
    let mut bits = vec![name, history_size_label(node.seq_len)];
    if !node.circular {
        bits.push("linear".into());
    }
    if let Some(op) = history_op_label(&node.operation) {
        bits.push(op.to_owned());
    }
    if let Some(dt) = Some(history_human_dt(&node.date)).filter(|s| !s.is_empty()) {
        bits.push(dt);
    }
    bits.join("  ")
}

/// Indented lineage lines, product first.
#[must_use]
pub fn history_tree_lines(root: &HistoryCheckNode) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(node: &HistoryCheckNode, depth: usize, out: &mut Vec<String>, n: &mut usize) {
        if *n >= 2000 || depth > 64 {
            return;
        }
        *n += 1;
        let pad = "  ".repeat(depth);
        out.push(format!("{pad}{}", history_tree_label(node)));
        for p in &node.parents {
            walk(p, depth + 1, out, n);
        }
    }
    let mut n = 0;
    walk(root, 0, &mut out, &mut n);
    out
}

/// Numbered protocol steps, earliest (deepest) first — left → right like the bench.
#[must_use]
pub fn history_protocol_steps(root: &HistoryCheckNode) -> Vec<HistoryProtocolStep> {
    let mut by_sig: Vec<(String, usize, HistoryProtocolStep)> = Vec::new();
    let mut stack = vec![(root, 0usize)];
    let mut n_seen = 0usize;
    while let Some((node, depth)) = stack.pop() {
        if n_seen >= 2000 || depth >= 64 {
            break;
        }
        n_seen += 1;
        if !node.parents.is_empty() {
            let sig = format!(
                "{}\0{}\0{}",
                history_clean_name(&node.name),
                node.seq_len,
                node.operation.trim()
            );
            if !by_sig.iter().any(|(s, _, _)| s == &sig) {
                by_sig.push((
                    sig,
                    depth,
                    HistoryProtocolStep {
                        number: 0,
                        operation: history_op_label(&node.operation)
                            .unwrap_or(node.operation.trim())
                            .to_owned(),
                        product: history_clean_name(&node.name),
                        inputs: node
                            .parents
                            .iter()
                            .map(|p| history_clean_name(&p.name))
                            .collect(),
                        enzymes: node
                            .regenerated_sites
                            .iter()
                            .filter(|s| s.pos > 0 && !s.name.is_empty())
                            .map(|s| s.name.clone())
                            .collect(),
                        seq_len: node.seq_len,
                    },
                ));
            } else if let Some((_, d, _)) = by_sig.iter_mut().find(|(s, _, _)| s == &sig) {
                *d = (*d).max(depth);
            }
        }
        for p in node.parents.iter().rev() {
            stack.push((p, depth + 1));
        }
    }
    by_sig.sort_by_key(|b| std::cmp::Reverse(b.1));
    by_sig
        .into_iter()
        .enumerate()
        .map(|(i, (_, _, mut step))| {
            step.number = i + 1;
            step
        })
        .collect()
}

/// Render protocol lines for the overlay.
#[must_use]
pub fn history_protocol_lines(root: &HistoryCheckNode) -> Vec<String> {
    let steps = history_protocol_steps(root);
    if steps.is_empty() {
        let op = history_op_label(&root.operation).unwrap_or("");
        if op.is_empty() {
            return vec!["Single record — no construction steps recorded.".into()];
        }
        return vec![format!("Single record — {op}, no further steps recorded.")];
    }
    steps
        .into_iter()
        .map(|s| {
            let ingredients = if s.inputs.is_empty() {
                String::new()
            } else {
                format!("{} into ", s.inputs.join(" + "))
            };
            let enz = if s.enzymes.is_empty() {
                String::new()
            } else {
                format!("  cut {}", s.enzymes.join(","))
            };
            format!(
                "{}. {}  {ingredients}{} ({} bp){enz}",
                s.number, s.operation, s.product, s.seq_len
            )
        })
        .collect()
}

/// Detail-pane lines: conditions, primers, chemistry, enzymes.
#[must_use]
pub fn history_detail_lines(node: &HistoryCheckNode) -> Vec<String> {
    let mut lines = vec![
        history_tree_label(node),
        format!(
            "{} bp  {}",
            node.seq_len,
            if node.circular { "circular" } else { "linear" }
        ),
    ];
    if !node.date.is_empty() {
        let human = history_human_dt(&node.date);
        if !human.is_empty() {
            lines.push(human);
        }
    }
    for (k, v) in &node.parameters {
        lines.push(format!("{k} = {v}"));
    }
    for (k, v) in &node.end_chemistry {
        lines.push(format!("end {k}: {v}"));
    }
    for s in &node.regenerated_sites {
        if s.pos == 0 {
            lines.push(format!("assembly enzyme {}", s.name));
        } else {
            lines.push(format!("regenerated {} @{}", s.name, s.pos));
        }
    }
    for pr in &node.primer_details {
        let nm = if pr.name.is_empty() {
            "primer"
        } else {
            pr.name.as_str()
        };
        if pr.binding_sites.is_empty() {
            lines.push(format!("primer {nm}"));
        }
        for bs in &pr.binding_sites {
            let strand = if bs.strand == 0 { "+" } else { "-" };
            let tm = if bs.tm.is_empty() {
                String::new()
            } else {
                format!("  Tm {}", bs.tm)
            };
            lines.push(format!("primer {nm}  {}  {strand}{tm}", bs.location));
        }
    }
    lines
}

/// Parse CommercialSaaS `<HistoryTree>` XML into a check node. `None` on garbage.
#[must_use]
pub fn parse_history_xml(xml: &str) -> Option<HistoryCheckNode> {
    let forest = parse_xml(xml)?;
    let root_el = forest
        .iter()
        .find(|e| e.tag == "HistoryTree")
        .or_else(|| forest.iter().find(|e| e.tag == "Node"))?;
    let node_el = if root_el.tag == "HistoryTree" {
        root_el.children.iter().find(|e| e.tag == "Node")?
    } else {
        root_el
    };
    Some(check_node_from_elem(node_el))
}

fn check_node_from_elem(el: &XmlElem) -> HistoryCheckNode {
    let circular = matches!(
        attr(el, "circular").to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    );
    let seq_len = attr(el, "seqLen").parse().unwrap_or(0);
    let mut primer_details = Vec::new();
    for block in el.children.iter().filter(|c| c.tag == "Primers") {
        for pr in block.children.iter().filter(|c| c.tag == "Primer") {
            let mut binding_sites = Vec::new();
            for bs in pr.children.iter().filter(|c| c.tag == "BindingSite") {
                binding_sites.push(HistoryBindingSite {
                    location: attr(bs, "location").to_owned(),
                    strand: coerce_int(attr(bs, "boundStrand")),
                    annealed_bases: attr(bs, "annealedBases").to_owned(),
                    tm: attr(bs, "meltingTemperature").to_owned(),
                });
            }
            primer_details.push(HistoryPrimerDetail {
                name: attr(pr, "name").to_owned(),
                sequence: attr(pr, "sequence").to_owned(),
                binding_sites,
            });
        }
    }
    let mut end_chemistry = Vec::new();
    for key in ["upstreamModification", "downstreamModification"] {
        let v = attr(el, key);
        if !v.is_empty() {
            let label = if key.starts_with("up") {
                "upstream"
            } else {
                "downstream"
            };
            end_chemistry.push((label.into(), v.to_owned()));
        }
    }
    HistoryCheckNode {
        name: attr(el, "name").to_owned(),
        operation: attr(el, "operation").to_owned(),
        seq_len,
        circular,
        date: attr(el, "date").to_owned(),
        parents: el
            .children
            .iter()
            .filter(|c| c.tag == "Node")
            .map(check_node_from_elem)
            .collect(),
        regenerated_sites: el
            .children
            .iter()
            .filter(|c| c.tag == "RegeneratedSite")
            .map(|c| RegeneratedSiteClaim {
                name: attr(c, "name").to_owned(),
                pos: coerce_int(attr(c, "pos")),
            })
            .collect(),
        primer_details,
        parameters: el
            .children
            .iter()
            .filter(|c| c.tag == "Parameter")
            .map(|c| (attr(c, "name").to_owned(), attr(c, "val").to_owned()))
            .collect(),
        end_chemistry,
        input_summaries: el
            .children
            .iter()
            .filter(|c| c.tag == "InputSummary")
            .map(|c| HistoryInputSummary {
                manipulation: attr(c, "manipulation").to_owned(),
                name1: attr(c, "name1").to_owned(),
                name2: attr(c, "name2").to_owned(),
                val1: coerce_int(attr(c, "val1")),
                val2: coerce_int(attr(c, "val2")),
            })
            .collect(),
    }
}

fn coerce_int(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn fmt_int(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let s = n.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{sign}{}", grouped.chars().rev().collect::<String>())
}

#[derive(Clone, Debug)]
struct XmlElem {
    tag: String,
    attrs: Vec<(String, String)>,
    children: Vec<XmlElem>,
}

fn attr<'a>(el: &'a XmlElem, key: &str) -> &'a str {
    el.attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

fn parse_xml(xml: &str) -> Option<Vec<XmlElem>> {
    let chars: Vec<char> = xml.chars().collect();
    let mut i = 0usize;
    let mut stack: Vec<XmlElem> = Vec::new();
    let mut roots: Vec<XmlElem> = Vec::new();
    let mut nodes = 0usize;
    while i < chars.len() {
        if nodes > HISTORY_PARSE_MAX_NODES || stack.len() > HISTORY_PARSE_MAX_DEPTH {
            return None;
        }
        if chars[i] != '<' {
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'!') && chars.get(i + 2) == Some(&'-') {
            if let Some(end) = find_sub(&chars, i, &['-', '-', '>']) {
                i = end;
                continue;
            }
            return None;
        }
        if chars.get(i + 1) == Some(&'?') {
            if let Some(end) = find_char(&chars, i + 2, '>') {
                i = end + 1;
                continue;
            }
            return None;
        }
        let closing = chars.get(i + 1) == Some(&'/');
        let name_start = if closing { i + 2 } else { i + 1 };
        let mut j = name_start;
        while j < chars.len()
            && (chars[j].is_ascii_alphanumeric()
                || chars[j] == '_'
                || chars[j] == '-'
                || chars[j] == ':')
        {
            j += 1;
        }
        if j == name_start {
            return None;
        }
        let tag: String = chars[name_start..j].iter().collect();
        let (attrs, self_close, after) = parse_attrs(&chars, j)?;
        i = after;
        if closing {
            let finished = stack.pop()?;
            if finished.tag != tag {
                return None;
            }
            if let Some(parent) = stack.last_mut() {
                parent.children.push(finished);
            } else {
                roots.push(finished);
            }
            continue;
        }
        nodes += 1;
        let elem = XmlElem {
            tag,
            attrs,
            children: Vec::new(),
        };
        if self_close {
            if let Some(parent) = stack.last_mut() {
                parent.children.push(elem);
            } else {
                roots.push(elem);
            }
        } else {
            stack.push(elem);
        }
    }
    if !stack.is_empty() {
        return None;
    }
    Some(roots)
}

type AttrParse = (Vec<(String, String)>, bool, usize);

fn parse_attrs(chars: &[char], mut i: usize) -> Option<AttrParse> {
    let mut attrs = Vec::new();
    loop {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            return None;
        }
        if chars[i] == '/' && chars.get(i + 1) == Some(&'>') {
            return Some((attrs, true, i + 2));
        }
        if chars[i] == '>' {
            return Some((attrs, false, i + 1));
        }
        let start = i;
        while i < chars.len()
            && (chars[i].is_ascii_alphanumeric()
                || chars[i] == '_'
                || chars[i] == '-'
                || chars[i] == ':')
        {
            i += 1;
        }
        if i == start {
            return None;
        }
        let key: String = chars[start..i].iter().collect();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() || chars[i] != '=' {
            return None;
        }
        i += 1;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let quote = *chars.get(i)?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        i += 1;
        let vstart = i;
        while i < chars.len() && chars[i] != quote {
            i += 1;
        }
        if i >= chars.len() {
            return None;
        }
        let val: String = chars[vstart..i].iter().collect();
        i += 1;
        attrs.push((key, val));
    }
}

fn find_sub(chars: &[char], from: usize, needle: &[char]) -> Option<usize> {
    let mut i = from;
    while i + needle.len() <= chars.len() {
        if chars[i..i + needle.len()] == *needle {
            return Some(i + needle.len());
        }
        i += 1;
    }
    None
}

fn find_char(chars: &[char], from: usize, c: char) -> Option<usize> {
    chars[from..].iter().position(|x| *x == c).map(|p| from + p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecori_claim_warns_and_does_not_mutate_sequence() {
        let seq = "ATGCATGCATGC";
        let original = seq.to_owned();
        let node = HistoryCheckNode {
            name: "pLie.dna".into(),
            operation: "insertFragment".into(),
            seq_len: seq.len(),
            circular: false,
            regenerated_sites: vec![RegeneratedSiteClaim {
                name: "EcoRI".into(),
                pos: 12,
            }],
            ..HistoryCheckNode::default()
        };
        let warnings = history_node_warnings(&node, seq);
        assert_eq!(seq, original);
        assert_eq!(seq, "ATGCATGCATGC");
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("EcoRI") && w.contains("does not occur")),
            "{warnings:?}"
        );
        let present = HistoryCheckNode {
            regenerated_sites: vec![RegeneratedSiteClaim {
                name: "EcoRI".into(),
                pos: 1,
            }],
            seq_len: 6,
            ..HistoryCheckNode::default()
        };
        assert!(history_node_warnings(&present, "GAATTC").is_empty());
        let marker = HistoryCheckNode {
            regenerated_sites: vec![RegeneratedSiteClaim {
                name: "BsaI".into(),
                pos: 0,
            }],
            ..HistoryCheckNode::default()
        };
        assert!(history_node_warnings(&marker, seq).is_empty());
    }

    #[test]
    fn human_dt_universal_format() {
        assert_eq!(history_human_dt("2026-06-09T14:30"), "JUN 9 2026 14:30");
        assert_eq!(history_human_dt("2026.12.25"), "DEC 25 2026");
        assert_eq!(history_human_dt(""), "");
        assert_eq!(history_human_dt("not a date"), "");
    }

    #[test]
    fn node_count_and_protocol_order() {
        let xml = "<HistoryTree>\
            <Node name=\"prod.dna\" seqLen=\"900\" circular=\"1\" operation=\"goldenGateAssembly\">\
            <Node name=\"vec.dna\" seqLen=\"500\" circular=\"1\" operation=\"invalid\"/>\
            <Node name=\"ins.dna\" seqLen=\"400\" circular=\"0\" operation=\"invalid\"/>\
            </Node></HistoryTree>";
        assert_eq!(history_node_count_of_xml(xml), 3);
        let root = parse_history_xml(xml).expect("parse");
        assert_eq!(root.parents.len(), 2);
        let steps = history_protocol_steps(&root);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].operation, "Golden Gate");
        assert_eq!(steps[0].number, 1);
        assert!(history_tree_lines(&root)[0].contains("prod"));
    }
}
