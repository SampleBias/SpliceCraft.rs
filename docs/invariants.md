# Sacred invariants (Rust names)

Port of the Python project's numbered invariants. Each must have a regression
test once the owning stage is done. Tags: `[INV-01]` … `[INV-10]`.

Upstream prose: [Binomica-Labs/SpliceCraft CLAUDE.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/CLAUDE.md)
and [docs/invariants.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/invariants.md)
(`[PIT-]` / `[INV-]` beyond the core ten).

## Core ten

### [INV-01] Palindromic enzymes scanned forward only

A palindromic site scanned on both strands double-counts. Scan the forward
strand only; emit the bottom-strand hit as a `recut`, not a second `resite`.

**Rust home:** `splicecraft_bio` restriction scanner (stage 01).
**Upstream:** `_scan_restriction_sites`.

### [INV-02] Reverse-strand resite positions use forward coordinates

A reverse hit found at index `p` after reverse-complement is stored as `p`,
not `n - p - site_len`. Map the cut column with
`rev_cut_col = site_len - fwd_cut`.

**Rust home:** `splicecraft_bio`.
**Upstream:** `_scan_restriction_sites` / `_enzyme_cuts`.

### [INV-03] Reverse-complement is full IUPAC

`rc` uses a complete IUPAC complement table, not just ACGT.

**Rust home:** `splicecraft_bio::rc`.
**Upstream:** `_rc` / `_IUPAC_COMP`.

### [INV-04] IUPAC patterns are cached

Compiled IUPAC matchers live in a process cache. Do not recompile per base.

**Rust home:** `splicecraft_bio` pattern cache.
**Upstream:** `_iupac_pattern` / `_PATTERN_CACHE`.

### [INV-05] Circular wrap midpoint

```
arc_len = (end - start).rem_euclid(total)
mid     = (start + arc_len / 2).rem_euclid(total)
```

The naive `(start + end) / 2` puts the label opposite the actual arc when
the feature wraps the origin.

**Rust home:** `splicecraft_core` circular math.
**Upstream:** wrap-midpoint formula in the hub / biology.

### [INV-06] Circular wrap restriction scan

Scan `seq + seq[..max_site_len.saturating_sub(1)]`. Each wrap hit emits:

- two **resite** pieces: labeled tail `[p, n)` + unlabeled head
  `[0, (p + site_len) - n)`
- one **recut** at `(p + fwd_cut) % n`

Resite-counting code must count **labeled** pieces only.

**Rust home:** `splicecraft_bio` scanner.
**Upstream:** `_scan_restriction_sites` wrap branch.

### [INV-07] Data-file saves always back up

Every JSON library write goes through one chokepoint:

- schema envelope `{"_schema_version": 1, "entries": [...]}`
- load path accepts a legacy bare list
- write: tempfile + `fsync` + atomic replace + `.bak` rotation
- suspicious-shrink refuse (do not replace a large library with empty)
- **re-raise on failure** so callers can notify; never swallow

**Rust home:** `splicecraft_persist::safe_save_json` (stage 02).
**Upstream:** `_safe_save_json`.

### [INV-08] Wrap-aware feature length

`feat_len(start, end, total)` returns `(total - start) + end` when
`end < start`. All sort keys, length displays, and biology checks route
through it.

**Rust home:** `splicecraft_core::feat_len`.
**Upstream:** `_feat_len`.

### [INV-09] Wrap-feature integrity in record edits

Flattening a wrap feature by taking `min(parts.start)` silently destroys
origin-spanning annotations. `rebuild_record_with_edit` must shift each
part and only collapse when a single part survives.

**Rust home:** `splicecraft_core` record edits.
**Upstream:** `_rebuild_record_with_edit`.

### [INV-10] Undo snapshots are deep clones

Push / undo / redo clone the whole `Record`. Shared mutability across the
undo stack is a data-loss bug.

**Rust home:** `splicecraft_tui::UndoStack` + `Record: Clone` that is deep.
**Upstream:** `_push_undo` / `_action_undo` / `_action_redo`.

## Named tests (stage 16 audit)

Each core tag has at least one `invNN_*` test. Do not rename these without
updating [`parity.md`](parity.md).

| Tag | Named tests |
|---|---|
| [INV-01] | `splicecraft-bio`: `inv01_ecori_single_site_not_double_counted`, `inv01_palindrome_one_recut`, `inv01_three_ecori_sites`, `inv01_cut_count_badge`, `inv01_full_catalog_still_counts_ecori_once`, `inv01_planted_ecori_counted_once` |
| [INV-02] | `splicecraft-bio`: `inv02_bsai_forward_and_reverse_coords`, `inv02_bsai_reverse_recut_off_by_one` |
| [INV-03] | `splicecraft-bio`: `inv03_rc_acgt_ground_truth`, `inv03_each_iupac_code`, `inv03_rc_uppercases_and_preserves_len`, `inv03_rc_is_involutive` |
| [INV-04] | `splicecraft-bio`: `inv04_plain_and_degenerate_patterns`, `inv04_cache_same_object`, `inv04_rejects_non_iupac` |
| [INV-05] | `splicecraft-core`: `inv05_non_wrapped_midpoint`, `inv05_degenerate_returns_start`, `inv05_wrap_around`, `inv05_naive_formula_is_opposite_on_wrap`, `inv05_midpoint_lies_on_the_arc`, `inv05_half_and_more_than_half` |
| [INV-06] | `splicecraft-bio`: `inv06_circular_wrap_ecori`, `inv06_wrap_type_iis_ext_and_rec_bounds`, `inv06_non_wrap_rec_bounds_equal_span`, `inv06_wrap_reverse_bsai` |
| [INV-07] | `splicecraft-persist`: `inv07_bak_exists_after_second_save`, `inv07_crash_between_write_and_replace_leaves_previous_intact`, `inv07_shrink_refuse_does_not_overwrite_large_fixture_with_empty` |
| [INV-08] | `splicecraft-core`: `inv08_feat_len_linear_and_wrap`, `inv08_sort_key_orders_wrap_features` |
| [INV-09] | `splicecraft-core`: `inv09_wrap_survives_mid_arc_insert`, `inv09_insert_at_origin_keeps_wrap`, `inv09_linear_feature_shifts_after_upstream_insert`, `inv09_replace_consumes_feature` |
| [INV-10] | `splicecraft-tui`: `inv10_edit_undo_is_deep_clone` |

## Standing contracts (from upstream pitfalls)

These are not the core ten but agents must not regress them once the
matching stage lands. Grep upstream `docs/invariants.md` for the `[PIT-]` /
`[INV-]` tag before editing that area.

| Tag | Contract |
|---|---|
| `[PIT]` wrap features | `end < start` is legal on circular records |
| data-dir leaf | `splicecraft-rs`, never `splicecraft` |
| sequences in logs | forbidden; redact DNA-shaped strings |
| online egress | off until a setting is ticked; demo fail-closes |
| identity display | a true sub-100% alignment never rounds up to "100%" |
| Primer3 linear-only | circular templates must be handled by *this* stack, not assumed linear |
| GPL primer3 crate | not a default MIT dependency |

When you add a new invariant, give it a tag, a test, and a one-line pointer
in the owning stage file.
