# Stage 01 — Core + sacred biology

**Status:** done
**Depends on:** 00
**Primary crates:** `splicecraft-core`, `splicecraft-util`, `splicecraft-bio`

## Goal

Port the biology kernel so restriction math, circular features, IUPAC, and
translation are correct **before** any TUI can edit DNA. This is the fast
inner loop: `cargo test -p splicecraft-bio`.

## Upstream (read before coding)

Fetch with `gh`; do not commit the files.

- `splicecraft_biology.py` — `_rc`, `_iupac_pattern`, `_scan_restriction_sites`,
  `_enzyme_cuts`, `_translate_cds`, `_feat_len` (if still here / re-exported)
- `splicecraft.py` / `splicecraft_record.py` — wrap midpoint, `_bp_in`,
  `_rebuild_record_with_edit`
- `splicecraft_util.py` — sanitizers, `_now` / `_monotonic`
- `tests/test_dna_sanity.py` — **port these tests**, do not invent a weaker set
- `tests/test_circular_math.py` if present
- [`CLAUDE.md` sacred list](https://github.com/Binomica-Labs/SpliceCraft/blob/master/CLAUDE.md)

Permalink base:
`https://raw.githubusercontent.com/Binomica-Labs/SpliceCraft/master/`

## Rust targets

### `splicecraft-core`

- `Record` { `name`, `sequence`, `circular`, `features`, `molecule_type` }
- `Feature` { `kind`, `start`, `end`, `strand`, `label`, `qualifiers`, wrap-capable }
- `feat_len(start, end, total)`
- `bp_in(pos, start, end, total)` — wrap-aware membership
- `wrap_midpoint(start, end, total)` — [INV-05]
- `rebuild_record_with_edit` — [INV-09]
- Coordinates: document 0-based half-open internally; convert at I/O later

### `splicecraft-util`

- `now` / `monotonic` single time source
- label / filename sanitizers (no path traversal)
- DNA-shaped redaction helper for logs (even if logging is later)

### `splicecraft-bio`

- `rc(&[u8]) -> Vec<u8>` full IUPAC — [INV-03]
- `iupac_pattern` + cache — [INV-04]
- `scan_restriction_sites` — [INV-01] [INV-02] [INV-06]
- `enzyme_cuts` / digest split helpers
- `translate_cds` with a standard table; non-standard tables can stub to
  table 11 until stage 09
- A small built-in enzyme catalog (enough for EcoRI, HindIII, BsaI, Esp3I,
  BbsI, palindromes + Type IIS). Full NEB catalog can land in stage 07.

Braille rendering is **not** this stage.

## Sacred invariants

[INV-01] [INV-02] [INV-03] [INV-04] [INV-05] [INV-06] [INV-08] [INV-09]

Each gets at least one named test. Palindrome double-count and wrap-scan
two-piece resite are non-negotiable.

## Acceptance

- [x] `cargo test -p splicecraft-core -p splicecraft-util -p splicecraft-bio`
- [x] Ported equivalents of `tests/test_dna_sanity.py` assertions pass
- [x] Property tests (e.g. `proptest`) for: `rc(rc(s))` on IUPAC; wrap
      `feat_len`; palindromic sites counted once
- [x] Type IIS (BsaI) cut positions use forward coordinates on both strands
- [x] Wrap feature survives a mid-arc insertion without flattening
- [x] `clippy -D warnings` still clean
- [x] No `.py` files

## Forbidden

- Calling NCBI or writing any data dir
- Depending on `bio` crate in a way that **replaces** wrap-aware feature
  math (external crates may help alphabets, not circular feature integrity)
- GPL `primer3`
- TUI sequence editing

## Handoff

Stage 02 implements `safe_save_json` and the write chokepoint. Do not save
libraries until that lands.
