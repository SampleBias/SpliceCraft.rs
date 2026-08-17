# Staged build

Each file in this directory is a **contract** for one agent session.

## Current stage

**14 — Agent API + CLI** is next.

Stages 00–13 are done.

Ledger: [`STATUS.md`](STATUS.md).

## Rules

- Implement **exactly one** open stage.
- Read [`AGENTS.md`](../../AGENTS.md) and [`CLAUDE.md`](../../CLAUDE.md) first.
- Fetch upstream Python with `gh`; do not vendor it.
- Do not mark a stage done until its acceptance list is green.

## Index

| Id | File | Crates |
|---|---|---|
| 00 | [00-bootstrap.md](00-bootstrap.md) | workspace + `splicecraft-tui` welcome |
| 01 | [01-core-biology.md](01-core-biology.md) | `core`, `util`, `bio` |
| 02 | [02-persist.md](02-persist.md) | `persist` |
| 03 | [03-file-io.md](03-file-io.md) | `io` |
| 04 | [04-ratatui-shell.md](04-ratatui-shell.md) | `splicecraft-tui` shell |
| 05 | [05-map-sequence.md](05-map-sequence.md) | map + sequence editor |
| 06 | [06-library.md](06-library.md) | collections + feature library |
| 07 | [07-enzymes-primers.md](07-enzymes-primers.md) | enzymes + `primer` |
| 08 | [08-cloning.md](08-cloning.md) | `clone` + Constructor |
| 09 | [09-mutato-codon-synthesis.md](09-mutato-codon-synthesis.md) | `codon` + Mutato + Synthesis |
| 10 | [10-simulator-gels.md](10-simulator-gels.md) | `gels` + PCR sim |
| 11 | [11-sequencing.md](11-sequencing.md) | Plasmidsaurus, align, AB1 |
| 12 | [12-experiments-history.md](12-experiments-history.md) | notebook + History UI |
| 13 | [13-search.md](13-search.md) | BLAST / HMM / ORFs |
| 14 | [14-agent-api-cli.md](14-agent-api-cli.md) | `agent` + `cli` |
| 15 | [15-satellites.md](15-satellites.md) | map export, BABS, OT-2, migrate |
| 16 | [16-parity-gate.md](16-parity-gate.md) | feature checklist |

## Stage file template

Every stage file has: Goal, Upstream, Rust targets, Invariants, Acceptance,
Forbidden, Handoff.
