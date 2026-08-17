//! Commercial `.dna` TLV packets: cookie, DNA, features, xz history.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use splicecraft_core::{Feature, FeaturePart, Record};
use splicecraft_util::sanitize_label;

use crate::error::IoError;
use crate::fasta::BULK_IMPORT_MAX_BYTES;

/// 0x00 DNA payload (flags + ASCII).
pub const PACKET_DNA: u8 = 0x00;
/// 0x07 xz-compressed history XML.
pub const PACKET_HISTORY: u8 = 0x07;
/// 0x09 cookie (format magic + version shorts).
pub const PACKET_COOKIE: u8 = 0x09;
/// 0x0A features XML.
pub const PACKET_FEATURES: u8 = 0x0A;

const COOKIE_MAGIC: [u8; 8] = [0x53, 0x6e, 0x61, 0x70, 0x47, 0x65, 0x6e, 0x65];
const COOKIE_SEQ_TYPE: u16 = 1;
const COOKIE_EXP_VER: u16 = 15;
const COOKIE_IMP_VER: u16 = 19;
const HISTORY_MAX_XML: usize = 32 * 1024 * 1024;

/// One TLV packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnaPacket {
    /// Type byte.
    pub kind: u8,
    /// Payload (not including the 5-byte header).
    pub payload: Vec<u8>,
}

/// Walk packets. Empty input yields nothing; non-empty must start with a cookie.
pub fn iter_dna_packets(data: &[u8]) -> Result<Vec<DnaPacket>, IoError> {
    let n = data.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    if n < 13 {
        return Err(IoError::parse(format!(
            "CommercialSaaS .dna file too short for cookie packet ({n} bytes; need at least 13)"
        )));
    }
    if data[0] != PACKET_COOKIE {
        return Err(IoError::parse(format!(
            "CommercialSaaS .dna file does not start with the cookie packet (first byte 0x{:02X}, expected 0x{PACKET_COOKIE:02X}). Not a valid .dna file.",
            data[0]
        )));
    }
    if data.len() < 5 + COOKIE_MAGIC.len() || data[5..5 + COOKIE_MAGIC.len()] != COOKIE_MAGIC {
        return Err(IoError::parse(
            "CommercialSaaS .dna cookie packet payload doesn't carry the expected format magic. File is corrupt or not a valid .dna file.",
        ));
    }
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset < n {
        if offset + 5 > n {
            break;
        }
        let kind = data[offset];
        let length = u32::from_be_bytes(data[offset + 1..offset + 5].try_into().unwrap()) as usize;
        let payload_start = offset + 5;
        let payload_end = payload_start.saturating_add(length);
        if payload_end > n {
            return Err(IoError::parse(format!(
                "CommercialSaaS packet length overrun at offset {offset}: type=0x{kind:02X} declared {length} bytes but only {} bytes remain.",
                n - payload_start
            )));
        }
        out.push(DnaPacket {
            kind,
            payload: data[payload_start..payload_end].to_vec(),
        });
        offset = payload_end;
    }
    Ok(out)
}

/// Serialise one packet.
#[must_use]
pub fn build_dna_packet(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(kind);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Cookie packet (14-byte payload).
#[must_use]
pub fn build_cookie_packet() -> Vec<u8> {
    let mut payload = Vec::with_capacity(14);
    payload.extend_from_slice(&COOKIE_MAGIC);
    payload.extend_from_slice(&COOKIE_SEQ_TYPE.to_be_bytes());
    payload.extend_from_slice(&COOKIE_EXP_VER.to_be_bytes());
    payload.extend_from_slice(&COOKIE_IMP_VER.to_be_bytes());
    build_dna_packet(PACKET_COOKIE, &payload)
}

/// DNA packet: flag bit 0x01 = circular, then lowercase ASCII.
pub fn build_dna_seq_packet(seq: &str, circular: bool) -> Result<Vec<u8>, IoError> {
    if let Some(ch) = seq.chars().find(|c| *c as u32 > 127) {
        return Err(IoError::parse(format!(
            "DNA packet sequence contains non-ASCII character(s) {ch:?}"
        )));
    }
    let flags = if circular { 0x01 } else { 0x00 };
    let mut payload = Vec::with_capacity(1 + seq.len());
    payload.push(flags);
    payload.extend_from_slice(seq.to_ascii_lowercase().as_bytes());
    Ok(build_dna_packet(PACKET_DNA, &payload))
}

/// Decompress the 0x07 history packet, or `None` if absent.
pub fn extract_history_xml(data: &[u8]) -> Result<Option<String>, IoError> {
    for pkt in iter_dna_packets(data)? {
        if pkt.kind != PACKET_HISTORY {
            continue;
        }
        let mut decoder = Cursor::new(&pkt.payload);
        let mut out = Vec::new();
        lzma_rs::xz_decompress(&mut decoder, &mut out).map_err(|e| {
            IoError::parse(format!(".dna history packet (0x07) is not valid xz: {e}"))
        })?;
        if out.len() > HISTORY_MAX_XML {
            return Err(IoError::parse(format!(
                ".dna history XML too large after decompression: >{HISTORY_MAX_XML} bytes"
            )));
        }
        let xml = String::from_utf8(out)
            .map_err(|e| IoError::parse(format!(".dna history XML is not valid UTF-8: {e}")))?;
        return Ok(Some(xml));
    }
    Ok(None)
}

/// xz-compress history XML.
pub fn pack_history_payload(xml: &str) -> Result<Vec<u8>, IoError> {
    let mut input = Cursor::new(xml.as_bytes());
    let mut out = Vec::new();
    lzma_rs::xz_compress(&mut input, &mut out)
        .map_err(|e| IoError::parse(format!("history xz compress failed: {e}")))?;
    Ok(out)
}

/// Replace / insert / strip the 0x07 packet. Other packets are preserved.
pub fn inject_history_xml(data: &[u8], new_xml: Option<&str>) -> Result<Vec<u8>, IoError> {
    let new_packet = match new_xml {
        Some(xml) if !xml.is_empty() => Some(build_dna_packet(
            PACKET_HISTORY,
            &pack_history_payload(xml)?,
        )),
        _ => None,
    };
    let packets = iter_dna_packets(data)?;
    let has_history = packets.iter().any(|p| p.kind == PACKET_HISTORY);
    let mut out = Vec::new();
    let mut emitted = false;
    for pkt in &packets {
        if pkt.kind == PACKET_HISTORY {
            if let Some(p) = &new_packet
                && !emitted
            {
                out.extend_from_slice(p);
                emitted = true;
            }
            continue;
        }
        out.extend_from_slice(&build_dna_packet(pkt.kind, &pkt.payload));
        if !has_history
            && pkt.kind == PACKET_COOKIE
            && let Some(p) = &new_packet
            && !emitted
        {
            out.extend_from_slice(p);
            emitted = true;
        }
    }
    if let Some(p) = &new_packet
        && !emitted
    {
        out.extend_from_slice(p);
    }
    Ok(out)
}

/// Minimal `.dna` bytes: cookie + DNA (+ optional history).
pub fn write_dna_bytes(record: &Record, history_xml: Option<&str>) -> Result<Vec<u8>, IoError> {
    if record.sequence.is_empty() {
        return Err(IoError::parse("record has empty sequence"));
    }
    let mut parts = Vec::new();
    parts.extend_from_slice(&build_cookie_packet());
    parts.extend_from_slice(&build_dna_seq_packet(&record.sequence, record.circular)?);
    parts.extend_from_slice(&build_features_packet(record));
    if let Some(xml) = history_xml.filter(|s| !s.is_empty()) {
        parts.extend_from_slice(&build_dna_packet(
            PACKET_HISTORY,
            &pack_history_payload(xml)?,
        ));
    }
    Ok(parts)
}

fn build_features_packet(record: &Record) -> Vec<u8> {
    let feats: Vec<&Feature> = record
        .features
        .iter()
        .filter(|f| f.kind != "source")
        .collect();
    let mut xml = format!("<Features nextValidID=\"{}\">", feats.len());
    for (i, feat) in feats.iter().enumerate() {
        let name = xml_escape(&feat.label);
        let kind = xml_escape(&feat.kind);
        xml.push_str(&format!(
            "<Feature recentID=\"{i}\" name=\"{name}\" type=\"{kind}\">"
        ));
        if feat.parts.len() >= 2 {
            for p in &feat.parts {
                let a = p.start + 1;
                let b = p.end.max(p.start + 1);
                xml.push_str(&format!("<Segment range=\"{a}-{b}\"/>"));
            }
        } else if feat.is_wrap() {
            let total = record.len();
            xml.push_str(&format!(
                "<Segment range=\"{}-{total}\"/><Segment range=\"1-{}\"/>",
                feat.start + 1,
                feat.end.max(1)
            ));
        } else {
            let a = feat.start + 1;
            let b = feat.end.max(feat.start + 1);
            xml.push_str(&format!("<Segment range=\"{a}-{b}\"/>"));
        }
        xml.push_str("</Feature>");
    }
    xml.push_str("</Features>");
    build_dna_packet(PACKET_FEATURES, xml.as_bytes())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Load a `.dna` file into a [`Record`].
pub fn load_dna_path(path: &Path) -> Result<Record, IoError> {
    let meta = fs::metadata(path)?;
    if meta.len() > BULK_IMPORT_MAX_BYTES {
        return Err(IoError::rejected(format!(
            "Plasmid file is {} bytes (cap {BULK_IMPORT_MAX_BYTES})",
            meta.len()
        )));
    }
    let data = fs::read(path)?;
    let mut rec = dna_bytes_to_record(&data)?;
    if rec.name.is_empty() || rec.name.starts_with('<') {
        rec.name = sanitize_label(
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("plasmid"),
            200,
        );
        rec.id = rec.name.clone();
    }
    Ok(rec)
}

/// Parse TLV bytes into a record (sequence + features).
pub fn dna_bytes_to_record(data: &[u8]) -> Result<Record, IoError> {
    let packets = iter_dna_packets(data)?;
    let mut seq = String::new();
    let mut circular = false;
    let mut features = Vec::new();
    for pkt in &packets {
        match pkt.kind {
            PACKET_DNA => {
                if pkt.payload.is_empty() {
                    continue;
                }
                circular = pkt.payload[0] & 0x01 != 0;
                seq = String::from_utf8_lossy(&pkt.payload[1..]).to_ascii_uppercase();
            }
            PACKET_FEATURES => {
                if let Ok(xml) = std::str::from_utf8(&pkt.payload) {
                    features = parse_features_xml(xml);
                }
            }
            _ => {}
        }
    }
    if seq.is_empty() {
        return Err(IoError::parse("CommercialSaaS .dna file has no DNA packet"));
    }
    let name = "plasmid".to_owned();
    let mut rec = Record::new(&name, seq, circular);
    rec.features = features;
    Ok(rec)
}

fn parse_features_xml(xml: &str) -> Vec<Feature> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<Feature ") {
        rest = &rest[start + 9..];
        let Some(end_tag) = rest.find("</Feature>") else {
            break;
        };
        let body = &rest[..end_tag];
        rest = &rest[end_tag + 10..];
        let name = attr(body, "name").unwrap_or("feature");
        let kind = attr(body, "type").unwrap_or("misc_feature");
        let mut parts = Vec::new();
        let mut scan = body;
        while let Some(s) = scan.find("<Segment") {
            scan = &scan[s + 8..];
            let Some(close) = scan.find('>') else {
                break;
            };
            let tag = &scan[..close];
            if let Some(rng) = attr(tag, "range") {
                let (a, b) = parse_range(rng);
                if b > a {
                    parts.push(FeaturePart {
                        start: a,
                        end: b,
                        strand: 1,
                    });
                }
            }
            scan = &scan[close + 1..];
        }
        if parts.is_empty() {
            continue;
        }
        if parts.len() == 2 && parts[0].start > parts[1].start {
            // tail then head already
            let feat = Feature {
                kind: kind.into(),
                start: parts[0].start,
                end: parts[1].end,
                strand: 1,
                label: name.into(),
                qualifiers: Default::default(),
                parts: Vec::new(),
            };
            out.push(feat);
        } else if parts.len() == 2 && parts[1].start == 0 {
            out.push(Feature {
                kind: kind.into(),
                start: parts[0].start,
                end: parts[1].end,
                strand: 1,
                label: name.into(),
                qualifiers: Default::default(),
                parts: Vec::new(),
            });
        } else if parts.len() == 1 {
            out.push(Feature::new(kind, parts[0].start, parts[0].end, 1, name));
        } else {
            let start = parts[0].start;
            let end = parts.last().map(|p| p.end).unwrap_or(start);
            out.push(Feature {
                kind: kind.into(),
                start,
                end,
                strand: 1,
                label: name.into(),
                qualifiers: Default::default(),
                parts,
            });
        }
    }
    out
}

fn attr<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("{key}=\"");
    let i = s.find(&pat)?;
    let rest = &s[i + pat.len()..];
    let j = rest.find('"')?;
    Some(&rest[..j])
}

fn parse_range(rng: &str) -> (usize, usize) {
    let (a, b) = rng.split_once('-').unwrap_or((rng, rng));
    let start_1 = a.parse::<usize>().unwrap_or(1);
    let end_1 = b.parse::<usize>().unwrap_or(start_1);
    (start_1.saturating_sub(1), end_1)
}
