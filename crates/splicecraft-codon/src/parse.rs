//! TSV and Kazusa-HTML codon-table parsers (offline).

use std::collections::HashMap;

use regex::Regex;
use splicecraft_bio::{CODON_TABLE, codon_aa};

use crate::error::CodonError;
use crate::table::UsageTable;

const TSV_MAX_CHARS: usize = 1_000_000;

fn aa3(token: &str) -> Option<char> {
    Some(match token {
        "ALA" => 'A',
        "ARG" => 'R',
        "ASN" => 'N',
        "ASP" => 'D',
        "CYS" => 'C',
        "GLN" => 'Q',
        "GLU" => 'E',
        "GLY" => 'G',
        "HIS" => 'H',
        "ILE" => 'I',
        "LEU" => 'L',
        "LYS" => 'K',
        "MET" => 'M',
        "PHE" => 'F',
        "PRO" => 'P',
        "SER" => 'S',
        "THR" => 'T',
        "TRP" => 'W',
        "TYR" => 'Y',
        "VAL" => 'V',
        "STOP" | "TER" | "END" => '*',
        _ => return None,
    })
}

/// Parse a tab/whitespace/comma codon-usage table.
pub fn parse_tsv(text: &str) -> Result<UsageTable, CodonError> {
    if text.len() > TSV_MAX_CHARS {
        return Err(CodonError::parse("codon table is too large (1 MB cap)"));
    }
    let mut raw = UsageTable::new();
    for (lineno, line) in text.lines().enumerate() {
        let lineno = lineno + 1;
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let toks: Vec<&str> = s
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .collect();
        if toks.is_empty() {
            continue;
        }
        let codon = toks[0].to_ascii_uppercase().replace('U', "T");
        if codon.len() != 3 || codon.chars().any(|b| !matches!(b, 'A' | 'C' | 'G' | 'T')) {
            continue;
        }
        if CODON_TABLE.iter().all(|(c, _)| *c != codon) {
            return Err(CodonError::parse(format!(
                "line {lineno}: {codon:?} is not a valid codon"
            )));
        }
        if raw.get(&codon).is_some() {
            return Err(CodonError::parse(format!(
                "line {lineno}: duplicate codon {codon:?}"
            )));
        }
        let expected = codon_aa(&codon);
        let mut aa_given: Option<char> = None;
        let mut numeric = Vec::new();
        for t in &toks[1..] {
            let tu = t.to_ascii_uppercase();
            if tu.len() == 1 {
                let ch = tu.chars().next().unwrap();
                if "ACDEFGHIKLMNPQRSTVWY".contains(ch) || ch == '*' || ch == '.' {
                    aa_given = Some(if ch == '*' || ch == '.' { '*' } else { ch });
                    continue;
                }
            }
            if let Some(aa) = aa3(&tu) {
                aa_given = Some(aa);
                continue;
            }
            if t.parse::<f64>().is_ok() {
                numeric.push(*t);
            }
        }
        if let Some(aa) = aa_given
            && aa != expected
        {
            return Err(CodonError::parse(format!(
                "line {lineno}: codon {codon:?} encodes {expected:?} but the file says {aa:?}"
            )));
        }
        if numeric.is_empty() {
            return Err(CodonError::parse(format!(
                "line {lineno}: no count/frequency column for {codon:?}"
            )));
        }
        let ints: Vec<&str> = numeric
            .iter()
            .copied()
            .filter(|t| {
                t.parse::<f64>()
                    .ok()
                    .is_some_and(|v| (v - v.round()).abs() < f64::EPSILON)
            })
            .collect();
        let count = if let Some(last) = ints.last() {
            last.parse::<f64>().unwrap().round() as i64
        } else {
            (numeric.last().unwrap().parse::<f64>().unwrap() * 1000.0).round() as i64
        };
        if count < 0 {
            return Err(CodonError::parse(format!(
                "line {lineno}: negative count for {codon:?}"
            )));
        }
        raw.insert(codon, expected, count);
    }
    if raw.is_empty() {
        return Err(CodonError::parse(
            "no codon rows found — expected lines like 'GCT A 120' or 'GCT 120' (codon then count)",
        ));
    }
    Ok(raw)
}

/// Parse Kazusa showcodon.cgi GCG-format HTML. Incomplete (<64) → None.
#[must_use]
pub fn parse_kazusa_html(html: &str) -> Option<UsageTable> {
    let lower = html.to_ascii_lowercase();
    let text = if let Some(a) = lower.find("<pre>") {
        let start = a + 5;
        let end = lower[start..]
            .find("</pre>")
            .map(|e| start + e)
            .unwrap_or(html.len());
        html.get(start..end.min(html.len())).unwrap_or(html)
    } else {
        html
    };
    let pat = Regex::new(r"(?i)\b([ACGTU]{3})\b\s+(\d+(?:\.\d+)?)").ok()?;
    let mut raw = UsageTable::new();
    let mut seen = HashMap::new();
    for cap in pat.captures_iter(text) {
        let dna = cap[1].to_ascii_uppercase().replace('U', "T");
        if CODON_TABLE.iter().all(|(c, _)| *c != dna) || seen.contains_key(&dna) {
            continue;
        }
        let count = cap[2].parse::<f64>().ok()?.round() as i64;
        seen.insert(dna.clone(), ());
        let aa = codon_aa(&dna);
        raw.insert(dna, aa, count);
    }
    if raw.len() < 64 {
        return None;
    }
    Some(raw)
}
