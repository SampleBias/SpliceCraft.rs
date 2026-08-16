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

**Rust home:** TUI undo (stage 05) + `Record: Clone` that is deep.
**Upstream:** `_push_undo` / `_action_undo` / `_action_redo`.

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
