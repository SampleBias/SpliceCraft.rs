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
| `splicecraft_search.py` | `splicecraft-io` online + HMM-DB download; `splicecraft-bio` local BLAST | 13 |
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
| `_gb_text_to_record` | `splicecraft_io::gb_text_to_record` |
| `_record_to_gb_text` | `splicecraft_io::record_to_gb_text` |
| `_sanitize_accession` | `splicecraft_io::sanitize_accession` |
| `_record_to_gff3` | `splicecraft_io::record_to_gff3` |
| `PlasmidApp` keys / `MenuBar` | `splicecraft_tui::{Action, AppState, KEY_TABLE}` |
| `get_system_commands` | `splicecraft_tui::palette_commands` |
| `_reverse_complement_record` | `splicecraft_bio::reverse_complement_record` |
| `_rotate_seq_record` | `splicecraft_core::rotate_record` |
| `_push_undo` / undo stack | `splicecraft_tui::UndoStack` |
| `_BrailleCanvas` / map geometry | `splicecraft_tui::render_map` |
| `_load_library` / `_save_library` | `splicecraft_persist::{load,save}_library` + `LibraryStore` |
| `_classify_collisions` / `_ensure_unique_copy_name` | `splicecraft_persist::{classify_entry,unique_copy_name}` |
| `_bulk_import_folder` | `splicecraft_io::bulk_import_folder` |
| `_primer_tm` | `splicecraft_primer::primer_tm` |
| `_pick_binding_region` | `splicecraft_primer::pick_binding_region` |
| `_design_generic_primers` | `splicecraft_primer::design_generic_primers` |
| `_design_cloning_primers` | `splicecraft_primer::design_cloning_primers` |
| `_design_detection_primers` | `splicecraft_primer::design_detection_primers` |
| `_primer_binding_sites` | `splicecraft_primer::primer_binding_sites` |
| `_rederive_primer_binding` | `splicecraft_primer::rederive_primer_binding` |
| `_primer_check_confidence` | `splicecraft_primer::primer_check_confidence` |
| `_export_primers_to_csv` | `splicecraft_primer::export_primers_csv` |
| `_NEB_ENZYMES` / `_all_enzymes` | `splicecraft_bio::{neb_enzymes,all_enzymes}` |
| `_load/_save_enzyme_collections` | `splicecraft_persist::EnzymeStore` |
| `_ends_compatible` / `_ligate_fragments` / `_close_circular` | `splicecraft_clone::{ends_compatible,ligate_fragments,close_circular}` |
| `_make_synthetic_fragment` / `_rc_fragment` | `splicecraft_clone::{make_synthetic_fragment,rc_fragment}` |
| `_simulate_traditional_cloning` | `splicecraft_clone::simulate_traditional_cloning` |
| `_gibson_overlap_len` / `_simulate_gibson_assembly` | `splicecraft_clone::{gibson_overlap_len,simulate_gibson_assembly}` |
| `_design_homology_arms` | `splicecraft_clone::design_homology_arms` |
| `_design_gb_primers` | `splicecraft_clone::design_gb_primers` |
| `_BUILTIN_GRAMMARS` | `splicecraft_clone::{gb_l0,moclo_plant,GrammarStore}` |
| `_l0_part_from_syn_fragment` | `splicecraft_clone::l0_part_from_syn_fragment` |
| `_classify_part_from_plasmid` | `splicecraft_clone::classify_part_from_plasmid` |
| `_simulate_golden_gate` | `splicecraft_clone::simulate_golden_gate` |
| `_simulate_pcr` | `splicecraft_gels::simulate_pcr` |
| `_agarose_mobility` / `_gel_bands_for_lane` | `splicecraft_gels::{agarose_mobility,gel_bands_for_lane}` |
| `_render_gel_image` | `splicecraft_gels::render_gel_image` |
| `_load_gels` / `_save_gels` | `splicecraft_persist::{load_gels,save_gels}` / `GelStore` |
| `_extract_gel_refs` | `splicecraft_gels::extract_gel_refs` |
| `_format_identity_pct` | `splicecraft_util::format_identity_pct` |
| `_normalize_dna_for_align` | `splicecraft_bio::normalize_dna_for_align` |
| `_pairwise_align` | `splicecraft_io::pairwise_align` |
| `_alignment_to_target_segments` / bar | `splicecraft_io::{alignment_to_target_segments,render_alignment_bar}` |
| zip member safety / extract | `splicecraft_io::{is_safe_zip_member_name,extract_gbk_member}` |
| `_plasmidsaurus_*` zip / API | `splicecraft_io::{parse_plasmidsaurus_zip,plasmidsaurus_list_items,…}` |
| AB1 / Phred | `splicecraft_io::load_ab1` |
| `.dna` TLV / history XML | `splicecraft_io::{load_dna_path,write_dna_bytes,extract_history_xml}` |
| `_extract_plasmid_refs` / `_extract_action_refs` / `_extract_gel_refs` | `splicecraft_persist::{extract_plasmid_refs,extract_action_refs,extract_experiment_gel_refs}` |
| `_save_experiment_image` | `splicecraft_persist::save_experiment_image` |
| `_history_node_warnings` | `splicecraft_clone::history_node_warnings` |
| `_h_recover_history_from_dna` | `splicecraft_io::recover_history_from_dna` |
| `_find_orfs` | `splicecraft_bio::find_orfs` |
| `_blast_search` / `_blast_search_pure` | `splicecraft_bio::blast_search` |
| `_hmmscan_run` (ungapped fallback) | `splicecraft_bio::hmmscan_ungapped` |
| `_ncbi_blast_online` | `splicecraft_io::ncbi_blast_online` |
| `_hmmer_web_hmmscan` | `splicecraft_io::hmmer_web_hmmscan` |
| `_hmm_db_perform_download` | `splicecraft_io::hmm_db_perform_download` |
| `_load/_save_hmm_db_catalog` | `splicecraft_persist::{load,save}_hmm_catalog` |
| `_get_setting` / `allow_online_search` | `splicecraft_persist::{allow_online_search,set_setting_bool}` |
| `_agent_endpoint` | `splicecraft_agent::Registry` / `builtin` |
| `_agent_dirty_guard` | `splicecraft_agent::dirty_guard` |
| `_check_agent_write_path` | `splicecraft_agent::check_write_path` |
| `_check_agent_read_path_ancestors` | `splicecraft_agent::check_read_path` |
| `cmd_call` | `splicecraft_cli::execute_call` |

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
