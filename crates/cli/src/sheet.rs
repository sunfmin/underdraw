//! Every state of a description at each of [`WIDTHS`], on one sheet:
//! `render --all`. A row a state, in the order `check` lists them, its
//! pages side by side, each under a caption naming its state and width as
//! the gallery does ("default · 390"), on the gallery's desk.

use std::collections::BTreeSet;

use underdraw_format::Lang;
use underdraw_layout::{Align, Draw, Ink, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Rect as Area};
use vello::peniko::Fill;

use crate::chrome::{self, CAPTION, MARGIN};
use crate::draw::{Description, Laid, WIDTHS};
use crate::{paint, text};

/// Every state laid out at each width, and where each page sits.
pub struct Sheet {
    /// The pages, each state's at each width in turn.
    pub pages: Vec<Placed>,
    /// The sheet's size in points.
    pub width: f32,
    pub height: f32,
}

/// One state's page at one width, and where its top left corner is on the
/// sheet, in points.
pub struct Placed {
    pub laid: Laid,
    pub x: f32,
    pub y: f32,
}

/// The sheet drawn: its scene, at what scale, and its size in pixels.
pub struct Drawn {
    pub scene: Scene,
    pub scale: f64,
    pub pixels: [u32; 2],
}

impl Sheet {
    /// Every state of `description` laid out at each of [`WIDTHS`], as one
    /// `render` would lay it out given `drawn` and `lang`; refused as
    /// `render` refuses a description with findings.
    pub fn lay_out(
        description: &Description,
        drawn: Option<&BTreeSet<String>>,
        lang: Lang,
    ) -> anyhow::Result<Sheet> {
        let mut pages = Vec::new();
        let mut y = MARGIN as f32;
        let mut width = MARGIN as f32;
        for state in description.states() {
            let mut x = MARGIN as f32;
            let mut tallest = 0.0f32;
            for page in WIDTHS {
                let laid = description.lay_out(&state.wanted(page, 1), drawn, lang)?;
                tallest = tallest.max(laid.page.height);
                let next = x + laid.page.width + MARGIN as f32;
                pages.push(Placed { laid, x, y: y + CAPTION as f32 });
                x = next;
            }
            width = width.max(x);
            y += CAPTION as f32 + tallest + MARGIN as f32;
        }
        Ok(Sheet { pages, width, height: y })
    }

    /// The sheet at `scale` pixels per point, or less when that would make
    /// a side longer than `max_side` pixels, as one page is.
    pub fn draw(&mut self, scale: u32, max_side: u32) -> anyhow::Result<Drawn> {
        let tallest = f64::from(max_side) / f64::from(self.height.max(self.width).max(1.0));
        let scale = f64::from(scale).min(tallest);
        let pixels =
            [(f64::from(self.width) * scale).ceil() as u32, (f64::from(self.height) * scale).ceil() as u32];
        let mut scene = Scene::new();
        let desk = Area::new(0.0, 0.0, f64::from(self.width), f64::from(self.height));
        scene.fill(Fill::NonZero, Affine::scale(scale), chrome::DESK, None, &desk);
        let mut captions = Vec::new();
        for Placed { laid, x, y } in &mut self.pages {
            let page = laid.scene(scale);
            scene.append(&page, Some(Affine::translate((f64::from(*x) * scale, f64::from(*y) * scale))));
            let rect = Rect { x: *x, y: *y - CAPTION as f32, width: laid.page.width, height: CAPTION as f32 };
            captions.push(caption(rect, format!("{} · {}", laid.shown.name(), laid.page.width)));
        }
        let mut typesetter = text::Typesetter::new(None).map_err(anyhow::Error::msg)?;
        paint::draws(&mut scene, Affine::scale(scale), &captions, chrome::colour, &mut typesetter);
        Ok(Drawn { scene, scale, pixels })
    }
}

/// `text`, a page's caption, in `rect`, as the gallery sets one.
fn caption(rect: Rect, text: String) -> Draw {
    Draw::Text {
        rect,
        text,
        font: chrome::CAPTION_FONT,
        ink: Ink::Muted,
        max_lines: Some(1),
        align: Align::Start,
        underline: false,
        strike: false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../format/tests/airbnb-listing");

    #[test]
    fn every_state_has_a_row_and_each_width_a_column() {
        let description = Description::read(Path::new(DRAFT)).unwrap();
        let mut sheet = Sheet::lay_out(&description, None, Lang::En).unwrap();
        let states = description.states();
        assert_eq!(sheet.pages.len(), states.len() * WIDTHS.len());
        for (row, state) in states.iter().enumerate() {
            let pages = &sheet.pages[row * 2..row * 2 + 2];
            assert_eq!([pages[0].laid.page.width, pages[1].laid.page.width], WIDTHS.map(|w| w as f32));
            assert!(pages.iter().all(|p| &p.laid.shown == state));
            assert_eq!(pages[0].y, pages[1].y, "a state's pages side by side");
            assert!(pages[0].x + pages[0].laid.page.width < pages[1].x, "and apart");
            if row > 0 {
                let above = &sheet.pages[row * 2 - 2..row * 2];
                let bottom = above.iter().map(|p| p.y + p.laid.page.height).fold(0.0, f32::max);
                assert!(pages[0].y - CAPTION as f32 > bottom, "each row under the one before");
            }
        }
        let last = sheet.pages.last().unwrap();
        assert!(sheet.width > last.x + last.laid.page.width);
        let drawn = sheet.draw(2, 4096).unwrap();
        assert!(drawn.pixels[0].max(drawn.pixels[1]) <= 4096, "a tall sheet is drawn smaller");
        assert!(drawn.scale < 2.0);
    }
}
