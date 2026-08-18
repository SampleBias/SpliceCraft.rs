# Stage 17 — Theme + colored chrome

**Status:** done
**Depends on:** 16
**Primary crates:** `splicecraft-tui`

## Goal

Stop the bland gray/cyan shell. Port the upstream **visual language** into
Ratatui: theme tokens, feature-type colors, styled map/sequence/sidebar,
activatable-looking menu bar, and a shortcut footer. Biology and persist
paths stay untouched.

Read [`docs/theme.md`](../theme.md) end to end before coding.

## Upstream (read before coding)

- `splicecraft_util.py` — `_DEFAULT_TYPE_COLORS`
- `splicecraft_widgets.py` — `_FEATURE_PALETTE`, `_resolve_feature_color`,
  `_xterm_index_to_hex`
- Hub `PlasmidMap` / `FeatureSidebar` / `SequencePanel` / `MenuBar` paint
  paths (colors only)
- `splicecraftScreenshot.png` — density / color reference
- [`docs/theme.md`](../theme.md)

```bash
gh api repos/Binomica-Labs/SpliceCraft/contents/splicecraft_util.py \
  --jq .download_url
```

## Rust targets

- `splicecraft_tui::theme` (or equivalent module): chrome colors, type →
  `ratatui::style::Color`, xterm-256 → RGB/hex helper, `resolve_feature_color`
- Styled render path: map labels + RE ticks, sequence bases/AA/feature lane,
  feature sidebar Type|Label rows use resolved colors
- Library selected row: primary bg + dark fg (not a gray `>` prefix alone)
- Menu bar: brand chip + 16 menus with distinct / highlightable styling
  (activation wiring may complete in stage 19; **look** live here)
- Footer: shortcut strip per [`docs/theme.md`](../theme.md) (toast may
  temporarily replace the strip)
- Keep `render_map` / `render_sequence` geometry tests green; add styled
  variants or span overlays without changing midpoints / braille samples

## Sacred invariants

None new. Do not alter scan / `rc` / IUPAC / wrap / `rebuild_record_with_edit`
/ persist. Color is paint-only.

## Acceptance

- [x] `docs/theme.md` exists and is referenced from this stage
- [x] Type-color table matches upstream hex for CDS, promoter, terminator,
      primer_bind, rep_origin, misc_feature (spot-check in a unit test)
- [x] `resolve_feature_color` respects qualifier → type default → palette[0]
- [x] Drawing the demo / tiny plasmid uses **non-gray** spans for at least
      one feature label on the map and one feature sidebar row
  (`TestBackend` or styled-line unit test)
- [x] Sequence AA lane is green-styled when a CDS is visible
- [x] Footer no longer shows `stage 16` as the primary chrome; shortcuts or
      toast only
- [x] `cargo test -p splicecraft-tui` + `cargo test --workspace`
- [x] `cargo fmt --all -- --check` and `clippy -D warnings` clean
- [x] `docs/parity.md` notes visual chrome as in progress / done for this
      wave (map/seq color, sidebar color, footer)

## Forbidden

- Vendoring `.py` or copying TCSS into the tree
- Writing the Python XDG leaf `splicecraft`
- Changing layout percentages (that is stage 18)
- Wiring every missing keybinding (stage 19)
- Mouse / sticky-cut tint (stage 20)
- Declaring pixel-identical Textual parity

## Handoff

Stage 18 locks pane sizes and overlay density on top of this theme.
Do not start 18 in the same session unless the user asked for more than one
stage.
