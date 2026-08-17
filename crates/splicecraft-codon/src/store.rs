//! Persistent codon-table registry. Writes go through `safe_save_json`. [INV-07]

use serde_json::{Value, json};

use splicecraft_bio::codon_aa;
use splicecraft_persist::{
    DataLayout, PersistError, load_codon_tables, log_event, save_codon_tables,
};

use crate::table::{TableEntry, UsageTable, builtin_k12};

/// In-memory codon-table library. First load seeds E. coli K12 (taxid 83333).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodonTableStore {
    /// Registry rows.
    pub entries: Vec<TableEntry>,
}

impl CodonTableStore {
    /// In-memory registry seeded with E. coli K12 (no disk).
    #[must_use]
    pub fn with_builtin_k12() -> Self {
        Self {
            entries: vec![TableEntry {
                name: "E. coli K12".into(),
                taxid: "83333".into(),
                source: "builtin".into(),
                added: String::new(),
                raw: builtin_k12(),
            }],
        }
    }

    /// Load from a layout; seed K12 when the taxid is missing.
    #[must_use]
    pub fn load(layout: &DataLayout) -> Self {
        let loaded = load_codon_tables(layout);
        let mut entries = Vec::new();
        for v in &loaded.entries {
            if let Some(e) = decode_entry(v) {
                entries.push(e);
            }
        }
        if !entries.iter().any(|e| e.taxid == "83333") {
            entries.insert(
                0,
                TableEntry {
                    name: "E. coli K12".into(),
                    taxid: "83333".into(),
                    source: "builtin".into(),
                    added: String::new(),
                    raw: builtin_k12(),
                },
            );
        }
        Self { entries }
    }

    /// Persist through the chokepoint.
    pub fn persist(&self, layout: &DataLayout) -> Result<(), PersistError> {
        let values: Vec<Value> = self.entries.iter().map(encode_entry).collect();
        save_codon_tables(layout, &values)?;
        log_event(
            "codon.tables.saved",
            &[("n", &self.entries.len().to_string())],
        );
        Ok(())
    }

    /// Look up by taxid (preferred) or case-insensitive name.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&TableEntry> {
        let k = key.trim();
        self.entries.iter().find(|e| e.taxid == k).or_else(|| {
            let kl = k.to_ascii_lowercase();
            self.entries
                .iter()
                .find(|e| e.name.to_ascii_lowercase() == kl)
        })
    }

    /// Insert or replace by taxid (when non-empty) else name.
    pub fn add(&mut self, name: &str, taxid: &str, raw: UsageTable, source: &str) -> TableEntry {
        let taxid = taxid.trim().to_owned();
        let name = {
            let n = name.trim();
            if n.is_empty() { "?" } else { n }
        }
        .to_owned();
        let entry = TableEntry {
            name: name.clone(),
            taxid: taxid.clone(),
            source: source.to_owned(),
            added: String::new(),
            raw,
        };
        self.entries.retain(|e| {
            if !taxid.is_empty() {
                e.taxid != taxid
            } else {
                e.name != name
            }
        });
        self.entries.push(entry.clone());
        entry
    }
}

fn decode_entry(v: &Value) -> Option<TableEntry> {
    let obj = v.as_object()?;
    let raw_blob = obj.get("raw")?.as_object()?;
    let mut raw = UsageTable::new();
    for (c, val) in raw_blob {
        let arr = val.as_array()?;
        if arr.len() != 2 {
            continue;
        }
        let codon = c.to_ascii_uppercase().replace('U', "T");
        if codon.len() != 3 {
            continue;
        }
        let aa = codon_aa(&codon);
        if aa == '?' {
            continue;
        }
        let count = arr
            .get(1)?
            .as_i64()
            .or_else(|| arr.get(1)?.as_u64().map(|n| n as i64))?;
        if count < 0 {
            continue;
        }
        raw.insert(codon, aa, count);
    }
    if raw.is_empty() {
        return None;
    }
    Some(TableEntry {
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned(),
        taxid: obj
            .get("taxid")
            .map(|t| {
                t.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| t.to_string())
            })
            .unwrap_or_default(),
        source: obj
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("user")
            .to_owned(),
        added: obj
            .get("added")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        raw,
    })
}

fn encode_entry(e: &TableEntry) -> Value {
    let mut raw = serde_json::Map::new();
    for (c, aa, n) in e.raw.iter() {
        raw.insert(c.to_owned(), json!([aa.to_string(), n]));
    }
    json!({
        "name": e.name,
        "taxid": e.taxid,
        "source": e.source,
        "added": e.added,
        "raw": raw,
    })
}
