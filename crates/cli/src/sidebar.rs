//! The gallery's sidebar: what the person reads about what is picked, and
//! what they can do with it, on the right of the window. It is made of
//! cards, each a title over blocks of text and buttons, outlined in its
//! title's ink: who it is from (the person's cobalt, the agent's green, the
//! orange of what was assumed, dashed as a guess is in a picture). A button,
//! an item of the inbox or a fold is clicked by its own box, never by the
//! line it is on. Under the cards, fixed at its foot while they scroll,
//! the box the person writes in to the conversation shown, and its Send
//! button. The sidebar knows nothing of
//! what a button does: each carries the window's own `A`, handed back when
//! it is clicked. Drawn by hand, as the rest of the window is (ADR 0017).

use underdraw_layout::{Align, Draw, Font, Ink, Measure, Rect};

use crate::chrome::{BIG, BUBBLE, BUBBLE_RADIUS, FACT, INSET, MARK, MARK_INSET, MARK_RADIUS, MARK_TALL};
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
    /// Secondary text, in gray: a hint, a group's name.
    Muted(String),
    /// A fact: its name in gray, and its value beside it.
    Fact(String, String),
    /// Text in large type: what an assumption guessed.
    Big(String),
    /// One line of a conversation, in a bubble: the person's on the right,
    /// in the notes' blue; the agent's on the left, in gray.
    Bubble(By, String),
    /// A button: what it says, and what a click on it does.
    Button(String, A),
    /// A fold: a line in the notes' blue, without a box, that a click opens
    /// or closes, saying which with its ▸ or ▾.
    Fold(String, A),
    /// An item of a list, clicked to show it: a mark of what it is (an
    /// assumption's "?", a note's id) on the mark's ink, what it is on, and
    /// what it says, in its own ink; it says nothing when that is empty.
    Item(Item<A>),
    /// Buttons side by side, sharing the card's width.
    Row(Vec<(String, A)>),
    /// The field being written in, with its caret: what it holds, and what
    /// it says while empty.
    Field(Shown, String),
}

/// The box at the sidebar's foot, fixed there below the cards as they
/// scroll: the field the person writes in to the conversation shown, and
/// the button that sends what it holds.
pub struct Foot<A> {
    pub field: Shown,
    /// What the field says while it holds nothing.
    pub empty: String,
    /// What the button says.
    pub send: String,
    /// What a click on the button does; none while there is nothing to
    /// send, the button then gray and clicked to no effect.
    pub does: Option<A>,
}

/// An item of a list, as [`Block::Item`] holds it.
pub struct Item<A> {
    pub mark: String,
    pub mark_ink: Ink,
    pub on: String,
    pub says: String,
    pub ink: Ink,
    pub does: A,
}

impl<A> Block<A> {
    /// What it says: a line for each button of a row; a fact's name and
    /// value two spaces apart; an item's mark and what it is on, then what
    /// it says.
    #[cfg(test)]
    pub fn says(&self) -> Vec<String> {
        match self {
            Block::Text(text)
            | Block::Muted(text)
            | Block::Big(text)
            | Block::Button(text, _)
            | Block::Fold(text, _)
            | Block::Bubble(_, text) => vec![text.clone()],
            Block::Fact(name, value) => vec![format!("{name}  {value}")],
            Block::Item(item) => std::iter::once(format!("{} {}", item.mark, item.on))
                .chain((!item.says.is_empty()).then(|| item.says.clone()))
                .collect(),
            Block::Row(buttons) => buttons.iter().map(|(text, _)| text.clone()).collect(),
            Block::Field(shown, empty) => {
                vec![if shown.text.is_empty() { empty.clone() } else { shown.text.clone() }]
            }
        }
    }
}

/// The sidebar laid out: what to draw, where each button is, in window
/// points, with what it says, and where the caret of a field is.
pub struct Laid<'a, A> {
    /// The cards, drawn above [`Laid::foot`] and cut there.
    pub draws: Vec<Draw>,
    /// The box at the foot, drawn over the cards.
    pub foot_draws: Vec<Draw>,
    /// Where the box at the foot starts, from the window's top: where the
    /// cards are cut; the sidebar's bottom when there is none.
    pub foot: f64,
    pub buttons: Vec<(Rect, &'a str, &'a A)>,
    /// The caret of the field being written in, for an input method to put
    /// what it offers beside.
    pub caret: Option<Rect>,
    /// Where the text of that field is: a point in it is a place in its
    /// text, from its top-left.
    pub field: Option<Rect>,
    /// How far down its last card ends, from its top, unscrolled.
    pub height: f64,
}

impl<A> Laid<'_, A> {
    /// What the button under `at`, a point in the window, does: one of
    /// the box at the foot there, else one of the cards above it, never a
    /// card's under the box.
    pub fn at(&self, at: (f64, f64)) -> Option<&A> {
        let below = at.1 >= self.foot;
        self.buttons
            .iter()
            .find(|(r, _, _)| (f64::from(r.y) >= self.foot) == below && over(*r, at))
            .map(|(_, _, does)| *does)
    }
}

/// How wide the text of a card is in a sidebar `width` wide: the card's
/// width inside its padding.
fn text_width(width: f64) -> f64 {
    width - 2.0 * GUTTER - 2.0 * PADDING
}

/// How wide the text of a field is in a sidebar `width` wide: a card's,
/// inside the field's outline.
pub fn field_width(width: f64) -> f64 {
    text_width(width) - 2.0 * INSET
}

/// `cards` laid out one under another in a sidebar `width` wide whose left
/// edge is at `left` and whose top is `top` (scrolled), its text set by
/// `typesetter` to wrap within that width; and `foot`, when there is one,
/// fixed at its bottom, `bottom` from the window's top.
pub fn lay_out<'a, A>(
    cards: &'a [Card<A>],
    foot: Option<&'a Foot<A>>,
    (left, width): (f64, f64),
    (top, bottom): (f64, f64),
    typesetter: &mut Typesetter,
) -> Laid<'a, A> {
    let inner = text_width(width);
    let x = left + GUTTER + PADDING;
    let mut laid = Laid {
        draws: Vec::new(),
        foot_draws: Vec::new(),
        foot: bottom,
        buttons: Vec::new(),
        caret: None,
        field: None,
        height: 0.0,
    };
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
                Block::Fact(name, value) => {
                    let wide = FACT.min(inner);
                    let named = words(&mut inside, typesetter, rect(x, y, wide, 0.0), name, BODY, Ink::Muted);
                    let at = rect(x + wide, y, inner - wide, 0.0);
                    named.max(words(&mut inside, typesetter, at, value, BODY, Ink::Text))
                }
                Block::Fold(said, does) => {
                    let tall = set(typesetter, said, BUTTON, inner);
                    inside.push(text(rect(x, y, inner, tall), said.clone(), BUTTON, Ink::Note));
                    laid.buttons.push((rect(x, y, inner, tall), said, does));
                    tall
                }
                Block::Item(item) => self::item(&mut inside, &mut laid, typesetter, (x, y, inner), item),
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
                    let (tall, caret, words) = field(&mut inside, typesetter, (x, y, inner), shown, empty);
                    (laid.caret, laid.field) = (Some(caret), Some(words));
                    tall
                }
            };
        }
        y += PADDING;
        laid.draws.push(Draw::Box {
            rect: rect(left + GUTTER, card_top, width - 2.0 * GUTTER, y - card_top),
            radius: CARD_RADIUS,
            fill: Some(Ink::Paper),
            stroke: Some(match card.ink {
                Ink::Assumed | Ink::Note | Ink::Answered => card.ink,
                _ => Ink::Line,
            }),
            dashed: card.ink == Ink::Assumed,
        });
        laid.draws.extend(inside);
        y += GUTTER;
    }
    laid.height = y - top;
    if let Some(foot) = foot {
        self::foot(&mut laid, typesetter, (left, width), bottom, foot);
    }
    laid
}

/// `foot` laid at the bottom of a sidebar `width` wide from `left`, its
/// bottom edge `bottom` from the window's top: on the paper, under a
/// hairline, the field a card's text wide, and the Send button under it
/// on the right.
fn foot<'a, A>(
    laid: &mut Laid<'a, A>,
    typesetter: &mut Typesetter,
    (left, width): (f64, f64),
    bottom: f64,
    foot: &'a Foot<A>,
) {
    let inner = text_width(width);
    let x = left + GUTTER + PADDING;
    let field_tall = set(typesetter, &foot.field.text, BODY, inner - 2.0 * INSET) + 2.0 * INSET;
    let send_wide = f64::from(typesetter.text(&foot.send, BUTTON, None).width) + 2.0 * PADDING;
    let send_tall = f64::from(BUTTON.line_height) + 2.0 * BUTTON_PAD;
    let top = bottom - (PADDING + field_tall + BETWEEN + send_tall + PADDING);
    laid.foot = top;
    let mut draws = vec![
        Draw::Box {
            rect: rect(left, top, width, bottom - top),
            radius: 0.0,
            fill: Some(Ink::Paper),
            stroke: None,
            dashed: false,
        },
        Draw::Line { from: [left as f32, top as f32], to: [(left + width) as f32, top as f32], ink: Ink::Line },
    ];
    let (_, caret, words) = field(&mut draws, typesetter, (x, top + PADDING, inner), &foot.field, &foot.empty);
    (laid.caret, laid.field) = (Some(caret), Some(words));
    let at = rect(x + inner - send_wide, top + PADDING + field_tall + BETWEEN, send_wide, send_tall);
    match &foot.does {
        Some(does) => {
            draws.push(Draw::Box {
                rect: at,
                radius: BUTTON_RADIUS,
                fill: Some(Ink::Note),
                stroke: None,
                dashed: false,
            });
            let inside = rect(f64::from(at.x), f64::from(at.y) + BUTTON_PAD, send_wide, f64::from(BUTTON.line_height));
            draws.push(centred(inside, foot.send.clone(), Ink::Paper));
            laid.buttons.push((at, &foot.send, does));
        }
        None => {
            draws.push(Draw::Box {
                rect: at,
                radius: BUTTON_RADIUS,
                fill: Some(Ink::Paper),
                stroke: Some(Ink::Line),
                dashed: false,
            });
            let inside = rect(f64::from(at.x), f64::from(at.y) + BUTTON_PAD, send_wide, f64::from(BUTTON.line_height));
            draws.push(centred(inside, foot.send.clone(), Ink::Muted));
        }
    }
    laid.foot_draws = draws;
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

/// An item of a list at the top of `(x, y, wide)`, in a box clicked to do
/// what it does: its mark on the left, what it is on beside it, and what it
/// says under that; how tall.
fn item<'a, A>(
    draws: &mut Vec<Draw>,
    laid: &mut Laid<'a, A>,
    measure: &mut dyn Measure,
    (x, y, wide): (f64, f64, f64),
    item: &'a Item<A>,
) -> f64 {
    let mark_wide = (f64::from(measure.text(&item.mark, MARK, None).width) + 2.0 * MARK_INSET).max(MARK_TALL);
    let left = x + INSET + mark_wide + INSET;
    let words_wide = x + wide - INSET - left;
    let mut tall = set(measure, &item.on, TITLE, words_wide);
    let mut inside = vec![text(rect(left, y + INSET, words_wide, tall), item.on.clone(), TITLE, Ink::Text)];
    if !item.says.is_empty() {
        tall += words(
            &mut inside,
            measure,
            rect(left, y + INSET + tall, words_wide, 0.0),
            &item.says,
            BODY,
            item.ink,
        );
    }
    let around = rect(x, y, wide, tall + 2.0 * INSET);
    draws.push(Draw::Box {
        rect: around,
        radius: BUTTON_RADIUS,
        fill: Some(Ink::Paper),
        stroke: Some(Ink::Line),
        dashed: false,
    });
    let top = y + INSET + (f64::from(TITLE.line_height) - MARK_TALL) / 2.0;
    draws.push(Draw::Box {
        rect: rect(x + INSET, top, mark_wide, MARK_TALL),
        radius: MARK_RADIUS,
        fill: Some(item.mark_ink),
        stroke: None,
        dashed: false,
    });
    draws.push(Draw::Text {
        rect: rect(
            x + INSET,
            top + (MARK_TALL - f64::from(MARK.line_height)) / 2.0,
            mark_wide,
            f64::from(MARK.line_height),
        ),
        text: item.mark.clone(),
        font: MARK,
        ink: Ink::Paper,
        max_lines: Some(1),
        align: Align::Center,
        underline: false,
        strike: false,
    });
    draws.extend(inside);
    laid.buttons.push((around, &item.on, &item.does));
    tall + 2.0 * INSET
}

/// One line of a conversation in a bubble at the top of `(x, y, wide)`:
/// the person's on the right, filled with the notes' blue; the agent's on
/// the left, outlined in its green; how tall.
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
    let (left, fill, stroke, ink) = match by {
        By::Person => (x + wide - box_wide, Ink::Note, None, Ink::Paper),
        By::Agent => (x, Ink::Paper, Some(Ink::Answered), Ink::Text),
    };
    draws.push(Draw::Box {
        rect: rect(left, y, box_wide, box_tall),
        radius: BUBBLE_RADIUS,
        fill: Some(fill),
        stroke,
        dashed: false,
    });
    draws.push(text(rect(left + INSET, y + INSET, words_wide, tall), said.to_string(), BODY, ink));
    box_tall
}

/// The field being written in, a card's text wide, from `(x, y)`,
/// outlined as focused: its text, what is composed in it underlined, the
/// caret, and what is selected shaded; `empty` in gray while it holds
/// nothing. How tall, where its caret is, and where its text is.
fn field(
    draws: &mut Vec<Draw>,
    typesetter: &mut Typesetter,
    (x, y, wide): (f64, f64, f64),
    shown: &Shown,
    empty: &str,
) -> (f64, Rect, Rect) {
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
    if !shown.selected.is_empty() {
        for span in typesetter.spans(&shown.text, BODY, inner as f32, shown.selected.clone()) {
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
    (box_tall, caret, rect(left, top, inner, tall))
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

/// `text` on one line, centred in `rect`, as a button says it.
fn centred(rect: Rect, text: String, ink: Ink) -> Draw {
    Draw::Text {
        rect,
        text,
        font: BUTTON,
        ink,
        max_lines: Some(1),
        align: Align::Center,
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
        let laid = lay_out(&cards, None, (1000.0, 360.0), (0.0, 800.0), &mut typesetter);
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
        assert!(f64::from(first.x) >= 1000.0 && f64::from(first.x + first.width) <= 1000.0 + 360.0);
        assert!(laid.height > f64::from(second.y + second.height), "the cards end below their last block");
    }

    #[test]
    fn narrower_the_cards_rewrap_their_text_within_the_width() {
        let cards = vec![Card::new(
            "one".to_string(),
            vec![Block::Text("a long line ".repeat(20)), Block::Button("go".to_string(), 1)],
        )];
        let mut typesetter = Typesetter::new(None).unwrap();
        let wide = lay_out(&cards, None, (1000.0, 360.0), (0.0, 800.0), &mut typesetter);
        let narrow = lay_out(&cards, None, (1080.0, 280.0), (0.0, 800.0), &mut typesetter);
        assert!(narrow.height > wide.height, "taller, the text wrapped to more lines");
        let button = narrow.buttons[0].0;
        assert!(f64::from(button.x) >= 1080.0 && f64::from(button.x + button.width) <= 1080.0 + 280.0);
        let middle = (f64::from(button.x + button.width / 2.0), f64::from(button.y + button.height / 2.0));
        assert_eq!(narrow.at(middle), Some(&1), "the button clicked by its own box");
    }

    #[test]
    fn the_box_at_the_foot_stays_there_and_sends_only_what_it_holds() {
        let cards: Vec<Card<u8>> =
            vec![Card::new("one".to_string(), vec![Block::Button("under the box".to_string(), 1)])];
        let mut typesetter = Typesetter::new(None).unwrap();
        let mut field = crate::field::Field::new(&typesetter, field_width(360.0));
        let foot = |field: &crate::field::Field, does| Foot {
            field: field.shown(),
            empty: "say".to_string(),
            send: "Send".to_string(),
            does,
        };
        let empty = foot(&field, None);
        let laid = lay_out(&cards, Some(&empty), (1000.0, 360.0), (0.0, 800.0), &mut typesetter);
        assert!(laid.foot < 800.0 && laid.foot > 700.0, "at the bottom: {}", laid.foot);
        let words = laid.field.unwrap();
        assert!(f64::from(words.y) > laid.foot, "the field in it");
        assert_eq!(laid.buttons.len(), 1, "an empty box has no Send to click");
        field.type_in("the price is cut");
        let full = foot(&field, Some(2));
        for scrolled in [0.0, -500.0] {
            let laid = lay_out(&cards, Some(&full), (1000.0, 360.0), (scrolled, 800.0), &mut typesetter);
            let send = laid.buttons.iter().find(|(_, said, _)| *said == "Send").unwrap().0;
            assert!(f64::from(send.y) > laid.foot && f64::from(send.y + send.height) <= 800.0, "fixed at the foot");
            let middle = (f64::from(send.x + send.width / 2.0), f64::from(send.y + send.height / 2.0));
            assert_eq!(laid.at(middle), Some(&2));
        }
        let card_button = |laid: &Laid<u8>| laid.buttons.iter().find(|(_, _, does)| **does == 1).unwrap().0;
        let laid = lay_out(&cards, Some(&full), (1000.0, 360.0), (laid_under(&cards, &mut typesetter), 800.0), &mut typesetter);
        let under = card_button(&laid);
        let middle = (f64::from(under.x + under.width / 2.0), f64::from(under.y + under.height / 2.0));
        assert!(middle.1 > laid.foot, "scrolled under the box");
        assert_ne!(laid.at(middle), Some(&1), "a card's button under the box is not clicked");
    }

    /// How far down the cards must start for their first button to lie
    /// behind the box at the foot of a sidebar 800 tall.
    fn laid_under(cards: &[Card<u8>], typesetter: &mut Typesetter) -> f64 {
        let laid = lay_out(cards, None, (1000.0, 360.0), (0.0, 800.0), typesetter);
        790.0 - f64::from(laid.buttons[0].0.y)
    }
}
