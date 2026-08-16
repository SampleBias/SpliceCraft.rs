# Stage 02 — Persist + data safety

**Status:** done
**Depends on:** 01
**Primary crates:** `splicecraft-persist`

## Goal

Make disk writes boring and impossible to aim at the wrong directory. The
user's library is the product.

## Upstream (read before coding)

- `splicecraft_persistence.py` — `_safe_save_json`, `_safe_load_json`,
  `_extract_entries`, chokepoint `_refuse_unauthorized_write`
- `splicecraft_backup.py` — snapshots, `.bak`, migrate archive, master-delete
  *enumeration* (wipe UI is stage 15)
- `splicecraft_dataaccess.py` — domain `_load_X` / `_save_X` shapes
- `docs/data-safety.md`
- `tests/test_data_safety.py`, `tests/test_universal_save.py`,
  `tests/test_migrate_data.py` (read for contracts; migrate zip can wait
  for stage 15)

## Rust targets

- Resolve data dir: `$XDG_DATA_HOME/splicecraft-rs` (leaf constant already
  exists). On Windows/macOS use the `directories` crate **with the same leaf**.
- `safe_save_json(path, value)`:
  - schema envelope `{"_schema_version": 1, "entries": [...]}` where applicable
  - tempfile in the same directory + `fsync` + atomic replace
  - rotate `.bak`
  - suspicious-shrink refuse (configurable threshold; default: refuse replacing
    a non-trivial file with empty/near-empty)
  - **return/propagate errors** — never swallow
- `safe_load_json` accepts envelope **and** legacy bare list
- Write chokepoint: saves fail unless `authorize_writes_for_sandbox` or the
  real app opted in
- Crash-recovery autosave slot (debounced `.gb` snapshot path; writer can
  stub until stage 05 calls it)
- Structured log events without sequence payloads
- Tests **must** set `XDG_DATA_HOME` to a tempdir and assert the resolved
  path is inside it. A test that can resolve the real user dir fails the stage.

## Sacred invariants

[INV-07] plus data-dir leaf ≠ `splicecraft`.

## Acceptance

- [x] Atomic replace: crash between write and replace leaves the previous file
      intact (simulate by writing tmp and not replacing)
- [x] `.bak` exists after a second save
- [x] Shrink-refuse does not overwrite a large fixture with `[]`
- [x] Unauthorized save returns an error
- [x] Sandbox test: resolved dir contains the temp prefix
- [x] Negative test: leaf is never `splicecraft`
- [x] `cargo test -p splicecraft-persist`
- [x] Default `cargo test --workspace` cannot touch `~/.local/share/splicecraft`
      or `~/.local/share/splicecraft-rs`

## Forbidden

- `std::fs::write` on library JSON outside the chokepoint
- Sharing the Python data dir “for compatibility”
- Logging sequence strings

## Handoff

Stage 03 reads/writes GenBank using `Record` from stage 01. Provenance comment
will use `splicecraft_util::version()`.
