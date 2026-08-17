//! Build an in-memory BLAST / HMMscan DB from the plasmid library.

use splicecraft_bio::{
    BlastDb, BlastProgram, BlastSubject, build_blastn_db, build_blastp_db, build_hmmscan_db,
    protein_subjects_from_cds, protein_subjects_from_orfs,
};
use splicecraft_persist::LibraryStore;

use crate::gb_text_to_record;

/// Index the library for local BLASTN / BLASTP / HMMscan (ungapped fallback).
#[must_use]
pub fn blast_db_from_library(
    store: &LibraryStore,
    program: BlastProgram,
    six_frame: bool,
) -> BlastDb {
    let mut subjects = Vec::new();
    for col in &store.collections {
        for entry in &col.plasmids {
            push_entry(&mut subjects, program, six_frame, &col.name, entry);
        }
    }
    if subjects.is_empty() {
        for entry in &store.plasmids {
            push_entry(&mut subjects, program, six_frame, &store.active, entry);
        }
    }
    match program {
        BlastProgram::Blastn => build_blastn_db(subjects),
        BlastProgram::Blastp => build_blastp_db(subjects),
        BlastProgram::Hmmscan => build_hmmscan_db(subjects),
    }
}

fn push_entry(
    subjects: &mut Vec<BlastSubject>,
    program: BlastProgram,
    six_frame: bool,
    collection: &str,
    entry: &splicecraft_persist::LibraryEntry,
) {
    if entry.gb_text.is_empty() {
        return;
    }
    let Ok(rec) = gb_text_to_record(&entry.gb_text) else {
        return;
    };
    match program {
        BlastProgram::Blastn => {
            subjects.push(BlastSubject {
                id: entry.id.clone(),
                name: entry.name.clone(),
                collection: collection.to_owned(),
                kind: "plasmid".into(),
                seq_fwd: rec.sequence.to_ascii_uppercase(),
                seq_rev: None,
            });
        }
        BlastProgram::Blastp | BlastProgram::Hmmscan => {
            let feats: Vec<_> = rec
                .features
                .iter()
                .map(|f| (f.kind.clone(), f.start, f.end, f.strand, f.label.clone()))
                .collect();
            subjects.extend(protein_subjects_from_cds(
                &entry.id,
                &entry.name,
                collection,
                &rec.sequence,
                &feats,
            ));
            if six_frame {
                subjects.extend(protein_subjects_from_orfs(
                    &entry.id,
                    &entry.name,
                    collection,
                    &rec.sequence,
                    rec.circular,
                ));
            }
        }
    }
}
