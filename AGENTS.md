# AGENTS.md — SpliceCraft.rs

Handoff for coding agents. Read this, then [`CLAUDE.md`](CLAUDE.md), then the
**single open stage** in [`docs/stages/`](docs/stages/).

## Mission

Rewrite [Binomica-Labs/SpliceCraft](https://github.com/Binomica-Labs/SpliceCraft)
in Rust + Ratatui with **feature parity** and **zero Python** in this tree.
This is a behavioral reimplementation, not a transpile.

## How to work

1. Open [`docs/stages/STATUS.md`](docs/stages/STATUS.md). The first stage that
   is not `done` is the only stage you implement. If every stage is done,
   do not invent a stage 17 — remaining work is ordinary issues in
   [`docs/parity.md`](docs/parity.md).
2. Read that stage file end to end. Grep [`docs/invariants.md`](docs/invariants.md)
   and [`docs/upstream.md`](docs/upstream.md) for the tags it names.
3. Fetch upstream Python **over the network** (`gh api` / raw GitHub). Do not
   copy `.py` files into this repo.
4. Implement, add tests, run:

   ```bash
   cargo test --workspace
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   ```

   Plus any crate-specific command in the stage file (e.g.
   `cargo test -p splicecraft-bio`).
5. Mark the stage `done` in `STATUS.md` only when every acceptance item is
   green. Do not start the next stage in the same session unless the user
   asked for more than one stage.

## Hard rules

- **One open stage.** Do not skip. Do not “just also land” stage N+1 chrome.
- **Zero Python.** No `.py`, no `requirements.txt`, no PyO3, no
  `include_str!` of Python. Consult upstream via `gh`.
- **Biology before chrome.** Stages 01–03 must stay green. TUI edits that
  write the library are forbidden until persist (stage 02) is done.
- **Saves are nuclear.** One chokepoint: `splicecraft_persist::safe_save_json`
  (name may match the stage-02 API). Tests sandbox `XDG_DATA_HOME` or they
  do not call persist.
- **Data dir is `splicecraft-rs`.** Never `splicecraft` (Python app).
- **Sequences never go in logs.**
- **Online stays off** until a setting is ticked. Demo mode fail-closes egress.
- **Do not take the crates.io `primer3` crate** as a default dependency (GPL-2.0).
  Port the original’s own Tm / designer math first.
- **Do not edit** `docs/stages/00-bootstrap.md` to rewrite history; append
  notes in later stages if the workspace layout must change.

## Crate layers (no upward imports)

```
splicecraft-util, splicecraft-core          L0
splicecraft-bio                             L0/L1
splicecraft-persist                         L1
splicecraft-io                              L1/L2
splicecraft-codon, splicecraft-primer       L2
splicecraft-clone, splicecraft-gels         L3
splicecraft-tui, splicecraft-agent, cli     apps
```

`tests/import-layers` analog: if you add a crate, wire it in the workspace
`Cargo.toml` and keep the DAG. Cycles fail the stage.

## Upstream

Mapping: [`docs/upstream.md`](docs/upstream.md).

```bash
gh api repos/Binomica-Labs/SpliceCraft/contents/splicecraft_biology.py \
  --jq .download_url
```

Pin permalinks to `master` unless a stage file names a commit.

## Definition of done (every stage)

- Acceptance tests in the stage file pass.
- `cargo test --workspace` passes.
- `clippy -D warnings` is clean.
- No `.py` files (`find . -name '*.py'` empty, ignoring `target/`).
- STATUS.md updated.
