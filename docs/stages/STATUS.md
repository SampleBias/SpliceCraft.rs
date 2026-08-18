# Stage status

Update this ledger when a stage's acceptance list is fully green. Do not
check a box early.

- [x] 00 Bootstrap
- [x] 01 Core + sacred biology
- [x] 02 Persist + data safety
- [x] 03 File I/O
- [x] 04 Ratatui shell
- [x] 05 Map + sequence editor
- [x] 06 Library
- [x] 07 Enzymes + primers
- [x] 08 Cloning workbench
- [x] 09 Mutato + codon + synthesis
- [x] 10 Simulator + gels
- [x] 11 Sequencing
- [x] 12 Experiments + History UI
- [x] 13 Search
- [x] 14 Agent API + CLI
- [x] 15 Satellite features
- [x] 16 Parity gate

**Next:** 20 Mouse + remaining polish (see `docs/theme.md`).

UI polish track (post parity gate):

- [x] 17 Theme + colored chrome
- [x] 18 Layout density + overlay polish
- [x] 19 Interaction keys + menu activation
- [ ] 20 Mouse + remaining polish

Stage 19 notes: F10 focuses the 16-label menu bar; ←/→ highlight; Enter
opens the same overlay as the palette. File is a dropdown (Open / Fetch /
New / Keep / Save / Quit). Alt letters match upstream Help (no Alt+F).
Daily-driver keys: Ctrl+B BLAST, Ctrl+P primers, F6/Alt+H history,
Ctrl+S save through the persist chokepoint, Ctrl+F find both strands,
Ctrl+A select-all, Ctrl+C / Alt+C in-app clipboard (no OSC-52), Ctrl+N
paste-DNA new plasmid, `f` NCBI accession prompt (settings-gated;
`fetch_genbank` stays NetworkDisabled in the default build), Alt+Shift+F
add feature from selection. Library Space/mark/`c` and bulk export keys
are deferred to stage 20. Mouse hits on the bar are stage 20.

Stage 18 notes: Fixed side panes (`SIDE_PANE_COLS=32`) and sequence strip
(`SEQUENCE_ROWS=14`) on terminals ≥100×24. Yellow-on-black footer shortcuts
(toast replaces temporarily). Library header/search/button chrome. Feature
Type cells use darkened type-color backgrounds. Map braille is white with
bp ticks; sequence panel paints colored feature arrow bars + green AA.

Stage 17 notes: `docs/theme.md` is the visual contract. `splicecraft_tui::theme`
ports `_DEFAULT_TYPE_COLORS` + `_FEATURE_PALETTE` resolve order. Map/sequence
paint via `render_*_styled`; feature sidebar Type|Label uses resolved colors;
library selection is primary-on-dark; menu bar has a live highlight chip;
footer is the shortcut strip (no `stage NN` chrome). Layout percentages are
unchanged until stage 18.

Stage 16 notes: `docs/parity.md` walks every upstream `docs/features.md`
heading plus keybindings and agent-api groups. Intentional differences
(data-dir leaf, Ratatui, no GPL primer3, HMMER fallback, first-wave
agent registry) are listed first. Splice/cassette, sticky-cut tint,
NCBI fetch chrome, EMBL/FASTQ/GFF3 import, and BABS/OT-2 extras are
named gaps. Each [INV-01]…[INV-10] has an `invNN_*` test. README
installs with `cargo install --path crates/splicecraft`. Welcome chrome
is `SpliceCraft.rs`, not a Python wrapper.

Stage 15 notes: publication map SVG/PNG uses one geometry pass (size
300–6000, atomic write to a user-chosen path — not `safe_save_json`).
BABS talks to Ollama on loopback only; public URLs are refused before
any transport. OT-2 compiler emits Protocol API v2 text + JSON from a
fixture deck; motion always requires `confirm`. Migrate zip is
checksum-verified (`splicecraft-migrate.json` + `data/`). Master Delete
needs the module sentinel, write authorisation, and a triple TUI
confirm (default No, type DELETE, 3 s cooldown); the agent still has
no wipe endpoint. Splice-site scoring / cassette assembler is a named
gap in `docs/parity.md`.

Stage 14 notes: axum serves `127.0.0.1` only (default port 6701).
`/healthz` and `/tools` are unauthenticated and bare (no `data`
envelope). Other endpoints need `Authorization: Bearer` from
`$XDG_DATA_HOME/splicecraft-rs/agent_token` (never the Python leaf).
Writes use the dirty-guard (`force` in the JSON body only) and the
persist chokepoint; unauthorized process writes are 403. Master wipe
is not registered. `blast-online` / `hmmscan-online` return 403 when
`allow_online_search` is off. `splicecraft-cli call` hits the same
registry over HTTP.

Stage 13 notes: six-frame ORF finder reports wrap + `length_aa` (never
`(end - start)` on a full lap). Local BLASTN/BLASTP/HMMscan is the
ungapped seed-extend engine (HMMER is not in the default MIT build;
short queries use the same fallback). Online NCBI/EBI is mockable,
HTTPS + allowlist, and errors when `allow_online_search` is off.
Cancel stops the poll loop and deletes the BLAST RID. HMM-DB catalog
re-injects `pfam-a` / `ncbifam`; downloads are chokepoint-gated and
never fetch Pfam in default CI.

Stage 12 notes: lab notebook persists `experiments.json` /
`experiment_projects.json` through `safe_save_json`. Image attaches are
gated by the persist chokepoint. History warnings are read-only (claimed
EcoRI with no site leaves DNA untouched). `recover-history-from-dna`
matches by exact sequence identity and never thins an existing lineage.
Sticky-cut visualization remains deferred from stage 05.

Stage 11 notes: honest identity display (99.6% never becomes 100%). Zip
members with `../` are refused. Plasmidsaurus zip/API ingest tags
`plasmidsaurus:` and never overwrites library rows. Linear-map overlay
plus `j` jump-to-variant. Sticky-cut visualization remains deferred from
stage 05.

Stage 10 notes: Helling–Goodman–Boyer mobility with form factors
(supercoiled 0.7× / nicked 1.4×). Gels persist as `gels.json` through
`safe_save_json`. PCR is exact-match (plus 3′ partial for 5′ flaps),
capped at 50 amplicons. Sticky-cut visualization remains deferred from
stage 05.

Stage 09 notes: codon silent-repair of internal Type IIS now runs on
coding inserts when a codon table is supplied. Mutato Tm is Wallace only
(no primer3). Sticky-cut visualization remains deferred from stage 05.

Stage 08 notes: codon silent-repair of internal Type IIS sites landed in
stage 09. Sticky-cut visualization remains deferred from stage 05.

Stage 05 notes: sticky-cut visualization (upstream/downstream tint on a
clicked Type IIS site) is deferred. Restriction overlay draws labeled
resite ticks; unique / 6+ / collection filters landed in stage 07.
