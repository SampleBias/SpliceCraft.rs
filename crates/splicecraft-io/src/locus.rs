//! GenBank LOCUS sanitise. The display name lives on [`splicecraft_core::Record::name`].

/// NCBI relaxed LOCUS name length (spec is 16).
pub const GB_LOCUS_NAME_MAX: usize = 28;

/// Collapse whitespace, map illegal LOCUS chars to `_`, cap length.
#[must_use]
pub fn sanitize_locus_name(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join("_");
    let mut s: String = collapsed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.len() > GB_LOCUS_NAME_MAX {
        s.truncate(GB_LOCUS_NAME_MAX);
    }
    while s.ends_with('_') {
        s.pop();
    }
    if s.is_empty() { "PLASMID".into() } else { s }
}

/// True when the human name cannot be stored verbatim on the LOCUS line.
#[must_use]
pub fn display_name_needs_comment(display: &str) -> bool {
    let locus = sanitize_locus_name(display);
    !display.is_empty() && display != locus
}
