# Stage 07 — Enzymes + primers

**Status:** not started
**Depends on:** 01, 05, 06
**Primary crates:** `splicecraft-bio`, `splicecraft-primer`, `splicecraft-tui`

## Goal

NEB-scale enzyme catalog + collections; primer designers; primer library
lifecycle; primer-check / in-silico PCR listing.

## Upstream (read before coding)

- Enzyme catalog in the hub / `splicecraft_dataaccess.py` / biology
- `splicecraft_primer.py` — `_primer_tm`, `_design_*_primers`,
  `_primer_binding_sites`, `_rederive_primer_binding`,
  `_primer_check_confidence`
- `tests/test_primers.py`, `tests/test_primer_check.py`,
  `tests/test_primer_collections.py`, `tests/test_enzyme_collections.py`,
  `tests/test_primer_binding_rotation.py`
- Primer3-py usage in the hub — **reimplement Tm/design in MIT Rust**.
  Do **not** add crates.io `primer3` (GPL-2.0) as a default dependency.
  Optional later: feature-flag spawn of a system `primer3` binary.

## Rust targets

- 200+ NEB enzymes (port the catalog data as Rust/JSON **you type or
  generate at build from a documented non-Python source**; if you
  transcribe from upstream, treat it as data, not code)
- Custom enzymes + named collections; active collection scopes scans
- Multi-cutter superscript cut-counts on the map
- Detection / cloning / Golden Braid / generic designers
- Primer records: Designed → Ordered → Validated
- Primer-check: one oligo vs library (identity, strand, position);
  two oligos → amplicon length; 3′ binding so 5′ tails do not vanish
- CSV + IDT bulk-upload export
- Binding re-derived after rotation so display == anneal [rotation invariant]

## Sacred invariants

[INV-01] [INV-02] [INV-06] for overlay counts. Primer binding after
rotation must match the map.

## Acceptance

- [ ] Palindrome still counted once with the full catalog
- [ ] Tm function has golden tests vs a few upstream examples you record
      in `tests/data/` (numbers, not Python)
- [ ] Primer-check 3′ identity test
- [ ] IDT CSV columns documented and tested
- [ ] Default features remain MIT (no GPL primer3 crate)
- [ ] `cargo test -p splicecraft-primer -p splicecraft-bio`

## Forbidden

- `primer3 = "..."` in workspace default dependencies
- Shipping sequences to NCBI from this stage

## Handoff

Stage 08 cloning workbench consumes primers, enzymes, and grammars.
