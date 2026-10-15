//! Lays a screen in one state out as a wireframe: where each node goes, and
//! the boxes, text and placeholders that draw it, in grays only. Text is
//! measured by whoever renders it, through [`Measure`]; nothing here reads a
//! font file or draws a pixel.

extern crate flux_alloc;
extern crate flux_core;

mod check;
mod look;
mod place;
mod tokens;

use serde::Serialize;
use serde_json::Value;
use underdraw_format::{Lang, Node, Screen};

pub use check::{CHECKED_WIDTHS, check};

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Font {
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// The few inks a wireframe is drawn in; the renderer gives each its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ink {
    /// Text and outlines that matter.
    Text,
    /// Secondary text.
    Muted,
    /// Hairlines and quiet outlines.
    Line,
    /// Placeholder areas: images, maps, skeletons.
    Fill,
    /// Filled primary actions.
    Strong,
    /// The page itself, and text on a strong fill.
    Paper,
    /// What the agent guessed, for the person to confirm.
    Assumed,
    /// The see-through dark a dialog's backdrop lays over the page.
    Scrim,
    /// A note the person wrote, open: never part of a wireframe, only of
    /// the gallery drawn around it.
    Note,
    /// A note the agent answered last, for the person to read.
    Answered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Start,
    Center,
    End,
}

/// One thing to draw, in page coordinates (points), in order.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "draw", rename_all = "lowercase")]
pub enum Draw {
    /// A rectangle; its radius is already no more than half its shorter side.
    Box { rect: Rect, radius: f32, fill: Option<Ink>, stroke: Option<Ink>, dashed: bool },
    /// Text set inside `rect`, wrapped to its width, at most `max_lines`.
    Text {
        rect: Rect,
        text: String,
        font: Font,
        ink: Ink,
        max_lines: Option<u32>,
        align: Align,
        underline: bool,
        strike: bool,
    },
    /// The two diagonals of `rect`: an image that is not drawn.
    Cross { rect: Rect, ink: Ink },
    /// A straight line.
    Line { from: [f32; 2], to: [f32; 2], ink: Ink },
    /// `draws`, cut to `rect` with corners of `radius`.
    Clip { rect: Rect, radius: f32, draws: Vec<Draw> },
    /// A tag on a node: a filled circle in `ink` with `text` on it.
    Tag { rect: Rect, text: String, ink: Ink },
}

/// Where one node of the screen went.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Placed {
    pub id: String,
    pub component: String,
    #[serde(rename = "box")]
    pub rect: Rect,
    /// The number tagged on it in the picture, when a person can act on it.
    pub label: Option<u32>,
    pub assumed: Option<String>,
}

impl Placed {
    /// Where its orange "?" is drawn, when it was guessed: a click there is
    /// on its assumption.
    pub fn guess_tag(&self) -> Option<Rect> {
        self.assumed.as_ref().map(|_| place::guess_tag(self.rect))
    }
}

/// A screen laid out: its size and what to draw.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub draws: Vec<Draw>,
    /// Every node with an id, in document order.
    pub nodes: Vec<Placed>,
    /// The ids of nodes floated over others (overlays) that run off the
    /// page, so the picture cuts them.
    pub cut: Vec<String>,
}

/// Sets text, for layout: the size `text` takes in `font`, wrapped to
/// `max_width` (as narrow as its longest word when 0; on one line when none).
pub trait Measure {
    fn text(&mut self, text: &str, font: Font, max_width: Option<f32>) -> Size;
}

/// `root` laid out `width` points wide, its height whatever its content
/// takes, with `tokens` (tokens.json) for spaces, radii and fonts, and each
/// component drawn as `screen` (the screen `root` is a state of) says
/// in the components it may use; a node of a component none of them
/// defines is drawn as a box with its name, and assumed, said in `lang`.
pub fn layout(
    root: &Node,
    tokens: &Value,
    screen: &Screen,
    width: f32,
    lang: Lang,
    measure: &mut dyn Measure,
) -> Result<Page, String> {
    let tokens = tokens::Tokens(tokens);
    let elem = look::Catalog { tokens: &tokens, components: &screen.components, lang }.node(root);
    place::place(&elem, width, measure).map_err(|e| format!("layout failed: {e}"))
}
