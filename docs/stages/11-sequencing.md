# Stage 11 — Sequencing

**Status:** done
**Depends on:** 03, 05, 06
**Primary crates:** `splicecraft-io`, `splicecraft-tui`

## Goal

Verify constructs against reads: Plasmidsaurus zip + API, pairwise alignment
overlay, AB1 traces, verification report that never rounds 99.x% to 100%.

## Upstream (read before coding)

- `splicecraft_fileio.py` — zip member safety, Plasmidsaurus parsers, AB1
- Hub Sequencing screen, alignment overlay
- `docs/subsystems.md` `[SUB-plasmidsaurus]`
- `tests/test_plasmidsaurus_api.py`, `tests/test_alignment_overlay.py`,
  `tests/test_commercialsaas_io.py` (`.dna` increment may land here)

Alignment: port a Myers / Hirschberg implementation in Rust (MIT). Optional
`edlib`-style crate only if license is MIT/Apache. Do not require unmaintained
C deps for default CI.

## Rust targets

- Safe zip extract (no path traversal, size caps)
- Align read vs loaded plasmid; colored match/mismatch/gap on linear map
- Click bar → jump sequence panel
- Bulk auto-align a folder
- Verification grades: verified / near / partial / divergent
- **Display rule:** identity `< 100` never formats as `100%`
- Plasmidsaurus API: env-first credentials, 10 req/min awareness, cache
  listing ~2 min; default tests mock HTTP
- AB1 load with Phred
- `.dna` TLV reader/writer can start here if not done in stage 03; history
  XML round-trip

## Sacred invariants

Identity formatting. Zip slip forbidden. Network allowlist for the API host
only.

## Acceptance

- [x] `99.6%` renders as `99.6%` or `99%`, never `100%`
- [x] Zip `../` member rejected
- [x] Alignment mismatch positions test on a tiny pair
- [x] API client unit-tested with a mock, no live network in CI
- [x] `cargo test -p splicecraft-io -p splicecraft-tui`

## Notes

- Identity: `format_identity_pct` escalates 0–4 decimal places so a true
  sub-100 value never prints as `100%` (pathological leftovers become
  `<100%`).
- Zip: `..` / absolute / drive-letter members are refused; extract is
  size-capped. Plasmidsaurus ingest tags `plasmidsaurus:<run>:<sample>`
  and uses `LibraryStore::import_without_overwrite` (never clobber).
- Align: Myers / Hirschberg global pairwise with IUPAC counting. Linear map
  overlay is `=` / `X` / `.`; `j` jumps the sequence cursor to the
  highlighted variant.
- Grades: verified (99.5% ungapped, 99% coverage, 0 gaps, 0 SNPs) / near
  (95% / 80%) / partial / divergent.
- API: env-first `PLASMIDSAURUS_CLIENT_ID` / `_SECRET`, host allowlist
  `app.plasmidsaurus.com`, ~2 min listing cache. Default transport is
  offline; tests inject a mock. No live HTTP in CI.
- AB1 carries base-calls + Phred. `.dna` TLV (cookie `0x09`, DNA `0x00`,
  features `0x0A`, xz history `0x07`) round-trips history XML. Bulk import
  still skips `.dna` (open via `load_path`).
- TUI Sequencing overlay: zip | align | Sanger | report.

## Forbidden

- Live Plasmidsaurus calls in default CI
- Overwriting library entries on import (`plasmidsaurus::` tag, never clobber)

## Handoff

Stage 12 experiments notebook + History viewer.
