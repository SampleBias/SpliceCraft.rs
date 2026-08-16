# Stage 00 — Bootstrap

**Status:** done
**Depends on:** nothing
**Primary crates:** workspace, `splicecraft`, `splicecraft-tui`

## Goal

Public GitHub repo, Cargo workspace with layered stub crates, a Ratatui
welcome frame that paints `SpliceCraft.rs`, agent stage docs, CI.

## Upstream

None required. Product spec:
[Binomica-Labs/SpliceCraft README](https://github.com/Binomica-Labs/SpliceCraft/blob/master/README.md).

## Rust targets

- Workspace members listed in the root `Cargo.toml`
- `splicecraft_persist::XDG_DATA_DIR_LEAF == "splicecraft-rs"`
- `splicecraft_tui::draw_welcome` testable on `TestBackend`
- Binaries: `splicecraft`, `splicecraft-cli`

## Sacred invariants

Data-dir leaf must not be `splicecraft`. Test lives in `splicecraft-persist`.

## Acceptance

- [x] `cargo test --workspace` passes
- [x] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [x] Welcome frame contains `SpliceCraft.rs`
- [x] No `.py` files in the tree
- [x] Stage docs 00–16, `STATUS.md`, `upstream.md`, `invariants.md`,
      `AGENTS.md`, `CLAUDE.md` exist
- [x] Public repo `SampleBias/SpliceCraft.rs`

## Forbidden

- Vendoring upstream Python
- Writing `~/.local/share/splicecraft/`
- Taking GPL `primer3` as a default dependency
- Implementing feature biology in this stage

## Handoff

Stage 01 fills `splicecraft-core`, `splicecraft-util`, and `splicecraft-bio`.
