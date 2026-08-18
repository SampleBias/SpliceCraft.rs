//! Top menu bar catalog. Labels match upstream `MenuBar.MENUS`.
//!
//! Click-to-open is stage 20. Keyboard: F10 focuses the bar, Left/Right
//! highlight, Enter opens (File is the dropdown exception).

use crate::action::Action;

/// Menu labels left → right (upstream `MenuBar.MENUS`).
pub const MENUS: &[&str] = &[
    "File",
    "Settings",
    "BLAST",
    "Enzymes",
    "Features",
    "Primers",
    "Mutato",
    "Synthesis",
    "Parts",
    "Constructor",
    "Simulator",
    "Sequencing",
    "Experiments",
    "History",
    "AUTOLAB",
    "BABS",
];

/// File dropdown rows (Open / Fetch / Quit are the stage-19 minimum).
pub const FILE_ITEMS: &[(&str, Action)] = &[
    ("Open file", Action::OpenPathPrompt),
    ("Fetch from NCBI", Action::OpenFetch),
    ("New plasmid", Action::OpenNewPlasmid),
    ("Keep in library", Action::KeepRecord),
    ("Save", Action::SaveRecord),
    ("Quit", Action::Quit),
];

/// Direct-open action for a non-File menu (same as the palette / overlay).
#[must_use]
pub fn menu_action(name: &str) -> Option<Action> {
    Some(match name {
        "Settings" => Action::OpenSettings,
        "BLAST" => Action::OpenSearch,
        "Enzymes" => Action::OpenEnzymes,
        "Features" => Action::SaveSelectedFeature,
        "Primers" => Action::OpenPrimerDesign,
        "Mutato" => Action::OpenMutato,
        "Synthesis" => Action::OpenSynthesis,
        "Parts" => Action::OpenParts,
        "Constructor" => Action::OpenConstructor,
        "Simulator" => Action::OpenSimulator,
        "Sequencing" => Action::OpenSequencing,
        "Experiments" => Action::OpenExperiments,
        "History" => Action::OpenHistory,
        "AUTOLAB" => Action::OpenAutolab,
        "BABS" => Action::OpenBabs,
        _ => return None,
    })
}

/// Upstream Help `Alt`+letter → menu index (File has no Alt letter).
#[must_use]
pub fn alt_menu_action(ch: char) -> Option<Action> {
    let name = match ch.to_ascii_lowercase() {
        's' => "Settings",
        'n' => "Enzymes",
        'p' => "Primers",
        'y' => "Synthesis",
        'r' => "Parts",
        'i' => "Simulator",
        'q' => "Sequencing",
        'x' => "Experiments",
        'h' => "History",
        'u' => "AUTOLAB",
        _ => return None,
    };
    menu_action(name)
}
