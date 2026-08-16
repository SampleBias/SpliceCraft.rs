# Stage 15 — Satellite features

**Status:** not started
**Depends on:** 05, 06, 14 (API hooks where upstream has them)
**Primary crates:** `splicecraft-tui`, `splicecraft-persist`, `splicecraft-io`

## Goal

Everything still missing for feature parity that was deferred: publication
map export, BABS, AUTOLAB/OT-2, migrate zip, Master Delete, settings editors,
splice/cassette.

This stage may be split into PRs internally, but STATUS stays one checkbox
until each subsection's acceptance is green.

## Upstream (read before coding)

- `splicecraft_mapimage.py` — SVG + PNG geometry (`[INV-158]`)
- `splicecraft_babs.py` — Ollama localhost, `[INV-139]`
- `splicecraft_opentrons.py` — OT-2
- `splicecraft_backup.py` — migrate archive, master delete targets
- `splicecraft_splice.py`, `splicecraft_splice_model.py`,
  `splicecraft_cassette.py`
- Settings / grammar / enzyme / codon editors in the hub
- Matching `tests/test_mapimage.py`, `tests/test_babs.py`,
  `tests/test_opentrons.py`, `tests/test_master_delete.py`,
  `tests/test_migrate_data.py`

## Rust targets

### Map image

- One geometry pass → SVG (stdlib/string) and PNG (`image` crate)
- Size 300–6000 px, transparent background option, labels/sites toggles
- Bulk export marked plasmids

### BABS

- Local Ollama only (loopback). No cloud API keys.
- Streaming markdown, context lifebar, slash commands, transcript export
- Agent mode: same endpoints as stage 14; **ask before every write**
- Physical robot motion always confirms
- Corpus/paper scraper optional; online lookups off until a setting is ticked
- Never send plasmid sequence to non-loopback hosts from BABS

### AUTOLAB

- Find OT-2s, deck grid, step designer, compile Opentrons protocol
- Simulate/analyze; run behind arm + health checks
- Scriptable via agent; motion gated

### Data life-cycle

- Migrate `.zip` checksum-verified export/import
- Master Delete: triple-gated, uses backup target enumeration, resets
  in-memory state
- Pre-update snapshot hooks if you add a self-update path (optional)

### Settings + splice

- Settings JSON via chokepoint
- Splice-site scoring + cassette assembler if you can port tests;
  otherwise keep a tracked gap in stage 16 until done

## Sacred invariants

[INV-07]. Demo/web refuse on export paths that upstream gates. BABS
loopback exception must not weaken SSRF for public fetches.

## Acceptance

- [ ] SVG export contains plasmid name and is well-formed XML
- [ ] PNG write is atomic to a **user-chosen path** (not the data dir
      chokepoint)
- [ ] Ollama client tests mock `127.0.0.1`; refuse a public URL
- [ ] Master Delete test only runs in a sandbox and leaves the real home
      dir untouched
- [ ] Migrate zip round-trip in tempdirs
- [ ] OT-2 compiler produces JSON/protocol text from a fixture deck
      (no live robot in CI)

## Forbidden

- Cloud LLM providers
- Ungated robot motion
- Master Delete without triple confirm in the TUI

## Handoff

Stage 16 parity gate vs upstream `docs/features.md`.
