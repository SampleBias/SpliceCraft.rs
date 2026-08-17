//! Whole-record reverse-complement. Coordinates mirror; wrap stays wrap.

use splicecraft_core::{Feature, FeaturePart, Record};

use crate::iupac::rc;

/// Reverse-complement `src`. Feature DNA on ±1 strands stays the same 5′→3′
/// extract; strand 0 stays directionless. [INV-03] via [`rc`].
#[must_use]
pub fn reverse_complement_record(src: &Record) -> Record {
    let n = src.len();
    if n == 0 {
        return src.clone();
    }
    let mut out = src.clone();
    out.sequence = rc(&src.sequence);
    out.features = src
        .features
        .iter()
        .filter_map(|f| flip_feature(f, n))
        .collect();
    out
}

fn flip_feature(src: &Feature, n: usize) -> Option<Feature> {
    let parts = if src.parts.is_empty() {
        vec![FeaturePart {
            start: src.start,
            end: src.end,
            strand: src.strand,
        }]
    } else {
        src.parts.clone()
    };
    let mut new_parts = Vec::with_capacity(parts.len());
    for p in &parts {
        if p.start > n || p.end > n {
            return None;
        }
        // Wrap encoding (`end < start`) is valid; `[s, e)` maps to `[n-e, n-s)`.
        new_parts.push(FeaturePart {
            start: n - p.end,
            end: n - p.start,
            strand: flip_strand(p.strand),
        });
    }
    let mut out = src.clone();
    out.strand = flip_strand(src.strand);
    out.encode_from_parts(new_parts, n);
    Some(out)
}

fn flip_strand(strand: i8) -> i8 {
    match strand {
        1 => -1,
        -1 => 1,
        other => other,
    }
}

/// 5′→3′ bases under a feature (wrap concatenates tail+head; reverse uses [`rc`]).
#[must_use]
pub fn extract_feature(record: &Record, feat: &Feature) -> String {
    let n = record.len();
    if feat.parts.is_empty() {
        return extract_span(&record.sequence, feat.start, feat.end, feat.strand, n);
    }
    let mut out = String::new();
    for p in &feat.parts {
        out.push_str(&extract_span(
            &record.sequence,
            p.start,
            p.end,
            if p.strand == 0 { feat.strand } else { p.strand },
            n,
        ));
    }
    out
}

fn extract_span(seq: &str, start: usize, end: usize, strand: i8, n: usize) -> String {
    let raw = if end < start && n > 0 {
        let mut s = seq.get(start..).unwrap_or("").to_owned();
        s.push_str(seq.get(..end).unwrap_or(""));
        s
    } else {
        seq.get(start..end).unwrap_or("").to_owned()
    };
    if strand < 0 {
        rc(&raw)
    } else {
        raw.to_ascii_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splicecraft_core::Feature;

    fn sample() -> Record {
        let seq = "ATGGGCCCRYKMTAGCTAGCTAGCATCGATCGGGTTTAAACCC";
        let mut rec = Record::new("t", seq, true);
        rec.features.push(Feature::new("CDS", 3, 15, 1, "fwd"));
        rec.features.push(Feature::new("CDS", 20, 30, -1, "rev"));
        rec.features
            .push(Feature::new("misc_feature", 31, 36, 0, "arrowless"));
        rec.features
            .push(Feature::new("misc_feature", 38, 3, 1, "wrap"));
        rec
    }

    fn by_label<'a>(rec: &'a Record, name: &str) -> &'a Feature {
        rec.features
            .iter()
            .find(|f| f.label == name)
            .unwrap_or_else(|| panic!("{name}"))
    }

    #[test]
    fn flip_keeps_stranded_extract() {
        let r = sample();
        let f = reverse_complement_record(&r);
        assert_eq!(f.len(), r.len());
        for name in ["fwd", "rev", "wrap"] {
            assert_eq!(
                extract_feature(&r, by_label(&r, name)),
                extract_feature(&f, by_label(&f, name)),
                "{name}"
            );
        }
        assert!(by_label(&f, "wrap").is_wrap());
        assert_eq!(by_label(&f, "arrowless").strand, 0);
        assert_eq!(
            extract_feature(&f, by_label(&f, "arrowless")),
            rc(&extract_feature(&r, by_label(&r, "arrowless")))
        );
    }

    #[test]
    fn flip_is_involution_on_sequence() {
        let r = sample();
        let twice = reverse_complement_record(&reverse_complement_record(&r));
        assert_eq!(twice.sequence, r.sequence.to_ascii_uppercase());
    }
}
