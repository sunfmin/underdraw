//! The gallery's sidebar: what the person reads about what is picked, and
//! what they can do with it, on the right of the window. It is made of
//! cards, each a title over blocks of text and buttons; a button is clicked
//! by its own box, never by the line it is on. The sidebar knows nothing of
//! what a button does: each carries the window's own `A`, handed back when
//! it is clicked. Drawn by hand, as the rest of the window is (ADR 0017).

use underdraw_layout::{Align, Draw, Font, Ink, Measure, Rect};

pub use crate::chrome::SIDEBAR as WIDTH;
use crate::chrome::{BIG, BUBBLE, BUBBLE_RADIUS, INSET};
use crate::chrome::{
    BUTTON, BUTTON_PAD, BUTTON_RADIUS, CARD_BETWEEN as BETWEEN, CARD_PADDING as PADDING, CARD_RADIUS,
    CARD_TEXT as BODY, CARD_TITLE as TITLE, GUTTER,
};
use crate::field::Shown;
use crate::notes::By;
use crate::text::Typesetter;

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
    /// Text in large type: what an assumption guessed.
    Big(String),
    /// One line of a conversation, in a bubble: the person's on the right,
    /// in the notes' blue; the agent's on the left, in gray.
    Bubble(By, String),
    /// A button: what it says, and what a click on it does.
    Button(String, A),
    /// Buttons side by side, sharing the card's width.
    Row(Vec<(String, A)>),
    /// The field being written in, with its caret: what it holds, and what
    /// it says while empty.
    Field(Shown, String),
}

impl<A> Block<A> {
    /// What it says: a line for each button of a row.
    #[cfg(test)]
    pub fn says(&self) -> Vec<&str> {
        match self {
            Block::Text(text) | Block::Muted(text) | Block::Big(text) | Block::Button(text, _) => vec![text],
            Block::Bubble(_, text) => vec![text],
            Block::Row(buttons) => buttons.iter().map(|(text, _)| text.as_str()).collect(),
            Block::Field(shown, empty) => vec![if shown.text.is_empty() { empty } else { &shown.text }],
        }
    }
}

/// The sidebar laid out: what to draw, where each button is, in window
/// points, with what it says, and where the caret of a field is.
pub struct Laid<'a, A> {
    pub draws: Vec<Draw>,
    pub buttons: Vec<(Rect, &'a str, &'a A)>,
    /// The caret of the field being written in, for an input method to put
    /// what it offers beside.
    pub caret: Option<Rect>,
    /// How far down its last card ends, from its top, unscrolled.
    pub height: f64,
}

impl<A> Laid<'_, A> {
    /// What the button under `at`, a point in the window, does.
    pub fn at(&self, at: (f64, f64)) -> Option<&A> {
        self.buttons.iter().find(|(r, _, _)| over(*r, at)).map(|(_, _, does)| *does)
    }
}

/// `cards` laid out one under another in a sidebar whose left edge is at
/// `left` and whose top is `top` (scrolled), its text set by `typesetter`.
pub fn lay_out<'a, A>(cards: &'a [Card<A>], left: f64, top: f64, typesetter: &mut Typesetter) -> Laid<'a, A> {
    let inner = WIDTH - 2.0 * GUTTER - 2.0 * PADDING;
    let x = left + GUTTER + PADDING;
    let mut laid = Laid { draws: Vec::new(), buttons: Vec::new(), caret: None, height: 0.0 };
    let mut y = top + GUTTER;
    for card in cards {
        let card_top = y;
        let mut inside = Vec::new();
        y += PADDING;
        let tall = set(typesetter, &card.title, TITLE, inner);
        inside.push(text(rect(x, y, inner, tall), card.title.clone(), TITLE, card.ink));
        y += tall;
        for block in &card.blocks {
            y += BETWEEN;
            y += match block {
                Block::Text(said) => {
                    words(&mut inside, typesetter, rect(x, y, inner, 0.0), said, BODY, Ink::Text)
                }
                Block::Muted(said) => {
                    words(&mut inside, typesetter, rect(x, y, inner, 0.0), said, BODY, Ink::Muted)
                }
                Block::Big(said) => {
                    words(&mut inside, typesetter, rect(x, y, inner, 0.0), said, BIG, Ink::Text)
                }
                Block::Bubble(by, said) => bubble(&mut inside, typesetter, (x, y, inner), *by, said),
                Block::Button(said, does) => {
                    button(&mut inside, &mut laid, typesetter, rect(x, y, inner, 0.0), said, does)
                }
                Block::Row(buttons) => {
                    let gap = BETWEEN;
                    let each =
                        (inner - gap * (buttons.len().max(1) - 1) as f64) / buttons.len().max(1) as f64;
                    let tall = buttons
                        .iter()
                        .map(|(said, _)| {
                            set(typesetter, said, BUTTON, each - 2.0 * PADDING) + 2.0 * BUTTON_PAD
                        })
                        .fold(0.0, f64::max);
                    for (i, (said, does)) in buttons.iter().enumerate() {
                        let at = rect(x + i as f64 * (each + gap), y, each, tall);
                        button(&mut inside, &mut laid, typesetter, at, said, does);
                    }
                    tall
                }
                Block::Field(shown, empty) => {
                    let (tall, caret) = field(&mut inside, typesetter, (x, y, inner), shown, empty);
                    laid.caret = Some(caret);
                    tall
                }
            };
        }
        y += PADDING;
        laid.draws.push(Draw::Box {
            rect: rect(left + GUTTER, card_top, WIDTH - 2.0 * GUTTER, y - card_top),
            radius: CARD_RADIUS,
            fill: Some(Ink::Paper),
            stroke: Some(Ink::Line),
            dashed: false,
        });
        laid.draws.extend(inside);
        y += GUTTER;
    }
    laid.height = y - top;
    laid
}

/// `said` set in `font` and `ink` at `at`, as wide as it; how tall.
fn words(
    draws: &mut Vec<Draw>,
    measure: &mut dyn Measure,
    at: Rect,
    said: &str,
    font: Font,
    ink: Ink,
) -> f64 {
    let wide = f64::from(at.width);
    let tall = set(measure, said, font, wide);
    draws.push(text(rect(f64::from(at.x), f64::from(at.y), wide, tall), said.to_string(), font, ink));
    tall
}

/// A button saying `said`, as wide as `at` and as tall as its words need
/// (at least `at`'s height), clicked to do `does`; how tall.
fn button<'a, A>(
    draws: &mut Vec<Draw>,
    laid: &mut Laid<'a, A>,
    measure: &mut dyn Measure,
    at: Rect,
    said: &'a str,
    does: &'a A,
) -> f64 {
    let (x, y, wide) = (f64::from(at.x), f64::from(at.y), f64::from(at.width));
    let tall =
        (set(measure, said, BUTTON, wide - 2.0 * PADDING) + 2.0 * BUTTON_PAD).max(f64::from(at.height));
    let around = rect(x, y, wide, tall);
    draws.push(Draw::Box {
        rect: around,
        radius: BUTTON_RADIUS,
        fill: Some(Ink::Paper),
        stroke: Some(Ink::Muted),
        dashed: false,
    });
    let inside = rect(x + PADDING, y + BUTTON_PAD, wide - 2.0 * PADDING, tall - 2.0 * BUTTON_PAD);
    draws.push(text(inside, said.to_string(), BUTTON, Ink::Text));
    laid.buttons.push((around, said, does));
    tall
}

/// One line of a conversation in a bubble at the top of `(x, y, wide)`:
/// the person's on the right, the agent's on the left; how tall.
fn bubble(
    draws: &mut Vec<Draw>,
    measure: &mut dyn Measure,
    (x, y, wide): (f64, f64, f64),
    by: By,
    said: &str,
) -> f64 {
    let most = wide * BUBBLE - 2.0 * INSET;
    let set_in = measure.text(said, BODY, Some(most as f32));
    let (words_wide, tall) =
        (f64::from(set_in.width).min(most), f64::from(set_in.height).max(f64::from(BODY.line_height)));
    let (box_wide, box_tall) = (words_wide + 2.0 * INSET, tall + 2.0 * INSET);
    let (left, fill, ink) = match by {
        By::Person => (x + wide - box_wide, Ink::Note, Ink::Paper),
        By::Agent => (x, Ink::Line, Ink::Text),
    };
    draws.push(Draw::Box {
        rect: rect(left, y, box_wide, box_tall),
        radius: BUBBLE_RADIUS,
        fill: Some(fill),
        stroke: None,
        dashed: false,
    });
    draws.push(text(rect(left + INSET, y + INSET, words_wide, tall), said.to_string(), BODY, ink));
    box_tall
}

/// The field being written in at the top of `(x, y, wide)`, outlined as
/// focused: its text, what is composed in it underlined, the caret, and
/// everything shaded when it is selected; `empty` in gray while it holds
/// nothing. How tall, and where its caret is.
fn field(
    draws: &mut Vec<Draw>,
    typesetter: &mut Typesetter,
    (x, y, wide): (f64, f64, f64),
    shown: &Shown,
    empty: &str,
) -> (f64, Rect) {
    let inner = wide - 2.0 * INSET;
    let (left, top) = (x + INSET, y + INSET);
    let tall = set(typesetter, &shown.text, BODY, inner);
    let box_tall = tall + 2.0 * INSET;
    draws.push(Draw::Box {
        rect: rect(x, y, wide, box_tall),
        radius: BUTTON_RADIUS,
        fill: Some(Ink::Paper),
        stroke: Some(Ink::Note),
        dashed: false,
    });
    let moved = |span: Rect| {
        rect(left + f64::from(span.x), top + f64::from(span.y), f64::from(span.width), f64::from(span.height))
    };
    if shown.all {
        for span in typesetter.spans(&shown.text, BODY, inner as f32, 0..shown.text.len()) {
            draws.push(Draw::Box {
                rect: moved(span),
                radius: 0.0,
                fill: Some(Ink::Fill),
                stroke: None,
                dashed: false,
            });
        }
    }
    if shown.text.is_empty() {
        draws.push(text(rect(left, top, inner, tall), empty.to_string(), BODY, Ink::Muted));
    } else {
        draws.push(text(rect(left, top, inner, tall), shown.text.clone(), BODY, Ink::Text));
    }
    if !shown.composing.is_empty() {
        for span in typesetter.spans(&shown.text, BODY, inner as f32, shown.composing.clone()) {
            let at = moved(span);
            let foot = at.y + at.height - 2.0;
            draws.push(Draw::Line { from: [at.x, foot], to: [at.x + at.width, foot], ink: Ink::Text });
        }
    }
    let caret = typesetter
        .spans(&shown.text, BODY, inner as f32, shown.caret..shown.caret)
        .first()
        .map_or(rect(left, top, 1.0, f64::from(BODY.line_height)), |span| moved(*span));
    draws.push(Draw::Line {
        from: [caret.x, caret.y],
        to: [caret.x, caret.y + caret.height],
        ink: Ink::Note,
    });
    (box_tall, caret)
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
