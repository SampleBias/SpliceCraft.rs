//! Startup splash: greyscale B-DNA helix + `SpliceCraft.rs` banner.
//!
//! Geometry follows upstream `SplashScreen` (right-handed B-DNA, 1.78 pitch /
//! diameter, 127° strand offset). Colour is greyscale instead of the rainbow
//! palette. Any key dismisses; `--no-splash` / `SPLICECRAFT_NO_SPLASH` skips.

use std::f64::consts::PI;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::WELCOME_TITLE;
use crate::render::{BrailleCanvas, CharCanvas};

/// Helix rotation rate (upstream `_HELIX_TURNS_PER_SECOND`).
pub(crate) const HELIX_TURNS_PER_SECOND: f64 = 0.55;
/// Frame cadence while the splash is up (upstream 25 FPS).
pub(crate) const HELIX_TICK_MS: u64 = 40;

const GOLD: Color = Color::Rgb(255, 215, 0);
const LOGO_FG: Color = Color::White;
const TAGLINE_FG: Color = Color::Rgb(230, 230, 230);
const CREDIT_FG: Color = Color::White;

/// Cosmic figlet, 125 cols × 6 rows. Used when the terminal is wide enough.
const LOGO_COSMIC: &str = concat!(
    r#" .::::::.::::::::::. :::     :::  .,-::::: .,::::::   .,-::::: :::::::..    :::.    .-:::::':::::::::::::::::::..   .::::::."#,
    "\n",
    r#";;;`    ` `;;;```.;;;;;;     ;;;,;;;'````' ;;;;'''' ,;;;'````' ;;;;``;;;;   ;;`;;   ;;;'''' ;;;;;;;;'''';;;;``;;;; ;;;`    `"#,
    "\n",
    r#"'[==/[[[[, `]]nnn]]' [[[     [[[[[[         [[cccc  [[[         [[[,/[[['  ,[[ '[[, [[[,,==      [[      [[[,/[[[' '[==/[[[[,"#,
    "\n",
    r#"  '''    $  $$$""    $$'     $$$$$$         $$""""  $$$         $$$$$$c   c$$$cc$$$c`$$$"``      $$      $$$$$$c     '''    $"#,
    "\n",
    r#" 88b    dP  888o    o88oo,.__888`88bo,__,o, 888oo,__`88bo,__,o, 888b "88bo,888   888,888         88, d8b 888b "88bo,88b    dP"#,
    "\n",
    r#"  "YMmMY"   YMMMb   """"YUMMMMMM  "YUMMMMMP"""""YUMMM "YUMMMMMP"MMMM   "W" YMM   ""` "MM,        MMM YMP MMMM   "W"  "YMmMY""#,
    "\n",
);
const LOGO_COSMIC_WIDTH: usize = 125;

/// "big" figlet, 63 cols × 8 rows.
const LOGO_BIG: &str = concat!(
    r#"  _____       _ _           _____            __ _"#,
    "\n",
    r#" / ____|     | (_)         / ____|          / _| |"#,
    "\n",
    r#"| (___  _ __ | |_  ___ ___| |     _ __ __ _| |_| |_   _ __ ___"#,
    "\n",
    r#" \___ \| '_ \| | |/ __/ _ \ |    | '__/ _` |  _| __| | '__/ __|"#,
    "\n",
    r#" ____) | |_) | | | (_|  __/ |____| | | (_| | | | |_ _| |  \__ \"#,
    "\n",
    r#"|_____/| .__/|_|_|\___\___|\_____|_|  \__,_|_|  \__(_)_|  |___/"#,
    "\n",
    r#"       | |"#,
    "\n",
    r#"       |_|"#,
    "\n",
);
const LOGO_BIG_WIDTH: usize = 63;

/// "small" figlet, 52 cols × 5 rows.
const LOGO_SMALL: &str = concat!(
    r#" ___      _ _         ___           __ _"#,
    "\n",
    r#"/ __|_ __| (_)__ ___ / __|_ _ __ _ / _| |_   _ _ ___"#,
    "\n",
    r#"\__ \ '_ \ | / _/ -_) (__| '_/ _` |  _|  _|_| '_(_-<"#,
    "\n",
    r#"|___/ .__/_|_\__\___|\___|_| \__,_|_|  \__(_)_| /__/"#,
    "\n",
    r#"    |_|"#,
    "\n",
);
const LOGO_SMALL_WIDTH: usize = 52;

/// 5-pixel disk (Euclidean radius 2) — chunky ribbon like upstream.
const DISK: [(i32, i32); 13] = [
    (-2, 0),
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -2),
    (0, -1),
    (0, 0),
    (0, 1),
    (0, 2),
    (1, -1),
    (1, 0),
    (1, 1),
    (2, 0),
];

/// Paint the splash into `frame`. `phase` is radians of helix rotation.
pub fn draw_splash(frame: &mut Frame<'_>, phase: f64) {
    let area = frame.area();
    let lines = compose_splash(area.width, area.height, phase);
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(Color::Black)),
        area,
    );
}

/// Pure compose for tests (`TestBackend` or string asserts).
#[must_use]
pub fn compose_splash(width: u16, height: u16, phase: f64) -> Vec<Line<'static>> {
    let w = width as usize;
    let h = height as usize;
    if w < 8 || h < 4 {
        return vec![Line::from(Span::styled(
            WELCOME_TITLE,
            Style::default().fg(LOGO_FG).add_modifier(Modifier::BOLD),
        ))];
    }
    let mut dots = BrailleCanvas::new(w, h);
    let mut text = CharCanvas::new(w, h);
    draw_helix(&mut dots, w, h, phase);
    draw_logo(&mut text, w, h);
    dots.to_styled_lines(&text, false, Color::DarkGray)
}

/// Helix phase from elapsed splash time.
#[must_use]
pub fn helix_phase(elapsed_secs: f64) -> f64 {
    (elapsed_secs * 2.0 * PI * HELIX_TURNS_PER_SECOND) % (2.0 * PI)
}

fn draw_helix(bc: &mut BrailleCanvas, w: usize, h: usize, phase_anim: f64) {
    let px_w = (w * 2) as f64;
    let px_h = (h * 4) as f64;
    let d_len = px_w.hypot(px_h);
    if d_len < 8.0 {
        return;
    }
    let ux = px_w / d_len;
    let uy = -px_h / d_len;
    let vx = -uy;
    let vy = ux;

    let amp = (px_w.min(px_h) * 0.22).max(20.0);
    let period = (2.0 * amp * 1.78).max(60.0);
    let delta_phi = 0.706 * PI;
    let gap_px = 6.0;
    let n_hues = 24usize;
    let n_samples = d_len as i32;
    let sx = 0.0;
    let sy = px_h - 1.0;

    for i in 0..=n_samples {
        let t = f64::from(i) * d_len / f64::from(n_samples);
        let phase = 2.0 * PI * t / period + phase_anim;
        let cx_axis = sx + t * ux;
        let cy_axis = sy + t * uy;
        let sa = phase.sin();
        let sb = (phase + delta_phi).sin();
        let ax = cx_axis + amp * sa * vx;
        let ay = cy_axis + amp * sa * vy;
        let bx = cx_axis + amp * sb * vx;
        let by = cy_axis + amp * sb * vy;
        let za = phase.cos();
        let zb = (phase + delta_phi).cos();
        let hue = ((t * n_hues as f64 / d_len) as usize) % n_hues;
        let a_color = grey_at(hue, za < zb);
        let b_color = grey_at(hue, zb <= za);
        let near_crossing = (ax - bx).hypot(ay - by) < gap_px;
        let skip_a = near_crossing && za < zb;
        let skip_b = near_crossing && zb < za;
        if !skip_a {
            stamp_disk(bc, ax as i32, ay as i32, a_color);
        }
        if !skip_b {
            stamp_disk(bc, bx as i32, by as i32, b_color);
        }
    }

    let rungs_per_turn = 10.0;
    let rung_dt = period / rungs_per_turn;
    let n_rungs = (d_len / rung_dt) as i32;
    for j in 0..=n_rungs {
        let t = f64::from(j) * rung_dt;
        let phase = 2.0 * PI * t / period + phase_anim;
        let cx_axis = sx + t * ux;
        let cy_axis = sy + t * uy;
        let sa = phase.sin();
        let sb = (phase + delta_phi).sin();
        let ax = cx_axis + amp * sa * vx;
        let ay = cy_axis + amp * sa * vy;
        let bx = cx_axis + amp * sb * vx;
        let by = cy_axis + amp * sb * vy;
        let dist = (ax - bx).hypot(ay - by);
        if dist <= 10.0 {
            continue;
        }
        let hue = ((t * n_hues as f64 / d_len) as usize) % n_hues;
        let color = grey_at(hue, true);
        let n_steps = dist as i32 + 1;
        for k in 0..=n_steps {
            let f = f64::from(k) / f64::from(n_steps);
            let px = ax + (bx - ax) * f;
            let py = ay + (by - ay) * f;
            bc.set_pixel_colored(px as i32, py as i32, Some(color));
            bc.set_pixel_colored(px as i32, py as i32 + 1, Some(color));
        }
    }
}

fn stamp_disk(bc: &mut BrailleCanvas, x: i32, y: i32, color: Color) {
    for (dx, dy) in DISK {
        bc.set_pixel_colored(x + dx, y + dy, Some(color));
    }
}

/// Greyscale analog of upstream's 24-hue rainbow. `dim` is the back strand.
fn grey_at(i: usize, dim: bool) -> Color {
    let t = i as f64 / 24.0;
    let v = 0.38 + 0.62 * (0.5 + 0.5 * (2.0 * PI * t).cos());
    let scale = if dim { 0.42 } else { 1.0 };
    let b = (v * scale * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::Rgb(b, b, b)
}

fn draw_logo(tc: &mut CharCanvas, w: usize, h: usize) {
    let (logo, logo_w) = pick_logo(w);
    let lines: Vec<&str> = logo.lines().filter(|l| !l.is_empty()).collect();
    let logo_h = lines.len();
    let mut row_off = h.saturating_sub(logo_h).saturating_sub(8) / 2;
    let info_h = 5usize;
    if row_off + logo_h + 2 + info_h > h {
        row_off = h.saturating_sub(logo_h + 2 + info_h);
    }
    let col_off = w.saturating_sub(logo_w) / 2;

    for (i, ln) in lines.iter().enumerate() {
        let row = row_off + i;
        if row >= h {
            break;
        }
        for (j, ch) in ln.chars().enumerate() {
            if ch != ' ' {
                tc.put_colored((col_off + j) as i32, row as i32, ch, Some(LOGO_FG));
            }
        }
    }

    let version = env!("CARGO_PKG_VERSION");
    let credit = format!("{WELCOME_TITLE}   ·   v{version}");
    let info = [
        (
            "·  I n - T e r m i n a l   P l a s m i d   W o r k b e n c h  ·",
            TAGLINE_FG,
        ),
        ("", TAGLINE_FG),
        (credit.as_str(), CREDIT_FG),
        ("", TAGLINE_FG),
        ("press any key to begin", GOLD),
    ];
    let info_row = row_off + logo_h + 2;
    for (i, (line, color)) in info.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let row = info_row + i;
        if row >= h {
            break;
        }
        let col = w.saturating_sub(line.chars().count()) / 2;
        for (j, ch) in line.chars().enumerate() {
            if ch != ' ' {
                tc.put_colored((col + j) as i32, row as i32, ch, Some(*color));
            }
        }
    }
}

fn pick_logo(width: usize) -> (&'static str, usize) {
    if width >= LOGO_COSMIC_WIDTH + 4 {
        (LOGO_COSMIC, LOGO_COSMIC_WIDTH)
    } else if width >= LOGO_BIG_WIDTH + 4 {
        (LOGO_BIG, LOGO_BIG_WIDTH)
    } else if width >= LOGO_SMALL_WIDTH + 4 {
        (LOGO_SMALL, LOGO_SMALL_WIDTH)
    } else {
        (WELCOME_TITLE, WELCOME_TITLE.len())
    }
}

/// True when helix cells are greyscale (gold prompt is the only chroma).
#[must_use]
pub fn splash_colors_are_greyscale(area: Rect, lines: &[Line<'_>]) -> bool {
    let _ = area;
    for line in lines {
        for span in &line.spans {
            if let Color::Rgb(r, g, b) = span.style.fg.unwrap_or(Color::Reset) {
                let chroma = r.abs_diff(g).max(g.abs_diff(b)).max(r.abs_diff(b));
                if chroma > 8 && !is_gold(r, g, b) {
                    return false;
                }
            }
        }
    }
    true
}

fn is_gold(r: u8, g: u8, b: u8) -> bool {
    r.abs_diff(255) < 8 && g.abs_diff(215) < 8 && b.abs_diff(0) < 8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::lines_contain_braille;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn splash_text(width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| draw_splash(frame, 0.0))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn lines_plain(width: u16, height: u16) -> Vec<String> {
        compose_splash(width, height, 0.0)
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|s| s.content.into_owned())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn splash_names_splicecraft_rs() {
        let text = splash_text(80, 24);
        assert!(
            text.contains(WELCOME_TITLE),
            "splash must say SpliceCraft.rs:\n{text}"
        );
        assert!(
            text.contains("press any key to begin"),
            "missing begin prompt:\n{text}"
        );
        assert!(
            text.contains("P l a s m i d") || text.contains("Plasmid"),
            "missing workbench tagline:\n{text}"
        );
        assert!(
            !text.to_ascii_lowercase().contains("python package"),
            "must not present as a Python wrapper:\n{text}"
        );
    }

    #[test]
    fn splash_helix_is_braille_and_greyscale() {
        let lines = compose_splash(80, 24, 0.4);
        let plain: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        assert!(
            lines_contain_braille(&plain),
            "expected braille helix, got:\n{}",
            plain.join("\n")
        );
        assert!(
            splash_colors_are_greyscale(Rect::new(0, 0, 80, 24), &lines),
            "helix must stay grey / black / white (gold prompt allowed)"
        );
        assert!(
            !plain.join("\n").contains("python"),
            "splash chrome is SpliceCraft.rs"
        );
    }

    #[test]
    fn splash_fits_narrow_terminal() {
        let text = splash_text(40, 12);
        assert!(
            text.contains(WELCOME_TITLE),
            "narrow splash still names the app:\n{text}"
        );
        assert!(
            text.contains("press any key to begin") || text.contains("begin"),
            "narrow splash keeps a continue prompt:\n{text}"
        );
        let _ = lines_plain(40, 12);
    }

    #[test]
    fn wide_splash_uses_cosmic_letterforms() {
        let text = splash_text(140, 32);
        assert!(
            text.contains("YMmMY") || text.contains("SpliceCraft.rs"),
            "wide splash should use cosmic figlet or the title:\n{text}"
        );
    }
}
