# Stage 19 — Interaction keys + menu activation

**Status:** done
**Depends on:** 18
**Primary crates:** `splicecraft-tui`, `splicecraft-io` (fetch only)

## Goal

Close the **daily-driver keybinding and menu gaps** that make the Rust
workbench feel unfinished even after colors land. Prefer wiring existing
overlays / libraries over new subsystems.

Gaps tracked in [`docs/parity.md`](../parity.md) (Keybindings) and
[`docs/theme.md`](../theme.md) (chrome roles).

## Upstream (read before coding)

- [docs/keybindings.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/keybindings.md)
- Hub `MenuBar` — click opens tool (File is the dropdown exception)
- Fetch / New Plasmid / Find / clipboard / primer / BLAST bindings

## Rust targets

### Keys (must land unless already done)

| Binding | Behavior |
|---|---|
| `Ctrl+B` | Open Search / BLAST overlay |
| `Ctrl+P` | Open Primer design |
| `F6` (and/or `Alt+H`) | Open History |
| `Ctrl+S` | Save loaded record through persist chokepoint (or clear toast if nothing dirty) |
| `Ctrl+F` | Find DNA subsequence (both strands); jump cursor |
| `Ctrl+A` | Select-all sequence (or document intentional seq-pane limit) |
| `Ctrl+C` / `Alt+C` | Copy selection top strand / bottom RC |
| `Ctrl+N` | New Plasmid prompt (paste sequence → memory record; annotate optional/stub) |
| `f` | NCBI fetch UI using `splicecraft_io::fetch_genbank` (online gated by settings) |
| `Alt+Shift+F` | Add feature from selection (minimal dialog: label + type) |

### Menu bar

- Highlight menu with Left/Right (or number/`Alt` letter where already in
  upstream Help); Enter opens the same action as the palette / overlay
- File: at least Open / Fetch / Quit reachable without Ctrl+K

### Library (if time; else ticket in STATUS notes)

- `Space` mark, `u` undelete already exist — add `c` clear marks if marks land
- Defer bulk `m` / `y` / `p` / `s` / `h` to stage 20 notes if incomplete

## Sacred invariants

[INV-07] on any save. Online fetch stays fail-closed unless
`allow_online_lookups` / search setting is armed. Never log sequences.

## Acceptance

- [x] `Ctrl+B`, `Ctrl+P`, `F6` open live overlays (not stub toasts)
- [x] `f` fetch path is not a stage-13 stub when online is allowed; offline
      shows a clear toast
- [x] `Ctrl+C` / `Alt+C` copy tests (or documented terminal clipboard limit)
- [x] Menu bar can open Primers and BLAST without the palette
- [x] `KEY_TABLE` / `?` help updated
- [x] `docs/parity.md` keybinding rows updated
- [x] Workspace tests + fmt + clippy clean
- [x] Sandboxed persist in any save test (`XDG_DATA_HOME`)

## Forbidden

- Ungated NCBI / online calls in default CI
- Writing Python leaf `splicecraft`
- Master Delete without existing triple confirm
- Mouse support (stage 20)

## Notes

- File opens with **F10** (focus) then **Enter** (dropdown). Upstream has no
  Alt+F for File. Alt letters match Help: `s` Settings, `n` Enzymes,
  `p` Primers, `y` Synthesis, `r` Parts, `i` Simulator, `q` Sequencing,
  `x` Experiments, `h` History, `u` AUTOLAB.
- Copy is an **in-app** buffer. The toast says there is no OSC-52 / system
  clipboard.
- NCBI: TUI calls `fetch_genbank`. Default CI stays `NetworkDisabled`. Both
  online settings off → toast to enable Settings.
- Library Space / mark cycle / `c` / bulk `m` `y` `p` `s` `h` deferred to
  stage 20.

## Handoff

Stage 20 adds mouse (including menu-bar hits), sticky-cut tint, and leftover
library mark/export chrome.
