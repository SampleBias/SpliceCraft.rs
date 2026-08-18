# Stage 18 — Layout density + overlay polish

**Status:** done
**Depends on:** 17
**Primary crates:** `splicecraft-tui`

## Goal

Match upstream **spacing**: fixed-width library/features, fixed-height
sequence strip, map takes remaining space, focus brighten, overlays that
feel like the same dialogs (not oversized cyan boxes).

Contract: [`docs/theme.md`](../theme.md) (Layout density + Chrome roles).

## Upstream (read before coding)

- `PlasmidApp.CSS` — `#top-row`, `LibraryPanel { width: 32 }`,
  `FeatureSidebar { width: 32 }`, `SequencePanel { height: 14 }`
- Panel `focus-within { background: #0c0c0c }`
- Modal centering / max-size habits (Ratatui `centered` helpers)
- `tests/test_ui_layout.py` (behavioral intent only)

## Rust targets

- `draw_body` constraints: Library ~32 cols, Features ~32 cols, Map `Min(1)`,
  Sequence `Length(14)` (or theme constants), full width under top row
- Focused pane: border + subtle `#0c0c0c` fill behind content
- Library: search line under header; keep list dense
- Feature sidebar: columnar Type | Label (bp/strand when width allows)
- Overlays (help, palette, tools): consistent padding, title, border color
  from theme; cap size to viewport on small terminals (40×12 still paints)
- Resize tests on 40×12 and 160×40 still pass

## Sacred invariants

None new. Paint / layout only.

## Acceptance

- [x] Default `FocusMode::All` uses fixed side widths + fixed sequence height
      from theme constants (documented in `docs/theme.md`)
- [x] Focused pane background differs from unfocused (visible in
      `TestBackend` style or a small draw unit test)
- [x] Help / palette / Settings overlays share one dialog chrome helper
- [x] 40×12 and 160×40 draws do not panic
- [x] `cargo test -p splicecraft-tui` + workspace / fmt / clippy gates
- [x] `docs/parity.md` layout notes updated if any gap closed

## Forbidden

- Soft percentage-only layout that ignores the 32 / 14 targets on normal
  terminals (≥100×30)
- Persist writes outside the chokepoint
- Starting stage 19 key work in this session unless requested

## Handoff

Stage 19 wires missing keys and menu activation so the denser chrome is
actually operable like upstream.
