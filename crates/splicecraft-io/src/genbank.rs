//! GenBank text ↔ [`Record`]. Wrap features stay `end < start`. [INV-08] [INV-09]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

use gb_io::reader::SeqReader;
use gb_io::seq::{Feature as GbFeature, Location, Seq, Topology};
use splicecraft_core::{Feature, FeaturePart, Record};
use splicecraft_persist::atomic_write_text;
use splicecraft_util::{sanitize_label, version};

use crate::error::IoError;
use crate::locus::{display_name_needs_comment, sanitize_locus_name};

/// Defence-in-depth cap before handing text to the parser.
pub const GB_TEXT_MAX_BYTES: usize = 64 * 1024 * 1024;

/// File ingest cap (chromosome dumps).
pub const GB_INGEST_MAX_BYTES: u64 = 256 * 1024 * 1024;

const SC_STRAND_QUAL: &str = "SpliceCraft_strand";
const DISPLAY_NAME_MARKER: &str = "SpliceCraft-name:";
const PROVENANCE_NEEDLE: &str = "Created by SpliceCraft";

/// Parse GenBank text into our [`Record`].
pub fn gb_text_to_record(text: &str) -> Result<Record, IoError> {
    if text.is_empty() {
        return Err(IoError::parse("empty GenBank text"));
    }
    if text.len() > GB_TEXT_MAX_BYTES {
        return Err(IoError::parse(format!(
            "GenBank text too large to parse ({} bytes > {GB_TEXT_MAX_BYTES} cap)",
            text.len()
        )));
    }
    let mut reader = SeqReader::new(Cursor::new(text.as_bytes()));
    let seq = reader
        .next()
        .ok_or_else(|| IoError::parse("no records found in GenBank text"))?
        .map_err(|e| IoError::parse(format!("GenBank parse failed: {e}")))?;
    if reader.next().is_some() {
        return Err(IoError::parse("multiple GenBank records in text"));
    }
    Ok(seq_to_record(seq))
}

/// Serialise a [`Record`] to GenBank text. Stamps `Created by SpliceCraft.rs`.
pub fn record_to_gb_text(record: &Record) -> Result<String, IoError> {
    let seq = record_to_seq(record);
    let mut buf = Vec::new();
    seq.write(&mut buf)
        .map_err(|e| IoError::parse(format!("GenBank write failed: {e}")))?;
    String::from_utf8(buf).map_err(|e| IoError::parse(format!("GenBank write was not UTF-8: {e}")))
}

/// Load a single-record GenBank file (size-capped). `.dna` is refused.
pub fn load_genbank(path: &Path) -> Result<Record, IoError> {
    if crate::detect::detect_format(path) == crate::detect::SeqFormat::CommercialDna {
        return Err(IoError::DnaDeferred {
            path: path.to_path_buf(),
        });
    }
    let meta = std::fs::metadata(path)?;
    if meta.len() > GB_INGEST_MAX_BYTES {
        return Err(IoError::rejected(format!(
            "Plasmid file is {} bytes (cap {GB_INGEST_MAX_BYTES})",
            meta.len()
        )));
    }
    let text = std::fs::read_to_string(path)?;
    let mut rec = gb_text_to_record(&text)?;
    if rec.name.is_empty() || rec.name.starts_with('<') {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plasmid");
        rec.name = sanitize_label(stem, 200);
        if rec.id.is_empty() {
            rec.id = sanitize_locus_name(stem);
        }
    }
    Ok(rec)
}

/// Atomic GenBank export to a user-chosen path (not the data-dir chokepoint).
pub fn export_genbank_to_path(record: &Record, path: &Path) -> Result<(), IoError> {
    let text = record_to_gb_text(record)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write_text(path, &text).map_err(|e| IoError::parse(e.to_string()))?;
    let _ = gb_text_to_record(&text)?;
    Ok(())
}

fn seq_to_record(seq: Seq) -> Record {
    let sequence = String::from_utf8_lossy(&seq.seq).to_ascii_uppercase();
    let total = sequence.len();
    let circular = seq.topology == Topology::Circular;
    let comments = seq.comments.clone();
    let display = restore_display_name(&comments);
    let locus = seq
        .name
        .clone()
        .or_else(|| seq.accession.clone())
        .unwrap_or_else(|| "PLASMID".into());
    let name = display.unwrap_or_else(|| locus.clone());
    let id = seq.accession.clone().unwrap_or_else(|| locus.clone());
    let molecule_type = seq.molecule_type.clone().unwrap_or_else(|| "DNA".into());
    let features = seq
        .features
        .iter()
        .filter_map(|f| gb_feature_to_ours(f, total))
        .collect();
    Record {
        name,
        id,
        sequence,
        circular,
        features,
        molecule_type,
        comments,
    }
}

fn record_to_seq(record: &Record) -> Seq {
    let locus = sanitize_locus_name(&record.name);
    let mut comments = record.comments.clone();
    let joined = comments.join("\n");
    if !joined.contains(PROVENANCE_NEEDLE) {
        comments.push(format!(
            "Created by SpliceCraft.rs v{} on {}",
            version(),
            today_iso()
        ));
    }
    if display_name_needs_comment(&record.name) && !joined.contains(DISPLAY_NAME_MARKER) {
        comments.push(format!(
            "{DISPLAY_NAME_MARKER} {}",
            record.name.replace(['\n', '\r'], " ")
        ));
    }
    let mut seq = Seq::empty();
    seq.name = Some(locus.clone());
    seq.accession = Some(if record.id.is_empty() {
        locus.clone()
    } else {
        sanitize_locus_name(&record.id)
    });
    seq.definition = Some(record.name.clone());
    seq.topology = if record.circular {
        Topology::Circular
    } else {
        Topology::Linear
    };
    seq.molecule_type = Some(if record.molecule_type.is_empty() {
        "DNA".into()
    } else {
        record.molecule_type.clone()
    });
    seq.division = "SYN".into();
    seq.comments = comments;
    seq.seq = record.sequence.to_ascii_uppercase().into_bytes();
    seq.len = Some(seq.seq.len());
    seq.features = record
        .features
        .iter()
        .map(|f| ours_to_gb_feature(f, record.len()))
        .collect();
    seq
}

fn gb_feature_to_ours(f: &GbFeature, total: usize) -> Option<Feature> {
    let kind = f.kind.as_ref().to_owned();
    let mut quals = BTreeMap::new();
    for q in &f.qualifiers {
        let key = q.0.to_string();
        let val = q.1.clone().unwrap_or_default();
        let val = if key == "primer_seq" {
            val.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .to_ascii_uppercase()
        } else {
            val
        };
        quals
            .entry(key)
            .and_modify(|e: &mut String| {
                if !e.is_empty() && !val.is_empty() {
                    e.push('\n');
                    e.push_str(&val);
                }
            })
            .or_insert(val);
    }
    let mut strand = location_strand(&f.location);
    if quals
        .get(SC_STRAND_QUAL)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("none"))
    {
        strand = 0;
        quals.remove(SC_STRAND_QUAL);
    }
    let (start, end, parts) = location_to_span(&f.location, total)?;
    let label = feature_label(&kind, &quals);
    Some(Feature {
        kind,
        start,
        end,
        strand,
        label,
        qualifiers: quals,
        parts,
    })
}

fn ours_to_gb_feature(f: &Feature, total: usize) -> GbFeature {
    let location = feature_to_location(f, total);
    let mut qualifiers = Vec::new();
    if !f.label.is_empty() {
        qualifiers.push((Cow::Borrowed("label"), Some(f.label.clone())));
    }
    for (k, v) in &f.qualifiers {
        if k == "label" {
            continue;
        }
        for line in split_qual_lines(v) {
            qualifiers.push((Cow::Owned(k.clone()), Some(line)));
        }
    }
    if f.strand == 0 && f.kind != "source" {
        qualifiers.push((Cow::Borrowed(SC_STRAND_QUAL), Some("none".into())));
    }
    GbFeature {
        kind: Cow::Owned(f.kind.clone()),
        location,
        qualifiers,
    }
}

fn split_qual_lines(v: &str) -> Vec<String> {
    if !v.contains('\n') && !v.contains('\r') {
        return vec![v.to_owned()];
    }
    let s = v.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<String> = s
        .split('\n')
        .filter(|ln| !ln.trim().is_empty())
        .map(str::to_owned)
        .collect();
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn feature_to_location(f: &Feature, total: usize) -> Location {
    let wrap = f.end < f.start && total > 0;
    let loc = if wrap {
        Location::Join(vec![
            Location::simple_range(f.start as i64, total as i64),
            Location::simple_range(0, f.end as i64),
        ])
    } else if f.parts.len() > 1 {
        Location::Join(
            f.parts
                .iter()
                .map(|p| Location::simple_range(p.start as i64, p.end as i64))
                .collect(),
        )
    } else {
        Location::simple_range(f.start as i64, f.end as i64)
    };
    if f.strand < 0 {
        Location::Complement(Box::new(loc))
    } else {
        loc
    }
}

fn location_strand(loc: &Location) -> i8 {
    match loc {
        Location::Complement(_) => -1,
        _ => 1,
    }
}

fn location_to_span(loc: &Location, total: usize) -> Option<(usize, usize, Vec<FeaturePart>)> {
    match loc {
        Location::Complement(inner) => location_to_span(inner, total),
        Location::Range((start, _), (end, _)) => {
            let s = (*start).max(0) as usize;
            let e = (*end).max(0) as usize;
            Some((s, e, Vec::new()))
        }
        Location::Join(parts) | Location::Order(parts) => {
            let ranges: Vec<(usize, usize)> = parts
                .iter()
                .filter_map(|p| match p {
                    Location::Range((s, _), (e, _)) => {
                        Some(((*s).max(0) as usize, (*e).max(0) as usize))
                    }
                    Location::Complement(inner) => match inner.as_ref() {
                        Location::Range((s, _), (e, _)) => {
                            Some(((*s).max(0) as usize, (*e).max(0) as usize))
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            if let Some((start, end)) = wrap_from_join(&ranges, total) {
                return Some((start, end, Vec::new()));
            }
            if ranges.is_empty() {
                return None;
            }
            let start = ranges.iter().map(|r| r.0).min()?;
            let end = ranges.iter().map(|r| r.1).max()?;
            let parts = ranges
                .into_iter()
                .map(|(s, e)| FeaturePart {
                    start: s,
                    end: e,
                    strand: 0,
                })
                .collect();
            Some((start, end, parts))
        }
        _ => None,
    }
}

fn wrap_from_join(ranges: &[(usize, usize)], total: usize) -> Option<(usize, usize)> {
    if ranges.len() != 2 || total == 0 {
        return None;
    }
    let (a0, a1) = ranges[0];
    let (b0, b1) = ranges[1];
    if a1 == total && b0 == 0 && a0 > b1 {
        return Some((a0, b1));
    }
    if b1 == total && a0 == 0 && b0 > a1 {
        return Some((b0, a1));
    }
    None
}

fn feature_label(kind: &str, quals: &BTreeMap<String, String>) -> String {
    for k in ["label", "gene", "product"] {
        if let Some(v) = quals.get(k) {
            let t = v.lines().next().unwrap_or("").trim();
            if !t.is_empty() {
                return t.to_owned();
            }
        }
    }
    kind.to_owned()
}

fn restore_display_name(comments: &[String]) -> Option<String> {
    let blob = comments.join("\n");
    let idx = blob.find(DISPLAY_NAME_MARKER)?;
    let rest = blob[idx + DISPLAY_NAME_MARKER.len()..].trim_start();
    let name = rest.lines().next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn today_iso() -> String {
    let secs = splicecraft_util::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}
