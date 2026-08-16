# Stage 03 — File I/O

**Status:** done
**Depends on:** 01, 02
**Primary crates:** `splicecraft-io`

## Goal

Load and save plasmids from disk and (opt-in) NCBI. Default test suite stays
offline.

## Upstream (read before coding)

- `splicecraft_record.py` — `_gb_text_to_record`, `_record_to_gb_text`,
  LOCUS sanitise, arrowless-feature round-trip
- `splicecraft_fileio.py` — FASTA, GFF3, `load_genbank`, format detect,
  `fetch_genbank` / Entrez
- `splicecraft_net.py` — SSRF-hardened opener, timeouts, redirect cap,
  `_sanitize_accession`
- `tests/test_genbank_io.py`, `tests/test_new_formats.py`
- `.dna` / Commercial SaaS codec is **deferred** (increment at end of this
  crate or stage 11). Do not block the TUI on `.dna`.

Suggested parser: `gb-io` for GenBank bytes, then map into `splicecraft_core::Record`
so wrap features stay under our invariants. Do not let a third-party type
become the app-wide record.

## Rust targets

- Detect FASTA / GenBank / GFF3
- Round-trip GenBank with features (including wrap) and topology
- Stamp exports: `Created by SpliceCraft.rs v{version}` COMMENT
- FASTA import/export (topology may be lost; default circular for plasmids
  only when a header hint or setting says so — match upstream)
- GFF3 export
- NCBI fetch behind a feature flag / function that tests never call unless
  `#[ignore]` + network
- SSRF: refuse link-local, RFC1918, loopback for **public** fetches; NCBI
  hosts allowlisted
- Accession sanitizer
- Size caps for bulk import (port upstream constants)

## Sacred invariants

[INV-08] [INV-09] on round-trip. Wrap features must survive GenBank write/read.

## Acceptance

- [x] Fixture round-trip: synthetic circular record with a wrap feature
- [x] LOCUS / illegal-character sanitise has a test
- [x] Export COMMENT contains `SpliceCraft.rs`
- [x] FASTA in/out
- [x] Network fetch is not invoked by default tests
- [x] `cargo test -p splicecraft-io`
- [x] No Python

## Forbidden

- Downloading the user's real NCBI traffic in CI
- Writing fetched records into the unsandboxed data dir
- Implementing Plasmidsaurus zip here (stage 11)

## Handoff

Stage 04 builds the Ratatui chrome (menus, panes, `?`, Ctrl+K) **without**
mutating records on disk. Loading a file into memory is OK if persist
sandbox rules hold.

Implementation notes:

- GenBank bytes go through `gb-io` 0.9, then map into
  `splicecraft_core::Record`. Qualifiers are `(Cow, Option<String>)` tuples.
- Wrap features write as `join(tail, head)` and read back as `end < start`.
- NCBI: `fetch_genbank` sanitises the accession and allowlists the host, then
  returns `NetworkDisabled` (no socket). Feature `ncbi` is reserved.
- GFF3 is export-only in this stage. `.dna` returns `IoError::DnaDeferred`.
