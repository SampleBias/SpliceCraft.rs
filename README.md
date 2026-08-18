# SpliceCraft.rs

Terminal-based plasmid map viewer, sequence editor, and cloning / mutagenesis
workbench. This is an independent **Rust + Ratatui** rewrite of
[Binomica Labs' SpliceCraft](https://github.com/Binomica-Labs/SpliceCraft)
(Python + Textual). Behavioral spec and sacred biology come from that project;
the code here is original Rust.

**Status:** stages 00–16 done. Feature checklist: [`docs/parity.md`](docs/parity.md).
Splice/cassette scoring and a few Textual-only extras remain tracked gaps —
they are listed there, not silently omitted.

## Install

From a checkout:

```bash
cargo install --path crates/splicecraft
splicecraft
```

Or run without installing:

```bash
cargo run -p splicecraft        # splash → workbench (? help, Ctrl+K, q / Esc quit)
splicecraft --no-splash         # skip the DNA entry screen
```

Requires Rust 1.88+ (Ratatui 0.30). crates.io publication is later; the
install path above is the supported one.

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo run -p splicecraft-cli -- version
cargo run -p splicecraft -- --headless --agent-port 6701   # localhost API only
cargo run -p splicecraft-cli -- call list-library
```

## Screenshot

[`docs/screenshot.txt`](docs/screenshot.txt) is an 80×18 TestBackend capture
of the workbench after **Load demo plasmid** (title `SpliceCraft.rs`,
theme chrome (stage 17+). Braille maps look right in a real terminal; regenerate
with `SPLICECRAFT_WRITE_SCREENSHOT=1 cargo test -p splicecraft-tui --lib workbench_about`.

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
| `splicecraft-io` | GenBank / FASTA / GFF / NCBI / sequencing / `.dna` |
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
- [`docs/parity.md`](docs/parity.md) — upstream feature checklist (done / gap / intentional)
- [`docs/upstream.md`](docs/upstream.md) — Python file → crate map + permalinks
- [`docs/stages/`](docs/stages/) — the build contract

## License

MIT. Inspired by SpliceCraft (MIT) by Binomica Labs. This repository does not
vendor Python source.
