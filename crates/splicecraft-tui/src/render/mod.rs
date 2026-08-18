//! Pure renderers: `Record` → lines. Ratatui paints styled variants.

mod canvas;
mod map;
mod sequence;

pub(crate) use canvas::{BrailleCanvas, CharCanvas};
pub use canvas::lines_contain_braille;
pub use map::{MapOptions, feature_label_bp, render_map, render_map_styled};
pub use sequence::{SeqView, render_sequence, render_sequence_styled};
