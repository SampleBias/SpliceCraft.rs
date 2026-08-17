# Stage 10 — Simulator + gels

**Status:** done
**Depends on:** 07, 08
**Primary crates:** `splicecraft-gels`, `splicecraft-clone`, `splicecraft-tui`

## Goal

In-silico PCR amplicon enumeration and agarose gel rendering with a real
mobility curve.

## Upstream (read before coding)

- `splicecraft_gels.py` — `_agarose_mobility`, `_gel_bands_for_lane`,
  `_render_gel_image`, ladders, form factors
- Hub Simulator screen (PCR + gel tabs)
- `docs/subsystems.md` `[SUB-gels]`
- `tests/test_gels.py`, `tests/test_simulator.py`

## Rust targets

- Exact-match primer binding, wrap-aware on circular templates
- Cap result count (upstream 50) to flag mispriming runaway
- Save amplicon to library as linear DNA with `primer_bind` features
- Gel: 0.5–4.0% agarose, Helling–Goodman–Boyer mobility
  (`distance ∝ -log10(bp)` in the resolution window)
- Supercoiled faster than linear; nicked slower
- Up to 8 lanes; ladder / uncut / digest / PCR
- Persist `gels.json` through the chokepoint; `&gel` ids for stage 12

## Sacred invariants

Wrap PCR [INV-06]-adjacent: origin-spanning amplicons are legal. Saves
[INV-07].

## Acceptance

- [x] Circular template wrap amplicon included
- [x] Mobility: larger band migrates less than smaller at 1% (numeric)
- [x] Supercoiled vs linear order test
- [x] Gel reload from sandboxed JSON
- [x] `cargo test -p splicecraft-gels`

## Notes

- PCR: exact-match plus 3′-anchored partial fallback for cloning flaps.
  Cap is 50 products (`PCR_MAX_AMPLICONS`). IUPAC primers error instead
  of silently returning empty. Save-to-library writes a linear record
  with `primer_bind` features at both ends.
- Gel: 0.5–4.0% windows, in-window distance ∝ `-log10(bp)` compressed
  into `[0.03, 0.97]`, damped edges keep size order. Lanes: ladder /
  uncut (SC+nicked) / digest / PCR. UI cap 8 lanes.
- Persist: `gels.json` via `save_gels` / `load_gels`. `&gel` ids via
  `extract_gel_refs` for stage 12.
- TUI Simulator overlay: PCR | gel. `g` pins an amplicon as a frozen
  PCR lane; `s` saves the amplicon or gel snapshot.

## Forbidden

- Pretend-log-scale that is not the HGB curve
- Writing gels outside persist

## Handoff

Stage 11 sequencing verification.
