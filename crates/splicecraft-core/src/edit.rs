//! Record rebuild after insert/replace. Sacred [INV-09].

use crate::record::{FeaturePart, Record};

/// How a sequence edit consumes the `[s, e)` window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    /// Insert `new_bases` at `s` (the `[s, e)` span is ignored for deletion).
    Insert,
    /// Replace `[s, e)` with `new_bases`.
    Replace,
}

/// Rebuild `src` onto `new_seq`, shifting every feature per-part.
///
/// Wrap features (`end < start`) are expanded to tail + head, shifted, then
/// re-encoded. A single surviving part collapses to a linear feature. We never
/// flatten a wrap by taking `min(parts.start)` — that is the Biopython bug
/// [INV-09] exists to prevent.
#[must_use]
pub fn rebuild_record_with_edit(
    src: &Record,
    new_seq: &str,
    mode: EditMode,
    s: usize,
    e: usize,
    new_bases: &str,
) -> Record {
    let ins_len = new_bases.len();
    let del_len = match mode {
        EditMode::Insert => 0,
        EditMode::Replace => e.saturating_sub(s),
    };
    let delta = ins_len as i64 - del_len as i64;
    let new_len = new_seq.len();
    let src_total = src.len();

    let mut out = Record {
        name: src.name.clone(),
        id: src.id.clone(),
        sequence: new_seq.to_owned(),
        circular: src.circular,
        features: Vec::with_capacity(src.features.len()),
        molecule_type: src.molecule_type.clone(),
        comments: src.comments.clone(),
    };

    for feat in &src.features {
        let parts = feat.effective_parts(src_total);
        let is_wrap_canonical = mode == EditMode::Insert
            && src_total > 0
            && parts.len() >= 2
            && parts.iter().any(|p| p.start == 0)
            && parts.iter().any(|p| p.end == src_total);

        let mut new_parts: Vec<FeaturePart> = Vec::new();
        if is_wrap_canonical && s == 0 && ins_len > 0 {
            let mut sorted = parts.clone();
            sorted.sort_by_key(|p| p.start);
            if let Some(head) = sorted.first() {
                new_parts.push(FeaturePart {
                    start: 0,
                    end: head.end + ins_len,
                    strand: head.strand,
                });
            }
            for part in sorted.iter().skip(1) {
                new_parts.push(FeaturePart {
                    start: part.start + ins_len,
                    end: part.end + ins_len,
                    strand: part.strand,
                });
            }
        } else if is_wrap_canonical && s == src_total && ins_len > 0 {
            let mut sorted = parts.clone();
            sorted.sort_by_key(|p| p.start);
            if let Some((tail, rest)) = sorted.split_last() {
                for part in rest {
                    new_parts.push(part.clone());
                }
                new_parts.push(FeaturePart {
                    start: tail.start,
                    end: tail.end + ins_len,
                    strand: tail.strand,
                });
            }
        } else {
            let ctx = ShiftCtx {
                mode,
                s,
                e,
                ins_len,
                delta,
                new_len,
            };
            for part in &parts {
                if let Some((ns, ne)) = shift_range(part.start, part.end, ctx) {
                    new_parts.push(FeaturePart {
                        start: ns,
                        end: ne,
                        strand: part.strand,
                    });
                }
            }
        }

        if new_parts.is_empty() {
            continue;
        }

        let mut rebuilt = feat.clone();
        rebuilt.encode_from_parts(new_parts, new_len);
        out.features.push(rebuilt);
    }

    out
}

#[derive(Clone, Copy)]
struct ShiftCtx {
    mode: EditMode,
    s: usize,
    e: usize,
    ins_len: usize,
    delta: i64,
    new_len: usize,
}

fn shift_range(fs: usize, fe: usize, ctx: ShiftCtx) -> Option<(usize, usize)> {
    let (new_fs, new_fe) = match ctx.mode {
        EditMode::Insert => {
            if fe <= ctx.s {
                (fs, fe)
            } else if fs >= ctx.s {
                (fs + ctx.ins_len, fe + ctx.ins_len)
            } else {
                (fs, fe + ctx.ins_len)
            }
        }
        EditMode::Replace => {
            if fe <= ctx.s {
                (fs, fe)
            } else if fs >= ctx.e {
                (add_delta(fs, ctx.delta), add_delta(fe, ctx.delta))
            } else if fs <= ctx.s && fe >= ctx.e {
                (fs, add_delta(fe, ctx.delta))
            } else if fs < ctx.s {
                (fs, ctx.s + ctx.ins_len)
            } else {
                (ctx.s + ctx.ins_len, add_delta(fe, ctx.delta))
            }
        }
    };
    let new_fs = new_fs.min(ctx.new_len);
    let new_fe = new_fe.min(ctx.new_len);
    if new_fe <= new_fs {
        None
    } else {
        Some((new_fs, new_fe))
    }
}

fn add_delta(x: usize, delta: i64) -> usize {
    let v = x as i64 + delta;
    if v < 0 { 0 } else { v as usize }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circular::feat_len;
    use crate::record::Feature;

    fn wrap_record() -> Record {
        let mut rec = Record::new("pWrap", "N".repeat(100), true);
        rec.features
            .push(Feature::new("CDS", 80, 20, 1, "wrap_cds"));
        rec
    }

    #[test]
    fn inv09_wrap_survives_mid_arc_insert() {
        let src = wrap_record();
        assert!(src.features[0].is_wrap());
        let insert_at = 90;
        let bases = "XXXXX";
        let mut new_seq = src.sequence.clone();
        new_seq.insert_str(insert_at, bases);
        let out = rebuild_record_with_edit(
            &src,
            &new_seq,
            EditMode::Insert,
            insert_at,
            insert_at,
            bases,
        );
        assert_eq!(out.len(), 105);
        assert_eq!(out.features.len(), 1);
        let f = &out.features[0];
        assert!(
            f.is_wrap(),
            "wrap flattened to [{}, {}) — INV-09 regression",
            f.start,
            f.end
        );
        assert_eq!(f.start, 80);
        assert_eq!(f.end, 20);
        assert_eq!(feat_len(f.start, f.end, out.len()), 45);
        assert!(f.parts.is_empty());
    }

    #[test]
    fn inv09_insert_at_origin_keeps_wrap() {
        let src = wrap_record();
        let bases = "AAAA";
        let new_seq = format!("{bases}{}", src.sequence);
        let out = rebuild_record_with_edit(&src, &new_seq, EditMode::Insert, 0, 0, bases);
        let f = &out.features[0];
        assert!(f.is_wrap(), "origin insert flattened wrap: {f:?}");
        assert_eq!(f.end, 24);
        assert_eq!(f.start, 84);
    }

    #[test]
    fn inv09_linear_feature_shifts_after_upstream_insert() {
        let mut src = Record::new("pLin", "A".repeat(50), true);
        src.features.push(Feature::new("CDS", 20, 30, 1, "mid"));
        let bases = "TTT";
        let mut new_seq = src.sequence.clone();
        new_seq.insert_str(5, bases);
        let out = rebuild_record_with_edit(&src, &new_seq, EditMode::Insert, 5, 5, bases);
        assert_eq!(out.features[0].start, 23);
        assert_eq!(out.features[0].end, 33);
    }

    #[test]
    fn inv09_replace_consumes_feature() {
        let mut src = Record::new("p", "A".repeat(20), false);
        src.features
            .push(Feature::new("misc_feature", 5, 8, 1, "gone"));
        let new_seq = format!("{}{}", &src.sequence[..5], &src.sequence[8..]);
        let out = rebuild_record_with_edit(&src, &new_seq, EditMode::Replace, 5, 8, "");
        assert!(out.features.is_empty());
    }
}
