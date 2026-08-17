//! Undo stack [INV-10] and in-place sequence edits.

use splicecraft_core::{EditMode, Record, rebuild_record_with_edit};

/// Snapshot depth matching upstream.
pub const UNDO_LIMIT: usize = 50;

/// Deep-cloned record history. Mutating a restored record must not change
/// a stack entry.
#[derive(Clone, Debug, Default)]
pub struct UndoStack {
    undo: Vec<Record>,
    redo: Vec<Record>,
}

impl UndoStack {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a **clone** of `current` and clear redo.
    pub fn push(&mut self, current: &Record) {
        self.undo.push(current.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Pop undo; a clone of `current` moves onto redo.
    pub fn undo(&mut self, current: &Record) -> Option<Record> {
        let prev = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(prev)
    }

    /// Pop redo; a clone of `current` moves onto undo.
    pub fn redo(&mut self, current: &Record) -> Option<Record> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        Some(next)
    }

    /// Borrow the latest undo snapshot (for the deep-clone test).
    #[must_use]
    pub fn peek_undo(&self) -> Option<&Record> {
        self.undo.last()
    }

    /// Borrow the latest redo snapshot.
    #[must_use]
    pub fn peek_redo(&self) -> Option<&Record> {
        self.redo.last()
    }

    /// Drop the last undo snapshot without restoring it.
    pub fn pop_silent(&mut self) {
        let _ = self.undo.pop();
    }
}

/// Insert `bases` at `at` (before). Features shift via [INV-09].
#[must_use]
pub fn insert_bases(record: &Record, at: usize, bases: &str) -> Record {
    let at = at.min(record.len());
    let mut new_seq = record.sequence.clone();
    new_seq.insert_str(at, &bases.to_ascii_uppercase());
    rebuild_record_with_edit(record, &new_seq, EditMode::Insert, at, at, bases)
}

/// Delete `[from, to)` (half-open).
#[must_use]
pub fn delete_span(record: &Record, from: usize, to: usize) -> Record {
    let n = record.len();
    let from = from.min(n);
    let to = to.min(n).max(from);
    let mut new_seq = record.sequence.clone();
    new_seq.replace_range(from..to, "");
    rebuild_record_with_edit(record, &new_seq, EditMode::Replace, from, to, "")
}

/// Smallest feature (by wrap-aware length) that contains `bp`.
#[must_use]
pub fn smallest_enclosing(record: &Record, bp: usize) -> Option<usize> {
    let n = record.len();
    record
        .features
        .iter()
        .enumerate()
        .filter(|(_, f)| f.kind != "source" && f.contains_bp(bp))
        .min_by_key(|(_, f)| f.len_on(n))
        .map(|(i, _)| i)
}
