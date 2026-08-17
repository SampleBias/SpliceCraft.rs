//! Turn a ligation / Gibson / GG product into a [`Record`] with parent features.

use splicecraft_core::{Feature, Record};

use crate::fragment::FragFeature;
use crate::history::HistoryNode;

/// Build a product record. Wrap features (`end < start`) are kept. [INV-09]
#[must_use]
pub fn product_record(
    name: &str,
    seq: &str,
    circular: bool,
    features: &[FragFeature],
    history: &HistoryNode,
) -> Record {
    let mut rec = Record::new(name, seq.to_ascii_uppercase(), circular);
    rec.features = features
        .iter()
        .filter(|f| f.kind != "source")
        .map(FragFeature::to_core)
        .collect();
    rec.comments.push(history.comment_line());
    rec
}

/// Attach a history comment to an existing record.
pub fn stamp_history(rec: &mut Record, history: &HistoryNode) {
    let line = history.comment_line();
    if !rec.comments.iter().any(|c| c == &line) {
        rec.comments.push(line);
    }
}

/// Copy parent features that still sit inside the product (linear coords).
#[must_use]
pub fn carry_parent_features(parents: &[&Record], product_len: usize) -> Vec<Feature> {
    let mut out = Vec::new();
    for rec in parents {
        for f in &rec.features {
            if f.kind == "source" {
                continue;
            }
            if f.start < product_len && f.end <= product_len {
                out.push(f.clone());
            }
        }
    }
    out
}
