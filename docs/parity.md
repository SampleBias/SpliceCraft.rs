# Feature parity

Audit of [upstream `docs/features.md`](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/features.md)
(fetched from `master` for stage 16) against this tree. Status is one of:

| Status | Meaning |
|---|---|
| **done** | User-visible behaviour is here and tested |
| **gap** | Missing or only a stub; listed so it is not a silent omission |
| **intentional** | Different on purpose (safety, license, or toolkit) |

Upstream walked: `docs/features.md` headings, `docs/keybindings.md`,
`docs/agent-api.md` endpoint groups, and the sacred ten in upstream
`CLAUDE.md` (Rust names in [`invariants.md`](invariants.md)).

This rewrite is **not** a Python wrapper. The binary and welcome chrome
say **SpliceCraft.rs**.

## Intentional differences

These must stay different even if a later issue closes a **gap**.

| Topic | Python | SpliceCraft.rs |
|---|---|---|
| Data-dir leaf | `~/.local/share/splicecraft/` | `~/.local/share/splicecraft-rs/` (`splicecraft_persist::XDG_DATA_DIR_LEAF`). Sharing the Python leaf is forbidden. |
| UI toolkit | Textual | Ratatui. Braille maps and key chords are ported; mouse-first Textual chrome is not pixel-identical. |
| Primer Tm / design | Primer3 (and the crates.io `primer3` crate is GPL-2.0) | MIT designers + Wallace Tm in `splicecraft-primer`. Circular templates stay on this stack. |
| Local BLAST / HMM | `pyhmmer` / HMMER 3 in-process | Ungapped seed-extend in `splicecraft-bio`. HMMER is not a default MIT dependency. Short queries use the same fallback upstream uses below the HMMER minimum. |
| Language | Python + Biopython | Rust only. `find . -name '*.py'` (excluding `target/`) is empty. |
| Agent surface | ~220–234 endpoints | First-wave registry for implemented stages (~30 names). Remainder is deferred below, not silently claimed. |
| Master Delete confirm | Typed `YES` | Typed `DELETE` plus default-No + 3 s cooldown. No agent wipe endpoint (same as upstream). |
| Map PNG | Super-sample + LANCZOS | `image` crate raster of the same geometry pass. SVG is the publication-quality vector path. |
| Quit | `Ctrl+Q` | `q` / Esc / `Ctrl+Q` (terminal-native). |

## Sacred invariants

Every core tag has a named `invNN_*` test. Table: [`invariants.md`](invariants.md)
(Named tests). `cargo test --workspace` is the gate.

| Tag | Status |
|---|---|
| [INV-01] palindromic enzymes forward-only | **done** |
| [INV-02] reverse-strand resite forward coords | **done** |
| [INV-03] full-IUPAC reverse complement | **done** |
| [INV-04] IUPAC pattern cache | **done** |
| [INV-05] circular wrap midpoint | **done** |
| [INV-06] circular wrap restriction scan | **done** |
| [INV-07] JSON saves back up through one chokepoint | **done** |
| [INV-08] wrap-aware feature length | **done** |
| [INV-09] wrap-feature integrity on edit | **done** |
| [INV-10] undo snapshots are deep clones | **done** |

## Features (`docs/features.md`)

### View

| Heading | Status | Notes |
|---|---|---|
| Startup splash (DNA helix) | **done** | Greyscale braille B-DNA (not the upstream rainbow); cosmic/big/small `SpliceCraft.rs` figlet; any key continues. Skip with `--no-splash` or `SPLICECRAFT_NO_SPLASH=1`. |
| Braille circular / linear maps | **done** | `render_map` / `render_map_styled`; feature labels colored (stage 17). `v` toggles linear. ASCII density ramp is the documented fallback. |
| Export map as PNG / SVG | **done** | One geometry pass; size 300–6000; atomic write to a **user-chosen** path (not `safe_save_json`). PNG anti-aliasing is **intentional** (no LANCZOS super-sample). |
| Per-base sequence panel | **done** | Two-strand, wrap-aware feature lane, CDS AA (green) at codon midpoint; bases tinted by enclosing feature (stage 17). |
| Per-strand sticky-cut tint | **gap** | Deferred since stage 05 (upstream/downstream blue/red on click). Restriction overlay ticks and unique / 6+ filters are **done**. |
| 200+ NEB enzymes + Type IIS | **done** | Stage 07 catalog; `r` / `u` / `6`; collection cycle `[` / `]`. |

### Edit

| Heading | Status | Notes |
|---|---|---|
| In-place edits + undo / redo | **done** | 50-deep `UndoStack`; [INV-10]. |
| Feature CRUD (add / merge / split / delete / rename / recolor) | **gap** | Keep selected feature → feature library is **done**. Full Insert/Edit Feature dialog is not ported. Sequence delete and library-row delete are **done**. |
| Clipboard top / bottom strand | **gap** | No `Ctrl+C` / `Alt+C` selection copy yet. |
| Crash-recovery autosave | **done** | 3 s debounce through the persist chokepoint. |
| Flip record / set origin | **done** | `Alt+Shift+R` / `Alt+Shift+O`; linear re-origin refused. Display rotate is Left/Right (not `[` / `]` — those cycle enzyme collections). |

### Synthesis

| Heading | Status | Notes |
|---|---|---|
| DNA synthesis composer | **done** | Synthesis overlay. |
| Protein composer + codon tables | **done** | Built-in + persisted tables; Type IIS scrub on coding inserts. |
| Codon-table picker (Kazusa / genome / TSV / chart) | **gap** | Store + active table are **done**; Kazusa fetch, genome-from-FASTA builder, and usage-grid chart are not in the TUI. |

### Cloning

| Heading | Status | Notes |
|---|---|---|
| Cloning grammars (GB L0, MoClo Plant) | **done** | Built-ins + `cloning_grammars.json`. |
| Domesticator | **done** | Constructor tab; silent Type IIS repair when a codon table is supplied. |
| Parts Bin + classify-from-plasmid | **done** | |
| Constructor (Traditional / Gibson / Golden Gate) | **done** | Homology-arm design, linearize-at, session save. |
| Construction-history XML on products | **done** | History overlay + `.dna` recover. |

### Primer design

| Heading | Status | Notes |
|---|---|---|
| Detection / cloning / Golden Braid / generic | **done** | MIT path. Primer3 is **intentional** (not a default dep). |
| Primer library lifecycle | **done** | Designed → Ordered → Validated. |
| Primer check | **done** | |

### Mutagenesis

| Heading | Status | Notes |
|---|---|---|
| SOE-PCR site-directed mutagenesis | **done** | Mutato tab. |
| Scrub (QuikChange / Golden Braid) | **done** | Substitution-only; GB self-check via digest+ligate. |

### Simulate

| Heading | Status | Notes |
|---|---|---|
| In-silico PCR | **done** | Exact-match + 3′ partial for 5′ flaps; cap 50. |
| Agarose gel renderer | **done** | Helling–Goodman–Boyer; supercoiled 0.7× / nicked 1.4×. |

### Search

| Heading | Status | Notes |
|---|---|---|
| In-process BLASTN / BLASTP / HMMscan | **done** | Ungapped engine. HMMER is **intentional**. |
| Six-frame ORF indexing | **done** | Wrap + `length_aa`; never `end - start` on a full lap. |
| Online BLAST / HMM | **done** | HTTPS + allowlist; off until `allow_online_search`. Cancel deletes the BLAST RID. |
| Cross-collection plasmid find | **done** | Search overlay Find tab. |
| Pairwise sequencing alignment | **done** | Honest identity (99.6% never becomes 100%). |
| Plasmidsaurus zip / API | **done** | `plasmidsaurus:` tag; no overwrite. |
| New L0 part from synthetic fragment | **done** | Constructor + `l0_part_from_syn_fragment`. |
| Sanger `.ab1` | **done** | |
| New Plasmid modal (`Ctrl+N`) | **done** | Paste IUPAC DNA into a prompt → memory record. Annotate with `Alt+Shift+F` from a selection. |
| NCBI accession fetch (`f`) | **done** | TUI/palette call `splicecraft_io::fetch_genbank`. Offline / lookups-off → clear toast. Default build still `NetworkDisabled` (fail-closed). |
| ORF finder UI | **done** | Search overlay. |

### Library

| Heading | Status | Notes |
|---|---|---|
| Collections + keep (`Alt+K`) | **done** | Collision policy COPY / ALT / overwrite. |
| Bulk import folder | **done** | `.gb` / `.gbk` / `.dna` / FASTA. |
| `.dna` round-trip | **done** | Sequence, features, history XML. |
| Construction history viewer | **done** | `F6` analog via palette / History overlay. |
| Library fuzzy search | **done** | Natural sort. |
| Feature library | **done** | |
| Bulk export collection | **done** | GenBank folder export. Map-image bulk format is a **gap** in the folder exporter (library `export_plasmid_maps` exists). |
| Export marked plasmids as images | **gap** | Geometry + `export_plasmid_maps` are **done**; Space-mark + `p` is not. |
| Staging marks Ⓜ / Ⓒ / Ⓧ | **gap** | Session delete + `u` undelete is **done**; the four-state mark cycle is not. |

### File formats

| Format | Status | Notes |
|---|---|---|
| GenBank | **done** | Import + export; wrap features survive. |
| EMBL | **gap** | Detected and refused (`IoError::Rejected`). |
| Commercial `.dna` | **done** | |
| FASTA | **done** | |
| Sanger `.ab1` | **done** | Import only. |
| FASTQ | **gap** | Not ingested. |
| GFF3 | **gap** (import) / **done** (export) | `record_to_gff3` writes wrap-aware rows; `load_path` refuses standalone GFF3. |
| Plasmidsaurus zip | **done** | Traversal members refused. |
| Circular map PNG / SVG | **done** | |

Reads use size-cap + symlink refusal. JSON library writes use
`safe_save_json`. Map / migrate / raw bytes use `atomic_write_bytes` to a
user path.

### Experiments lab notebook

| Heading | Status | Notes |
|---|---|---|
| Projects + entries | **done** | `experiments.json` / `experiment_projects.json`. |
| Compose + image attach | **done** | Chokepoint-gated; no Win/Mac clipboard grab (Pillow `ImageGrab` is Python-only — **intentional**). |
| Cross-refs `@` / `!` / `&` | **done** | Ctrl+G jump. |
| Spellcheck (F7) | **done** | Masked word list in persist; not `pyspellchecker`. |

### Gels

| Heading | Status | Notes |
|---|---|---|
| Save gel snapshots | **done** | `gels.json` through the chokepoint. |
| Gel library browser | **done** | Simulator overlay. |

### Protein motifs

| Heading | Status | Notes |
|---|---|---|
| Curated catalog | **done** | |
| User overrides | **done** | `protein_motifs.json`. |

### Recovery + data safety

| Heading | Status | Notes |
|---|---|---|
| Atomic JSON + `.bak` + timestamped rotation + shrink spill | **done** | [INV-07]. Daily `snapshots/` tier + Restore-from-backup modal are a **gap**. |
| Master Delete | **done** | Sentinel + write authorisation + triple TUI confirm. Sandbox tests leave the real home dir untouched. |
| Pre-update snapshots | **gap** | No pip/pipx/uv self-update path. |
| Migrate zip | **done** | Checksummed `splicecraft-migrate.json` + `data/`. |

### BABS assistant

| Heading | Status | Notes |
|---|---|---|
| Local Ollama chat | **done** | Loopback only; public URLs refused before transport. Sequences are never sent off-loopback. |
| Slash commands | **done** | `/help` `/clear` `/export` `/model`. Upstream extras (`/agent`, `/learn`, …) are a **gap**. |
| Agent mode / autonomy | **gap** | |
| Corpus / learn / index-library / memory | **gap** | Would need the separate Babs repo; not vendored. |
| Online lookups (FPbase, UniProt, …) | **gap** | Setting `allow_online_lookups` exists and is human-armed; lookup clients are not ported. |
| Model manager / paper scraper | **gap** | |
| Cloud LLMs | **intentional** | Forbidden. |

### AUTOLAB (OT-2)

| Heading | Status | Notes |
|---|---|---|
| Compile Protocol API v2 from a deck | **done** | Fixture deck → JSON + protocol text. No live robot in CI. |
| Find robots / deck editor / labware / library bind | **gap** | Overlay compiles the fixture only. |
| Analyze / run / lights / home | **gap** | Motion always requires `confirm`; this build never actuates hardware. |
| Protocol + custom-labware CRUD | **gap** | Persist paths exist; no TUI/agent CRUD yet. |

### Drive it from outside the GUI

| Heading | Status | Notes |
|---|---|---|
| Localhost agent API | **done** | `127.0.0.1` only; bearer token under the Rust data dir. |
| `splicecraft-cli call` | **done** | Same registry. |
| Full ~230-endpoint inventory | **gap** | See [Agent API](#agent-api). |

### Splice / cassette (stage 15 known gap)

| Heading | Status | Notes |
|---|---|---|
| Splice-site scoring (`splicecraft_splice.py` / `_splice_model.py`) | **gap** | Not ported. Do not treat as silent. |
| Cassette assembler (`splicecraft_cassette.py`) | **gap** | Not ported. |

## Keybindings

Upstream table: [docs/keybindings.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/keybindings.md).
Live help is `?`. Palette is `Ctrl+K`.

| Upstream | Status | SpliceCraft.rs |
|---|---|---|
| `[` / `]` rotate map origin | **intentional** | Left / Right rotate the view; `[` / `]` cycle enzyme collections. |
| `↑` reset origin | **done** | Home. |
| `,` / `.` map aspect | **gap** | |
| `v` / `l` / `r` | **done** | |
| `f` NCBI fetch | **done** | Accession prompt; settings-gated; `fetch_genbank` (default build: `NetworkDisabled`). |
| `Ctrl+O` / `o` open | **done** | |
| `Ctrl+N` new plasmid | **done** | Paste DNA → memory. |
| `Ctrl+B` BLAST | **done** | Search overlay. Also F10 → BLAST → Enter. |
| `Ctrl+K` palette | **done** | |
| `Alt+K` keep | **done** | |
| `Ctrl+A` / `Ctrl+S` / `Ctrl+F` | **done** | Select-all / save (keep via chokepoint) / find both strands. `Ctrl+E` (end-of-line) is still a **gap**. In-place IUPAC insert on the sequence pane is **done**. |
| `Alt+Shift+F` / `Alt+Shift+C` feature add / capture | **done** | Add = label [type] from selection; capture = Save selected feature (palette). |
| `Ctrl+P` primers | **done** | Also F10 → Primers → Enter. |
| `Enter` / `Delete` / undo / redo | **done** | |
| `Ctrl+C` / `Alt+C` copy | **done** | In-app clipboard (top / bottom RC). Toast documents no OSC-52. |
| `F1`–`F5` | **done** | |
| `F6` history | **done** | Also `Alt+H`. |
| `Alt+D` UI snapshot / hover debug | **gap** | |
| `?` help / `Ctrl+Q` quit | **done** | Also `q` / Esc. F10 menu bar documented in Help. |
| Library Space / `c` / `m` / `y` / `p` / `s` / `h` | **gap** | Delete + `u` undelete **done**. Deferred to stage 20. |
| Mouse click / drag / wheel | **gap** | Keyboard workbench only (stage 20). |
| 16 top-bar menus | **done** | F10 + ←/→ + Enter. File is a dropdown. Alt letters match upstream Help. |

## Agent API

Upstream: [docs/agent-api.md](https://github.com/Binomica-Labs/SpliceCraft/blob/master/docs/agent-api.md).
`/tools` is authoritative for **this** binary.

### Registered (first wave)

`healthz`, `tools`, `status`, `list-library`, `list-collections`,
`search-library`, `load-entry`, `features`, `list-features`, `get-sequence`,
`find-orfs`, `list-restriction-sites`, `save`, `add-current-to-library`,
`delete-from-library`, `get-settings`, `set-setting`, `list-hmm-databases`,
`blast`, `blast-online`, `hmmscan-online`, `hmmer-web`, `load-file`,
`export-genbank`, `export-migrate-archive`, `import-migrate-archive`,
`list-experiments`, `list-primers`, `list-gels`, `simulate-pcr`.

Security that **is** ported: loopback bind, Host-header check, bearer token
on writes, dirty-guard (`force` in the JSON body), persist chokepoint,
path sanitiser, `set-setting` cannot arm `allow_online_search` /
`allow_online_lookups`, sequences omitted from `status` / list payloads.

### Intentionally never registered

Whole-data wipe: `master-delete`, `wipe`, and the rest of
`splicecraft_agent::FORBIDDEN_ENDPOINT_NAMES`. Same reason as upstream
`test_no_agent_endpoint_exposes_wipe`.

### Deferred (reason)

Not registered yet. File ordinary issues; this is not a new bootstrap stage.

| Group | Why deferred |
|---|---|
| Record CRUD (`new-plasmid`, `add-features`, `undo` / `redo` over HTTP, `apply-gff3`, `transfer-annotations`, `discard-changes`) | TUI feature dialogs / GFF apply are themselves gaps or local-only. |
| Extra file exporters (`export-map-image`, EMBL, FASTA, GFF3, `.dna`, `bulk-export-collection`) | Libraries exist; HTTP wrappers were not in the stage-14 first wave. |
| Library mutations (`rename-plasmid`, `copy-plasmid(s)`, `move-plasmid(s)`, collection CRUD, statuses) | Keep / delete / search cover the TUI; bulk mark-cycle is a TUI gap. |
| Parts / grammars / entry-vector / domesticate / assemble-into-entry-vector | Clone crate is **done**; agent handlers were not duplicated. |
| Design (`gibson-assemble`, `traditional-clone`, `golden-gate-assemble`, `design-primers`, `scrub-plasmid`, `optimize-protein`, `lint-synthesis`, …) | Same: library **done**, HTTP **gap**. |
| Simulate gel / digest / diff / multi-align / verify-against-reads | TUI Sequencing / Simulator cover the interactive path. |
| History `get-history` / `recover-history-from-dna` | TUI History + Recover palette command are **done**. |
| Codon-table / enzyme / feature-library / primer-collection CRUD | Persist stores **done**; list-only where registered. |
| Online reference lookups, BABS learn/corpus/memory | BABS extras are a product gap; egress stays human-gated. |
| RNA fold / RBS / assemble-operon | No Rust port (would be a new subsystem). |
| HMM-DB download / set-active | Catalog list is registered; download stays TUI/chokepoint and off in default CI. |
| Plasmidsaurus list/download | TUI Sequencing is **done**. |
| Experiments / gels write endpoints | List is registered; compose stays in the TUI. |
| Backups restore UI / pre-update snapshots | Persist rotation **done**; restore modal **gap**. |
| OT-2 live (`ot2-run`, `ot2-analyze`, lights, home, …) | Compiler **done**; ungated motion is forbidden. Compile is TUI-only today. |
| `restart` / `shutdown` | Headless process control not ported. |

## Keybindings vs menus

The 16 menu labels in `menu.rs` (`File` … `BABS`) match upstream
`MenuBar.MENUS`. Keyboard activation is **done** (F10 / Alt letters / File
dropdown). Remaining File-adjacent gaps are library mark-cycle export
(stage 20) and live NCBI HTTP (default build stays `NetworkDisabled`).

## Post-1.0

UI chrome / color landed in stage **17** ([`docs/theme.md`](theme.md)). Layout
(**18**) and keys (**19**) are done. Mouse (**20**) remains open in
[`docs/stages/STATUS.md`](stages/STATUS.md). Other gaps remain ordinary issues
(no stage 21+ unless explicitly opened).
Do not add a Python compatibility module. Do not share the Python XDG leaf.
