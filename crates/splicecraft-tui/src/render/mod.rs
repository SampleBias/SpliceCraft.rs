//! Pure renderers: `Record` → `Vec<String>`. Ratatui only paints the lines.

mod canvas;
mod map;
mod sequence;

pub use canvas::lines_contain_braille;
pub use map::{MapOptions, feature_label_bp, render_map};
pub use sequence::{SeqView, render_sequence};
