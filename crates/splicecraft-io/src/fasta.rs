//! FASTA import / export. Topology is linear unless the header hints otherwise.

use std::path::Path;

use splicecraft_core::Record;
use splicecraft_persist::atomic_write_text;

use crate::error::IoError;

/// Bulk / single-file FASTA cap (50 MB).
pub const BULK_IMPORT_MAX_BYTES: u64 = 50 * 1024 * 1024;

const IUPAC: &[u8] = b"ACGTURYMKSWBDHVN-X*";

/// One FASTA record after parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FastaRecord {
    /// Header id (first token after `>`).
    pub id: String,
    /// Rest of the header line (topology hints live here).
    pub description: String,
    /// Uppercased sequence.
    pub sequence: String,
}

/// Parse FASTA text into zero or more records.
pub fn parse_fasta(text: &str) -> Result<Vec<FastaRecord>, IoError> {
    let mut out = Vec::new();
    let mut id = String::new();
    let mut desc = String::new();
    let mut seq = String::new();
    let mut in_rec = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('>') {
            if in_rec {
                push_fasta(&mut out, &id, &desc, &seq)?;
            }
            in_rec = true;
            let mut bits = rest.splitn(2, char::is_whitespace);
            id = bits.next().unwrap_or("fasta").to_owned();
            desc = bits.next().unwrap_or("").trim().to_owned();
            seq.clear();
        } else if in_rec {
            seq.push_str(line);
        }
    }
    if in_rec {
        push_fasta(&mut out, &id, &desc, &seq)?;
    }
    if out.is_empty() {
        return Err(IoError::parse("No FASTA records found in file."));
    }
    Ok(out)
}

fn push_fasta(out: &mut Vec<FastaRecord>, id: &str, desc: &str, seq: &str) -> Result<(), IoError> {
    let sequence = seq.to_ascii_uppercase();
    if sequence.is_empty() {
        return Err(IoError::parse("FASTA record has empty sequence."));
    }
    if let Some(bad) = sequence.bytes().find(|b| !IUPAC.contains(b)) {
        return Err(IoError::parse(format!(
            "Non-IUPAC characters in sequence: {}",
            bad as char
        )));
    }
    out.push(FastaRecord {
        id: if id.is_empty() {
            "fasta".into()
        } else {
            id.to_owned()
        },
        description: desc.to_owned(),
        sequence,
    });
    Ok(())
}

/// Header hint: `circular` or `plasmid` → circular; otherwise linear.
#[must_use]
pub fn detect_fasta_topology(header: &str) -> bool {
    let h = header.to_ascii_lowercase();
    h.contains("circular") || h.contains("plasmid")
}

/// Exactly one FASTA record; multi-record files error.
pub fn parse_fasta_single(text: &str) -> Result<FastaRecord, IoError> {
    let recs = parse_fasta(text)?;
    if recs.len() > 1 {
        return Err(IoError::parse(format!(
            "Multi-sequence FASTA not supported ({} records found). \
             Please provide a single-record FASTA.",
            recs.len()
        )));
    }
    Ok(recs.into_iter().next().expect("parse_fasta non-empty"))
}

/// Build a [`Record`]. Topology follows the header hint (default linear).
#[must_use]
pub fn fasta_to_record(fa: &FastaRecord) -> Record {
    let header = format!("{} {}", fa.id, fa.description);
    let circular = detect_fasta_topology(&header);
    let mut rec = Record::new(&fa.id, &fa.sequence, circular);
    rec.molecule_type = "DNA".into();
    rec
}

/// Load a single-record FASTA file.
pub fn load_fasta(path: &Path) -> Result<Record, IoError> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > BULK_IMPORT_MAX_BYTES {
        return Err(IoError::rejected(format!(
            "FASTA file is {} bytes (cap {BULK_IMPORT_MAX_BYTES})",
            meta.len()
        )));
    }
    let text = std::fs::read_to_string(path)?;
    let fa = parse_fasta_single(&text)?;
    Ok(fasta_to_record(&fa))
}

/// Write `>name\\nSEQ\\n` (unwrapped, uppercased).
pub fn record_to_fasta(name: &str, sequence: &str) -> Result<String, IoError> {
    let header = name.trim();
    let seq = sequence.trim().to_ascii_uppercase();
    if header.is_empty() {
        return Err(IoError::parse(
            "FASTA export needs a non-empty record name.",
        ));
    }
    if seq.is_empty() {
        return Err(IoError::parse("FASTA export needs a non-empty sequence."));
    }
    Ok(format!(">{header}\n{seq}\n"))
}

/// Atomic FASTA export to a user-chosen path.
pub fn export_fasta_to_path(name: &str, sequence: &str, path: &Path) -> Result<(), IoError> {
    let text = record_to_fasta(name, sequence)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write_text(path, &text).map_err(|e| IoError::parse(e.to_string()))
}

/// Export a [`Record`] as FASTA (uses `record.name`).
pub fn export_record_fasta(record: &Record, path: &Path) -> Result<(), IoError> {
    export_fasta_to_path(&record.name, &record.sequence, path)
}
