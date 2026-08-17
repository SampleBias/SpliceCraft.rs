//! Restore construction history from saved `.dna` originals. Sequence identity only.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use splicecraft_persist::{DataLayout, LibraryStore, log_event};

use crate::dna::{dna_bytes_to_record, extract_history_xml};
use crate::error::IoError;
use crate::genbank::gb_text_to_record;

/// Scan bound (upstream `_HISTORY_RECOVER_MAX_SIDECARS`).
pub const HISTORY_RECOVER_MAX_SIDECARS: usize = 20_000;
/// Memory bound for the sequence+xml index (upstream 512 MiB).
pub const HISTORY_RECOVER_MAX_INDEX_BYTES: usize = 512 * 1024 * 1024;

/// One plasmid that would gain a richer lineage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRecoverHit {
    /// Collection that holds the row.
    pub collection: String,
    /// Display name (may differ from the `.dna` file stem).
    pub name: String,
    /// `<Node>` count currently stored.
    pub nodes_before: usize,
    /// `<Node>` count in the sidecar.
    pub nodes_after: usize,
    /// Sidecar file name (never a sequence).
    pub source: String,
}

/// Dry-run or applied recovery report. No DNA in any field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRecoverReport {
    /// True when nothing was written.
    pub dry_run: bool,
    /// Distinct sequences indexed from sidecars.
    pub scanned_sidecars: usize,
    /// Rows that would change / did change.
    pub updated: Vec<HistoryRecoverHit>,
    /// Scan stopped early (count or memory cap).
    pub truncated: bool,
    /// Human reason when truncated or empty.
    pub note: String,
}

/// `(sequence → (node_count, history_xml, filename), note)`.
///
/// Keeps the richest history per exact sequence. Unreadable files are skipped.
#[must_use]
pub fn scan_dna_originals_for_history(
    dir: &Path,
) -> (HashMap<String, (usize, String, String)>, String) {
    let mut index: HashMap<String, (usize, String, String)> = HashMap::new();
    let mut note = String::new();
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            log_event("history.recover.list_failed", &[("reason", &e.to_string())]);
            return (
                index,
                format!("could not read dna_originals: {}", scrub_io(&e.to_string())),
            );
        }
    };
    let mut paths: Vec<_> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("dna"))
                == Some(true)
        })
        .collect();
    paths.sort();
    if paths.len() > HISTORY_RECOVER_MAX_SIDECARS {
        note = format!(
            "only the first {HISTORY_RECOVER_MAX_SIDECARS} of {} .dna files were scanned",
            paths.len()
        );
        log_event("history.recover.truncated", &[("reason", "sidecar cap")]);
        paths.truncate(HISTORY_RECOVER_MAX_SIDECARS);
    }
    let mut held = 0usize;
    for (i, p) in paths.iter().enumerate() {
        let meta = match fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let data = match fs::read(p) {
            Ok(d) => d,
            Err(e) => {
                log_event(
                    "history.recover.skip",
                    &[
                        (
                            "file",
                            p.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                        ),
                        ("reason", &e.to_string()),
                    ],
                );
                continue;
            }
        };
        let seq = match dna_bytes_to_record(&data) {
            Ok(rec) => rec.sequence.to_ascii_uppercase(),
            Err(e) => {
                log_event(
                    "history.recover.skip",
                    &[
                        (
                            "file",
                            p.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                        ),
                        ("reason", &e.to_string()),
                    ],
                );
                continue;
            }
        };
        if seq.is_empty() {
            continue;
        }
        let xml = match extract_history_xml(&data) {
            Ok(Some(x)) if !x.is_empty() => x,
            Ok(_) => continue,
            Err(e) => {
                log_event(
                    "history.recover.skip",
                    &[
                        (
                            "file",
                            p.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                        ),
                        ("reason", &e.to_string()),
                    ],
                );
                continue;
            }
        };
        let n = history_node_count_of_xml(&xml);
        let fname = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("sidecar.dna")
            .to_owned();
        let prev = index
            .get(&seq)
            .map(|(prev_n, prev_xml, _)| (*prev_n, prev_xml.len()));
        match prev {
            Some((prev_n, _)) if n <= prev_n => {}
            Some((_, prev_xml_len)) => {
                held = held.saturating_sub(prev_xml_len).saturating_add(xml.len());
                index.insert(seq, (n, xml, fname));
            }
            None => {
                held = held.saturating_add(seq.len()).saturating_add(xml.len());
                index.insert(seq, (n, xml, fname));
            }
        }
        if held > HISTORY_RECOVER_MAX_INDEX_BYTES {
            note = format!(
                "stopped after {} of {} .dna files — the index reached the {} MB memory budget",
                i + 1,
                paths.len(),
                HISTORY_RECOVER_MAX_INDEX_BYTES / (1024 * 1024)
            );
            log_event("history.recover.truncated", &[("reason", "memory cap")]);
            break;
        }
    }
    (index, note)
}

/// Count `<Node` elements in history XML (same rule as the History viewer).
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

/// Match library rows to `.dna` originals by **exact sequence identity**.
///
/// Updates **only** `history_xml`, and only when the sidecar has strictly more
/// `<Node>` elements. `dry_run` defaults true at the call site.
pub fn recover_history_from_dna(
    layout: &DataLayout,
    store: &mut LibraryStore,
    dry_run: bool,
) -> Result<HistoryRecoverReport, IoError> {
    let (index, scan_note) = scan_dna_originals_for_history(&layout.dna_originals_dir());
    if index.is_empty() {
        return Ok(HistoryRecoverReport {
            dry_run,
            scanned_sidecars: 0,
            updated: Vec::new(),
            truncated: !scan_note.is_empty(),
            note: if scan_note.is_empty() {
                "no .dna originals with history found".into()
            } else {
                scan_note
            },
        });
    }
    let before_counts: Vec<usize> = store.collections.iter().map(|c| c.plasmids.len()).collect();
    let mut updated = Vec::new();
    for coll in &mut store.collections {
        let cname = coll.name.clone();
        for e in &mut coll.plasmids {
            let seq = match gb_text_to_record(&e.gb_text) {
                Ok(rec) => rec.sequence.to_ascii_uppercase(),
                Err(_) => continue,
            };
            if seq.is_empty() {
                continue;
            }
            let Some((src_nodes, src_xml, src_file)) = index.get(&seq) else {
                continue;
            };
            let cur_nodes = history_node_count_of_xml(&e.history_xml);
            if *src_nodes <= cur_nodes {
                continue;
            }
            if !dry_run {
                e.history_xml = src_xml.clone();
            }
            updated.push(HistoryRecoverHit {
                collection: cname.clone(),
                name: e.name.clone(),
                nodes_before: cur_nodes,
                nodes_after: *src_nodes,
                source: src_file.clone(),
            });
        }
    }
    if let Some(col) = store.collections.iter().find(|c| c.name == store.active) {
        store.plasmids = col.plasmids.clone();
    }
    if updated.is_empty() {
        return Ok(HistoryRecoverReport {
            dry_run,
            scanned_sidecars: index.len(),
            updated,
            truncated: !scan_note.is_empty(),
            note: scan_note,
        });
    }
    if !dry_run {
        let after_counts: Vec<usize> = store.collections.iter().map(|c| c.plasmids.len()).collect();
        if after_counts != before_counts {
            return Err(IoError::rejected(
                "history recovery: entry count changed; nothing was saved",
            ));
        }
        store
            .persist(layout)
            .map_err(|e| IoError::rejected(format!("history recovery save failed: {e}")))?;
        log_event(
            "history.recovered",
            &[("n", &updated.len().to_string()), ("via", "recover")],
        );
    }
    Ok(HistoryRecoverReport {
        dry_run,
        scanned_sidecars: index.len(),
        updated,
        truncated: !scan_note.is_empty(),
        note: scan_note,
    })
}

fn scrub_io(msg: &str) -> String {
    msg.chars().take(160).collect()
}
