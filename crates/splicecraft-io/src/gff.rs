//! GFF3 export. Wrap features become two same-ID rows.

use std::fmt::Write as _;
use std::path::Path;

use splicecraft_core::Record;
use splicecraft_persist::atomic_write_text;

use crate::error::IoError;

/// Serialise `record` to GFF3 (spec 1.26). Coordinates are 1-based inclusive.
pub fn record_to_gff3(record: &Record) -> String {
    let seqid = gff_seqid(if record.id.is_empty() {
        &record.name
    } else {
        &record.id
    });
    let n = record.len();
    let mut out = String::from("##gff-version 3\n");
    if n > 0 {
        let _ = writeln!(out, "##sequence-region {seqid} 1 {n}");
    }
    let mut region_attrs = format!("ID={seqid}");
    if record.circular {
        region_attrs.push_str(";Is_circular=true");
    }
    if n > 0 {
        let _ = writeln!(
            out,
            "{seqid}\tSpliceCraft.rs\tregion\t1\t{n}\t.\t+\t.\t{region_attrs}"
        );
    }
    let mut auto_id = 0u32;
    for feat in &record.features {
        if feat.kind == "source" {
            continue;
        }
        auto_id += 1;
        let feat_id = format!("feat{auto_id}");
        let ftype = if feat.kind.is_empty() {
            "misc_feature"
        } else {
            feat.kind.as_str()
        };
        let mut attrs = format!("ID={feat_id}");
        if !feat.label.is_empty() {
            let _ = write!(attrs, ";Name={}", gff_escape(&feat.label));
        }
        for (k, v) in &feat.qualifiers {
            if k == "label" {
                continue;
            }
            if k == "gene" || k == "product" {
                let _ = write!(attrs, ";{k}={}", gff_escape(v));
            } else {
                let _ = write!(attrs, ";Note={}", gff_escape(&format!("{k}={v}")));
            }
        }
        let phase = if ftype.eq_ignore_ascii_case("CDS") {
            let cs = feat
                .qualifiers
                .get("codon_start")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1);
            format!("{}", (cs - 1).clamp(0, 2))
        } else {
            ".".into()
        };
        let strand = match feat.strand {
            1 => "+",
            -1 => "-",
            _ => ".",
        };
        let parts = if feat.is_wrap() && n > 0 {
            // Biological 5'→3': tail then head on + strand.
            vec![(feat.start, n), (0, feat.end)]
        } else if !feat.parts.is_empty() {
            feat.parts.iter().map(|p| (p.start, p.end)).collect()
        } else {
            vec![(feat.start, feat.end)]
        };
        for (ps, pe) in parts {
            if pe <= ps {
                continue;
            }
            let _ = writeln!(
                out,
                "{seqid}\tSpliceCraft.rs\t{ftype}\t{}\t{pe}\t.\t{strand}\t{phase}\t{attrs}",
                ps + 1
            );
        }
    }
    out
}

/// Atomic GFF3 export.
pub fn export_gff3_to_path(record: &Record, path: &Path) -> Result<(), IoError> {
    let text = record_to_gff3(record);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write_text(path, &text).map_err(|e| IoError::parse(e.to_string()))
}

fn gff_seqid(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return "plasmid".into();
    }
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn gff_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ';' => out.push_str("%3B"),
            '=' => out.push_str("%3D"),
            '%' => out.push_str("%25"),
            ',' => out.push_str("%2C"),
            '&' => out.push_str("%26"),
            '\t' => out.push_str("%09"),
            '\n' => out.push_str("%0A"),
            _ => out.push(c),
        }
    }
    out
}
