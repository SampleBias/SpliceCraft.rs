# Stage 06 — Library

**Status:** done
**Depends on:** 02, 03, 05
**Primary crates:** `splicecraft-persist`, `splicecraft-tui`

## Goal

Named plasmid collections, keep-current-record, feature library, collision
dialogs, bulk import/export.

## Upstream (read before coding)

- Hub `LibraryPanel`
- `splicecraft_dataaccess.py` — collections, library, features JSON
- `tests/test_collections.py`, `tests/test_features_library.py`,
  `tests/test_collision_flows.py`, `tests/test_library_bulk_mark.py`
- Collision policy: skip / copy / overwrite — always ask (or agent
  equivalent later)

## Rust targets

- `collections.json` / library entries via `safe_save_json`
- Active collection; natural sort (`pBin2` before `pBin10`)
- `Alt+K` keep loaded record into the active collection
- Feature library CRUD (color, strand, snippet)
- Name collision modal (TUI) with skip / copy / overwrite
- Bulk import a folder of `.gb` / `.gbk` / `.fasta` (`.dna` skipped with
  a count until the codec exists)
- Bulk export collection (GenBank / FASTA; PNG/SVG later in stage 15)
- Path sanitisation: no `..`, Windows device names, case-insensitive
  collision on export

## Sacred invariants

[INV-07] on every library write. Unauthorized writes still fail.

## Acceptance

- [x] Sandboxed test: keep → reload process (or reload from disk) sees the entry
- [x] Collision copy does not drop the original
- [x] Natural sort test
- [x] Bulk import isolates per-file failures
- [x] `cargo test --workspace`

## Forbidden

- Overwriting on name collision without an explicit choice
- Touching the Python data dir
- Silent empty-library replace (shrink-refuse)

## Handoff

Stage 07 enzymes + primers. Do not start it in the same session that closed
this stage.

Implementation notes:

- `LibraryStore` owns `collections.json` + `library.json` (active list
  mirrors the current collection). Writes go through `safe_save_json`.
- `Alt+K` keep asks on name collision (`s` skip / `c` copy / `o` overwrite).
  Copy uses ` NAME COPY` / ` COPY 2` and never drops the original.
- Feature snippets (`features.json`) have color, strand (incl. 0), and
  sequence. `source` is refused. Same collision modal.
- Bulk import walks `.gb`/`.gbk`/`.fasta`; `.dna` is counted and skipped.
  Per-file failures do not abort the batch. Name clashes in a folder are
  skipped (not overwritten). Export sanitises stems and is
  case-insensitive (`pUC19` vs `puc19`).
- Ctrl+O is a path prompt (no GUI file picker). Tests attach
  `DataLayout` under a tempfile sandbox.
