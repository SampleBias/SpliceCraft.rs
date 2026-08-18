# Stage 20 — Mouse + remaining polish

**Status:** open
**Depends on:** 19
**Primary crates:** `splicecraft-tui`

## Goal

Close the last **chrome** gaps that upstream users expect from a terminal
plasmid bench: mouse hit-testing on map/sequence, sticky-cut tint, and
leftover library mark/export UX. Still Ratatui — not a Textual clone.

Contract: [`docs/theme.md`](../theme.md). Gaps: [`docs/parity.md`](../parity.md).

## Upstream (read before coding)

- [docs/keybindings.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/keybindings.md)
  — Mouse section
- Hub click handlers on `PlasmidMap` / `SequencePanel` / restriction sites
- Sticky-cut upstream/downstream tint (blue / red per strand)
- Library mark cycle and `p` map-image export for marked plasmids

## Rust targets

- Enable crossterm mouse where safe; click DNA row → cursor; click feature
  bar → select feature; click RE site → highlight span
- Menu bar click-to-open (File dropdown on click) — keyboard landed in 19
- Sticky-cut visualization on Type IIS click (deferred since stage 05)
- Scroll wheel over map → rotate view (when map focused / hovered)
- Library: `Space` mark cycle if not done; `p` export marked maps via existing
  mapimage helpers; `c` clear marks
- Optional: `,` / `.` map aspect; `Alt+D` UI snapshot path under the Rust
  data dir only
- Update `docs/parity.md` mouse / sticky-cut / library rows

## Sacred invariants

[INV-01]–[INV-06] for any RE highlight path. Map geometry midpoint stays
[INV-05]. Snapshots / exports never write the Python leaf.

## Acceptance

- [ ] At least: click sequence places cursor; click feature sidebar row
      selects feature (TestBackend or integration with synthetic events)
- [ ] Sticky-cut tint draws distinct up/downstream colors on a Type IIS hit
- [ ] Wheel or documented key still rotates map
- [ ] Marked-plasmid map export works or is explicitly still **gap** with
      reason in `parity.md`
- [ ] Workspace tests + fmt + clippy clean
- [ ] `docs/theme.md` stages table still accurate; STATUS marks 20 done

## Forbidden

- Pixel-perfect Textual claim
- Sharing `~/.local/share/splicecraft/`
- New bootstrap stage 21 — further work returns to ordinary `parity.md`
  issues unless the user opens a new stage track

## Handoff

UI polish track complete. Remaining product gaps (cassette/splice, agent
endpoint expansion, OT-2 live, etc.) stay in [`docs/parity.md`](../parity.md).
