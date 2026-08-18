//! Visual chrome: feature-type colors and resolve order (upstream theme).
//!
//! Paint-only. Does not touch scan / wrap / persist. Contract: `docs/theme.md`.

use ratatui::style::Color;
use splicecraft_core::Feature;

/// Near-black panel fill when focused (upstream `#0c0c0c`).
pub const FOCUS_BG: Color = Color::Rgb(12, 12, 12);
/// Unfocused panel / screen background.
pub const PANEL_BG: Color = Color::Black;
/// Focused pane border / primary accent (Textual `$primary`-ish cyan).
pub const PRIMARY: Color = Color::Rgb(0, 200, 220);
/// Dim primary for menu / library header bars.
pub const PRIMARY_DARK: Color = Color::Rgb(0, 45, 70);
/// Unfocused border.
pub const BORDER_DIM: Color = Color::Rgb(50, 50, 55);
/// Default body text.
pub const TEXT: Color = Color::Rgb(170, 170, 170);
/// Menu labels (upstream light text on dark bar).
pub const MENU_FG: Color = Color::Rgb(210, 220, 230);
/// Bright text on primary chips / selected rows.
pub const TEXT_ON_PRIMARY: Color = Color::Black;
/// Restriction-site / enzyme accent.
pub const ENZYME_ACCENT: Color = Color::Magenta;
/// CDS amino-acid lane (upstream bright green).
pub const AA_GREEN: Color = Color::Rgb(80, 255, 100);
/// Cursor caret.
pub const CARET: Color = Color::Yellow;
/// Warning / collision border.
pub const WARN: Color = Color::Yellow;
/// Destructive overlay border.
pub const DANGER: Color = Color::Red;
/// Braille backbone (upstream crisp white ring).
pub const BRAILLE_FG: Color = Color::White;
/// Footer shortcuts (upstream yellow-on-black Footer).
pub const FOOTER_FG: Color = Color::Rgb(240, 220, 60);
pub const FOOTER_BG: Color = Color::Black;

/// Upstream `LibraryPanel` / `FeatureSidebar` width target.
pub const SIDE_PANE_COLS: u16 = 32;
/// Upstream `SequencePanel` height target.
pub const SEQUENCE_ROWS: u16 = 14;

/// Upstream `_FEATURE_PALETTE` xterm-256 indices.
pub const FEATURE_PALETTE_XTERM: &[u8] = &[
    39, 118, 208, 213, 51, 220, 196, 46, 201, 129, 166, 33, 226, 160, 87, 105, 154, 203, 81, 185,
];

/// Upstream `_DEFAULT_TYPE_COLORS` (hex).
pub const DEFAULT_TYPE_COLORS: &[(&str, &str)] = &[
    ("CDS", "#FFA500"),
    ("gene", "#FFD700"),
    ("mRNA", "#FFA07A"),
    ("tRNA", "#FF69B4"),
    ("rRNA", "#FF1493"),
    ("ncRNA", "#DA70D6"),
    ("misc_RNA", "#BA55D3"),
    ("promoter", "#00CED1"),
    ("terminator", "#DC143C"),
    ("RBS", "#00FF7F"),
    ("polyA_signal", "#FF6347"),
    ("regulatory", "#7FFFD4"),
    ("5'UTR", "#87CEEB"),
    ("3'UTR", "#4682B4"),
    ("intron", "#A9A9A9"),
    ("exon", "#90EE90"),
    ("operon", "#DDA0DD"),
    ("primer_bind", "#00BFFF"),
    ("protein_bind", "#F08080"),
    ("misc_binding", "#FF8C00"),
    ("repeat_region", "#CD853F"),
    ("LTR", "#8B4513"),
    ("mobile_element", "#8B008B"),
    ("rep_origin", "#9370DB"),
    ("oriT", "#BA55D3"),
    ("sig_peptide", "#ADFF2F"),
    ("mat_peptide", "#9ACD32"),
    ("transit_peptide", "#7CFC00"),
    ("propeptide", "#6B8E23"),
    ("misc_feature", "#20B2AA"),
    ("misc_recomb", "#48D1CC"),
    ("stem_loop", "#FF4500"),
    ("variation", "#800080"),
];

/// Footer shortcut strip (toast replaces this while active).
pub const FOOTER_SHORTCUTS: &str =
    "^q Quit  f Fetch  ^o Open  ^s Save  ^n New  ^a Select all  ^p Primers  ^b BLAST  ? Help";

/// Darken an RGB color for table-cell backgrounds (readable black text on top).
#[must_use]
pub fn darken(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            ((f32::from(r) * factor) as u8).max(8),
            ((f32::from(g) * factor) as u8).max(8),
            ((f32::from(b) * factor) as u8).max(8),
        ),
        other => other,
    }
}

/// Convert an xterm-256 index to RGB (default xterm cube / greyscale).
#[must_use]
pub fn xterm_index_to_rgb(idx: u8) -> (u8, u8, u8) {
    let n = u16::from(idx);
    if n < 16 {
        const ANSI16: [(u8, u8, u8); 16] = [
            (0, 0, 0),
            (128, 0, 0),
            (0, 128, 0),
            (128, 128, 0),
            (0, 0, 128),
            (128, 0, 128),
            (0, 128, 128),
            (192, 192, 192),
            (128, 128, 128),
            (255, 0, 0),
            (0, 255, 0),
            (255, 255, 0),
            (0, 0, 255),
            (255, 0, 255),
            (0, 255, 255),
            (255, 255, 255),
        ];
        return ANSI16[n as usize];
    }
    if n < 232 {
        let n = n - 16;
        let r = n / 36;
        let g = (n % 36) / 6;
        let b = n % 6;
        let level = |v: u16| -> u8 { if v == 0 { 0 } else { (55 + v * 40) as u8 } };
        return (level(r), level(g), level(b));
    }
    let v = (8 + (n - 232) * 10) as u8;
    (v, v, v)
}

/// Parse `#RGB` / `#RRGGBB` / `color(N)` / plain `N` into a [`Color`].
#[must_use]
pub fn parse_color_input(raw: &str) -> Option<Color> {
    let s = raw.trim();
    if s.is_empty() || s.contains('[') || s.contains(']') {
        return None;
    }
    if let Some(rest) = s.strip_prefix("color(").and_then(|r| r.strip_suffix(')')) {
        let idx: u8 = rest.trim().parse().ok()?;
        let (r, g, b) = xterm_index_to_rgb(idx);
        return Some(Color::Rgb(r, g, b));
    }
    if let Ok(idx) = s.parse::<u8>()
        && s.chars().all(|c| c.is_ascii_digit())
    {
        let (r, g, b) = xterm_index_to_rgb(idx);
        return Some(Color::Rgb(r, g, b));
    }
    let hex = s.strip_prefix('#').unwrap_or(s);
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

/// Built-in hex for a GenBank-style feature type.
#[must_use]
pub fn default_type_color(kind: &str) -> Option<Color> {
    DEFAULT_TYPE_COLORS
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(kind))
        .and_then(|(_, hex)| parse_color_input(hex))
}

/// Palette fallback (upstream `_FEATURE_PALETTE[0]` → `color(39)`).
#[must_use]
pub fn palette_fallback() -> Color {
    let (r, g, b) = xterm_index_to_rgb(FEATURE_PALETTE_XTERM[0]);
    Color::Rgb(r, g, b)
}

/// Resolve paint color: qualifier → type default → palette[0].
#[must_use]
pub fn resolve_feature_color(
    kind: &str,
    color_qualifier: Option<&str>,
    user_defaults: Option<&[(String, String)]>,
) -> Color {
    if let Some(raw) = color_qualifier
        && let Some(c) = parse_color_input(raw)
    {
        return c;
    }
    if let Some(map) = user_defaults {
        for (k, v) in map {
            if k.eq_ignore_ascii_case(kind)
                && let Some(c) = parse_color_input(v)
            {
                return c;
            }
        }
    }
    default_type_color(kind).unwrap_or_else(palette_fallback)
}

/// Color for a [`Feature`] (reads `/color` qualifier when present).
#[must_use]
pub fn feature_paint_color(feat: &Feature) -> Color {
    let q = feat
        .qualifiers
        .get("color")
        .map(String::as_str)
        .or_else(|| feat.qualifiers.get("Color").map(String::as_str));
    resolve_feature_color(&feat.kind, q, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_colors_match_upstream_spot_checks() {
        assert_eq!(parse_color_input("#FFA500"), Some(Color::Rgb(255, 165, 0)));
        assert_eq!(default_type_color("CDS"), Some(Color::Rgb(255, 165, 0)));
        assert_eq!(
            default_type_color("promoter"),
            Some(Color::Rgb(0, 206, 209))
        );
        assert_eq!(
            default_type_color("terminator"),
            Some(Color::Rgb(220, 20, 60))
        );
        assert_eq!(
            default_type_color("primer_bind"),
            Some(Color::Rgb(0, 191, 255))
        );
        assert_eq!(
            default_type_color("rep_origin"),
            Some(Color::Rgb(147, 112, 219))
        );
        assert_eq!(
            default_type_color("misc_feature"),
            Some(Color::Rgb(32, 178, 170))
        );
    }

    #[test]
    fn resolve_order_qualifier_then_type_then_palette() {
        let from_q = resolve_feature_color("CDS", Some("#112233"), None);
        assert_eq!(from_q, Color::Rgb(0x11, 0x22, 0x33));

        let from_type = resolve_feature_color("CDS", None, None);
        assert_eq!(from_type, Color::Rgb(255, 165, 0));

        let from_palette = resolve_feature_color("not_a_real_kind", None, None);
        assert_eq!(from_palette, palette_fallback());
        assert_eq!(from_palette, Color::Rgb(0x00, 0xAF, 0xFF));
    }

    #[test]
    fn short_hex_and_color_n() {
        assert_eq!(
            parse_color_input("#8c8"),
            Some(Color::Rgb(0x88, 0xcc, 0x88))
        );
        assert_eq!(
            parse_color_input("color(39)"),
            Some(Color::Rgb(0x00, 0xAF, 0xFF))
        );
    }

    #[test]
    fn reject_markup_injection() {
        assert!(parse_color_input("[red]").is_none());
    }
}
