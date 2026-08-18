# Stage 16 — Parity gate

**Status:** done
**Depends on:** 01–15
**Primary crates:** all

## Goal

Prove SpliceCraft.rs matches the Python product's user-visible features,
sacred invariants, and “zero Python” rule. This stage is documentation +
audit + missing-test fill, not a new subsystem.

## Upstream (read before coding)

- [docs/features.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/features.md)
  — walk every heading
- [docs/keybindings.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/keybindings.md)
- [CLAUDE.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/CLAUDE.md)
  sacred ten
- [docs/agent-api.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/agent-api.md)
  endpoint list (register or explicitly defer with a reason)

## Rust targets

- Feature checklist markdown in `docs/parity.md` (create in this stage)
  with each upstream heading: done / gap / intentional difference
- Intentional differences must be listed (data-dir leaf, Ratatui vs Textual
  visuals, GPL primer3, HMMER fallback)
- Screenshot / README refresh
- `find . -name '*.py'` empty (exclude `target/`)
- Invariant audit: each [INV-01]…[INV-10] has a named test
- CI still `fmt` + `clippy -D warnings` + `test --workspace`

## Sacred invariants

All of [`docs/invariants.md`](../invariants.md).

## Acceptance

- [x] `docs/parity.md` exists and has no undocumented gaps
- [x] Every core invariant test still passes
- [x] No Python sources in git
- [x] README describes install via `cargo install --path crates/splicecraft`
      (and/or crates.io later)
- [x] Welcome/about mentions SpliceCraft.rs, not the Python package name
      as if this were a wrapper

## Forbidden

- Shipping a `python` module “for compatibility”
- Declaring parity while 01–03 tests are red
- Sharing the Python XDG leaf

## Handoff

UI polish continues in stages **17–20** ([`docs/theme.md`](../theme.md)).
Other post-1.0 work (perf budgets, extra grammars, remaining **gap** rows in
[`docs/parity.md`](../parity.md)) stays ordinary issues after stage 20.

Splice-site scoring + cassette assembler are listed in `docs/parity.md`.
