//! Memory-only demo plasmids. Never written unless the user Keeps.
//!
//! Basic is the original 120 bp toy. Advanced is a synthetic teaching
//! plasmid so the map, feature sidebar, AA lane, and RE overlay have
//! something to show. Sequence content is never logged.

use splicecraft_bio::rc;
use splicecraft_core::{Feature, Record};

/// Filler 7-mer chosen so common 6-cutters are not accidental.
const FILL: &[u8] = b"CGTATAC";
/// Advanced demo length (bp).
pub const ADVANCED_DEMO_LEN: usize = 2400;

/// Tiny circular filler with a wrap feature + CDS. Sequence stays off logs.
#[must_use]
pub fn demo_record() -> Record {
    let mut seq = String::from("ATGAAATAG");
    seq.push_str(&"ATGC".repeat(28));
    seq.truncate(120);
    let mut rec = Record::new("pDemo", seq, true);
    rec.features.push(Feature::new("CDS", 0, 9, 1, "orf"));
    rec.features
        .push(Feature::new("misc_feature", 110, 8, 1, "wrap_ori"));
    rec
}

/// Richer circular teaching plasmid: many feature types, wrap origin,
/// reverse-strand CDS, and planted unique / non-unique cutters.
#[must_use]
pub fn demo_record_advanced() -> Record {
    let mut dna = fill_seq(ADVANCED_DEMO_LEN);

    // AT-rich ori spanning the origin (wrap feature 2280..80).
    plant_circular(&mut dna, 2280, &b"AATATA".repeat(34));

    plant_orf(&mut dna, 180, 220, b"GCA"); // bla 666 bp → 846
    plant_orf(&mut dna, 1200, 78, b"GCT"); // lacZα 240 bp → 1440

    let rop = atg_poly_stop(40, b"GAT");
    let rop_fwd = rc(std::str::from_utf8(&rop).expect("ascii orf"));
    plant(&mut dna, 1600, rop_fwd.as_bytes());

    plant(&mut dna, 960, b"GAATTC"); // EcoRI (unique)
    plant(&mut dna, 980, b"GGATCC"); // BamHI
    plant(&mut dna, 1000, b"AAGCTT"); // HindIII
    plant(&mut dna, 1020, b"CTGCAG"); // PstI
    plant(&mut dna, 1040, b"GAGCTC"); // SacI
    plant(&mut dna, 1060, b"TCTAGA"); // XbaI
    plant(&mut dna, 1080, b"GTCGAC"); // SalI
    plant(&mut dna, 1880, b"GGATCC"); // second BamHI (not unique)

    let seq = String::from_utf8(dna).expect("demo DNA is ASCII");
    let mut rec = Record::new("pDemoAdv", seq, true);
    rec.features.extend([
        Feature::new("rep_origin", 2280, 80, 1, "ori"),
        Feature::new("promoter", 100, 155, 1, "AmpR_p"),
        Feature::new("RBS", 158, 171, 1, "RBS"),
        Feature::new("CDS", 180, 846, 1, "bla"),
        Feature::new("terminator", 850, 890, 1, "AmpR_t"),
        Feature::new("primer_bind", 900, 920, 1, "M13_fwd"),
        Feature::new("misc_feature", 940, 1120, 1, "MCS"),
        Feature::new("promoter", 1140, 1195, 1, "lac_p"),
        Feature::new("CDS", 1200, 1440, 1, "lacZ_alpha"),
        Feature::new("terminator", 1450, 1490, 1, "T1"),
        Feature::new("primer_bind", 1500, 1520, -1, "M13_rev"),
        Feature::new("CDS", 1600, 1726, -1, "rop"),
        Feature::new("repeat_region", 1800, 1920, 1, "IS-like"),
        Feature::new("stem_loop", 2050, 2090, 1, "hairpin"),
        Feature::new("tRNA", 2100, 2174, 1, "tRNA-dummy"),
    ]);
    rec
}

fn fill_seq(len: usize) -> Vec<u8> {
    (0..len).map(|i| FILL[i % FILL.len()]).collect()
}

fn plant(seq: &mut [u8], at: usize, motif: &[u8]) {
    seq[at..at + motif.len()].copy_from_slice(motif);
}

fn plant_circular(seq: &mut [u8], start: usize, motif: &[u8]) {
    let n = seq.len();
    for (i, b) in motif.iter().enumerate() {
        seq[(start + i) % n] = *b;
    }
}

fn atg_poly_stop(aa: usize, codon: &[u8; 3]) -> Vec<u8> {
    let mut v = Vec::with_capacity(6 + aa * 3);
    v.extend_from_slice(b"ATG");
    for _ in 0..aa {
        v.extend_from_slice(codon);
    }
    v.extend_from_slice(b"TAA");
    v
}

fn plant_orf(seq: &mut [u8], start: usize, aa: usize, codon: &[u8; 3]) {
    let orf = atg_poly_stop(aa, codon);
    plant(seq, start, &orf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_demo_stays_tiny() {
        let rec = demo_record();
        assert_eq!(rec.name, "pDemo");
        assert_eq!(rec.len(), 120);
        assert_eq!(rec.features.len(), 2);
        assert!(rec.features.iter().any(|f| f.is_wrap()));
    }

    #[test]
    fn advanced_demo_is_richer() {
        let basic = demo_record();
        let adv = demo_record_advanced();
        assert_eq!(adv.name, "pDemoAdv");
        assert_eq!(adv.len(), ADVANCED_DEMO_LEN);
        assert!(adv.len() > basic.len() * 10);
        assert!(adv.features.len() > basic.features.len() + 5);
        assert!(adv.circular);
        assert!(adv.features.iter().any(|f| f.is_wrap()));
        assert!(adv.features.iter().any(|f| f.strand < 0));
        let kinds: Vec<_> = adv.features.iter().map(|f| f.kind.as_str()).collect();
        for need in [
            "CDS",
            "promoter",
            "terminator",
            "RBS",
            "rep_origin",
            "primer_bind",
            "misc_feature",
        ] {
            assert!(kinds.contains(&need), "missing {need} in {kinds:?}");
        }
        let eco = adv.sequence.matches("GAATTC").count();
        let bam = adv.sequence.matches("GGATCC").count();
        assert_eq!(eco, 1, "EcoRI should be unique");
        assert_eq!(bam, 2, "BamHI should appear twice");
        let rop = adv.features.iter().find(|f| f.label == "rop").expect("rop");
        assert_eq!(rop.strand, -1);
        let orf = atg_poly_stop(40, b"GAT");
        let fwd = rc(std::str::from_utf8(&orf).unwrap());
        assert_eq!(&adv.sequence[rop.start..rop.end], fwd.as_str());
    }
}
