//! Record rotation (re-origin). Flip (reverse-complement) lives in `splicecraft-bio`.

use crate::circular::feat_len;
use crate::record::{Feature, FeaturePart, Record};

/// Rotate a circular record so `offset` becomes base 0.
///
/// Linear records and `offset == 0` are returned unchanged. A feature that
/// spans the whole molecule stays `[0, n)`. Wrap encoding is preserved
/// (`end < start`) when the rotated span still crosses the origin.
#[must_use]
pub fn rotate_record(src: &Record, offset: usize) -> Record {
    let n = src.len();
    if n == 0 || offset.is_multiple_of(n) || !src.circular {
        return src.clone();
    }
    let offset = offset % n;
    let mut out = src.clone();
    out.sequence = format!("{}{}", &src.sequence[offset..], &src.sequence[..offset]);
    out.features = src
        .features
        .iter()
        .map(|f| rotate_feature(f, offset, n))
        .collect();
    out
}

fn rotate_feature(src: &Feature, offset: usize, n: usize) -> Feature {
    let mut out = src.clone();
    if !src.parts.is_empty() {
        let parts: Vec<FeaturePart> = src
            .parts
            .iter()
            .filter_map(|p| rotate_span(p.start, p.end, p.strand, offset, n))
            .collect();
        out.encode_from_parts(parts, n);
        return out;
    }
    let flen = feat_len(src.start, src.end, n);
    if flen >= n {
        out.start = 0;
        out.end = n;
        out.parts.clear();
        return out;
    }
    if let Some(part) = rotate_span(src.start, src.end, src.strand, offset, n) {
        out.encode_from_parts(vec![part], n);
    }
    out
}

fn rotate_span(
    start: usize,
    end: usize,
    strand: i8,
    offset: usize,
    n: usize,
) -> Option<FeaturePart> {
    if n == 0 {
        return None;
    }
    let flen = feat_len(start, end, n);
    if flen == 0 {
        return None;
    }
    if flen >= n {
        return Some(FeaturePart {
            start: 0,
            end: n,
            strand,
        });
    }
    let new_s = (start + n - offset) % n;
    let new_e = if new_s + flen == n {
        n
    } else {
        (new_s + flen) % n
    };
    Some(FeaturePart {
        start: new_s,
        end: new_e,
        strand,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Feature;

    #[test]
    fn linear_is_unchanged() {
        let mut rec = Record::new("lin", "ATGCATGC", false);
        rec.features.push(Feature::new("CDS", 2, 6, 1, "x"));
        let out = rotate_record(&rec, 3);
        assert_eq!(out.sequence, rec.sequence);
        assert_eq!(out.features[0].start, 2);
    }

    #[test]
    fn circular_offset_moves_origin() {
        let mut rec = Record::new("c", "ABCDEFGH", true);
        rec.features
            .push(Feature::new("misc_feature", 2, 5, 1, "mid"));
        let out = rotate_record(&rec, 2);
        assert_eq!(out.sequence, "CDEFGHAB");
        assert_eq!(out.features[0].start, 0);
        assert_eq!(out.features[0].end, 3);
    }

    #[test]
    fn rotate_can_create_wrap() {
        let mut rec = Record::new("c", "ABCDEFGH", true);
        rec.features
            .push(Feature::new("misc_feature", 6, 8, 1, "tail"));
        let out = rotate_record(&rec, 7);
        assert_eq!(out.sequence, "HABCDEFG");
        let f = &out.features[0];
        assert!(f.is_wrap() || f.start == 7 || f.contains_bp(0) || f.start == 7);
        assert_eq!(f.len_on(out.len()), 2);
    }
}
