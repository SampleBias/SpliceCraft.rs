//! Built-in protein motifs and user overrides. Saves go through persist. [INV-07]

use serde::{Deserialize, Serialize};
use serde_json::Value;

use splicecraft_persist::{
    DataLayout, PersistError, load_protein_motifs, log_event, save_protein_motifs,
};

/// One protein motif (tag / linker / cleavage / 2A).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProteinMotif {
    /// Display name (`His6`).
    pub name: String,
    /// Feature type (`Tag`, `Linker`, …).
    pub feature_type: String,
    /// Amino-acid sequence (`*` allowed).
    pub sequence: String,
    /// Hex colour.
    pub color: String,
    /// One-line description.
    #[serde(default)]
    pub description: String,
}

/// Built-in motif catalog (upstream `_PROTEIN_MOTIFS`).
#[must_use]
pub fn builtin_motifs() -> Vec<ProteinMotif> {
    [
        (
            "His6",
            "Tag",
            "HHHHHH",
            "#1E40AF",
            "Hexahistidine affinity tag (Ni-NTA / IMAC purification).",
        ),
        (
            "His8",
            "Tag",
            "HHHHHHHH",
            "#3B82F6",
            "Octahistidine — tighter Ni-NTA binding than 6xHis.",
        ),
        (
            "His10",
            "Tag",
            "HHHHHHHHHH",
            "#60A5FA",
            "Decahistidine — even tighter, for harsh-wash purification.",
        ),
        (
            "FLAG",
            "Tag",
            "DYKDDDDK",
            "#0E7490",
            "FLAG tag (anti-FLAG M2 affinity purification).",
        ),
        (
            "3xFLAG",
            "Tag",
            "DYKDHDGDYKDHDIDYKDDDDK",
            "#06B6D4",
            "Triple FLAG — higher sensitivity for low-expression targets.",
        ),
        (
            "HA",
            "Tag",
            "YPYDVPDYA",
            "#67E8F9",
            "Influenza hemagglutinin epitope (anti-HA antibodies).",
        ),
        (
            "Myc",
            "Tag",
            "EQKLISEEDL",
            "#4338CA",
            "c-Myc epitope tag (9E10 antibody).",
        ),
        (
            "V5",
            "Tag",
            "GKPIPNPLLGLDST",
            "#6366F1",
            "V5 epitope tag (paramyxovirus).",
        ),
        (
            "Strep-II",
            "Tag",
            "WSHPQFEK",
            "#A78BFA",
            "Strep-Tactin affinity tag (mild biotin elution).",
        ),
        (
            "T7",
            "Tag",
            "MASMTGGQQMG",
            "#7C3AED",
            "T7 leader peptide (anti-T7 monoclonal).",
        ),
        (
            "NLS (SV40)",
            "Signal",
            "PKKKRKV",
            "#EAB308",
            "Classical SV40 large T-antigen nuclear localisation signal.",
        ),
        (
            "NLS (bipartite)",
            "Signal",
            "KRPAATKKAGQAKKKK",
            "#FBBF24",
            "Nucleoplasmin bipartite NLS.",
        ),
        (
            "NES",
            "Signal",
            "LPPLERLTL",
            "#F59E0B",
            "HIV-Rev nuclear export signal (CRM1-dependent).",
        ),
        (
            "GSG",
            "Linker",
            "GSG",
            "#94A3B8",
            "Minimal flexible linker.",
        ),
        ("GGS", "Linker", "GGS", "#64748B", "Short flexible linker."),
        (
            "(GGGGS)x3",
            "Linker",
            "GGGGSGGGGSGGGGS",
            "#475569",
            "Classic flexible linker for scFv / fusion proteins.",
        ),
        (
            "(GGGGS)x4",
            "Linker",
            "GGGGSGGGGSGGGGSGGGGS",
            "#57534E",
            "Longer flexible linker for domain separation.",
        ),
        (
            "EAAAK x3",
            "Linker",
            "EAAAKEAAAKEAAAK",
            "#78716C",
            "Rigid α-helical linker.",
        ),
        (
            "TEV",
            "Cleavage",
            "ENLYFQG",
            "#B91C1C",
            "TEV protease site (cuts between Q and G).",
        ),
        (
            "PreScission",
            "Cleavage",
            "LEVLFQGP",
            "#DC2626",
            "HRV 3C / PreScission protease site (cuts between Q and G).",
        ),
        (
            "Thrombin",
            "Cleavage",
            "LVPRGS",
            "#F87171",
            "Thrombin cleavage site.",
        ),
        (
            "Factor Xa",
            "Cleavage",
            "IEGR",
            "#EC4899",
            "Factor Xa protease site.",
        ),
        (
            "Furin",
            "Cleavage",
            "RRRR",
            "#DB2777",
            "Furin recognition site (R-X-K/R-R minimal).",
        ),
        (
            "P2A",
            "2A",
            "GSGATNFSLLKQAGDVEENPGP",
            "#15803D",
            "Porcine teschovirus 2A self-cleaving peptide.",
        ),
        (
            "T2A",
            "2A",
            "GSGEGRGSLLTCGDVEENPGP",
            "#16A34A",
            "Thosea asigna 2A peptide.",
        ),
        (
            "E2A",
            "2A",
            "GSGQCTNYALLKLAGDVESNPGP",
            "#22C55E",
            "Equine rhinitis A 2A peptide.",
        ),
        (
            "F2A",
            "2A",
            "GSGVKQTLNFDLLKLAGDVESNPGP",
            "#10B981",
            "Foot-and-mouth-disease virus 2A peptide.",
        ),
        (
            "Kozak start",
            "Motif",
            "M",
            "#C026D3",
            "Start codon (methionine) — required N-terminal.",
        ),
        ("Stop", "Motif", "*", "#A21CAF", "Translation stop codon."),
        (
            "FLAG+Stop",
            "Motif",
            "DYKDDDDK*",
            "#D946EF",
            "FLAG tag followed by stop — quick C-terminal tagging.",
        ),
    ]
    .into_iter()
    .map(
        |(name, feature_type, sequence, color, description)| ProteinMotif {
            name: name.into(),
            feature_type: feature_type.into(),
            sequence: sequence.into(),
            color: color.into(),
            description: description.into(),
        },
    )
    .collect()
}

/// Built-ins merged with user overrides (user wins on name).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MotifStore {
    /// User-only rows persisted to disk.
    pub user: Vec<ProteinMotif>,
}

impl MotifStore {
    /// Load user overrides.
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let loaded = load_protein_motifs(layout);
        let user = loaded
            .entries
            .iter()
            .filter_map(|v| serde_json::from_value::<ProteinMotif>(v.clone()).ok())
            .filter(|m| !m.name.is_empty())
            .collect();
        Self { user }
    }

    /// Persist user rows only.
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        let values: Vec<Value> = self
            .user
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();
        save_protein_motifs(layout, &values)?;
        log_event("codon.motifs.saved", &[("n", &self.user.len().to_string())]);
        Ok(())
    }

    /// Merged catalog: builtins with user name-overrides, then novel user motifs.
    #[must_use]
    pub fn merged(&self) -> Vec<ProteinMotif> {
        let mut out = builtin_motifs();
        let mut used = std::collections::HashSet::new();
        for u in &self.user {
            used.insert(u.name.clone());
            if let Some(slot) = out.iter_mut().find(|m| m.name == u.name) {
                *slot = u.clone();
            } else {
                out.push(u.clone());
            }
        }
        let _ = used;
        out
    }

    /// Upsert a user motif.
    pub fn upsert(&mut self, motif: ProteinMotif) {
        if motif.name.is_empty() || motif.sequence.is_empty() {
            return;
        }
        if let Some(slot) = self.user.iter_mut().find(|m| m.name == motif.name) {
            *slot = motif;
        } else {
            self.user.push(motif);
        }
    }
}
