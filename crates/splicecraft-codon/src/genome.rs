//! Offline CDS-FASTA → codon-usage table (HEG / whole-genome).

use std::collections::{HashMap, HashSet};

use regex::Regex;
use splicecraft_bio::{CODON_TABLE, codon_aa};

use crate::error::CodonError;
use crate::table::UsageTable;

/// Stats returned with a genome-built table.
#[derive(Clone, Debug, PartialEq)]
pub struct GenomeStats {
    /// `heg` or `genome`.
    pub mode: String,
    /// CDS records counted.
    pub n_cds_total: usize,
    /// Ribosomal-protein CDS counted.
    pub n_cds_heg: usize,
    /// Codons summed into the table.
    pub n_codons: i64,
    /// Distinct amino acids (excluding stop).
    pub aa_coverage: usize,
    /// Residues backfilled from whole-genome counts in HEG mode.
    pub backfilled: Vec<char>,
    /// GC3 of the built table.
    pub gc3: f64,
}

fn is_rprotein(header: &str) -> bool {
    let h = header.to_ascii_lowercase();
    if h.contains("transferase") {
        return false;
    }
    if h.contains("ribosomal protein") {
        return true;
    }
    Regex::new(r"\[gene=rp[lsm]")
        .ok()
        .is_some_and(|re| re.is_match(&h))
}

/// Build a usage table from an in-frame CDS FASTA.
pub fn build_from_cds_fasta(
    text: &str,
    mode: &str,
) -> Result<(UsageTable, GenomeStats), CodonError> {
    if mode != "heg" && mode != "genome" {
        return Err(CodonError::parse(format!(
            "unknown mode {mode:?} (expected 'heg' or 'genome')"
        )));
    }
    let mut genome_counts: HashMap<String, i64> = HashMap::new();
    let mut heg_counts: HashMap<String, i64> = HashMap::new();
    let mut n_cds_total = 0usize;
    let mut n_cds_heg = 0usize;

    let mut consume = |hdr: Option<&str>, parts: &str| {
        let Some(hdr) = hdr else {
            return;
        };
        let s = parts.to_ascii_uppercase().replace('U', "T");
        let usable = (s.len() / 3) * 3;
        if usable == 0 {
            return;
        }
        n_cds_total += 1;
        let heg = is_rprotein(hdr);
        if heg {
            n_cds_heg += 1;
        }
        for i in (0..usable).step_by(3) {
            let codon = &s[i..i + 3];
            if CODON_TABLE.iter().any(|(c, _)| *c == codon) {
                *genome_counts.entry(codon.to_owned()).or_insert(0) += 1;
                if heg {
                    *heg_counts.entry(codon.to_owned()).or_insert(0) += 1;
                }
            }
        }
    };

    let mut header: Option<String> = None;
    let mut seq_parts = String::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('>') {
            consume(header.as_deref(), &seq_parts);
            header = Some(rest.trim().to_owned());
            seq_parts.clear();
        } else {
            seq_parts.push_str(line);
        }
    }
    consume(header.as_deref(), &seq_parts);

    if genome_counts.is_empty() {
        return Err(CodonError::parse(
            "no usable CDS found — expected an in-frame CDS FASTA (e.g. NCBI cds_from_genomic.fna)",
        ));
    }
    let base = if mode == "heg" {
        &heg_counts
    } else {
        &genome_counts
    };
    if base.is_empty() {
        return Err(CodonError::parse(
            "no highly-expressed (ribosomal-protein) CDS found in this genome — try whole-genome mode",
        ));
    }

    let mut raw = UsageTable::new();
    for (c, _) in CODON_TABLE {
        if let Some(n) = base.get(*c) {
            raw.insert((*c).to_owned(), codon_aa(c), *n);
        }
    }
    let mut backfilled = Vec::new();
    if mode == "heg" {
        let have_aa: HashSet<char> = raw.iter().map(|(_, aa, _)| aa).collect();
        let mut seen_aa = HashSet::new();
        for (c, _) in CODON_TABLE {
            let Some(n) = genome_counts.get(*c) else {
                continue;
            };
            let aa = codon_aa(c);
            if !have_aa.contains(&aa) {
                raw.insert((*c).to_owned(), aa, *n);
                if seen_aa.insert(aa) {
                    backfilled.push(aa);
                }
            }
        }
        backfilled.sort_unstable();
    }

    let n_codons: i64 = raw.iter().map(|(_, _, n)| n).sum();
    let gc3 = if n_codons == 0 {
        0.0
    } else {
        let gc: i64 = raw
            .iter()
            .filter(|(c, _, _)| {
                c.as_bytes()
                    .get(2)
                    .is_some_and(|b| matches!(b, b'G' | b'C'))
            })
            .map(|(_, _, n)| n)
            .sum();
        gc as f64 / n_codons as f64 * 100.0
    };
    let aa_coverage = raw
        .iter()
        .map(|(_, aa, _)| aa)
        .filter(|aa| *aa != '*')
        .collect::<HashSet<_>>()
        .len();
    Ok((
        raw,
        GenomeStats {
            mode: mode.to_owned(),
            n_cds_total,
            n_cds_heg,
            n_codons,
            aa_coverage,
            backfilled,
            gc3: (gc3 * 10.0).round() / 10.0,
        },
    ))
}
