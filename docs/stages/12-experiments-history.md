# Stage 12 — Experiments + History UI

**Status:** done
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

- [x] Parse `@plasmid` / `!action` / `&gel` from a fixture body
- [x] Jump table resolves a known plasmid id
- [x] History warning test: node claims EcoRI, sequence has none → warn,
      sequence unchanged
- [x] Attachment write uses persist chokepoint
- [x] Workspace tests pass

## Notes

- Notebook: `experiments.json` + `experiment_projects.json` through
  `safe_save_json`. Default project is `Main Project`. Attach dir is
  `$DATA/experiments/<id>/`; writes go through `refuse_unauthorized_write`
  then `atomic_write_bytes` (10 MB/image, 100 MB/entry).
- Cross-refs: `@` / `!` / `&` with the same lookbehind + `;`/`=` reject as
  gels. Legacy `@plasmid:` / `@actions:` migrate on save. Ctrl+G jumps the
  first plasmid id; F7 spellchecks (URLs, backticks, refs, DNA-like tokens
  masked).
- History warnings are strictly read-only. Regenerated-site `pos == 0` is
  an assembly marker and is skipped. Absent EcoRI (`GAATTC`) on
  `ATGCATGCATGC` warns and leaves the sequence untouched.
- `recover_history_from_dna` matches library `gb_text` to `.dna` originals
  by exact sequence identity, writes **only** `history_xml` when the
  sidecar has strictly more `<Node>` elements, and defaults to dry-run.
  Caps: 20_000 sidecars, 512 MB index.
- TUI: Experiments (list / compose / attach) and History (protocol / tree /
  detail). Palette also has Recover history from `.dna`.

## Forbidden

- Editing user DNA to dismiss a History warning
- Unsandboxed image writes

## Handoff

Stage 13 BLAST / HMM / ORF finder / online search.
