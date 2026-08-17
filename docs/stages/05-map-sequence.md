# Stage 05 — Map + sequence editor

**Status:** done
**Depends on:** 01, 04 (02–03 required before **saving** edits)
**Primary crates:** `splicecraft-tui`, `splicecraft-bio`, `splicecraft-core`

## Goal

Circular and linear Unicode braille maps, per-base two-strand sequence panel,
feature sidebar, restriction overlay, in-place edits with undo.

## Upstream (read before coding)

- `splicecraft_render.py` — `_Canvas`, `_BrailleCanvas`, glyph LUTs
- Hub `PlasmidMap`, `SequencePanel`, `FeatureSidebar`
- Restriction overlay + cut-count superscripts (`EcoRI²`)
- Orientation: reverse-complement whole record; re-origin (`Alt+Shift+O`)
  refused on linear
- `tests/test_render.py`, `tests/test_edit_record.py`,
  `tests/test_orientation.py`, `tests/test_intron_render.py`

**Architecture:** `Record` → pure `render_map(...) -> Vec<String>` so map
geometry is unit-tested without a tty. Ratatui only paints those lines.

## Rust targets

- Circular braille ring + linear view toggle (`v`)
- Feature arcs / lanes, labels using [INV-05] midpoint
- Sequence panel: top strand, bottom RC, wrap-aware feature lanes, CDS
  single-letter AA at codon midpoint
- Click/keyboard selection; smallest enclosing feature on Enter
- Restriction overlay `r`; unique / 6+ / connectors filters can stub to
  all-sites until stage 07 catalog
- Sticky-cut visualization (upstream/downstream tint) if time; otherwise
  ticket in STATUS notes
- Undo/redo stack depth 50, **deep clone** [INV-10], per-record stashes
- Flip record; re-cut origin on circular only
- If persist exists: 3s-debounced crash-recovery autosave through the
  chokepoint

## Sacred invariants

[INV-05] [INV-08] [INV-09] [INV-10] plus scan overlays [INV-01]/[INV-06]
if sites are drawn.

## Acceptance

- [x] Pure function test: circular map of a tiny plasmid contains braille
      (or documented ASCII fallback) and the name/bp
- [x] Wrap feature label uses wrap midpoint, not the naive average
- [x] Edit + undo restores sequence and features (deep clone: mutating the
      restored record does not change the stack entry)
- [x] Re-origin refused on linear
- [x] `cargo test -p splicecraft-tui -p splicecraft-bio`
- [x] No Python

## Forbidden

- Flattening wrap features on edit
- Rounding or dropping origin-spanning sites
- Saving outside `safe_save_json`

## Handoff

Stage 06 adds collections, keep (`Alt+K`), feature library, collision policy.
Do not start it in the same session that closed this stage.

Implementation notes:

- `render_map` / `render_sequence` are pure `Record → Vec<String>`. Ratatui
  only paints those lines. Origin sits at 12 o'clock, clockwise. Labels use
  `wrap_midpoint` ([INV-05]). ASCII density ramp is the documented fallback.
- Sequence panel is top strand + **column-aligned** complement (not `rc` of
  the window), wrap-aware feature lane, CDS AA at codon midpoint.
- Flip: `splicecraft_bio::reverse_complement_record` (`[s,e) → [n-e, n-s)`,
  strand `+1 ↔ -1`, wrap stays wrap). Rotate: `splicecraft_core::rotate_record`
  (linear is a no-op). TUI `SetOriginHere` refuses linear.
- Undo depth 50, `Record::clone()` snapshots ([INV-10]). Edits go through
  `rebuild_record_with_edit` ([INV-09]).
- 3s-debounced crash-recovery `.gb` via `write_crash_recovery` after
  `authorize_writes("splicecraft-tui")`. Tests never authorise; default
  `try_autosave` is a no-op.
- Sticky-cut visualization skipped (ticket in STATUS). RE overlay is all
  labeled 6+ sites until stage 07.
