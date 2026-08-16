//! Shared helpers, sanitizers, and a single time source.

#![forbid(unsafe_code)]

use std::path::{Component, Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Stage that implements this crate's real helpers.
pub const IMPLEMENTATION_STAGE: u8 = 1;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Workspace package version, used by GenBank provenance stamps later.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Wall-clock time source. Every callsite should go through this.
#[must_use]
pub fn now() -> SystemTime {
    SystemTime::now()
}

/// Monotonic clock. Pair with [`now`] so tests can later inject both.
#[must_use]
pub fn monotonic() -> Instant {
    Instant::now()
}

/// Best-effort UTC timestamp (`seconds.millis` since epoch). Avoids extra deps.
#[must_use]
pub fn now_iso() -> String {
    match now().duration_since(UNIX_EPOCH) {
        Ok(d) => format!("{}.{:03}Z", d.as_secs(), d.subsec_millis()),
        Err(_) => "0.000Z".into(),
    }
}

/// Strip control / bidi spoof characters and cap length.
#[must_use]
pub fn sanitize_label(s: &str, max_len: usize) -> String {
    let cleaned: String = s.chars().filter(|c| !is_forbidden_control(*c)).collect();
    let trimmed = cleaned.trim();
    trimmed.chars().take(max_len).collect()
}

/// Plasmid / record name: no path separators, collapsed whitespace, fallback.
#[must_use]
pub fn sanitize_plasmid_name(raw: &str, fallback: &str, max_len: usize) -> String {
    let mut s = raw.replace(['\t', '\n', '\r', '\u{0b}', '\u{0c}'], " ");
    s = s
        .chars()
        .filter(|c| (*c as u32) >= 0x20 && *c != '\u{7f}')
        .collect();
    s = s.replace(['/', '\\'], " ");
    let cleaned = s
        .split_whitespace()
        .filter(|tok| *tok != "." && *tok != "..")
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        return fallback.to_owned();
    }
    let mut out: String = cleaned.chars().take(max_len).collect();
    while out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        fallback.to_owned()
    } else {
        out
    }
}

/// Filename stem: no traversal, no reserved Windows device names.
#[must_use]
pub fn sanitize_filename(raw: &str) -> String {
    let name = Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(raw);
    let mut cleaned: String = name
        .chars()
        .filter(|c| *c != '/' && *c != '\\' && *c != '\0')
        .collect();
    cleaned = cleaned.replace("..", "_");
    if cleaned.is_empty() {
        return "unnamed".into();
    }
    if is_windows_reserved_stem(&cleaned) {
        format!("_{cleaned}")
    } else {
        cleaned
    }
}

/// Reject path components that escape a root (`..`).
#[must_use]
pub fn path_is_safe_under(path: &Path) -> bool {
    !path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Expand `~` for the current user only (`~other` is refused).
#[must_use]
pub fn sanitize_path(p: &str) -> Option<PathBuf> {
    if p.is_empty() {
        return None;
    }
    if p.starts_with('~') && p.len() > 1 && !p[1..].starts_with(['/', '\\']) {
        return None;
    }
    if let Some(rest) = p.strip_prefix("~/") {
        let home = dirs_home()?;
        return Some(home.join(rest));
    }
    if p == "~" {
        return dirs_home();
    }
    Some(PathBuf::from(p))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn is_windows_reserved_stem(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let up = stem.to_ascii_uppercase();
    matches!(
        up.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn is_forbidden_control(c: char) -> bool {
    let u = c as u32;
    (u <= 0x1f)
        || (0x7f..=0x9f).contains(&u)
        || matches!(
            u,
            0x200e
                | 0x200f
                | 0x2028
                | 0x2029
                | 0x202a
                | 0x202b
                | 0x202c
                | 0x202d
                | 0x202e
                | 0x2066
                | 0x2067
                | 0x2068
                | 0x2069
                | 0xfeff
        )
}

/// Natural sort so `pBin2` precedes `pBin10`.
#[must_use]
pub fn natural_sort_key(s: &str) -> Vec<NaturalToken> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() != digit && !buf.is_empty() {
            out.push(token_from(&buf, digit));
            buf.clear();
        }
        digit = c.is_ascii_digit();
        buf.push(c);
    }
    if !buf.is_empty() {
        out.push(token_from(&buf, digit));
    }
    out
}

fn token_from(buf: &str, digit: bool) -> NaturalToken {
    if digit {
        NaturalToken::Num(buf.parse::<u64>().unwrap_or(0))
    } else {
        NaturalToken::Text(buf.to_ascii_lowercase())
    }
}

/// One piece of a [`natural_sort_key`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NaturalToken {
    /// Non-numeric run.
    Text(String),
    /// Integer run.
    Num(u64),
}

const DNA_ALPHABET: &[char] = &[
    'A', 'C', 'G', 'T', 'U', 'R', 'Y', 'W', 'S', 'M', 'K', 'B', 'D', 'H', 'V', 'N',
];

/// Replace DNA-shaped strings so sequences never appear in logs.
#[must_use]
pub fn redact_for_log(s: &str) -> String {
    if looks_like_dna(s) {
        format!(
            "<dna {} bp>",
            s.chars().filter(|c| !c.is_whitespace()).count()
        )
    } else {
        s.to_owned()
    }
}

/// Heuristic: long, mostly IUPAC, no spaces.
#[must_use]
pub fn looks_like_dna(s: &str) -> bool {
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 12 {
        return false;
    }
    let dna = compact
        .chars()
        .filter(|c| DNA_ALPHABET.contains(&c.to_ascii_uppercase()))
        .count();
    dna * 5 >= compact.len() * 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-util");
    }

    #[test]
    fn version_is_semver() {
        assert!(version().split('.').count() >= 3);
    }

    #[test]
    fn now_and_monotonic_advance() {
        let a = monotonic();
        let _ = now();
        let b = monotonic();
        assert!(b >= a);
    }

    #[test]
    fn sanitize_label_strips_controls_and_caps() {
        assert_eq!(sanitize_label("  lacZ\n", 200), "lacZ");
        assert_eq!(sanitize_label("abcdef", 3), "abc");
        assert!(!sanitize_label("good\u{202e}evil", 200).contains('\u{202e}'));
    }

    #[test]
    fn sanitize_plasmid_name_drops_path_seps() {
        assert_eq!(
            sanitize_plasmid_name("../../etc/passwd", "assembly", 60),
            "etc passwd"
        );
        assert_eq!(sanitize_plasmid_name("   ", "assembly", 60), "assembly");
    }

    #[test]
    fn sanitize_filename_blocks_traversal_and_reserved() {
        assert!(!sanitize_filename("../x.gb").contains(".."));
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert!(path_is_safe_under(Path::new("foo/bar.gb")));
        assert!(!path_is_safe_under(Path::new("foo/../secret")));
    }

    #[test]
    fn sanitize_path_refuses_other_user_tilde() {
        assert!(sanitize_path("~daemon/.bashrc").is_none());
        assert!(sanitize_path("").is_none());
    }

    #[test]
    fn natural_sort_pbin2_before_pbin10() {
        let mut names = ["pBin10", "pBin2", "pBin1"];
        names.sort_by_key(|n| natural_sort_key(n));
        assert_eq!(names, ["pBin1", "pBin2", "pBin10"]);
    }

    #[test]
    fn redact_hides_dna_and_keeps_prose() {
        let dna = "ATGCATGCATGCATGC";
        assert_eq!(redact_for_log(dna), "<dna 16 bp>");
        assert_eq!(
            redact_for_log("opened collection Default"),
            "opened collection Default"
        );
    }
}
