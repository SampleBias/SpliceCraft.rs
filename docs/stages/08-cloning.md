# Stage 08 — Cloning workbench

**Status:** not started
**Depends on:** 07
**Primary crates:** `splicecraft-clone`, `splicecraft-tui`

## Goal

Simulate the real bench: Traditional ligation, Gibson, Golden Braid / MoClo,
Domesticator, Parts Bin, Constructor, L0-from-synthetic-fragment, history
nodes on products.

## Upstream (read before coding)

- `splicecraft_cloning.py` — `_simulate_traditional_cloning`,
  `_simulate_gibson_assembly`, `_ligate_fragments`, `_close_circular`,
  `_ends_compatible`, `_design_gb_primers`, `_excise_fragment_pair`,
  `_make_synthetic_fragment`
- `splicecraft_seqanalysis.py` — `_classify_part_from_plasmid`
- Hub Domesticator, Parts Bin, Constructor, GrammarEditor
- `tests/test_traditional_cloning.py`, `tests/test_gibson.py`,
  `tests/test_domesticator.py`, `tests/test_parts_bins.py`,
  `tests/test_l0_part_from_fragment.py`, `tests/test_constructor_preview.py`
- Built-in grammars: GB L0 (Esp3I), MoClo Plant (BsaI)

INV-127 upstream: “the design IS the product” — digest + ligate must
actually produce the claimed sequence.

## Rust targets

- Fragment end chemistry + ligation compatibility
- Traditional: 2-enzyme directional products, both orientations, refuse
  non-ligatable silently-dropped cases
- Gibson: N fragments, longest exact overlap per junction including wrap,
  min-overlap validate, reverse-orientation hint, linearize-at, design
  homology arms (idempotent)
- Grammars JSON (user-defined) via persist chokepoint
- Domesticator: 4-source picker, forbidden-site codon scrub (may call
  codon crate stubs until stage 09 — if codon is empty, limit to DNA-level
  or gate the UI)
- Parts Bin per-grammar; classify-by-digest
- Constructor tabs; product lands in library with parent features
- New Part from Syn Frag: real digest/ligate/close; refuse overhang mismatch
- History node on the product (full History UI is stage 12)
- Deletion undo for last-N library deletes (session)

## Sacred invariants

Digest math [INV-01][INV-02][INV-06]. Wrap features on products [INV-09].
Saves [INV-07].

## Acceptance

- [ ] Golden: ligate two known sticky fragments → expected circle
- [ ] Gibson wrap junction tested
- [ ] Type IIS domestication primer tails follow grammar pad/site/spacer/overhang
- [ ] Syn-frag mismatch refused
- [ ] `cargo test -p splicecraft-clone`

## Forbidden

- Filing a part that cannot assemble
- Rewriting history to hide a missing site (report instead — stage 12)

## Handoff

Stage 09 Mutato, codon optimizer, synthesis composers.
