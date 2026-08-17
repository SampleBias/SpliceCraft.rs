//! Format detection from extension and a cheap content sniff.

use std::fs;
use std::path::Path;

/// Recognised plasmid / sequence formats for stage 03.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqFormat {
    /// INSDC GenBank flatfile.
    GenBank,
    /// FASTA.
    Fasta,
    /// GFF3 (optionally with `##FASTA`).
    Gff3,
    /// EMBL (detect only; parser is GenBank-shaped later).
    Embl,
    /// Commercial `.dna` TLV.
    CommercialDna,
    /// Sanger ABIF trace.
    Ab1,
}

/// Pick a format from `path`. Unknown extensions sniff the first 256 bytes.
#[must_use]
pub fn detect_format(path: &Path) -> SeqFormat {
    let suffix = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match suffix.as_str() {
        "gb" | "gbk" | "genbank" => return SeqFormat::GenBank,
        "fa" | "fasta" | "fna" | "ffn" => return SeqFormat::Fasta,
        "gff" | "gff3" => return SeqFormat::Gff3,
        "embl" => return SeqFormat::Embl,
        "dna" => return SeqFormat::CommercialDna,
        "ab1" | "abi" => return SeqFormat::Ab1,
        _ => {}
    }
    sniff_head(path).unwrap_or(SeqFormat::GenBank)
}

fn sniff_head(path: &Path) -> Option<SeqFormat> {
    let head = fs::read(path).ok()?;
    let n = head.len().min(256);
    let head = &head[..n];
    if head.starts_with(b"ABIF") {
        return Some(SeqFormat::Ab1);
    }
    if head.starts_with(b"\x01\x00\x00\x00") || head.starts_with(b"SnapGene") {
        return Some(SeqFormat::CommercialDna);
    }
    let text = String::from_utf8_lossy(head);
    let trimmed = text.trim_start();
    if trimmed.starts_with("LOCUS ") {
        Some(SeqFormat::GenBank)
    } else if trimmed.starts_with('>') {
        Some(SeqFormat::Fasta)
    } else if trimmed.starts_with("##gff-version") || trimmed.starts_with("##gff") {
        Some(SeqFormat::Gff3)
    } else if trimmed.starts_with("ID ") {
        Some(SeqFormat::Embl)
    } else {
        None
    }
}
