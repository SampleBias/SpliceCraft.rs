//! Wrap-aware circular coordinate math. Sacred [INV-05] and [INV-08].

/// Circular-aware feature length.
///
/// A wrap feature (`end < start`) is `(total - start) + end` bp; a linear
/// feature is `end - start`. Coordinates are 0-based half-open.
#[must_use]
pub fn feat_len(start: usize, end: usize, total: usize) -> usize {
    if end < start {
        (total - start) + end
    } else {
        end - start
    }
}

/// Whether `bp` lies in the half-open span `[start, end)`, wrap-aware.
///
/// When `end < start` the span is `[start, ∞) ∪ [0, end)` — the origin wrap.
/// `total` is not required; wrap is encoded by `end < start`.
#[must_use]
pub fn bp_in(bp: usize, start: usize, end: usize) -> bool {
    if end >= start {
        start <= bp && bp < end
    } else {
        bp >= start || bp < end
    }
}

/// Label midpoint of a (possibly wrapping) arc. [INV-05]
///
/// `arc_len = (end - start) mod total`; `mid = (start + arc_len / 2) mod total`.
/// The naive `(start + end) / 2` puts the label opposite a wrap arc.
#[must_use]
pub fn wrap_midpoint(start: usize, end: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let start_i = start as i64;
    let end_i = end as i64;
    let total_i = total as i64;
    let arc_len = (end_i - start_i).rem_euclid(total_i);
    (start_i + arc_len / 2).rem_euclid(total_i) as usize
}

/// Circular-aware slice. `end == start` is empty, not the whole plasmid.
#[must_use]
pub fn slice_circular(seq: &str, start: usize, end: usize) -> String {
    if end >= start {
        seq.get(start..end).unwrap_or("").to_owned()
    } else {
        let tail = seq.get(start..).unwrap_or("");
        let head = seq.get(..end).unwrap_or("");
        let mut out = String::with_capacity(tail.len() + head.len());
        out.push_str(tail);
        out.push_str(head);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv05_non_wrapped_midpoint() {
        assert_eq!(wrap_midpoint(100, 200, 1000), 150);
        assert_eq!(wrap_midpoint(0, 500, 1000), 250);
        assert_eq!(wrap_midpoint(10, 12, 1000), 11);
    }

    #[test]
    fn inv05_degenerate_returns_start() {
        assert_eq!(wrap_midpoint(42, 42, 1000), 42);
    }

    #[test]
    fn inv05_wrap_around() {
        assert_eq!(wrap_midpoint(900, 100, 1000), 0);
        assert_eq!(wrap_midpoint(950, 50, 1000), 0);
        assert_eq!(wrap_midpoint(800, 200, 1000), 0);
        assert_eq!(wrap_midpoint(800, 300, 1000), 50);
        assert_eq!(wrap_midpoint(40, 10, 50), 0);
        assert_eq!(wrap_midpoint(45, 5, 50), 0);
    }

    #[test]
    fn inv05_naive_formula_is_opposite_on_wrap() {
        let total = 1000;
        let start = 900;
        let end = 100;
        let naive = (start as i64 + (end as i64 - start as i64) / 2).rem_euclid(total as i64);
        let correct = wrap_midpoint(start, end, total);
        assert_ne!(naive, correct as i64);
        assert_eq!(correct, 0);
        assert_eq!(naive, 500);
    }

    #[test]
    fn inv05_midpoint_lies_on_the_arc() {
        let cases = [
            (100, 200, 1000),
            (900, 100, 1000),
            (950, 50, 1000),
            (0, 999, 1000),
            (500, 500, 1000),
            (0, 1, 1000),
            (999, 0, 1000),
        ];
        for (start, end, total) in cases {
            let mid = wrap_midpoint(start, end, total);
            let arc_len = (end as i64 - start as i64).rem_euclid(total as i64);
            let dist = (mid as i64 - start as i64).rem_euclid(total as i64);
            assert!(
                dist <= arc_len,
                "midpoint({start},{end},{total})={mid} not on arc"
            );
        }
    }

    #[test]
    fn inv05_half_and_more_than_half() {
        assert_eq!(wrap_midpoint(600, 400, 1000), 0);
        assert_eq!(wrap_midpoint(0, 500, 1000), 250);
        assert_eq!(wrap_midpoint(500, 0, 1000), 750);
        assert_eq!(wrap_midpoint(0, 1000, 1000), 0);
        assert_eq!(wrap_midpoint(501, 499, 1000), 0);
    }

    #[test]
    fn inv08_feat_len_linear_and_wrap() {
        assert_eq!(feat_len(100, 200, 1000), 100);
        assert_eq!(feat_len(950, 100, 1000), 150);
        assert_eq!(feat_len(0, 100, 1000), 100);
        assert_eq!(feat_len(800, 0, 1000), 200);
        assert_eq!(feat_len(0, 1000, 1000), 1000);
        assert_eq!(feat_len(50, 51, 1000), 1);
        assert_eq!(feat_len(999, 0, 1000), 1);
    }

    #[test]
    fn inv08_sort_key_orders_wrap_features() {
        let feats = [
            (100, 200, "small_linear"),
            (950, 100, "wrap_150"),
            (500, 900, "big_linear"),
        ];
        let mut by_len = feats.to_vec();
        by_len.sort_by_key(|(s, e, _)| std::cmp::Reverse(feat_len(*s, *e, 1000)));
        assert_eq!(
            by_len.iter().map(|f| f.2).collect::<Vec<_>>(),
            ["big_linear", "wrap_150", "small_linear"]
        );
    }

    #[test]
    fn bp_in_half_open_linear() {
        assert!(!bp_in(9, 10, 20));
        assert!(bp_in(10, 10, 20));
        assert!(bp_in(15, 10, 20));
        assert!(bp_in(19, 10, 20));
        assert!(!bp_in(20, 10, 20));
    }

    #[test]
    fn bp_in_wrapped_feature() {
        for bp in [95, 96, 97, 98, 99, 0, 1, 2, 3, 4] {
            assert!(bp_in(bp, 95, 5), "bp {bp} should be in wrap");
        }
        for bp in [5, 6, 50, 94] {
            assert!(!bp_in(bp, 95, 5), "bp {bp} should not be in wrap");
        }
        assert!(bp_in(0, 990, 10));
        assert!(bp_in(999, 990, 10));
        assert!(!bp_in(500, 990, 10));
        assert!(!bp_in(0, 10, 10));
        assert!(!bp_in(10, 10, 10));
    }

    #[test]
    fn bp_in_full_circle_and_boundaries() {
        for bp in [0, 1, 500, 999] {
            assert!(bp_in(bp, 0, 1000));
        }
        assert!(bp_in(90, 90, 10));
        assert!(bp_in(99, 90, 10));
        assert!(bp_in(0, 90, 10));
        assert!(bp_in(9, 90, 10));
        assert!(!bp_in(10, 90, 10));
        assert!(!bp_in(50, 90, 10));
    }

    #[test]
    fn bp_in_contiguous_along_arc() {
        let total = 100;
        let cases = [(10, 30), (90, 10), (0, 50), (50, 0), (75, 25)];
        for (start, end) in cases {
            let arc_len = (end as i64 - start as i64).rem_euclid(total) as usize;
            let expected: std::collections::BTreeSet<usize> =
                (0..arc_len).map(|k| (start + k) % total as usize).collect();
            let actual: std::collections::BTreeSet<usize> = (0..total as usize)
                .filter(|&bp| bp_in(bp, start, end))
                .collect();
            assert_eq!(actual, expected, "start={start} end={end}");
        }
    }

    #[test]
    fn slice_circular_wrap_and_linear() {
        let seq = "A".repeat(10) + &"C".repeat(40) + &"T".repeat(10);
        assert_eq!(
            slice_circular(&seq, 50, 10),
            "T".repeat(10) + &"A".repeat(10)
        );
        assert_eq!(slice_circular(&seq, 10, 20), "C".repeat(10));
        assert_eq!(slice_circular(&seq, 5, 5), "");
    }
}
