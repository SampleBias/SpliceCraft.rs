# CLAUDE.md — AI agent context for SpliceCraft.rs

Read before touching biology, persistence, rendering, or any save path.

## Sacred: data-dir safety (read first)

User plasmids, collections, primers, and parts for **this** app live in:

```
$XDG_DATA_HOME/splicecraft-rs/     # default ~/.local/share/splicecraft-rs/
```

The constant is `splicecraft_persist::XDG_DATA_DIR_LEAF` (`"splicecraft-rs"`).

The Python SpliceCraft app uses `~/.local/share/splicecraft/`. **That directory
is off-limits.** A shared path would let a Rust bug destroy years of lab work.

Three hard rules:

1. **Sandbox before any persist call in tests or probes.** Set `XDG_DATA_HOME`
   to a temp dir *before* the process resolves the data path. Assert the
   resolved path contains the sandbox prefix.
2. **Domain `_save_*` helpers are nuclear.** Stage 02 introduces a write
   chokepoint. Calling save from outside sanctioned callers (the TUI main,
   the agent server, tests that opted in) must error.
3. **Never write the Python leaf `splicecraft`.** A test in
   `splicecraft-persist` already forbids renaming the leaf to collide.

## What this project is

Rust + Ratatui rewrite of SpliceCraft (Python + Textual + Biopython). Hub +
layered-siblings in the original become a Cargo workspace here. Sacred
biology and data-safety behavior are ported; Python source is not vendored.

**Repo:** [github.com/SampleBias/SpliceCraft.rs](https://github.com/SampleBias/SpliceCraft.rs)

## How to run

```bash
cargo run -p splicecraft
cargo test --workspace
cargo test -p splicecraft-bio          # fast inner loop after stage 01
```

Logs will live under the Rust data dir once stage 02 lands. Sequence content
is **never** logged.

## Sacred invariants (do not break)

Full text with Rust names: [`docs/invariants.md`](docs/invariants.md).

1. Palindromic enzymes scanned forward only.
2. Reverse-strand resite positions use forward coordinates.
3. Reverse-complement handles full IUPAC.
4. IUPAC regex patterns are cached.
5. Circular wrap midpoint uses modular arc length.
6. Circular wrap restriction scan uses `seq + seq[:max_site_len-1]` and
   emits two resite pieces + one recut.
7. Data-file saves always back up (atomic temp + fsync + replace + `.bak`).
8. Wrap-aware feature length.
9. Wrap-feature integrity across record edits.
10. Undo snapshots are deep clones.

Touching scan / `rc` / IUPAC / translate / `bp_in` / `feat_len` / wrap
midpoint / `rebuild_record_with_edit` must trip tests immediately.

## For future agents

1. [`AGENTS.md`](AGENTS.md) first, then the open stage file.
2. Grep `docs/invariants.md` + `docs/upstream.md` for the area you touch.
3. Eyeball real plasmids after stage 03: pUC19 (`L09137`) and pACYC184
   (`MW463917.1`) via NCBI fetch — only with network tests explicitly
   marked, never in default CI.
4. Dispatching a sub-agent? Quote the stage id and invariant numbers in
   its prompt.
