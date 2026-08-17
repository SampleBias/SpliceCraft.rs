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
            title: "Load demo plasmid",
            keywords: "demo memory pdemo",
            action: Action::LoadDemo,
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
            action: Action::Stub {
                name: "Fetch from NCBI",
                stage: 13,
            },
        },
        Command {
            title: "Settings",
            keywords: "prefs online",
            action: Action::Stub {
                name: "Settings",
                stage: 15,
            },
        },
        Command {
            title: "Enzyme collections",
            keywords: "neb custom restriction collection",
            action: Action::OpenEnzymes,
        },
        Command {
            title: "BLAST",
            keywords: "blastn hmmscan",
            action: Action::Stub {
                name: "BLAST",
                stage: 13,
            },
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
            keywords: "gibson moclo cloning",
            action: Action::Stub {
                name: "Constructor",
                stage: 8,
            },
        },
        Command {
            title: "Mutato — mutagenesis + Scrub",
            keywords: "mutato scrub",
            action: Action::Stub {
                name: "Mutato",
                stage: 9,
            },
        },
        Command {
            title: "Simulator",
            keywords: "pcr gel",
            action: Action::Stub {
                name: "Simulator",
                stage: 10,
            },
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
    hay.contains(&q) || is_subsequence(&q, &hay)
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
