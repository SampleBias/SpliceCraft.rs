//! Folder import / collection export. Per-file failures stay isolated.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use splicecraft_core::Record;
use splicecraft_persist::{LibraryEntry, unique_export_stem};
use splicecraft_util::sanitize_plasmid_name;

use crate::detect::SeqFormat;
use crate::error::IoError;
use crate::fasta::{BULK_IMPORT_MAX_BYTES, export_record_fasta, load_fasta};
use crate::genbank::{export_genbank_to_path, load_genbank, record_to_gb_text};

/// Directory-entry scan cap (upstream `_BULK_IMPORT_MAX_FILES`).
pub const BULK_IMPORT_MAX_FILES: usize = 5000;

/// One file that did not become a record (reason never contains sequence).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkFailure {
    /// Path that failed.
    pub path: PathBuf,
    /// Human reason (no DNA).
    pub reason: String,
}

/// Result of walking a folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BulkImportReport {
    /// Successfully parsed records.
    pub records: Vec<Record>,
    /// Per-file failures; the rest of the batch continues.
    pub failures: Vec<BulkFailure>,
    /// `.dna` files counted and skipped (codec is stage 11).
    pub skipped_dna: usize,
}

/// Build a library JSON row from a record (GenBank text, no log of bases).
pub fn record_to_library_entry(record: &Record) -> Result<LibraryEntry, IoError> {
    let name = sanitize_plasmid_name(&record.name, "plasmid", 256);
    let gb_text = record_to_gb_text(record)?;
    Ok(LibraryEntry {
        id: if record.id.is_empty() {
            name.clone()
        } else {
            record.id.clone()
        },
        name,
        size: record.len(),
        gb_text,
    })
}

/// Walk `folder` for `.gb` / `.gbk` / `.fasta`. `.dna` is counted, not loaded.
///
/// Symlinks and non-regular files are ignored. One corrupt file does not
/// abort the batch.
#[must_use]
pub fn bulk_import_folder(folder: &Path) -> BulkImportReport {
    let mut report = BulkImportReport::default();
    let children = match std::fs::read_dir(folder) {
        Ok(rd) => {
            let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
            v.sort();
            v
        }
        Err(e) => {
            report.failures.push(BulkFailure {
                path: folder.to_path_buf(),
                reason: format!("could not read folder: {e}"),
            });
            return report;
        }
    };
    if children.len() > BULK_IMPORT_MAX_FILES {
        report.failures.push(BulkFailure {
            path: folder.to_path_buf(),
            reason: format!(
                "folder has {} entries — only the first {BULK_IMPORT_MAX_FILES} were scanned",
                children.len()
            ),
        });
    }
    for path in children.into_iter().take(BULK_IMPORT_MAX_FILES) {
        match ingest_one(&path) {
            Ingest::Skip => {}
            Ingest::Dna => report.skipped_dna += 1,
            Ingest::Ok(rec) => report.records.push(rec),
            Ingest::Fail(reason) => report.failures.push(BulkFailure { path, reason }),
        }
    }
    report
}

enum Ingest {
    Skip,
    Dna,
    Ok(Record),
    Fail(String),
}

fn ingest_one(path: &Path) -> Ingest {
    let meta = match path.symlink_metadata() {
        Ok(m) => m,
        Err(e) => return Ingest::Fail(format!("lstat failed: {e}")),
    };
    if meta.file_type().is_symlink() || !meta.file_type().is_file() {
        return Ingest::Skip;
    }
    if meta.len() > BULK_IMPORT_MAX_BYTES {
        return Ingest::Fail(format!(
            "file too large ({} bytes; cap {BULK_IMPORT_MAX_BYTES})",
            meta.len()
        ));
    }
    match suffix_format(path) {
        None => Ingest::Skip,
        Some(SeqFormat::CommercialDna) => Ingest::Dna,
        Some(SeqFormat::GenBank) => match load_genbank(path) {
            Ok(rec) if rec.is_empty() => Ingest::Fail("empty sequence (no bases)".into()),
            Ok(rec) => Ingest::Ok(rec),
            Err(e) => Ingest::Fail(e.to_string()),
        },
        Some(SeqFormat::Fasta) => match load_fasta(path) {
            Ok(rec) if rec.is_empty() => Ingest::Fail("empty sequence (no bases)".into()),
            Ok(rec) => Ingest::Ok(rec),
            Err(e) => Ingest::Fail(e.to_string()),
        },
        Some(SeqFormat::Gff3 | SeqFormat::Embl) => Ingest::Skip,
    }
}

fn suffix_format(path: &Path) -> Option<SeqFormat> {
    let suffix = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match suffix.as_str() {
        "gb" | "gbk" | "genbank" => Some(SeqFormat::GenBank),
        "fa" | "fasta" | "fna" | "ffn" | "fas" => Some(SeqFormat::Fasta),
        "dna" => Some(SeqFormat::CommercialDna),
        _ => None,
    }
}

/// Export format for a collection dump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkExportFormat {
    /// `.gb`
    GenBank,
    /// `.fa`
    Fasta,
}

/// Write each record into `dir` with sanitised, case-insensitive-unique names.
pub fn bulk_export_folder(
    dir: &Path,
    records: &[Record],
    format: BulkExportFormat,
) -> Result<Vec<PathBuf>, IoError> {
    std::fs::create_dir_all(dir)?;
    let mut taken = HashSet::new();
    let mut written = Vec::new();
    for rec in records {
        let stem = unique_export_stem(&rec.name, &mut taken);
        let ext = match format {
            BulkExportFormat::GenBank => "gb",
            BulkExportFormat::Fasta => "fa",
        };
        let path = dir.join(format!("{stem}.{ext}"));
        if !splicecraft_util::path_is_safe_under(&path) {
            return Err(IoError::rejected(
                "export path escaped the destination folder",
            ));
        }
        match format {
            BulkExportFormat::GenBank => export_genbank_to_path(rec, &path)?,
            BulkExportFormat::Fasta => export_record_fasta(rec, &path)?,
        }
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_core::Record;
    use std::fs;

    #[test]
    fn bulk_import_isolates_per_file_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let good = tmp.path().join("good.fa");
        fs::write(&good, ">ok\nATGCATGCATGC\n").unwrap();
        fs::write(tmp.path().join("bad.gb"), "this is not genbank").unwrap();
        fs::write(
            tmp.path().join("skip.dna"),
            b"\x01\x00\x00\x00not-a-real-dna",
        )
        .unwrap();
        fs::write(tmp.path().join("notes.txt"), "ignore me").unwrap();
        let report = bulk_import_folder(tmp.path());
        assert_eq!(report.records.len(), 1, "{:?}", report.failures);
        assert_eq!(report.records[0].name, "ok");
        assert_eq!(report.skipped_dna, 1);
        assert!(
            report.failures.iter().any(|f| f.path.ends_with("bad.gb")),
            "{:?}",
            report.failures
        );
        assert!(!report.failures.iter().any(|f| f.reason.contains("ATGC")));
    }

    #[test]
    fn bulk_export_avoids_case_insensitive_clobber() {
        let tmp = tempfile::tempdir().unwrap();
        let a = Record::new("pUC19", "ATGCATGCATGC", true);
        let mut b = Record::new("puc19", "GGGGAAAA", false);
        b.id = "other".into();
        let paths = bulk_export_folder(tmp.path(), &[a, b], BulkExportFormat::Fasta).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].file_name().unwrap() == "pUC19.fa");
        assert!(paths[1].to_string_lossy().contains("puc19_2"));
        assert!(tmp.path().join("pUC19.fa").exists());
        assert!(tmp.path().join("puc19_2.fa").exists());
    }

    #[test]
    fn record_to_library_entry_has_size_not_logged_seq() {
        let rec = Record::new("pX", "ATGCATGCATGC", true);
        let e = record_to_library_entry(&rec).unwrap();
        assert_eq!(e.name, "pX");
        assert_eq!(e.size, 12);
        assert!(e.gb_text.contains("LOCUS"));
    }
}
