# Stage 04 — Ratatui shell

**Status:** not started
**Depends on:** 00 (01–03 should stay green; no library writes yet)
**Primary crates:** `splicecraft-tui`, `splicecraft` binary

## Goal

Replace the welcome frame with the workbench chrome: menu bar, three panes
(library / map / sequence placeholders), `?` help overlay, `Ctrl+K` command
palette stub, key table.

## Upstream (read before coding)

- `docs/keybindings.md`
- `docs/features.md` (View / File / Settings menus)
- Hub `PlasmidApp` keybindings, `MenuBar`
- `splicecraft_widgets.py` for palette / fuzzy search behavior
- `tests/test_command_palette.py`, `tests/test_ui_layout.py` (behavioral)

Textual widgets do not map 1:1. Re-implement in Ratatui: event → `Action` →
reduce `AppState` → draw.

## Rust targets

- `Action` enum (Quit, ToggleHelp, OpenPalette, FocusPane, …)
- Crossterm event loop; heavy work later uses `tokio` / worker threads
- Layout: top menu, left library stub, center map stub, bottom or right
  sequence stub (match screenshot intent: map + sequence + library)
- `?` keyboard reference overlay populated from a static table
- `Ctrl+K` fuzzy palette that lists commands (handlers may be stubs that
  toast “not implemented until stage N”)
- Status bar: filename / topology / length / stage
- Empty canvas when no record is loaded
- Optional: open a **memory-only** demo record if stage 01 types exist —
  do not persist it

## Sacred invariants

None new. Do not call persist saves. Do not log sequences.

## Acceptance

- [ ] `TestBackend` tests: help overlay contains a known binding (`q` quit)
- [ ] Palette lists at least Open, Help, Quit
- [ ] `q` / Esc still quits from the main view
- [ ] Resize does not panic (draw on 40x12 and 160x40 backends)
- [ ] Workspace tests + clippy clean
- [ ] Stages 01–03 tests still pass if those crates are filled

## Forbidden

- Implementing braille map geometry (stage 05)
- Writing `collections.json`
- Binding GPL primer3
- Agent HTTP server (stage 14)

## Handoff

Stage 05 paints the real map and sequence panel and introduces undo [INV-10].
