//! CSV + IDT bulk-upload export.
//!
//! # IDT columns
//!
//! `order_format = "idt"` writes IDT's Bulk-Input template:
//!
//! `Name,Sequence,Scale,Purification`
//!
//! Defaults are `25nm` / `STD`. A primer may override per-oligo via its
//! `scale` / `purification` fields. `generic` writes
//! `Name,Sequence,Length,Tm`.
//!
//! The whole export is refused if any oligo has a non-DNA character.

use crate::error::PrimerError;
use crate::library::PrimerRecord;

/// Default IDT scale column.
pub const IDT_DEFAULT_SCALE: &str = "25nm";
/// Default IDT purification column (standard desalting).
pub const IDT_DEFAULT_PURIFICATION: &str = "STD";

/// CSV column layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderFormat {
    /// `Name,Sequence,Length,Tm`
    Generic,
    /// `Name,Sequence,Scale,Purification`
    Idt,
}

impl OrderFormat {
    /// Parse the upstream `order_format` string.
    pub fn parse(s: &str) -> Result<Self, PrimerError> {
        match s {
            "generic" => Ok(Self::Generic),
            "idt" => Ok(Self::Idt),
            other => Err(PrimerError::UnknownFormat(other.to_owned())),
        }
    }
}

fn is_iupac_oligo(seq: &str) -> bool {
    !seq.is_empty()
        && seq
            .chars()
            .all(|c| splicecraft_bio::iupac::iupac_base_set(c).is_some())
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

/// Render primers as CSV text (no filesystem write).
pub fn export_primers_csv(
    primers: &[PrimerRecord],
    format: OrderFormat,
    scale: &str,
    purification: &str,
) -> Result<String, PrimerError> {
    let rows: Vec<&PrimerRecord> = primers
        .iter()
        .filter(|p| !p.sequence.trim().is_empty())
        .collect();
    if rows.is_empty() {
        return Err(PrimerError::NothingToExport);
    }
    let mut bad = Vec::new();
    for p in &rows {
        let seq = p.sequence.trim().to_ascii_uppercase();
        if !is_iupac_oligo(&seq) {
            bad.push(if p.name.is_empty() {
                "?".to_owned()
            } else {
                p.name.clone()
            });
        }
    }
    if !bad.is_empty() {
        let shown = if bad.len() > 8 {
            format!("{} …", bad[..8].join(", "))
        } else {
            bad.join(", ")
        };
        return Err(PrimerError::MalformedOligos(shown));
    }
    let mut out = String::new();
    match format {
        OrderFormat::Idt => {
            out.push_str("Name,Sequence,Scale,Purification\n");
            for p in rows {
                let seq = p.sequence.trim().to_ascii_uppercase();
                let name = if p.name.trim().is_empty() {
                    "primer"
                } else {
                    p.name.trim()
                };
                let sc = p
                    .scale
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(scale);
                let pu = p
                    .purification
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(purification);
                out.push_str(&format!(
                    "{},{},{},{}\n",
                    csv_escape(name),
                    csv_escape(&seq),
                    csv_escape(sc),
                    csv_escape(pu)
                ));
            }
        }
        OrderFormat::Generic => {
            out.push_str("Name,Sequence,Length,Tm\n");
            for p in rows {
                let seq = p.sequence.trim().to_ascii_uppercase();
                let name = if p.name.trim().is_empty() {
                    "primer"
                } else {
                    p.name.trim()
                };
                let tm = p.tm.map(|v| format!("{v:.1}")).unwrap_or_default();
                out.push_str(&format!(
                    "{},{},{},{}\n",
                    csv_escape(name),
                    csv_escape(&seq),
                    seq.len(),
                    tm
                ));
            }
        }
    }
    Ok(out)
}

/// IDT helper using the documented defaults.
pub fn export_idt_csv(primers: &[PrimerRecord]) -> Result<String, PrimerError> {
    export_primers_csv(
        primers,
        OrderFormat::Idt,
        IDT_DEFAULT_SCALE,
        IDT_DEFAULT_PURIFICATION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::PrimerRecord;

    fn rec(name: &str, seq: &str) -> PrimerRecord {
        PrimerRecord {
            name: name.into(),
            sequence: seq.into(),
            ..PrimerRecord::default()
        }
    }

    #[test]
    fn idt_columns_and_overrides() {
        let mut p1 = rec("P1", "ACGTACGT");
        p1.tm = Some(58.3);
        let mut p2 = rec("P2", "TTTT");
        p2.scale = Some("100nm".into());
        p2.purification = Some("HPLC".into());
        let csv = export_idt_csv(&[p1, p2]).unwrap();
        let rows: Vec<Vec<&str>> = csv.lines().map(|l| l.split(',').collect()).collect();
        assert_eq!(rows[0], ["Name", "Sequence", "Scale", "Purification"]);
        assert_eq!(rows[1], ["P1", "ACGTACGT", "25nm", "STD"]);
        assert_eq!(rows[2], ["P2", "TTTT", "100nm", "HPLC"]);
    }

    #[test]
    fn idt_refuses_malformed() {
        let err = export_idt_csv(&[rec("Bad", "ACGTXZ")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Bad"), "{msg}");
    }

    #[test]
    fn generic_quotes_comma_names() {
        let mut p = rec("P2, weird", "TTTT");
        p.tm = None;
        let csv = export_primers_csv(&[p], OrderFormat::Generic, "25nm", "STD").unwrap();
        assert!(csv.contains("\"P2, weird\""), "{csv}");
    }

    #[test]
    fn unknown_format() {
        assert!(OrderFormat::parse("snapgene").is_err());
    }
}
