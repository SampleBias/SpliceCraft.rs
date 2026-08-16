# Stage 13 — Search

**Status:** not started
**Depends on:** 01, 06, 03
**Primary crates:** `splicecraft-bio`, `splicecraft-io`, `splicecraft-tui`

## Goal

Local BLASTN/BLASTP/HMMscan against the library, online NCBI/EBI with a
cancel that actually stops, HMM-DB catalog, six-frame ORF finder.

## Upstream (read before coding)

- `splicecraft_search.py` — NCBI BLAST URL-API, EBI HMMER, HMM-DB download
- Hub pyhmmer in-process engine (`_blast_search`, `_hmmscan_run`)
- `splicecraft_seqanalysis.py` — `_find_orfs`
- `docs/subsystems.md` `[SUB-hmm-db]`
- `tests/test_blast.py`, `tests/test_online_blast.py`,
  `tests/test_hmm_db_catalog.py`

Local HMMER: prefer a Rust implementation or an optional bind. If HMMER
cannot ship MIT-clean in default builds, document a fallback ungapped
search (upstream already has a short-query fallback) and keep HMMscan
feature-gated. **Default CI must not require Pfam downloads.**

## Rust targets

- In-process DNA/protein search over the library (worker thread / tokio)
- Stale-record guard: ignore results if the canvas moved on
- Short-query fallback without HMMER
- ORF finder: six-frame, min AA length, ATG default, GTG/TTG opt-in,
  wrap-aware, **full lap** reporting (read length_aa, not start/end pair)
- Online tab: allowlist hosts, HTTPS, cancel via `CancellationToken`
- Agent/online search stays off until a setting is ticked
- HMM-DB catalog JSON; builtin pfam-a / ncbifam re-injected if missing;
  download pipeline chokepoint-guarded
- Fuzzy find plasmid across collections

## Sacred invariants

Network: SSRF fail-closed. Demo mode blocks egress. Sequences not logged.
ORF wrap [INV-08].

## Acceptance

- [ ] ORF wrap fixture lists wrap + exact AA length
- [ ] Full-lap ORF does not use a bogus start/end pair as length
- [ ] Online client tests use a mock; cancel stops polling
- [ ] Default tests do not hit the network
- [ ] Setting off → online search errors

## Forbidden

- Silent sequence upload
- Downloading Pfam in CI

## Handoff

Stage 14 agent HTTP API + CLI sidecar.
