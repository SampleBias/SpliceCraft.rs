# Stage 09 — Mutato + codon + synthesis

**Status:** not started
**Depends on:** 07, 08
**Primary crates:** `splicecraft-codon`, `splicecraft-primer`, `splicecraft-tui`

## Goal

Site-directed mutagenesis, clone-free restriction scrub, codon optimization,
DNA/protein/operon synthesis composers, L0 fragment wrap.

## Upstream (read before coding)

- `splicecraft_codon.py` — `_codon_optimize`, `_codon_allocate`,
  `_codon_fix_sites`, CAI/GC, tables
- `splicecraft_primer.py` — SOE / QuikChange / `_scrub_design`
- `splicecraft_cloning.py` — `_assemble_scrub_amplicons_real`, GB scrub
- Hub Mutato, Synthesis (DNA / Protein / Operon), codon-table manager
- `tests/test_mutagenize.py`, `tests/test_scrub.py`, `tests/test_codon.py`,
  `tests/test_synthesis.py`, `tests/test_operon_soe.py`

## Rust targets

- Genetic-code tables; Kazusa / TSV / genome-derived tables (genome build
  can be offline FASTA)
- Optimize with forbidden-site scrub; never introduce a new forbidden site
- Mutato: `L54A`-style on a CDS → SOE 4-primer; near-end fallback 2-primer
  modified-outer; only offer the shortcut when the primer carries the change
- Scrub tab: minimal synonymous fixes across overlapping frames; report
  un-curable; Apply via QuikChange or Golden Braid **real** digest+ligate
- Synthesis DNA editor (linear, features, live translation)
- Protein composer fills codons from the active table; motif library
- Operon RBS-in-context + Type IIS domestication primers
- L0 fragment nested overhangs (two-tier aware)

## Sacred invariants

Translation [INV-03] related IUPAC; codon scrub must not move feature
coordinates (substitution-only). GB recirc simulation must match the
cured plasmid before commit.

## Acceptance

- [ ] SOE primers for a mid-CDS mutation have the intended mismatch
- [ ] Near-end mutation uses 2-primer path
- [ ] Scrub does not create a new BsaI while killing Esp3I (fixture)
- [ ] GB recirc self-check fails closed
- [ ] `cargo test -p splicecraft-codon -p splicecraft-primer -p splicecraft-clone`

## Forbidden

- Silent wild-type amplification path
- GPL primer3 default dep

## Handoff

Stage 10 simulator + gels.
