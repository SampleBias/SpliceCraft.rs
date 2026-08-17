//! ABIF / AB1 Sanger traces: base-calls + Phred.

use std::fs;
use std::path::Path;

use splicecraft_core::Record;
use splicecraft_util::sanitize_label;

use crate::error::IoError;
use crate::fasta::BULK_IMPORT_MAX_BYTES;

/// One loaded Sanger trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ab1Trace {
    /// Sample / file stem.
    pub name: String,
    /// Base-called sequence (uppercase).
    pub sequence: String,
    /// Phred quality, one per base (empty if the PCON tag is missing).
    pub phred: Vec<u8>,
}

impl Ab1Trace {
    /// Mean Phred, or `None` when the channel is missing.
    #[must_use]
    pub fn mean_phred(&self) -> Option<f64> {
        if self.phred.is_empty() {
            return None;
        }
        let sum: u32 = self.phred.iter().map(|&q| u32::from(q)).sum();
        Some(sum as f64 / self.phred.len() as f64)
    }

    /// Linear DNA record. Topology is forced linear.
    #[must_use]
    pub fn to_record(&self) -> Record {
        Record::new(&self.name, &self.sequence, false)
    }
}

/// True when the path looks like an AB1 / ABI trace.
#[must_use]
pub fn is_ab1_path(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ab1" | "abi"))
}

/// Load a trace. Size-capped; sequence never logged.
pub fn load_ab1(path: &Path) -> Result<Ab1Trace, IoError> {
    let meta = fs::metadata(path)?;
    if meta.len() > BULK_IMPORT_MAX_BYTES {
        return Err(IoError::ab1(format!(
            "AB1 trace is {} bytes (cap {BULK_IMPORT_MAX_BYTES})",
            meta.len()
        )));
    }
    let data = fs::read(path)?;
    parse_ab1(&data, path)
}

fn parse_ab1(data: &[u8], path: &Path) -> Result<Ab1Trace, IoError> {
    if data.len() < 6 || &data[0..4] != b"ABIF" {
        return Err(IoError::ab1("not an ABIF trace (missing ABIF magic)"));
    }
    let dir = read_dir_entry(data, 6)?;
    if dir.num_elements <= 0 {
        return Err(IoError::ab1("ABIF directory is empty"));
    }
    let dir_off = dir.data_offset as usize;
    let n = dir.num_elements as usize;
    let mut seq = None;
    let mut seq_num = -1i32;
    let mut phred = None;
    let mut phred_num = -1i32;
    let mut sample = String::new();
    for i in 0..n {
        let off = dir_off.saturating_add(i.saturating_mul(28));
        let e = read_dir_entry(data, off)?;
        let name = e.name;
        if &name == b"PBAS"
            && e.number >= seq_num
            && let Ok(bytes) = tag_bytes(data, &e)
        {
            let s: String = bytes
                .into_iter()
                .map(|b| (b as char).to_ascii_uppercase())
                .filter(|c| splicecraft_bio::iupac::iupac_base_set(*c).is_some())
                .collect();
            if !s.is_empty() {
                seq = Some(s);
                seq_num = e.number;
            }
        } else if &name == b"PCON"
            && e.number >= phred_num
            && let Ok(bytes) = tag_bytes(data, &e)
        {
            phred = Some(bytes);
            phred_num = e.number;
        } else if &name == b"SMPL"
            && sample.is_empty()
            && let Ok(bytes) = tag_bytes(data, &e)
        {
            sample = String::from_utf8_lossy(&bytes)
                .trim_matches('\0')
                .trim()
                .to_owned();
        }
    }
    let sequence = seq.ok_or_else(|| IoError::ab1("no base-called sequence in AB1"))?;
    let phred = phred.unwrap_or_default();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("trace");
    let name = if sample.is_empty() {
        sanitize_label(stem, 200)
    } else {
        sanitize_label(&sample, 200)
    };
    Ok(Ab1Trace {
        name,
        sequence,
        phred,
    })
}

struct DirEntry {
    name: [u8; 4],
    number: i32,
    num_elements: i32,
    data_size: i32,
    data_offset: i32,
}

fn read_dir_entry(data: &[u8], off: usize) -> Result<DirEntry, IoError> {
    if off.saturating_add(28) > data.len() {
        return Err(IoError::ab1("truncated ABIF directory"));
    }
    let name = [data[off], data[off + 1], data[off + 2], data[off + 3]];
    Ok(DirEntry {
        name,
        number: i32::from_be_bytes(data[off + 4..off + 8].try_into().unwrap()),
        num_elements: i32::from_be_bytes(data[off + 12..off + 16].try_into().unwrap()),
        data_size: i32::from_be_bytes(data[off + 16..off + 20].try_into().unwrap()),
        data_offset: i32::from_be_bytes(data[off + 20..off + 24].try_into().unwrap()),
    })
}

fn tag_bytes(data: &[u8], e: &DirEntry) -> Result<Vec<u8>, IoError> {
    let size = e.data_size.max(0) as usize;
    if size == 0 {
        return Ok(Vec::new());
    }
    if size <= 4 {
        let raw = e.data_offset.to_be_bytes();
        return Ok(raw[..size].to_vec());
    }
    let start = e.data_offset.max(0) as usize;
    let end = start.saturating_add(size);
    if end > data.len() {
        return Err(IoError::ab1("ABIF tag overruns file"));
    }
    Ok(data[start..end].to_vec())
}

/// Build a minimal ABIF for tests (PBAS2 + optional PCON2 + SMPL).
pub fn write_test_ab1(seq: &str, phred: &[u8], sample: &str) -> Vec<u8> {
    let seq_b = seq.as_bytes().to_vec();
    let mut tags: Vec<([u8; 4], i32, Vec<u8>)> = vec![(*b"PBAS", 2, seq_b)];
    if !phred.is_empty() {
        tags.push((*b"PCON", 2, phred.to_vec()));
    }
    if !sample.is_empty() {
        tags.push((*b"SMPL", 1, sample.as_bytes().to_vec()));
    }
    let n = tags.len() as i32;
    let header = 128usize;
    let mut payloads = Vec::new();
    let mut entries = Vec::new();
    let mut cursor = header;
    for (name, number, body) in &tags {
        let data_size = body.len() as i32;
        let (data_offset, inline) = if body.len() <= 4 {
            let mut buf = [0u8; 4];
            buf[..body.len()].copy_from_slice(body);
            (i32::from_be_bytes(buf), true)
        } else {
            let off = cursor as i32;
            payloads.extend_from_slice(body);
            cursor += body.len();
            (off, false)
        };
        let _ = inline;
        let mut ent = Vec::with_capacity(28);
        ent.extend_from_slice(name);
        ent.extend_from_slice(&number.to_be_bytes());
        ent.extend_from_slice(&2i16.to_be_bytes()); // element type char
        ent.extend_from_slice(&1i16.to_be_bytes()); // element size
        ent.extend_from_slice(&(body.len() as i32).to_be_bytes());
        ent.extend_from_slice(&data_size.to_be_bytes());
        ent.extend_from_slice(&data_offset.to_be_bytes());
        ent.extend_from_slice(&0i32.to_be_bytes());
        entries.extend_from_slice(&ent);
    }
    let dir_off = cursor as i32;
    let mut out = vec![0u8; header];
    out[0..4].copy_from_slice(b"ABIF");
    out[4..6].copy_from_slice(&101i16.to_be_bytes());
    // tdir at offset 6
    out[6..10].copy_from_slice(b"tdir");
    out[10..14].copy_from_slice(&1i32.to_be_bytes());
    out[14..16].copy_from_slice(&1i16.to_be_bytes());
    out[16..18].copy_from_slice(&28i16.to_be_bytes());
    out[18..22].copy_from_slice(&n.to_be_bytes());
    out[22..26].copy_from_slice(&(n * 28).to_be_bytes());
    out[26..30].copy_from_slice(&dir_off.to_be_bytes());
    out.truncate(header);
    out.extend_from_slice(&payloads);
    // pad to dir_off if payloads started at header
    if out.len() < dir_off as usize {
        out.resize(dir_off as usize, 0);
    }
    out.extend_from_slice(&entries);
    out
}
