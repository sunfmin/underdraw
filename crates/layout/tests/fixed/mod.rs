//! A stand-in for the system's fonts: every character is half the font
//! size wide, so the geometry is exact.

use underdraw_layout::{Font, Measure, Size};

pub struct Fixed;

impl Measure for Fixed {
    fn text(&mut self, text: &str, font: Font, max_width: Option<f32>) -> Size {
        let advance = font.size / 2.0;
        let one_line = text.chars().count() as f32 * advance;
        let width = match max_width {
            Some(max) => one_line.min(max.max(advance)),
            None => one_line,
        };
        let lines = if width > 0.0 { (one_line / width).ceil().max(1.0) } else { 1.0 };
        Size { width, height: lines * font.line_height }
    }
}
