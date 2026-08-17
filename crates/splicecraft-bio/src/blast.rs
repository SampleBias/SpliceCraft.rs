//! Ungapped BLASTN / BLASTP (upstream `_blast_search_pure`) and HMMscan fallback.
//!
//! Default builds do **not** link HMMER. Queries shorter than a pyhmmer profile
//! (`< 20` nt / `< 6` aa) use this engine — the same short-query fallback as
//! upstream. HMMscan is the BLASTP path against protein subjects.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::iupac::rc;
use crate::orfs::find_orfs;
use crate::translate::translate_cds;

/// BLASTN seed length.
pub const BLASTN_K: usize = 11;
/// BLASTP seed length.
pub const BLASTP_K: usize = 3;
/// BLASTN match score.
pub const BLASTN_MATCH: i32 = 1;
/// BLASTN mismatch score.
pub const BLASTN_MISMATCH: i32 = -3;
/// BLASTN X-drop.
pub const BLASTN_X_DROP: i32 = 20;
/// BLASTN minimum HSP score.
pub const BLASTN_MIN_SCORE: i32 = 30;
/// BLASTN minimum fractional identity.
pub const BLASTN_MIN_ID: f64 = 0.70;
/// BLASTP X-drop.
pub const BLASTP_X_DROP: i32 = 25;
/// BLASTP minimum HSP score.
pub const BLASTP_MIN_SCORE: i32 = 30;
/// BLASTP minimum fractional identity.
pub const BLASTP_MIN_ID: f64 = 0.30;
/// Soft cap on ungapped extensions (repetitive query/subject).
pub const BLAST_MAX_EXTENSIONS: usize = 200_000;
/// Query length cap after sanitisation.
pub const MAX_BLAST_QUERY_LEN: usize = 100_000;
/// Below this, HMMER profiles are rejected — we stay on the ungapped path.
pub const PYHMMER_MIN_QUERY_BLASTN: usize = 20;
/// BLASTP / HMMscan HMMER floor.
pub const PYHMMER_MIN_QUERY_BLASTP: usize = 6;
/// Six-frame BLASTP ORF floor.
pub const BLASTP_MIN_ORF_AA: usize = 30;
/// HMMscan minimum protein query (upstream `_HMMSCAN_MIN_QUERY_LEN` is 10).
pub const HMMSCAN_MIN_QUERY_LEN: usize = 10;

const BLASTN_ALPHABET: &[u8] = b"ACGTNRYWSMKBDHV";
const BLASTP_ALPHABET: &[u8] = b"ACDEFGHIKLMNPQRSTVWYBZX*";

/// Which local engine to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlastProgram {
    /// DNA vs DNA (both strands).
    Blastn,
    /// Protein vs protein (or translated DNA query).
    Blastp,
    /// Same scoring as BLASTP; subjects are HMM consensus / CDS proteins.
    Hmmscan,
}

impl BlastProgram {
    /// Parse a program token.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "blastn" => Some(Self::Blastn),
            "blastp" => Some(Self::Blastp),
            "hmmscan" => Some(Self::Hmmscan),
            _ => None,
        }
    }

    /// Token used in hit records.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blastn => "blastn",
            Self::Blastp => "blastp",
            Self::Hmmscan => "hmmscan",
        }
    }

    /// Seed k-mer length.
    #[must_use]
    pub fn k(self) -> usize {
        match self {
            Self::Blastn => BLASTN_K,
            Self::Blastp | Self::Hmmscan => BLASTP_K,
        }
    }
}

/// One indexed subject (plasmid, CDS, or HMM consensus).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlastSubject {
    /// Stable id (never logged as sequence).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Collection label.
    pub collection: String,
    /// `plasmid` / `cds` / `orf6f` / `hmm`.
    pub kind: String,
    /// Forward sequence (DNA or protein).
    pub seq_fwd: String,
    /// Reverse-complement DNA when [`BlastProgram::Blastn`].
    pub seq_rev: Option<String>,
}

/// In-memory k-mer database.
#[derive(Clone, Debug, Default)]
pub struct BlastDb {
    /// Program the index was built for.
    pub program: String,
    /// Seed length.
    pub k: usize,
    /// Subjects in index order.
    pub subjects: Vec<BlastSubject>,
    /// `kmer → [(subject_idx, pos, strand)]`.
    pub kmer_index: HashMap<String, Vec<(usize, usize, i8)>>,
}

/// One ungapped HSP.
#[derive(Clone, Debug, PartialEq)]
pub struct BlastHit {
    /// Subject index.
    pub subject_idx: usize,
    /// Subject id.
    pub subject_id: String,
    /// Subject display name.
    pub subject_name: String,
    /// Collection.
    pub subject_collection: String,
    /// Subject kind.
    pub kind: String,
    /// `1` or `-1`.
    pub strand: i8,
    /// Query start (0-based, half-open).
    pub q_start: usize,
    /// Query end.
    pub q_end: usize,
    /// Subject start in forward coordinates.
    pub s_start: usize,
    /// Subject end in forward coordinates.
    pub s_end: usize,
    /// Ungapped score.
    pub score: i32,
    /// Exact letter matches.
    pub matches: i32,
    /// Aligned columns.
    pub aligned_len: usize,
    /// Percent identity (one decimal, NCBI-style).
    pub identity_pct: f64,
}

/// Strip FASTA headers, whitespace, and out-of-alphabet characters.
/// DNA pasted into BLASTP / HMMscan is translated in frame 1.
#[must_use]
pub fn detect_query_program(query: &str, program_hint: BlastProgram) -> (BlastProgram, String) {
    let raw = strip_fasta_headers(query);
    let q: String = raw
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    match program_hint {
        BlastProgram::Blastn => {
            let cleaned: String = q
                .chars()
                .filter(|c| BLASTN_ALPHABET.contains(&(*c as u8)))
                .collect();
            (BlastProgram::Blastn, truncate(&cleaned))
        }
        BlastProgram::Blastp | BlastProgram::Hmmscan => {
            if q.len() >= 9 {
                let alpha: String = q.chars().filter(|c| c.is_ascii_alphabetic()).collect();
                if !alpha.is_empty() {
                    let n_dna = alpha
                        .chars()
                        .filter(|c| matches!(c, 'A' | 'C' | 'G' | 'T' | 'N'))
                        .count();
                    if n_dna * 100 / alpha.len() >= 95 {
                        let dna: String = alpha
                            .chars()
                            .filter(|c| BLASTN_ALPHABET.contains(&(*c as u8)))
                            .collect();
                        let triplet = (dna.len() / 3) * 3;
                        if triplet >= 3 {
                            let protein = translate_cds(&dna[..triplet], 0, triplet, 1, 1);
                            return (program_hint, truncate(&protein));
                        }
                    }
                }
            }
            let cleaned: String = q
                .chars()
                .filter(|c| BLASTP_ALPHABET.contains(&(*c as u8)))
                .collect();
            (program_hint, truncate(&cleaned))
        }
    }
}

fn strip_fasta_headers(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.lines() {
        if line.starts_with('>') {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out.is_empty() { raw.to_owned() } else { out }
}

fn truncate(s: &str) -> String {
    s.chars().take(MAX_BLAST_QUERY_LEN).collect()
}

/// Index DNA subjects on both strands.
#[must_use]
pub fn build_blastn_db(subjects: Vec<BlastSubject>) -> BlastDb {
    index_db(BlastProgram::Blastn, subjects)
}

/// Index protein subjects (one strand).
#[must_use]
pub fn build_blastp_db(subjects: Vec<BlastSubject>) -> BlastDb {
    index_db(BlastProgram::Blastp, subjects)
}

/// Index HMM-consensus / CDS proteins for the ungapped HMMscan fallback.
#[must_use]
pub fn build_hmmscan_db(subjects: Vec<BlastSubject>) -> BlastDb {
    index_db(BlastProgram::Hmmscan, subjects)
}

fn index_db(program: BlastProgram, mut subjects: Vec<BlastSubject>) -> BlastDb {
    let k = program.k();
    let mut kmer_index: HashMap<String, Vec<(usize, usize, i8)>> = HashMap::new();
    for (sub_idx, sub) in subjects.iter_mut().enumerate() {
        sub.seq_fwd = sub.seq_fwd.to_ascii_uppercase();
        if program == BlastProgram::Blastn && sub.seq_rev.is_none() {
            sub.seq_rev = Some(rc(&sub.seq_fwd));
        }
        add_kmers(&mut kmer_index, k, sub_idx, &sub.seq_fwd, 1);
        if program == BlastProgram::Blastn
            && let Some(rev) = &sub.seq_rev
        {
            add_kmers(&mut kmer_index, k, sub_idx, rev, -1);
        }
    }
    BlastDb {
        program: program.as_str().into(),
        k,
        subjects,
        kmer_index,
    }
}

fn add_kmers(
    index: &mut HashMap<String, Vec<(usize, usize, i8)>>,
    k: usize,
    sub_idx: usize,
    seq: &str,
    strand: i8,
) {
    if seq.len() < k {
        return;
    }
    for i in 0..=seq.len() - k {
        index
            .entry(seq[i..i + k].to_owned())
            .or_default()
            .push((sub_idx, i, strand));
    }
}

/// Build BLASTP / HMMscan protein subjects from annotated CDS features.
pub fn protein_subjects_from_cds(
    plasmid_id: &str,
    plasmid_name: &str,
    collection: &str,
    sequence: &str,
    features: &[(String, usize, usize, i8, String)],
) -> Vec<BlastSubject> {
    let mut out = Vec::new();
    for (kind, start, end, strand, label) in features {
        if !kind.eq_ignore_ascii_case("CDS") {
            continue;
        }
        let protein = translate_cds(sequence, *start, *end, *strand, 1);
        let protein = protein.trim_end_matches('*').to_owned();
        if protein.len() < BLASTP_K {
            continue;
        }
        let name = if label.is_empty() {
            plasmid_name.to_owned()
        } else {
            label.clone()
        };
        out.push(BlastSubject {
            id: format!("{plasmid_id}:{name}"),
            name,
            collection: collection.to_owned(),
            kind: "cds".into(),
            seq_fwd: protein,
            seq_rev: None,
        });
    }
    out
}

/// Optional six-frame ORF subjects (`kind=orf6f`).
#[must_use]
pub fn protein_subjects_from_orfs(
    plasmid_id: &str,
    plasmid_name: &str,
    collection: &str,
    sequence: &str,
    circular: bool,
) -> Vec<BlastSubject> {
    let _ = plasmid_name;
    find_orfs(sequence, circular, BLASTP_MIN_ORF_AA, false)
        .into_iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let aa = o.aa_seq.trim_end_matches('*').to_owned();
            if aa.len() < BLASTP_MIN_ORF_AA {
                return None;
            }
            let label = format!("orf:{}{}:#{i}", if o.strand < 0 { "R" } else { "F" }, 1);
            Some(BlastSubject {
                id: format!("{plasmid_id}:{label}"),
                name: label,
                collection: collection.to_owned(),
                kind: "orf6f".into(),
                seq_fwd: aa,
                seq_rev: None,
            })
        })
        .collect()
}

/// Ungapped seed-and-extend. Empty when the query is shorter than `k`.
#[must_use]
pub fn blast_search(query: &str, db: &BlastDb, max_hits: usize) -> Vec<BlastHit> {
    let program = BlastProgram::parse(&db.program).unwrap_or(BlastProgram::Blastn);
    let k = db.k;
    if k == 0 || query.len() < k || db.subjects.is_empty() {
        return Vec::new();
    }
    let q = query.to_ascii_uppercase();
    let (x_drop, min_score, min_id, dna) = match program {
        BlastProgram::Blastn => (BLASTN_X_DROP, BLASTN_MIN_SCORE, BLASTN_MIN_ID, true),
        BlastProgram::Blastp | BlastProgram::Hmmscan => {
            (BLASTP_X_DROP, BLASTP_MIN_SCORE, BLASTP_MIN_ID, false)
        }
    };

    let mut seen = HashSet::new();
    let mut hsps = Vec::new();
    let mut extensions = 0usize;
    for q_pos in 0..=q.len() - k {
        if extensions >= BLAST_MAX_EXTENSIONS {
            break;
        }
        let kmer = &q[q_pos..q_pos + k];
        let Some(seeds) = db.kmer_index.get(kmer) else {
            continue;
        };
        for &(sub_idx, s_pos, strand) in seeds {
            extensions += 1;
            if extensions > BLAST_MAX_EXTENSIONS {
                break;
            }
            let sub = &db.subjects[sub_idx];
            let s_seq = if strand == -1 {
                sub.seq_rev.as_deref().unwrap_or(&sub.seq_fwd)
            } else {
                &sub.seq_fwd
            };
            let (q_lo, q_hi, s_lo, s_hi, score, matches) =
                ungapped_extend(&q, s_seq, q_pos, s_pos, k, dna, x_drop);
            let aligned_len = (q_hi - q_lo).max(1);
            if score < min_score {
                continue;
            }
            let ident = f64::from(matches) / aligned_len as f64;
            if ident < min_id {
                continue;
            }
            let key = (sub_idx, strand, q_lo, s_lo, q_hi);
            if !seen.insert(key) {
                continue;
            }
            let (s_lo_disp, s_hi_disp) = if strand == -1 && sub.seq_rev.is_some() {
                let fwd_len = sub.seq_fwd.len();
                (fwd_len - s_hi, fwd_len - s_lo)
            } else {
                (s_lo, s_hi)
            };
            hsps.push(BlastHit {
                subject_idx: sub_idx,
                subject_id: sub.id.clone(),
                subject_name: sub.name.clone(),
                subject_collection: sub.collection.clone(),
                kind: sub.kind.clone(),
                strand,
                q_start: q_lo,
                q_end: q_hi,
                s_start: s_lo_disp,
                s_end: s_hi_disp,
                score,
                matches,
                aligned_len,
                identity_pct: (ident * 1000.0).round() / 10.0,
            });
        }
    }
    hsps.sort_by(|a, b| {
        b.score.cmp(&a.score).then_with(|| {
            b.identity_pct
                .partial_cmp(&a.identity_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    hsps.truncate(max_hits);
    hsps
}

/// HMMscan fallback: ungapped protein search (no Pfam / hmmpress).
#[must_use]
pub fn hmmscan_ungapped(query_protein: &str, db: &BlastDb, max_hits: usize) -> Vec<BlastHit> {
    let cleaned: String = query_protein
        .chars()
        .filter(|c| BLASTP_ALPHABET.contains(&(*c as u8)))
        .collect::<String>()
        .to_ascii_uppercase();
    if cleaned.len() < HMMSCAN_MIN_QUERY_LEN {
        return Vec::new();
    }
    blast_search(&cleaned, db, max_hits)
}

fn ungapped_extend(
    s_query: &str,
    s_subject: &str,
    q_pos: usize,
    s_pos: usize,
    k: usize,
    dna: bool,
    x_drop: i32,
) -> (usize, usize, usize, usize, i32, i32) {
    let score_pair = |a: char, b: char| -> i32 {
        if dna {
            if a == b {
                BLASTN_MATCH
            } else {
                BLASTN_MISMATCH
            }
        } else {
            blosum62_score(a, b)
        }
    };
    let q = s_query.as_bytes();
    let s = s_subject.as_bytes();
    let mut seed_score = 0;
    let mut seed_matches = 0;
    for i in 0..k {
        let a = q[q_pos + i] as char;
        let b = s[s_pos + i] as char;
        seed_score += score_pair(a, b);
        if a == b {
            seed_matches += 1;
        }
    }

    let mut q_right = q_pos + k;
    let mut s_right = s_pos + k;
    let mut cur = seed_score;
    let mut best_right = cur;
    let mut best_q_right = q_right;
    let mut best_s_right = s_right;
    let mut matches_right = 0;
    let mut best_matches_right = 0;
    while q_right < q.len() && s_right < s.len() {
        let a = q[q_right] as char;
        let b = s[s_right] as char;
        cur += score_pair(a, b);
        if a == b {
            matches_right += 1;
        }
        if cur > best_right {
            best_right = cur;
            best_q_right = q_right + 1;
            best_s_right = s_right + 1;
            best_matches_right = matches_right;
        }
        if best_right - cur > x_drop {
            break;
        }
        q_right += 1;
        s_right += 1;
    }

    let mut q_left = q_pos as isize - 1;
    let mut s_left = s_pos as isize - 1;
    cur = seed_score;
    let mut best_left_delta = 0;
    let mut best_q_left = q_pos;
    let mut best_s_left = s_pos;
    let mut matches_left = 0;
    let mut best_matches_left = 0;
    while q_left >= 0 && s_left >= 0 {
        let a = q[q_left as usize] as char;
        let b = s[s_left as usize] as char;
        cur += score_pair(a, b);
        if a == b {
            matches_left += 1;
        }
        let delta = cur - seed_score;
        if delta > best_left_delta {
            best_left_delta = delta;
            best_q_left = q_left as usize;
            best_s_left = s_left as usize;
            best_matches_left = matches_left;
        }
        if best_left_delta + seed_score - cur > x_drop {
            break;
        }
        q_left -= 1;
        s_left -= 1;
    }

    (
        best_q_left,
        best_q_right,
        best_s_left,
        best_s_right,
        best_right + best_left_delta,
        seed_matches + best_matches_left + best_matches_right,
    )
}

fn blosum62_score(a: char, b: char) -> i32 {
    let table = blosum62_table();
    *table
        .get(&(a.to_ascii_uppercase(), b.to_ascii_uppercase()))
        .unwrap_or(&-4)
}

fn blosum62_table() -> &'static HashMap<(char, char), i32> {
    static TABLE: OnceLock<HashMap<(char, char), i32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        const LETTERS: &[u8] = b"ARNDCQEGHILKMFPSTWYVBZX*";
        #[rustfmt::skip]
        const ROWS: [&[i32]; 24] = [
            &[ 4,-1,-2,-2, 0,-1,-1, 0,-2,-1,-1,-1,-1,-2,-1, 1, 0,-3,-2, 0,-2,-1, 0,-4],
            &[-1, 5, 0,-2,-3, 1, 0,-2, 0,-3,-2, 2,-1,-3,-2,-1,-1,-3,-2,-3,-1, 0,-1,-4],
            &[-2, 0, 6, 1,-3, 0, 0, 0, 1,-3,-3, 0,-2,-3,-2, 1, 0,-4,-2,-3, 3, 0,-1,-4],
            &[-2,-2, 1, 6,-3, 0, 2,-1,-1,-3,-4,-1,-3,-3,-1, 0,-1,-4,-3,-3, 4, 1,-1,-4],
            &[ 0,-3,-3,-3, 9,-3,-4,-3,-3,-1,-1,-3,-1,-2,-3,-1,-1,-2,-2,-1,-3,-3,-2,-4],
            &[-1, 1, 0, 0,-3, 5, 2,-2, 0,-3,-2, 1, 0,-3,-1, 0,-1,-2,-1,-2, 0, 3,-1,-4],
            &[-1, 0, 0, 2,-4, 2, 5,-2, 0,-3,-3, 1,-2,-3,-1, 0,-1,-3,-2,-2, 1, 4,-1,-4],
            &[ 0,-2, 0,-1,-3,-2,-2, 6,-2,-4,-4,-2,-3,-3,-2, 0,-2,-2,-3,-3,-1,-2,-1,-4],
            &[-2, 0, 1,-1,-3, 0, 0,-2, 8,-3,-3,-1,-2,-1,-2,-1,-2,-2, 2,-3, 0, 0,-1,-4],
            &[-1,-3,-3,-3,-1,-3,-3,-4,-3, 4, 2,-3, 1, 0,-3,-2,-1,-3,-1, 3,-3,-3,-1,-4],
            &[-1,-2,-3,-4,-1,-2,-3,-4,-3, 2, 4,-2, 2, 0,-3,-2,-1,-2,-1, 1,-4,-3,-1,-4],
            &[-1, 2, 0,-1,-3, 1, 1,-2,-1,-3,-2, 5,-1,-3,-1, 0,-1,-3,-2,-2, 0, 1,-1,-4],
            &[-1,-1,-2,-3,-1, 0,-2,-3,-2, 1, 2,-1, 5, 0,-2,-1,-1,-1,-1, 1,-3,-1,-1,-4],
            &[-2,-3,-3,-3,-2,-3,-3,-3,-1, 0, 0,-3, 0, 6,-4,-2,-2, 1, 3,-1,-3,-3,-1,-4],
            &[-1,-2,-2,-1,-3,-1,-1,-2,-2,-3,-3,-1,-2,-4, 7,-1,-1,-4,-3,-2,-2,-1,-2,-4],
            &[ 1,-1, 1, 0,-1, 0, 0, 0,-1,-2,-2, 0,-1,-2,-1, 4, 1,-3,-2,-2, 0, 0, 0,-4],
            &[ 0,-1, 0,-1,-1,-1,-1,-2,-2,-1,-1,-1,-1,-2,-1, 1, 5,-2,-2, 0,-1,-1, 0,-4],
            &[-3,-3,-4,-4,-2,-2,-3,-2,-2,-3,-2,-3,-1, 1,-4,-3,-2,11, 2,-3,-4,-3,-2,-4],
            &[-2,-2,-2,-3,-2,-1,-2,-3, 2,-1,-1,-2,-1, 3,-3,-2,-2, 2, 7,-1,-3,-2,-1,-4],
            &[ 0,-3,-3,-3,-1,-2,-2,-3,-3, 3, 1,-2, 1,-1,-2,-2, 0,-3,-1, 4,-3,-2,-1,-4],
            &[-2,-1, 3, 4,-3, 0, 1,-1, 0,-3,-4, 0,-3,-3,-2, 0,-1,-4,-3,-3, 4, 1,-1,-4],
            &[-1, 0, 0, 1,-3, 3, 4,-2, 0,-3,-3, 1,-1,-3,-1, 0,-1,-3,-2,-2, 1, 4,-1,-4],
            &[ 0,-1,-1,-1,-2,-1,-1,-1,-1,-1,-1,-1,-1,-1,-2, 0, 0,-2,-1,-1,-1,-1,-1,-4],
            &[-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4, 1],
        ];
        let mut table = HashMap::new();
        for (i, row) in ROWS.iter().enumerate() {
            for (j, score) in row.iter().enumerate() {
                table.insert((LETTERS[i] as char, LETTERS[j] as char), *score);
            }
        }
        table
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dna_subject(seq: &str) -> BlastSubject {
        BlastSubject {
            id: "p1".into(),
            name: "pDemo".into(),
            collection: "Main".into(),
            kind: "plasmid".into(),
            seq_fwd: seq.into(),
            seq_rev: None,
        }
    }

    #[test]
    fn blastn_finds_planted_perfect_match() {
        let motif = "ACGTACGTACGTACGTACGTACGTACGTACGT";
        assert!(motif.len() >= 30);
        let subject = format!("{}{}{}", "T".repeat(40), motif, "G".repeat(40));
        let db = build_blastn_db(vec![dna_subject(&subject)]);
        let hits = blast_search(motif, &db, 5);
        assert!(!hits.is_empty(), "{hits:?}");
        assert_eq!(hits[0].subject_id, "p1");
        assert!(hits[0].identity_pct >= 99.0, "{}", hits[0].identity_pct);
        assert_eq!(hits[0].s_start, 40);
    }

    #[test]
    fn short_query_below_k_is_empty_not_an_error() {
        let db = build_blastn_db(vec![dna_subject(&"A".repeat(50))]);
        assert!(blast_search("ACGT", &db, 5).is_empty());
    }

    #[test]
    fn blastp_and_hmmscan_fallback_hit_identical_protein() {
        let prot = "MKKLLVVGGGTGGKTT";
        let sub = BlastSubject {
            id: "cds1".into(),
            name: "geneA".into(),
            collection: "Main".into(),
            kind: "cds".into(),
            seq_fwd: format!("{prot}AAAA"),
            seq_rev: None,
        };
        let db = build_blastp_db(vec![sub.clone()]);
        let hits = blast_search(prot, &db, 5);
        assert!(!hits.is_empty(), "{hits:?}");
        let hmm_db = build_hmmscan_db(vec![sub]);
        let hmm = hmmscan_ungapped(prot, &hmm_db, 5);
        assert!(!hmm.is_empty(), "{hmm:?}");
        assert_eq!(hmm[0].kind, "cds");
    }

    #[test]
    fn detect_translates_dna_for_blastp() {
        let (prog, q) = detect_query_program("ATGAAGAAACTC", BlastProgram::Blastp);
        assert_eq!(prog, BlastProgram::Blastp);
        assert!(q.starts_with('M'), "{q}");
    }
}
