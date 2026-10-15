//! The draw list as a vello scene: each ink given its colour, text set by
//! the same typesetter that measured it.

use parley::PositionedLayoutItem;
use underdraw_layout::{Align, Draw, Font, Ink, Page, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Circle, Line, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};

use crate::text::Typesetter;

/// The colour of each ink in a wireframe. Gray throughout, but for what
/// was assumed and the person's notes.
pub fn colour(ink: Ink) -> Color {
    let rgb = match ink {
        Ink::Text | Ink::Strong => [0x22, 0x22, 0x22],
        Ink::Muted => [0x6A, 0x6A, 0x6A],
        Ink::Line => [0xD0, 0xD0, 0xD0],
        Ink::Fill => [0xEC, 0xEC, 0xEC],
        Ink::Paper => [0xFF, 0xFF, 0xFF],
        Ink::Assumed => [0xF0, 0x8C, 0x00],
        Ink::Note => [0x1F, 0x6F, 0xEB],
        Ink::Answered => [0x1A, 0x9E, 0x4B],
        Ink::Scrim => return Color::from_rgba8(0x00, 0x00, 0x00, 0x59),
    };
    Color::from_rgb8(rgb[0], rgb[1], rgb[2])
}

/// `page` drawn at `scale` pixels per point.
pub fn scene(page: &Page, typesetter: &mut Typesetter, scale: f64) -> Scene {
    let mut scene = Scene::new();
    let at = Affine::scale(scale);
    let paper = RoundedRect::new(0.0, 0.0, f64::from(page.width), f64::from(page.height), 0.0);
    scene.fill(Fill::NonZero, at, colour(Ink::Paper), None, &paper);
    draws(&mut scene, at, &page.draws, colour, typesetter);
    scene
}

/// `list` painted into `scene`, its points taken through `at`, each ink in
/// the colour `colour` gives it: a wireframe's, or the gallery's chrome's.
pub fn draws(
    scene: &mut Scene,
    at: Affine,
    list: &[Draw],
    colour: fn(Ink) -> Color,
    typesetter: &mut Typesetter,
) {
    for draw in list {
        paint(scene, at, draw, colour, typesetter);
    }
}

fn paint(scene: &mut Scene, at: Affine, draw: &Draw, colour: fn(Ink) -> Color, typesetter: &mut Typesetter) {
    match draw {
        Draw::Box { rect, radius, fill, stroke, dashed } => {
            let shape = rounded(*rect, *radius);
            if let Some(ink) = fill {
                scene.fill(Fill::NonZero, at, colour(*ink), None, &shape);
            }
            if let Some(ink) = stroke {
                let mut line = Stroke::new(1.0);
                if *dashed {
                    line = line.with_dashes(0.0, [4.0, 3.0]);
                }
                scene.stroke(&line, at, colour(*ink), None, &shape);
            }
        }
        Draw::Text { rect, text, font, ink, max_lines, align, underline, strike } => {
            let words = Words { font: *font, ink: colour(*ink), max_lines: *max_lines, align: *align };
            set(scene, at, *rect, text, words, (*underline, *strike), typesetter);
        }
        Draw::Cross { rect, ink } => {
            let (x0, y0, x1, y1) = corners(*rect);
            let line = Stroke::new(1.0);
            scene.stroke(&line, at, colour(*ink), None, &Line::new((x0, y0), (x1, y1)));
            scene.stroke(&line, at, colour(*ink), None, &Line::new((x0, y1), (x1, y0)));
        }
        Draw::Line { from, to, ink } => {
            let line =
                Line::new((f64::from(from[0]), f64::from(from[1])), (f64::from(to[0]), f64::from(to[1])));
            scene.stroke(&Stroke::new(1.0), at, colour(*ink), None, &line);
        }
        Draw::Clip { rect, radius, draws } => {
            scene.push_clip_layer(Fill::NonZero, at, &rounded(*rect, *radius));
            for draw in draws {
                paint(scene, at, draw, colour, typesetter);
            }
            scene.pop_layer();
        }
        Draw::Tag { rect, text, ink } => {
            let (x0, y0, x1, y1) = corners(*rect);
            let circle = Circle::new(((x0 + x1) / 2.0, (y0 + y1) / 2.0), (x1 - x0) / 2.0);
            scene.fill(Fill::NonZero, at, colour(*ink), None, &circle);
            scene.stroke(&Stroke::new(1.5), at, colour(Ink::Paper), None, &circle);
            let font = Font { size: 11.0, weight: 700, line_height: rect.height };
            let words = Words { font, ink: colour(Ink::Paper), max_lines: Some(1), align: Align::Center };
            set(scene, at, *rect, text, words, (false, false), typesetter);
        }
    }
}

/// How much wider than measured a line is set when it is drawn.
const HAIR: f32 = 1.0;

struct Words {
    font: Font,
    ink: Color,
    max_lines: Option<u32>,
    align: Align,
}

/// `text` set in `rect`, its lines past `max_lines` left out.
fn set(
    scene: &mut Scene,
    at: Affine,
    rect: Rect,
    text: &str,
    words: Words,
    (underline, strike): (bool, bool),
    typesetter: &mut Typesetter,
) {
    // A hair wider than measured, so rounding never breaks a line the
    // measurement kept whole; a centred or end-aligned line is moved back by
    // its share of the hair, so it sits in `rect` and not in the wider line.
    let layout = typesetter.set(text, words.font, Some(rect.width + HAIR), words.align);
    let back = match words.align {
        Align::Start => 0.0,
        Align::Center => HAIR / 2.0,
        Align::End => HAIR,
    };
    let brush = words.ink;
    let origin = at * Affine::translate((f64::from(rect.x - back), f64::from(rect.y)));
    for (index, line) in layout.lines().enumerate() {
        if words.max_lines.is_some_and(|max| index >= max as usize) {
            break;
        }
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let (x, baseline) = (run.offset(), run.baseline());
            let font = run.run().font();
            let glyphs =
                run.positioned_glyphs().map(|glyph| vello::Glyph { id: glyph.id, x: glyph.x, y: glyph.y });
            let mut coords = Vec::new();
            for coord in run.run().normalized_coords() {
                coords.push(coord.to_bits());
            }
            scene
                .draw_glyphs(font)
                .font_size(run.run().font_size())
                .normalized_coords(&coords)
                .transform(origin)
                .brush(brush)
                .draw(Fill::NonZero, glyphs);
            let decoration = |y: f32| {
                Line::new((f64::from(x), f64::from(y)), (f64::from(x + run.advance()), f64::from(y)))
            };
            if underline {
                scene.stroke(&Stroke::new(1.0), origin, brush, None, &decoration(baseline + 2.0));
            }
            if strike {
                scene.stroke(
                    &Stroke::new(1.0),
                    origin,
                    brush,
                    None,
                    &decoration(baseline - words.font.size * 0.3),
                );
            }
        }
    }
}

fn rounded(rect: Rect, radius: f32) -> RoundedRect {
    let (x0, y0, x1, y1) = corners(rect);
    RoundedRect::new(x0, y0, x1, y1, f64::from(radius))
}

fn corners(rect: Rect) -> (f64, f64, f64, f64) {
    let (x, y) = (f64::from(rect.x), f64::from(rect.y));
    (x, y, x + f64::from(rect.width), y + f64::from(rect.height))
}
