# Stage 12 — Experiments + History UI

**Status:** not started
**Depends on:** 06, 08, 10
**Primary crates:** `splicecraft-tui`, `splicecraft-persist`, `splicecraft-clone`

## Goal

Markdown lab notebook with projects and live cross-refs; History viewer that
shows a protocol + lineage tree and **reports lies** instead of editing
sequences to make warnings disappear.

## Upstream (read before coding)

- `splicecraft_experiments.py` — normalize entries, `@plasmid` / `!action`
  / `&gel` extractors
- `splicecraft_history.py` — `_CommercialSaaSHistoryNode`, HistoryViewer
- `docs/subsystems.md` `[SUB-experiments]`
- `tests/test_experiments.py`, `tests/test_experiment_projects.py`,
  `tests/test_history.py`, `tests/test_origin_history.py`

## Rust targets

- Split-pane markdown editor; entries grouped into projects
- Cross-refs: type `@` / `!` / `&` and jump (`Ctrl+G` analog)
- Image attachments stored as blobs under the data dir via chokepoint;
  preview as half-blocks (no requirement for kitty graphics)
- Optional spellcheck (pure-Rust wordlist); `F7`
- History: numbered protocol (left → right like the bench) above a lineage
  tree
- Step detail: conditions, primers with pos/strand/Tm, end chemistry,
  enzymes regenerated
- If a claimed site is absent or a primer no longer binds, **show a
  warning**; do not mutate sequence or history to clear it
- `recover-history-from-dna` by sequence identity even if names differ

## Sacred invariants

[INV-07] for experiments JSON. History is append/report-only regarding
sequence truth.

## Acceptance

- [ ] Parse `@plasmid` / `!action` / `&gel` from a fixture body
- [ ] Jump table resolves a known plasmid id
- [ ] History warning test: node claims EcoRI, sequence has none → warn,
      sequence unchanged
- [ ] Attachment write uses persist chokepoint
- [ ] Workspace tests pass

## Forbidden

- Editing user DNA to dismiss a History warning
- Unsandboxed image writes

## Handoff

Stage 13 BLAST / HMM / ORF finder / online search.
