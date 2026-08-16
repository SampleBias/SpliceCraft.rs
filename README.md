# SpliceCraft.rs

Terminal-based plasmid map viewer, sequence editor, and cloning / mutagenesis
workbench. This is an independent **Rust + Ratatui** rewrite of
[Binomica Labs' SpliceCraft](https://github.com/Binomica-Labs/SpliceCraft)
(Python + Textual). Behavioral spec and sacred biology come from that project;
the code here is original Rust.

**Status:** stage 00 bootstrap. The TUI paints a welcome frame. Feature crates
are stubbed and layered. Later agents implement stages 01–16 until parity.
See [`docs/stages/README.md`](docs/stages/README.md).

## Quick start

```bash
cargo run -p splicecraft        # welcome TUI (q / Esc to quit)
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo run -p splicecraft-cli -- version
```

Requires Rust 1.88+ (Ratatui 0.30).

## What this is not

- Not a Python bind, PyO3 wrapper, or line-for-line transpile.
- Not a drop-in data-dir replacement. User data lives in
  `~/.local/share/splicecraft-rs/` (or `$XDG_DATA_HOME/splicecraft-rs/`).
  **Never** write the Python app's `~/.local/share/splicecraft/`.

## Crate map

| Crate | Role |
|---|---|
| `splicecraft-core` | Records, wrap-aware features, circular math |
| `splicecraft-util` | Helpers, time source, sanitizers |
| `splicecraft-bio` | IUPAC, reverse-complement, restriction scan, translate |
| `splicecraft-persist` | Atomic JSON, backups, write chokepoint |
| `splicecraft-io` | GenBank / FASTA / GFF / NCBI / later `.dna` |
| `splicecraft-codon` | Codon tables, optimize, forbidden-site scrub |
| `splicecraft-primer` | Primer design (MIT path; no GPL `primer3` crate) |
| `splicecraft-clone` | Traditional / Gibson / Golden Braid / MoClo |
| `splicecraft-gels` | Agarose mobility + gel render data |
| `splicecraft-tui` | Ratatui workbench |
| `splicecraft-agent` | Localhost JSON API |
| `splicecraft-cli` | `splicecraft-cli` sidecar |
| `splicecraft` | `splicecraft` binary |

## Documentation for agents

- [`AGENTS.md`](AGENTS.md) — how to pick and finish a stage
- [`CLAUDE.md`](CLAUDE.md) — sacred rules (data dir, biology, no Python)
- [`docs/invariants.md`](docs/invariants.md) — numbered invariants in Rust names
- [`docs/upstream.md`](docs/upstream.md) — Python file → crate map + permalinks
- [`docs/stages/`](docs/stages/) — the build contract

## License

MIT. Inspired by SpliceCraft (MIT) by Binomica Labs. This repository does not
vendor Python source.
