# Upstream map

This repo does **not** vendor Python. Agents read the original over GitHub.

**Upstream:** [github.com/Binomica-Labs/SpliceCraft](https://github.com/Binomica-Labs/SpliceCraft)
**Default ref:** `master`
**Raw base:** `https://raw.githubusercontent.com/Binomica-Labs/SpliceCraft/master/`

Fetch example:

```bash
gh api repos/Binomica-Labs/SpliceCraft/contents/splicecraft_biology.py \
  --jq .download_url
```

Do not commit fetched files.

## Python file → Rust crate

| Upstream | Rust crate | Stage |
|---|---|---|
| `splicecraft.py` hub (`PlasmidApp`, panels, keybindings) | `splicecraft-tui` + `splicecraft` binary | 04, 05, 06 |
| `splicecraft_biology.py` | `splicecraft-bio` | 01 |
| `splicecraft_state.py` | app state inside `splicecraft-tui` / persist (no global Python mirror) | 02, 04 |
| `splicecraft_logging.py` | `splicecraft-util` (logging) | 02 |
| `splicecraft_util.py` | `splicecraft-util` | 01 |
| `splicecraft_persistence.py` | `splicecraft-persist` | 02 |
| `splicecraft_backup.py` | `splicecraft-persist` (backup / migrate / master-delete enum) | 02, 15 |
| `splicecraft_dataaccess.py` | `splicecraft-persist` domain accessors | 02, 06 |
| `splicecraft_record.py` | `splicecraft-core` + `splicecraft-io` | 01, 03 |
| `splicecraft_fileio.py` | `splicecraft-io` | 03, 11 |
| `splicecraft_net.py` | `splicecraft-io` (SSRF-hardened fetch) | 03, 13 |
| `splicecraft_render.py` | `splicecraft-tui` (braille canvas; pure fn) | 05 |
| `splicecraft_mapimage.py` | `splicecraft-tui` or a later `splicecraft-mapimage` crate | 15 |
| `splicecraft_codon.py` | `splicecraft-codon` | 09 |
| `splicecraft_primer.py` | `splicecraft-primer` | 07, 09 |
| `splicecraft_cloning.py` | `splicecraft-clone` | 08 |
| `splicecraft_seqanalysis.py` | `splicecraft-bio` / `splicecraft-clone` (ORFs, part classifier) | 01, 08, 13 |
| `splicecraft_gels.py` | `splicecraft-gels` | 10 |
| `splicecraft_experiments.py` | TUI + persist (notebook) | 12 |
| `splicecraft_history.py` | `splicecraft-clone` model + TUI History | 08, 12 |
| `splicecraft_search.py` | search module (stage 13); keep out of persist | 13 |
| `splicecraft_agent.py` | `splicecraft-agent` | 14 |
| `splicecraft_cli.py` | `splicecraft-cli` | 14 |
| `splicecraft_babs.py` | TUI BABS (Ollama localhost) | 15 |
| `splicecraft_opentrons.py` | AUTOLAB | 15 |
| `splicecraft_modals.py` / `splicecraft_widgets.py` | `splicecraft-tui` | 04+ |
| `splicecraft_errors.py` | `thiserror` types per crate | 01+ |
| `splicecraft_demo_plasmids.py` | fixtures under `crates/*/tests/data` (not Python) | 04 |
| `splicecraft_splice.py` / `_splice_model.py` / `_cassette.py` | satellite (stage 15) | 15 |
| `tests/test_dna_sanity.py` | `splicecraft-bio` unit + proptest | 01 |
| `docs/features.md` | parity checklist | 16 |
| `docs/keybindings.md` | TUI keymap | 04, 05 |
| `docs/agent-api.md` | agent surface | 14 |
| `docs/data-safety.md` | persist behavior | 02 |

## Function index (core biology)

Use these names when grepping upstream, then implement under the Rust names
in [`invariants.md`](invariants.md).

| Upstream | Rust (target) |
|---|---|
| `_rc` | `splicecraft_bio::rc` |
| `_iupac_pattern` | `splicecraft_bio::iupac_pattern` |
| `_feat_len` | `splicecraft_core::feat_len` |
| `_bp_in` | `splicecraft_core::bp_in` |
| wrap midpoint | `splicecraft_core::wrap_midpoint` |
| `_scan_restriction_sites` | `splicecraft_bio::scan_restriction_sites` |
| `_enzyme_cuts` | `splicecraft_bio::enzyme_cuts` |
| `_translate_cds` | `splicecraft_bio::translate_cds` |
| `_rebuild_record_with_edit` | `splicecraft_core::rebuild_record_with_edit` |
| `_safe_save_json` | `splicecraft_persist::safe_save_json` |

## Docs worth fetching per area

| Area | Upstream docs |
|---|---|
| Features | `docs/features.md` |
| Keybindings | `docs/keybindings.md` |
| Architecture / concurrency | `docs/architecture.md` |
| Subsystems (gels, experiments, Plasmidsaurus, HMM-DB) | `docs/subsystems.md` |
| Agent API | `docs/agent-api.md` |
| CLI | `docs/cli.md` |
| Data safety | `docs/data-safety.md` |
| Pitfalls | `docs/invariants.md` |
