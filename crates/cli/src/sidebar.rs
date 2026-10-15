//! The gallery's sidebar: what the person reads about what is picked, and
//! what they can do with it, on the right of the window. It is made of
//! cards, each a title over blocks of text and buttons; a button is clicked
//! by its own box, never by the line it is on. The sidebar knows nothing of
//! what a button does: each carries the window's own `A`, handed back when
//! it is clicked. Drawn by hand, as the rest of the window is (ADR 0017).

use underdraw_layout::{Align, Draw, Font, Ink, Measure, Rect};

pub use crate::chrome::SIDEBAR as WIDTH;
/// How wide the text of a card is: its width inside its padding.
pub const TEXT_WIDTH: f64 = WIDTH - 2.0 * GUTTER - 2.0 * PADDING;
use crate::chrome::{
    BUTTON, BUTTON_PAD, BUTTON_RADIUS, CARD_BETWEEN as BETWEEN, CARD_PADDING as PADDING, CARD_RADIUS,
    CARD_TEXT as BODY, CARD_TITLE as TITLE, GUTTER,
};

/// One card: a title, and the blocks under it.
pub struct Card<A> {
    pub title: String,
    /// The title's ink: the notes' blue for a note, the strong ink otherwise.
    pub ink: Ink,
    pub blocks: Vec<Block<A>>,
}

impl<A> Card<A> {
    /// A card titled `title`, in the strong ink, holding `blocks`.
    pub fn new(title: String, blocks: Vec<Block<A>>) -> Card<A> {
        Card { title, ink: Ink::Text, blocks }
    }
}

/// What a card holds, one under another.
pub enum Block<A> {
    /// Text, wrapped to the card's width.
    Text(String),
    /// Secondary text, in gray: a field's name and its value, a hint.
    Muted(String),
    /// A button: what it says, and what a click on it does.
    Button(String, A),
    /// Text being written, with its caret: at most one in the sidebar.
    Field(Field),
}

/// Text being written, as it is drawn: what it says, an input method's
/// preedit included, and where its selection, its caret and that preedit
/// lie, each from the text's top-left, set in the cards' text font at
/// [`TEXT_WIDTH`].
pub struct Field {
    pub text: String,
    pub selection: Vec<Rect>,
    pub caret: Option<Rect>,
    /// Underlined, as it is not written yet.
    pub preedit: Vec<Rect>,
    pub height: f64,
}

impl<A> Block<A> {
    /// What it says.
    #[cfg(test)]
    pub fn says(&self) -> &str {
        match self {
            Block::Text(text) | Block::Muted(text) | Block::Button(text, _) => text,
            Block::Field(field) => &field.text,
        }
    }
}

/// The sidebar laid out: what to draw, and where each button is, in window
/// points.
pub struct Laid<'a, A> {
    pub draws: Vec<Draw>,
    pub buttons: Vec<(Rect, &'a A)>,
    /// Where the text being written is, when it is.
    pub field: Option<Rect>,
    /// How far down its last card ends, from its top, unscrolled.
    pub height: f64,
}

impl<A> Laid<'_, A> {
    /// What the button under `at`, a point in the window, does.
    pub fn at(&self, at: (f64, f64)) -> Option<&A> {
        self.buttons.iter().find(|(r, _)| over(*r, at)).map(|(_, does)| *does)
    }
}

/// `cards` laid out one under another in a sidebar whose left edge is at
/// `left` and whose top is `top` (scrolled), its text set by `measure`.
pub fn lay_out<'a, A>(cards: &'a [Card<A>], left: f64, top: f64, measure: &mut dyn Measure) -> Laid<'a, A> {
    let inner = TEXT_WIDTH;
    let x = left + GUTTER + PADDING;
    let (mut draws, mut buttons, mut at) = (Vec::new(), Vec::new(), None);
    let mut y = top + GUTTER;
    for card in cards {
        let card_top = y;
        let mut inside = Vec::new();
        y += PADDING;
        let tall = set(measure, &card.title, TITLE, inner);
        inside.push(text(rect(x, y, inner, tall), card.title.clone(), TITLE, card.ink));
        y += tall;
        for block in &card.blocks {
            y += BETWEEN;
            match block {
                Block::Text(said) | Block::Muted(said) => {
                    let ink = if matches!(block, Block::Muted(_)) { Ink::Muted } else { Ink::Text };
                    let tall = set(measure, said, BODY, inner);
                    inside.push(text(rect(x, y, inner, tall), said.clone(), BODY, ink));
                    y += tall;
                }
                Block::Button(said, does) => {
                    let tall = set(measure, said, BUTTON, inner - 2.0 * PADDING) + 2.0 * BUTTON_PAD;
                    let around = rect(x, y, inner, tall);
                    inside.push(Draw::Box {
                        rect: around,
                        radius: BUTTON_RADIUS,
                        fill: Some(Ink::Paper),
                        stroke: Some(Ink::Muted),
                        dashed: false,
                    });
                    let words =
                        rect(x + PADDING, y + BUTTON_PAD, inner - 2.0 * PADDING, tall - 2.0 * BUTTON_PAD);
                    inside.push(text(words, said.clone(), BUTTON, Ink::Text));
                    buttons.push((around, does));
                    y += tall;
                }
                Block::Field(field) => {
                    let tall = field.height.max(f64::from(BODY.line_height));
                    let moved = |r: &Rect| {
                        rect(x + f64::from(r.x), y + f64::from(r.y), r.width.into(), r.height.into())
                    };
                    for r in &field.selection {
                        inside.push(Draw::Box {
                            rect: moved(r),
                            radius: 0.0,
                            fill: Some(Ink::Fill),
                            stroke: None,
                            dashed: false,
                        });
                    }
                    inside.push(text(rect(x, y, inner, tall), field.text.clone(), BODY, Ink::Text));
                    for r in &field.preedit {
                        let r = moved(r);
                        let under = r.y + r.height - 1.0;
                        inside.push(Draw::Line {
                            from: [r.x, under],
                            to: [r.x + r.width, under],
                            ink: Ink::Text,
                        });
                    }
                    if let Some(caret) = &field.caret {
                        inside.push(Draw::Box {
                            rect: moved(caret),
                            radius: 0.0,
                            fill: Some(Ink::Text),
                            stroke: None,
                            dashed: false,
                        });
                    }
                    at = Some(rect(x, y, inner, tall));
                    y += tall;
                }
            }
        }
        y += PADDING;
        draws.push(Draw::Box {
            rect: rect(left + GUTTER, card_top, WIDTH - 2.0 * GUTTER, y - card_top),
            radius: CARD_RADIUS,
            fill: Some(Ink::Paper),
            stroke: Some(Ink::Line),
            dashed: false,
        });
        draws.extend(inside);
        y += GUTTER;
    }
    Laid { draws, buttons, field: at, height: y - top }
}

/// How tall `said` is set in `font`, wrapped at `wide`.
fn set(measure: &mut dyn Measure, said: &str, font: Font, wide: f64) -> f64 {
    f64::from(measure.text(said, font, Some(wide as f32)).height).max(f64::from(font.line_height))
}

fn text(rect: Rect, text: String, font: Font, ink: Ink) -> Draw {
    Draw::Text {
        rect,
        text,
        font,
        ink,
        max_lines: None,
        align: Align::Start,
        underline: false,
        strike: false,
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x: x as f32, y: y as f32, width: width.max(0.0) as f32, height: height.max(0.0) as f32 }
}

/// Whether `at` is on `r`, edges included.
fn over(r: Rect, at: (f64, f64)) -> bool {
    let (x, y) = (f64::from(r.x), f64::from(r.y));
    (x..=x + f64::from(r.width)).contains(&at.0) && (y..=y + f64::from(r.height)).contains(&at.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Typesetter;

    #[test]
    fn each_button_is_clicked_by_its_own_box() {
        let cards = vec![
            Card::new(
                "one".to_string(),
                vec![Block::Text("a long line ".repeat(20)), Block::Button("go".to_string(), 1)],
            ),
            Card::new(
                "two".to_string(),
                vec![Block::Button("stop".to_string(), 2), Block::Muted("hint".to_string())],
            ),
        ];
        let mut typesetter = Typesetter::new(None).unwrap();
        let laid = lay_out(&cards, 1000.0, 0.0, &mut typesetter);
        assert_eq!(laid.buttons.len(), 2);
        let (first, second) = (laid.buttons[0].0, laid.buttons[1].0);
        assert!(first.y + first.height < second.y, "the second below the first");
        assert!(first.height > 20.0, "a button is taller than its text");
        let middle = |r: Rect| (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0));
        assert_eq!(laid.at(middle(first)), Some(&1));
        assert_eq!(laid.at(middle(second)), Some(&2));
        assert_eq!(
            laid.at((middle(first).0, f64::from(first.y) - 2.0)),
            None,
            "the text above it does nothing"
        );
        assert!(f64::from(first.x) >= 1000.0 && f64::from(first.x + first.width) <= 1000.0 + WIDTH);
        assert!(laid.height > f64::from(second.y + second.height), "the cards end below their last block");
    }
}
