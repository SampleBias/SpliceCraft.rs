# Stage 10 — Simulator + gels

**Status:** not started
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

- [ ] Circular template wrap amplicon included
- [ ] Mobility: larger band migrates less than smaller at 1% (numeric)
- [ ] Supercoiled vs linear order test
- [ ] Gel reload from sandboxed JSON
- [ ] `cargo test -p splicecraft-gels`

## Forbidden

- Pretend-log-scale that is not the HGB curve
- Writing gels outside persist

## Handoff

Stage 11 sequencing verification.
