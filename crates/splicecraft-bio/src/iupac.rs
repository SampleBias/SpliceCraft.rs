//! IUPAC reverse-complement and cached recognition patterns.
//! Sacred [INV-03] and [INV-04].

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use regex::Regex;
use thiserror::Error;

/// IUPAC recognition-site → regex fragment.
const IUPAC_RE: &[(&str, &str)] = &[
    ("A", "A"),
    ("C", "C"),
    ("G", "G"),
    ("T", "T"),
    ("R", "[AG]"),
    ("Y", "[CT]"),
    ("W", "[AT]"),
    ("S", "[CG]"),
    ("M", "[AC]"),
    ("K", "[GT]"),
    ("B", "[CGT]"),
    ("D", "[AGT]"),
    ("H", "[ACT]"),
    ("V", "[ACG]"),
    ("N", "[ACGT]"),
];

const PATTERN_CACHE_MAX: usize = 256;

/// From / to strings for [`rc`] (U → A so RNA-form bases complement).
const IUPAC_COMP_FROM: &[u8] = b"ACGTURYWSMKBDHVN";
const IUPAC_COMP_TO: &[u8] = b"TGCAAYRWSKMVHDBN";

/// Errors from biology primitives.
#[derive(Debug, Error)]
pub enum BioError {
    /// Empty recognition site would match every position.
    #[error("recognition site {0:?} is empty / whitespace-only")]
    EmptySite(String),
    /// Site contains a character outside the IUPAC alphabet.
    #[error("recognition site {site:?} contains non-IUPAC character(s) {bad}")]
    NonIupac {
        /// The original site.
        site: String,
        /// Offending characters.
        bad: String,
    },
    /// Regex compilation failed (should not happen for validated IUPAC).
    #[error("invalid IUPAC regex: {0}")]
    Regex(#[from] regex::Error),
}

/// Full-IUPAC reverse complement. Output is uppercase. [INV-03]
#[must_use]
pub fn rc(seq: &str) -> String {
    let upper = seq.to_ascii_uppercase();
    let mut out = String::with_capacity(upper.len());
    for b in upper.bytes().rev() {
        out.push(comp_byte(b) as char);
    }
    out
}

fn comp_byte(b: u8) -> u8 {
    IUPAC_COMP_FROM
        .iter()
        .position(|&c| c == b)
        .map(|i| IUPAC_COMP_TO[i])
        .unwrap_or(b)
}

struct PatternLru {
    map: HashMap<String, Arc<Regex>>,
    order: VecDeque<String>,
}

impl PatternLru {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &str) -> Option<Arc<Regex>> {
        if self.map.contains_key(key) {
            if let Some(idx) = self.order.iter().position(|k| k == key) {
                let k = self.order.remove(idx).expect("index exists");
                self.order.push_back(k);
            }
            return self.map.get(key).cloned();
        }
        None
    }

    fn insert(&mut self, key: String, pat: Arc<Regex>) {
        if self.map.len() >= PATTERN_CACHE_MAX
            && let Some(old) = self.order.pop_front()
        {
            self.map.remove(&old);
        }
        self.order.push_back(key.clone());
        self.map.insert(key, pat);
    }

    fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

fn cache() -> &'static Mutex<PatternLru> {
    static CACHE: OnceLock<Mutex<PatternLru>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(PatternLru::new()))
}

/// Compile an IUPAC recognition site to a cached regex. [INV-04]
pub fn iupac_pattern(site: &str) -> Result<Arc<Regex>, BioError> {
    let key = site.to_ascii_uppercase();
    if key.trim().is_empty() {
        return Err(BioError::EmptySite(site.to_owned()));
    }
    {
        let mut guard = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pat) = guard.get(&key) {
            return Ok(pat);
        }
    }
    let mut bad = Vec::new();
    let mut pieces = String::new();
    for c in key.chars() {
        match iupac_frag(c) {
            Some(frag) => pieces.push_str(frag),
            None => bad.push(c),
        }
    }
    if !bad.is_empty() {
        let shown: String = bad
            .iter()
            .take(6)
            .map(|c| format!("{c:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(BioError::NonIupac {
            site: site.to_owned(),
            bad: shown,
        });
    }
    let pat = Arc::new(Regex::new(&pieces)?);
    {
        let mut guard = cache().lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(key, Arc::clone(&pat));
    }
    Ok(pat)
}

fn iupac_frag(c: char) -> Option<&'static str> {
    IUPAC_RE
        .iter()
        .find(|(k, _)| k.as_bytes()[0] == c as u8)
        .map(|(_, v)| *v)
}

/// True if `key` is currently in the pattern cache (tests / [INV-04]).
#[must_use]
pub fn pattern_cache_contains(key: &str) -> bool {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&key.to_ascii_uppercase())
}

/// Clear the IUPAC pattern cache (tests).
pub fn pattern_cache_clear() {
    cache().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Unambiguous bases admitted by one IUPAC code (`U` is treated as `T`).
#[must_use]
pub fn iupac_base_set(code: char) -> Option<&'static [u8]> {
    match code.to_ascii_uppercase() {
        'A' => Some(b"A"),
        'C' => Some(b"C"),
        'G' => Some(b"G"),
        'T' | 'U' => Some(b"T"),
        'R' => Some(b"AG"),
        'Y' => Some(b"CT"),
        'W' => Some(b"AT"),
        'S' => Some(b"CG"),
        'M' => Some(b"AC"),
        'K' => Some(b"GT"),
        'B' => Some(b"CGT"),
        'D' => Some(b"AGT"),
        'H' => Some(b"ACT"),
        'V' => Some(b"ACG"),
        'N' => Some(b"ACGT"),
        _ => None,
    }
}

/// True when IUPAC codes `a` and `b` share at least one unambiguous base.
#[must_use]
pub fn iupac_compatible(a: char, b: char) -> bool {
    match (iupac_base_set(a), iupac_base_set(b)) {
        (Some(sa), Some(sb)) => sa.iter().any(|c| sb.contains(c)),
        _ => false,
    }
}

/// Every `(pattern, start)` occurrence of each IUPAC pattern in `seq`.
///
/// Invalid patterns are skipped. Overlapping hits are reported (advance by 1).
#[must_use]
pub fn forbidden_hit_set(seq: &str, patterns: &[&str]) -> HashSet<(String, usize)> {
    let mut out = HashSet::new();
    for p in patterns {
        let Ok(pat) = iupac_pattern(p) else {
            continue;
        };
        for start in iter_match_starts(&pat, seq) {
            out.insert(((*p).to_owned(), start));
        }
    }
    out
}

/// Yield every match start, including overlapping tandem hits.
pub(crate) fn iter_match_starts(pat: &Regex, s: &str) -> Vec<usize> {
    let mut pos = 0;
    let mut out = Vec::new();
    while pos <= s.len() {
        match pat.find_at(s, pos) {
            None => break,
            Some(m) => {
                let start = m.start();
                out.push(start);
                pos = start + 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv03_rc_acgt_ground_truth() {
        assert_eq!(rc("ATGC"), "GCAT");
        assert_eq!(rc("AAATTT"), "AAATTT");
        assert_eq!(rc("GAATTC"), "GAATTC");
        assert_eq!(rc("GGTCTC"), "GAGACC");
    }

    #[test]
    fn inv03_each_iupac_code() {
        let pairs = [
            ("A", "T"),
            ("T", "A"),
            ("C", "G"),
            ("G", "C"),
            ("R", "Y"),
            ("Y", "R"),
            ("W", "W"),
            ("S", "S"),
            ("M", "K"),
            ("K", "M"),
            ("B", "V"),
            ("V", "B"),
            ("D", "H"),
            ("H", "D"),
            ("N", "N"),
        ];
        for (code, comp) in pairs {
            assert_eq!(rc(code), comp, "{code}");
        }
    }

    #[test]
    fn inv03_rc_uppercases_and_preserves_len() {
        assert_eq!(rc("acgt"), "ACGT");
        assert_eq!(rc("gaattc"), "GAATTC");
        for n in [0, 1, 2, 6, 20, 100] {
            let seq = "ACGTRY".repeat(n / 6 + 1);
            let seq = &seq[..n];
            assert_eq!(rc(seq).len(), n);
        }
    }

    #[test]
    fn inv04_plain_and_degenerate_patterns() {
        let p = iupac_pattern("GAATTC").unwrap();
        assert!(p.is_match("TTTGAATTCAAA"));
        assert!(!p.is_match("TTTGAATTTAAA"));
        let p = iupac_pattern("CYCGRG").unwrap();
        for m in ["CCCGAG", "CCCGGG", "CTCGAG", "CTCGGG"] {
            assert!(p.is_match(m), "CYCGRG must match {m}");
        }
        for m in ["CACGAG", "CCCGCC", "CTCGCC"] {
            assert!(!p.is_match(m), "CYCGRG must not match {m}");
        }
        let p = iupac_pattern("GGTNACC").unwrap();
        for b in ["A", "C", "G", "T"] {
            assert!(p.is_match(&format!("GGT{b}ACC")));
        }
    }

    #[test]
    fn inv04_cache_same_object() {
        pattern_cache_clear();
        let p1 = iupac_pattern("ATGCAT").unwrap();
        assert!(pattern_cache_contains("ATGCAT"));
        let p2 = iupac_pattern("ATGCAT").unwrap();
        assert!(Arc::ptr_eq(&p1, &p2));
        let p3 = iupac_pattern("atgcat").unwrap();
        assert!(Arc::ptr_eq(&p1, &p3));
    }

    #[test]
    fn inv04_rejects_non_iupac() {
        assert!(iupac_pattern("GGZCC").is_err());
        assert!(iupac_pattern("").is_err());
        assert!(iupac_pattern("   ").is_err());
    }

    #[test]
    fn iupac_compatible_overlap() {
        assert!(iupac_compatible('A', 'A'));
        assert!(iupac_compatible('A', 'N'));
        assert!(!iupac_compatible('A', 'C'));
        assert!(!iupac_compatible('R', 'Y'));
        assert!(iupac_compatible('R', 'M'));
        assert!(!iupac_compatible('A', '-'));
    }
}
