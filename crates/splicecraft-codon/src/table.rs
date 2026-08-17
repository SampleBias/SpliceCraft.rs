//! Usage tables, K12 builtin, apportionment, and ranked search.

use std::collections::{BTreeMap, HashMap};

use splicecraft_bio::{CODON_TABLE, codon_aa_table};
use splicecraft_util::{NaturalToken, natural_sort_key};

use crate::error::CodonError;

/// Insertion-ordered `{codon: (aa, count)}` usage table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageTable {
    /// Rows in first-seen order (matches upstream dict insertion).
    pub rows: Vec<(String, char, i64)>,
}

impl UsageTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a codon.
    #[must_use]
    pub fn get(&self, codon: &str) -> Option<(char, i64)> {
        let key = codon.to_ascii_uppercase();
        self.rows
            .iter()
            .find(|(c, _, _)| *c == key)
            .map(|(_, aa, n)| (*aa, *n))
    }

    /// Insert or replace a codon, preserving first-seen order on replace.
    pub fn insert(&mut self, codon: impl Into<String>, aa: char, count: i64) {
        let codon = codon.into().to_ascii_uppercase().replace('U', "T");
        if let Some(row) = self.rows.iter_mut().find(|(c, _, _)| *c == codon) {
            row.1 = aa;
            row.2 = count;
            return;
        }
        self.rows.push((codon, aa, count));
    }

    /// Number of codon rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Iterate `(codon, aa, count)`.
    pub fn iter(&self) -> impl Iterator<Item = (&str, char, i64)> {
        self.rows.iter().map(|(c, aa, n)| (c.as_str(), *aa, *n))
    }
}

/// E. coli K12 usage (Kazusa 83333), insertion order matches upstream.
pub static K12_ROWS: &[(&str, char, i64)] = &[
    ("GGG", 'G', 44),
    ("GGA", 'G', 47),
    ("GGT", 'G', 109),
    ("GGC", 'G', 171),
    ("GAG", 'E', 94),
    ("GAA", 'E', 224),
    ("GAT", 'D', 194),
    ("GAC", 'D', 105),
    ("GTG", 'V', 135),
    ("GTA", 'V', 59),
    ("GTT", 'V', 86),
    ("GTC", 'V', 60),
    ("GCG", 'A', 197),
    ("GCA", 'A', 108),
    ("GCT", 'A', 55),
    ("GCC", 'A', 162),
    ("AGG", 'R', 8),
    ("AGA", 'R', 7),
    ("AGT", 'S', 37),
    ("AGC", 'S', 85),
    ("AAG", 'K', 62),
    ("AAA", 'K', 170),
    ("AAT", 'N', 112),
    ("AAC", 'N', 125),
    ("ATG", 'M', 127),
    ("ATA", 'I', 19),
    ("ATT", 'I', 156),
    ("ATC", 'I', 93),
    ("ACG", 'T', 59),
    ("ACA", 'T', 33),
    ("ACT", 'T', 41),
    ("ACC", 'T', 117),
    ("TGG", 'W', 55),
    ("TGT", 'C', 30),
    ("TGC", 'C', 41),
    ("TAT", 'Y', 86),
    ("TAC", 'Y', 75),
    ("TTG", 'L', 61),
    ("TTA", 'L', 78),
    ("TTT", 'F', 101),
    ("TTC", 'F', 77),
    ("TCG", 'S', 41),
    ("TCA", 'S', 40),
    ("TCT", 'S', 29),
    ("TCC", 'S', 28),
    ("CGG", 'R', 21),
    ("CGA", 'R', 22),
    ("CGT", 'R', 108),
    ("CGC", 'R', 133),
    ("CAG", 'Q', 142),
    ("CAA", 'Q', 62),
    ("CAT", 'H', 81),
    ("CAC", 'H', 67),
    ("CTG", 'L', 240),
    ("CTA", 'L', 27),
    ("CTT", 'L', 61),
    ("CTC", 'L', 54),
    ("CCG", 'P', 137),
    ("CCA", 'P', 34),
    ("CCT", 'P', 43),
    ("CCC", 'P', 33),
    ("TAA", '*', 9),
    ("TAG", '*', 0),
    ("TGA", '*', 5),
];

/// Built-in E. coli K12 table.
#[must_use]
pub fn builtin_k12() -> UsageTable {
    let mut t = UsageTable::new();
    for (c, aa, n) in K12_ROWS {
        t.rows.push(((*c).to_owned(), *aa, *n));
    }
    t
}

/// Default restriction sites the optimizer scrubs.
#[must_use]
pub fn default_forbidden() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("BsaI".into(), "GGTCTC".into()),
        ("BsmBI".into(), "CGTCTC".into()),
        ("BbsI".into(), "GAAGAC".into()),
        ("EcoRI".into(), "GAATTC".into()),
        ("NdeI".into(), "CATATG".into()),
        ("XhoI".into(), "CTCGAG".into()),
        ("BamHI".into(), "GGATCC".into()),
        ("HindIII".into(), "AAGCTT".into()),
        ("NcoI".into(), "CCATGG".into()),
        ("SalI".into(), "GTCGAC".into()),
        ("KpnI".into(), "GGTACC".into()),
        ("SacI".into(), "GAGCTC".into()),
    ])
}

/// Host-expression hazards that are not restriction sites.
pub fn hazard_motifs(hosts: &[&str]) -> Result<BTreeMap<String, String>, CodonError> {
    let mut out = BTreeMap::new();
    let mut seen = std::collections::HashSet::new();
    for host in hosts {
        let key = host.trim().to_ascii_lowercase();
        let group: &[(&str, &str)] = match key.as_str() {
            "bacterial" => &[("Shine-Dalgarno", "AGGAGG")],
            "plant" => &[
                ("plant polyA signal", "AATAAA"),
                ("cryptic splice donor", "GTRAG"),
            ],
            "mammalian" => &[
                ("polyA signal", "AATAAA"),
                ("polyA signal variant", "ATTAAA"),
                ("cryptic splice donor", "GTRAG"),
            ],
            _ => return Err(CodonError::UnknownHost(host.to_string())),
        };
        for (name, motif) in group {
            if seen.insert(*motif) {
                out.insert((*name).to_owned(), (*motif).to_owned());
            }
        }
    }
    Ok(out)
}

/// `(aa → [(codon, frac) desc], codon → frac)`. AA labels come from the genetic code.
pub type AaCodonMap = HashMap<char, Vec<(String, f64)>>;
/// `codon → synonymous fraction`.
pub type CodonFracMap = HashMap<String, f64>;

/// `(aa → [(codon, frac) desc], codon → frac)`. AA labels come from the genetic code.
#[must_use]
pub fn build_aa_map(raw: &UsageTable, table_id: i32) -> (AaCodonMap, CodonFracMap) {
    let mut aa_total: HashMap<char, i64> = HashMap::new();
    let mut codon_aa: Vec<(String, char)> = Vec::new();
    for (codon, _aa, count) in &raw.rows {
        let canon = codon_aa_table(codon, table_id);
        if !CODON_TABLE.iter().any(|(c, _)| *c == codon.as_str()) {
            continue;
        }
        codon_aa.push((codon.clone(), canon));
        *aa_total.entry(canon).or_insert(0) += count;
    }
    let mut codon_frac = HashMap::new();
    for (codon, _aa, count) in &raw.rows {
        let Some((_, canon)) = codon_aa.iter().find(|(c, _)| c == codon) else {
            continue;
        };
        let total = aa_total.get(canon).copied().unwrap_or(1).max(1);
        codon_frac.insert(codon.clone(), *count as f64 / total as f64);
    }
    let mut aa_codons: HashMap<char, Vec<(String, f64)>> = HashMap::new();
    for (codon, canon) in &codon_aa {
        if *canon == '*' {
            continue;
        }
        let frac = codon_frac.get(codon).copied().unwrap_or(0.0);
        aa_codons
            .entry(*canon)
            .or_default()
            .push((codon.clone(), frac));
    }
    for list in aa_codons.values_mut() {
        list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    }
    (aa_codons, codon_frac)
}

/// Largest-remainder apportionment, then interleave so no codon clusters first.
#[must_use]
pub fn allocate(codons: &[(String, f64)], n: usize) -> Vec<String> {
    if n == 0 || codons.is_empty() {
        return Vec::new();
    }
    if codons.len() == 1 {
        return vec![codons[0].0.clone(); n];
    }
    let mut targets = Vec::with_capacity(codons.len());
    let mut remainders = Vec::with_capacity(codons.len());
    let mut allocated = 0usize;
    for (i, (_, frac)) in codons.iter().enumerate() {
        let exact = n as f64 * frac;
        let floored = exact as usize;
        targets.push(floored);
        remainders.push((exact - floored as f64, i));
        allocated += floored;
    }
    let shortage = n.saturating_sub(allocated) as i64;
    remainders.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for k in 0..shortage.max(0) as usize {
        let idx = remainders[k % remainders.len()].1;
        targets[idx] += 1;
    }
    let mut queues: Vec<Vec<String>> = codons
        .iter()
        .zip(targets.iter())
        .filter(|(_, cnt)| **cnt > 0)
        .map(|((codon, _), cnt)| vec![codon.clone(); *cnt])
        .collect();
    let mut interleaved = Vec::with_capacity(n);
    let mut i = 0usize;
    while queues.iter().any(|q| !q.is_empty()) {
        let idx = i % queues.len();
        if !queues[idx].is_empty() {
            interleaved.push(queues[idx].remove(0));
        }
        i += 1;
    }
    if interleaved.len() < n {
        interleaved.extend(std::iter::repeat_n(
            codons[0].0.clone(),
            n - interleaved.len(),
        ));
    }
    interleaved.truncate(n);
    interleaved
}

/// Split a display name into (genus, species) lowercased tokens.
#[must_use]
pub fn name_parts(name: &str) -> (String, String) {
    let parts: Vec<_> = name.split_whitespace().collect();
    let genus = parts
        .first()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let species = parts
        .get(1)
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    (genus, species)
}

/// One registry row for search / persist.
#[derive(Clone, Debug, PartialEq)]
pub struct TableEntry {
    /// Display name.
    pub name: String,
    /// NCBI taxid (string; empty if unknown).
    pub taxid: String,
    /// `builtin` / `kazusa` / `user` / `genome`.
    pub source: String,
    /// ISO date, optional.
    pub added: String,
    /// Usage counts.
    pub raw: UsageTable,
}

/// Ranked search. Empty query → natural-sort by name.
#[must_use]
pub fn search_tables(query: &str, entries: &[TableEntry]) -> Vec<TableEntry> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        let mut out = entries.to_vec();
        out.sort_by_key(|a| natural_sort_key(&a.name));
        return out;
    }
    let mut ranked: Vec<(i32, Vec<NaturalToken>, TableEntry)> = Vec::new();
    for e in entries {
        let name_lc = e.name.to_ascii_lowercase();
        let taxid_lc = e.taxid.to_ascii_lowercase();
        let (genus, species) = name_parts(&e.name);
        let rank = if !taxid_lc.is_empty() && taxid_lc == q {
            0
        } else if !taxid_lc.is_empty() && taxid_lc.starts_with(&q) {
            1
        } else if !genus.is_empty() && genus.starts_with(&q) {
            2
        } else if !species.is_empty() && species.starts_with(&q) {
            3
        } else if name_lc.contains(&q) {
            4
        } else {
            continue;
        };
        ranked.push((rank, natural_sort_key(&name_lc), e.clone()));
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    ranked.into_iter().map(|t| t.2).collect()
}
