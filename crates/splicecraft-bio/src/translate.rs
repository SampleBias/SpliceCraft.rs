//! CDS translation. NCBI `/transl_table` ids other than 1 apply reassignments.

use std::collections::HashMap;

use crate::iupac::rc;

/// Standard genetic code (NCBI table 1). Stops are `*`.
pub static CODON_TABLE: &[(&str, char)] = &[
    ("TTT", 'F'),
    ("TTC", 'F'),
    ("TTA", 'L'),
    ("TTG", 'L'),
    ("CTT", 'L'),
    ("CTC", 'L'),
    ("CTA", 'L'),
    ("CTG", 'L'),
    ("ATT", 'I'),
    ("ATC", 'I'),
    ("ATA", 'I'),
    ("ATG", 'M'),
    ("GTT", 'V'),
    ("GTC", 'V'),
    ("GTA", 'V'),
    ("GTG", 'V'),
    ("TCT", 'S'),
    ("TCC", 'S'),
    ("TCA", 'S'),
    ("TCG", 'S'),
    ("CCT", 'P'),
    ("CCC", 'P'),
    ("CCA", 'P'),
    ("CCG", 'P'),
    ("ACT", 'T'),
    ("ACC", 'T'),
    ("ACA", 'T'),
    ("ACG", 'T'),
    ("GCT", 'A'),
    ("GCC", 'A'),
    ("GCA", 'A'),
    ("GCG", 'A'),
    ("TAT", 'Y'),
    ("TAC", 'Y'),
    ("TAA", '*'),
    ("TAG", '*'),
    ("CAT", 'H'),
    ("CAC", 'H'),
    ("CAA", 'Q'),
    ("CAG", 'Q'),
    ("AAT", 'N'),
    ("AAC", 'N'),
    ("AAA", 'K'),
    ("AAG", 'K'),
    ("GAT", 'D'),
    ("GAC", 'D'),
    ("GAA", 'E'),
    ("GAG", 'E'),
    ("TGT", 'C'),
    ("TGC", 'C'),
    ("TGA", '*'),
    ("TGG", 'W'),
    ("CGT", 'R'),
    ("CGC", 'R'),
    ("CGA", 'R'),
    ("CGG", 'R'),
    ("AGT", 'S'),
    ("AGC", 'S'),
    ("AGA", 'R'),
    ("AGG", 'R'),
    ("GGT", 'G'),
    ("GGC", 'G'),
    ("GGA", 'G'),
    ("GGG", 'G'),
];

/// Look up a codon. Unknown → `?`.
#[must_use]
pub fn codon_aa(codon: &str) -> char {
    let key = codon.to_ascii_uppercase();
    CODON_TABLE
        .iter()
        .find(|(c, _)| *c == key)
        .map(|(_, aa)| *aa)
        .unwrap_or('?')
}

/// Normalize a GenBank `/transl_table` id (unknown → 1).
#[must_use]
pub fn normalize_table_id(table_id: i32) -> i32 {
    if table_id <= 0 { 1 } else { table_id }
}

/// Look up a codon under NCBI genetic-code `table_id`. Unknown id → table 1.
#[must_use]
pub fn codon_aa_table(codon: &str, table_id: i32) -> char {
    let key = codon.to_ascii_uppercase();
    match (normalize_table_id(table_id), key.as_str()) {
        (2, "AGA" | "AGG") => '*',
        (2, "ATA") => 'M',
        (2, "TGA") => 'W',
        (4, "TGA") => 'W',
        (5, "AGA" | "AGG") => 'S',
        (5, "ATA") => 'M',
        (5, "TGA") => 'W',
        (6, "TAA" | "TAG") => 'Q',
        (9, "AAA") => 'N',
        (9, "AGA" | "AGG") => 'S',
        (9, "TGA") => 'W',
        (10, "TGA") => 'C',
        (12, "CTG") => 'S',
        (13, "AGA" | "AGG") => 'G',
        (13, "ATA") => 'M',
        (13, "TGA") => 'W',
        (14, "AAA") => 'N',
        (14, "AGA" | "AGG") => 'S',
        (14, "TAA") => 'Y',
        (14, "TGA") => 'W',
        (16, "TAG") => 'L',
        (21, "AAA") => 'N',
        (21, "AGA" | "AGG") => 'S',
        (21, "ATA") => 'M',
        (21, "TGA") => 'W',
        (22, "TCA") => '*',
        (22, "TAG") => 'L',
        (23, "TTA") => '*',
        (24, "AGA") => 'S',
        (24, "AGG") => 'K',
        (24, "TGA") => 'W',
        (25, "TGA") => 'G',
        (26, "CTG") => 'A',
        (29, "TAA" | "TAG") => 'Y',
        (30, "TAA" | "TAG") => 'E',
        (33, "AGA") => 'S',
        (33, "AGG") => 'K',
        (33, "TAA") => 'Y',
        (33, "TGA") => 'W',
        (1 | 11, _) => codon_aa(&key),
        _ => codon_aa(&key),
    }
}

/// Table-1 slice (alt tables use [`codon_aa_table`] / [`genetic_code_map`]).
#[must_use]
pub fn codon_table_for(_table_id: i32) -> &'static [(&'static str, char)] {
    CODON_TABLE
}

/// Full codon→AA map for `table_id` (64 ACGT keys). Iteration is not ordered.
#[must_use]
pub fn genetic_code_map(table_id: i32) -> HashMap<String, char> {
    CODON_TABLE
        .iter()
        .map(|(c, _)| (c.to_string(), codon_aa_table(c, table_id)))
        .collect()
}

/// Ordered stop codons for `table_id` (standard-table walk order).
#[must_use]
pub fn stop_codons(table_id: i32) -> Vec<&'static str> {
    CODON_TABLE
        .iter()
        .filter(|(c, _)| codon_aa_table(c, table_id) == '*')
        .map(|(c, _)| *c)
        .collect()
}

/// Translate a CDS window. Wrap (`end < start`) concatenates tail+head.
/// A trailing `*` appears only when the last complete codon is a stop.
#[must_use]
pub fn translate_cds(
    full_seq: &str,
    start: usize,
    end: usize,
    strand: i8,
    codon_start: i32,
) -> String {
    let mut sub = if end < start {
        let mut s = full_seq.get(start..).unwrap_or("").to_owned();
        s.push_str(full_seq.get(..end).unwrap_or(""));
        s
    } else {
        full_seq.get(start..end).unwrap_or("").to_owned()
    };
    sub.make_ascii_uppercase();
    if strand < 0 {
        sub = rc(&sub);
    }
    let offset = (codon_start.clamp(1, 3) - 1) as usize;
    if offset > 0 && offset <= sub.len() {
        sub = sub[offset..].to_owned();
    } else if offset > sub.len() {
        sub.clear();
    }
    translate_extracted(&sub, 1)
}

/// Translate a CDS window with an NCBI `/transl_table` id.
#[must_use]
pub fn translate_cds_table(
    full_seq: &str,
    start: usize,
    end: usize,
    strand: i8,
    codon_start: i32,
    transl_table: i32,
) -> String {
    let mut sub = if end < start {
        let mut s = full_seq.get(start..).unwrap_or("").to_owned();
        s.push_str(full_seq.get(..end).unwrap_or(""));
        s
    } else {
        full_seq.get(start..end).unwrap_or("").to_owned()
    };
    sub.make_ascii_uppercase();
    if strand < 0 {
        sub = rc(&sub);
    }
    let offset = (codon_start.clamp(1, 3) - 1) as usize;
    if offset > 0 && offset <= sub.len() {
        sub = sub[offset..].to_owned();
    } else if offset > sub.len() {
        sub.clear();
    }
    translate_extracted(&sub, transl_table)
}

fn translate_extracted(sub: &str, transl_table: i32) -> String {
    let mut aa = String::new();
    let bytes = sub.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        let codon = std::str::from_utf8(&bytes[i..i + 3]).unwrap_or("NNN");
        aa.push(codon_aa_table(codon, transl_table));
        i += 3;
    }
    aa
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codon_table_complete_and_canonical() {
        assert_eq!(CODON_TABLE.len(), 64);
        let stops: Vec<_> = CODON_TABLE
            .iter()
            .filter(|(_, aa)| *aa == '*')
            .map(|(c, _)| *c)
            .collect();
        assert_eq!(stops, ["TAA", "TAG", "TGA"]);
        assert_eq!(codon_aa("ATG"), 'M');
    }

    #[test]
    fn translate_forward_and_context() {
        assert_eq!(translate_cds("ATGAAATAG", 0, 9, 1, 1), "MK*");
        let full = format!("GGGG{}CCCC", "ATGAAAAAA");
        assert_eq!(translate_cds(&full, 4, 13, 1, 1), "MKK");
        assert!(!translate_cds("ATGAAAAAA", 0, 9, 1, 1).ends_with('*'));
        assert!(translate_cds("ATGAAATAA", 0, 9, 1, 1).ends_with('*'));
    }

    #[test]
    fn translate_no_phantom_stop() {
        for nt in [
            "ATGAAAAAA",
            "ATGAAACAT",
            "ATGAAATAA",
            "ATGAAATAG",
            "ATGAAATGA",
        ] {
            let got = translate_cds(nt, 0, nt.len(), 1, 1);
            if nt.ends_with("TAA") || nt.ends_with("TAG") || nt.ends_with("TGA") {
                assert!(got.ends_with('*'), "{nt} -> {got}");
            } else {
                assert!(!got.ends_with('*'), "{nt} -> {got}");
            }
        }
    }

    #[test]
    fn translate_reverse_matches_forward_rc() {
        let fwd = "ATGAAATAG";
        let rc_cds = rc(fwd);
        let full = format!("NN{rc_cds}NN");
        assert_eq!(translate_cds(&full, 2, 11, -1, 1), "MK*");
    }

    #[test]
    fn translate_stops_and_partial_codon() {
        for stop in ["TAA", "TAG", "TGA"] {
            let seq = format!("ATGAAA{stop}");
            let aa = translate_cds(&seq, 0, 9, 1, 1);
            assert!(aa.ends_with('*'));
            assert_eq!(aa.len(), 3);
        }
        let aa = translate_cds("ATGAAAT", 0, 7, 1, 1);
        assert_eq!(aa.replace('*', ""), "MK");
    }

    #[test]
    fn translate_codon_start_and_unknown() {
        let seq = "XATGGCATAG";
        let aa = translate_cds(seq, 0, seq.len(), 1, 2);
        assert!(aa.contains("MA*"), "{aa}");
        let seq = "XXATGGCATAG";
        let aa = translate_cds(seq, 0, seq.len(), 1, 3);
        assert!(aa.contains("MA*"), "{aa}");
        assert!(translate_cds("ATGANATAG", 0, 9, 1, 1).contains('?'));
    }

    #[test]
    fn translate_reverse_iupac_and_wrap() {
        let fwd = "ATGARGTAG";
        let rc_cds = rc(fwd);
        let full = format!("AA{rc_cds}AA");
        assert_eq!(
            translate_cds(fwd, 0, 9, 1, 1),
            translate_cds(&full, 2, 11, -1, 1)
        );
        let seq = format!("{}XXATG", "AAATAG");
        assert_eq!(seq.len(), 11);
        assert_eq!(translate_cds(&seq, 8, 6, 1, 1), "MK*");
        let seq = format!("{}YY{}", "CAT", "CTATTT");
        assert_eq!(translate_cds(&seq, 5, 3, -1, 1), "MK*");
        let seq = format!("{}XXATG", "NNATAG");
        assert_eq!(translate_cds(&seq, 8, 6, 1, 1), "M?*");
    }

    #[test]
    fn translate_reverse_codon_start_2() {
        let fwd = "ATGAAATAG";
        let rc_cds = rc(fwd);
        let full = format!("{rc_cds}X");
        let cs2 = translate_cds(&full, 0, full.len(), -1, 2);
        assert!(cs2.contains("MK*"), "{cs2}");
    }

    #[test]
    fn alt_genetic_code_tables() {
        assert_eq!(codon_aa_table("TGA", 1), '*');
        assert_eq!(codon_aa_table("TGA", 4), 'W');
        assert_eq!(codon_aa_table("TAA", 6), 'Q');
        assert_eq!(codon_aa_table("TAG", 6), 'Q');
        assert_eq!(translate_cds_table("ATGTGATAA", 0, 9, 1, 1, 4), "MW*");
        assert_eq!(translate_cds_table("ATGTAA", 0, 6, 1, 1, 6), "MQ");
        assert_eq!(stop_codons(1), vec!["TAA", "TAG", "TGA"]);
        assert_eq!(stop_codons(4), vec!["TAA", "TAG"]);
        assert_eq!(stop_codons(6), vec!["TGA"]);
    }
}
