//! Command-palette catalog and fuzzy filter (upstream `get_system_commands`).

use crate::action::Action;

/// One palette row. Handlers may be stubs until a later stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    /// Title shown in the list (and matched by the filter).
    pub title: &'static str,
    /// Extra haystack tokens (not shown).
    pub keywords: &'static str,
    /// What [`crate::AppState::reduce`] should do.
    pub action: Action,
}

/// Built-in palette. Open / Help / Quit stay first so tiny terminals still
/// show the stage-04 acceptance set.
#[must_use]
pub fn palette_commands() -> &'static [Command] {
    &[
        Command {
            title: "Open file",
            keywords: "open gb genbank fasta load",
            action: Action::OpenPathPrompt,
        },
        Command {
            title: "Keep current plasmid",
            keywords: "keep altk library collection",
            action: Action::KeepRecord,
        },
        Command {
            title: "Bulk import folder",
            keywords: "import folder gb fasta",
            action: Action::BulkImportPrompt,
        },
        Command {
            title: "Bulk export collection",
            keywords: "export folder genbank",
            action: Action::BulkExportPrompt,
        },
        Command {
            title: "Save selected feature",
            keywords: "feature library snippet",
            action: Action::SaveSelectedFeature,
        },
        Command {
            title: "Help",
            keywords: "keys shortcuts ?",
            action: Action::ToggleHelp,
        },
        Command {
            title: "Quit",
            keywords: "exit q",
            action: Action::Quit,
        },
        Command {
            title: "Load demo plasmid (basic)",
            keywords: "demo memory pdemo tiny",
            action: Action::LoadDemo,
        },
        Command {
            title: "Load demo plasmid (advanced)",
            keywords: "demo memory pdemoadv rich features",
            action: Action::LoadDemoAdvanced,
        },
        Command {
            title: "Undo",
            keywords: "undo revert",
            action: Action::Undo,
        },
        Command {
            title: "Redo",
            keywords: "redo",
            action: Action::Redo,
        },
        Command {
            title: "Flip sequence",
            keywords: "flip reverse complement rc",
            action: Action::FlipRecord,
        },
        Command {
            title: "Set origin here",
            keywords: "origin rotate recut circular",
            action: Action::SetOriginHere,
        },
        Command {
            title: "Fetch from NCBI",
            keywords: "fetch accession entrez",
            action: Action::OpenFetch,
        },
        Command {
            title: "New plasmid",
            keywords: "new paste sequence ctrl n",
            action: Action::OpenNewPlasmid,
        },
        Command {
            title: "Settings",
            keywords: "prefs online lookups",
            action: Action::OpenSettings,
        },
        Command {
            title: "Export plasmid map",
            keywords: "svg png publication figure mapimage",
            action: Action::ExportMapPrompt,
        },
        Command {
            title: "Export migrate archive",
            keywords: "backup zip migrate export",
            action: Action::ExportMigratePrompt,
        },
        Command {
            title: "Import migrate archive",
            keywords: "restore zip migrate import",
            action: Action::ImportMigratePrompt,
        },
        Command {
            title: "BABS",
            keywords: "ollama chat llm local",
            action: Action::OpenBabs,
        },
        Command {
            title: "AUTOLAB",
            keywords: "ot2 opentrons protocol deck",
            action: Action::OpenAutolab,
        },
        Command {
            title: "Master Delete (wipe all data)",
            keywords: "wipe factory reset destroy",
            action: Action::OpenMasterDelete,
        },
        Command {
            title: "Enzyme collections",
            keywords: "neb custom restriction collection",
            action: Action::OpenEnzymes,
        },
        Command {
            title: "BLAST",
            keywords: "blastn hmmscan orf search pfam",
            action: Action::OpenSearch,
        },
        Command {
            title: "Primer design",
            keywords: "primer tm cloning detection golden",
            action: Action::OpenPrimerDesign,
        },
        Command {
            title: "Primer check",
            keywords: "pcr oligo identity amplicon",
            action: Action::OpenPrimerCheck,
        },
        Command {
            title: "Constructor",
            keywords: "gibson moclo cloning traditional domesticator",
            action: Action::OpenConstructor,
        },
        Command {
            title: "Parts Bin",
            keywords: "parts grammar classify",
            action: Action::OpenParts,
        },
        Command {
            title: "Delete selected plasmid",
            keywords: "library delete remove",
            action: Action::LibraryDelete,
        },
        Command {
            title: "Undo last library delete",
            keywords: "library undelete restore",
            action: Action::LibraryUndelete,
        },
        Command {
            title: "Mutato — mutagenesis + Scrub",
            keywords: "mutato scrub soe quikchange",
            action: Action::OpenMutato,
        },
        Command {
            title: "Synthesis",
            keywords: "dna protein operon codon motif",
            action: Action::OpenSynthesis,
        },
        Command {
            title: "Simulator",
            keywords: "pcr gel agarose mobility",
            action: Action::OpenSimulator,
        },
        Command {
            title: "Sequencing",
            keywords: "plasmidsaurus zip align ab1 sanger verify identity",
            action: Action::OpenSequencing,
        },
        Command {
            title: "Experiments",
            keywords: "notebook lab notes markdown project gel",
            action: Action::OpenExperiments,
        },
        Command {
            title: "History",
            keywords: "lineage protocol construction tree warnings",
            action: Action::OpenHistory,
        },
        Command {
            title: "Recover history from .dna",
            keywords: "recover dna originals lineage sidecar",
            action: Action::RecoverHistory,
        },
    ]
}

/// Case-insensitive contains or subsequence match (upstream palette feel).
#[must_use]
pub fn fuzzy_match(query: &str, command: &Command) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return true;
    }
    let hay = format!("{} {}", command.title, command.keywords).to_ascii_lowercase();
    fuzzy_text_match(query, &hay)
}

/// Filter the catalog in declaration order.
#[must_use]
pub fn filter_commands(query: &str) -> Vec<Command> {
    palette_commands()
        .iter()
        .copied()
        .filter(|c| fuzzy_match(query, c))
        .collect()
}

/// Case-insensitive contains or subsequence (palette + plasmid find).
#[must_use]
pub fn fuzzy_text_match(query: &str, hay: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return true;
    }
    let h = hay.to_ascii_lowercase();
    h.contains(&q) || is_subsequence(&q, &h)
}

fn is_subsequence(query: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    for qc in query.chars() {
        loop {
            match it.next() {
                Some(hc) if hc == qc => break,
                Some(_) => {}
                None => return false,
            }
        }
    }
    true
}
