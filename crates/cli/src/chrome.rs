//! The gallery's chrome: everything the window draws that is not a
//! wireframe, in one look, declared here and nowhere else. The wireframes
//! are what is shown, gray on white paper (GLOSSARY: Wireframe); the
//! chrome lays them on a cool drafting desk, each picture a sheet with a
//! soft shadow, and keeps its own colour for what the person and the agent
//! say about them: cobalt for the person's notes and journeys, green for
//! what the agent answered, and the wireframes' own orange for what was
//! assumed, so that orange means the same in the picture and around it.

use underdraw_layout::{Font, Ink};
use vello::peniko::Color;

/// The desk the pictures lie on, behind everything else in the pane.
pub const DESK: Color = Color::from_rgb8(0xE8, 0xEB, 0xEF);
/// The tree's surface, and the sidebar's under its cards.
pub const SIDE: Color = Color::from_rgb8(0xF7, 0xF8, 0xFA);
/// The shadow a sheet casts on the desk, and how soft and how far down.
pub const SHADOW: Color = Color::from_rgba8(0x1A, 0x22, 0x33, 0x24);
pub const SHADOW_BLUR: f64 = 5.0;
pub const SHADOW_DROP: f64 = 2.0;

/// The colour of each ink in the chrome, as [`crate::paint`] takes it: the
/// chrome draws with the inks a wireframe does, each meaning what it means
/// there, in the chrome's own colours.
pub fn colour(ink: Ink) -> Color {
    let rgb = match ink {
        Ink::Text => [0x1F, 0x23, 0x2A],
        Ink::Strong => [0x0B, 0x0E, 0x14],
        Ink::Muted => [0x66, 0x6E, 0x7C],
        Ink::Line => [0xD2, 0xD7, 0xDF],
        // What is picked: a line of the tree, a mark's ground.
        Ink::Fill => [0xDD, 0xE5, 0xFA],
        Ink::Paper => [0xFF, 0xFF, 0xFF],
        Ink::Assumed => [0xF0, 0x8C, 0x00],
        Ink::Note => [0x2B, 0x58, 0xD6],
        Ink::Answered => [0x15, 0x86, 0x58],
        Ink::Scrim => return Color::from_rgba8(0x0B, 0x0E, 0x14, 0x59),
    };
    Color::from_rgb8(rgb[0], rgb[1], rgb[2])
}

/// The chrome's type, one family (the system's) in four sizes: what is
/// read, the tree's lines; a caption over a picture; a mark
/// (a count, a journey's step, a flow's arrow); and the overview's arrows.
pub const BODY: Font = Font { size: 13.0, weight: 400, line_height: 16.0 };
pub const CAPTION_FONT: Font = Font { size: 12.0, weight: 500, line_height: 16.0 };
pub const MARK: Font = Font { size: 11.0, weight: 600, line_height: 14.0 };
pub const ARROW_FONT: Font = Font { size: 12.0, weight: 600, line_height: 16.0 };
/// The weight of what heads something: a description in the tree, a
/// state's name over its picture in the overview, a flow's box.
pub const HEADING: u16 = 600;
/// The sidebar's type: a card's title, what it says, and its buttons'.
pub const CARD_TITLE: Font = Font { size: 13.0, weight: 600, line_height: 18.0 };
pub const CARD_TEXT: Font = Font { size: 13.0, weight: 400, line_height: 18.0 };
pub const BUTTON: Font = Font { size: 13.0, weight: 600, line_height: 16.0 };

/// The sidebar's width as the window opens, and the least it is dragged
/// to, in points: the gallery keeps the width it has now.
pub const SIDEBAR: f64 = 360.0;
pub const SIDEBAR_LEAST: f64 = 280.0;
/// The space around the sidebar's cards, and between them; inside a card,
/// around what it holds; and between one block of a card and the next.
pub const GUTTER: f64 = 12.0;
pub const CARD_PADDING: f64 = 12.0;
pub const CARD_BETWEEN: f64 = 6.0;
/// How much taller than its text a button is.
pub const BUTTON_PAD: f64 = 7.0;
/// The tree's width as the window opens, and the least it is dragged to,
/// in points: the gallery keeps the width it has now.
pub const TREE: f64 = 260.0;
pub const TREE_LEAST: f64 = 160.0;
/// How near a line between two panes, either side, the pointer takes hold
/// of it, in points.
pub const GRIP: f64 = 4.0;
/// One row of the tree, in points.
pub const ROW: f64 = 28.0;
/// How far in each directory level moves a row.
pub const INDENT: f64 = 14.0;
/// The space around the pictures, and between them.
pub const MARGIN: f64 = 28.0;
/// The caption over each picture.
pub const CAPTION: f64 = 24.0;
/// The space between one finding and the next.
pub const GAP: f64 = 12.0;
/// How wide and, at most, how tall a state's picture is in its
/// description's overview, and the space between two, where the arrows run.
pub const THUMB: f64 = 160.0;
pub const THUMB_TALL: f64 = 320.0;
pub const SPACE: f64 = 72.0;
/// How far apart the arrows leaving, or reaching, one box of the flow are.
pub const ARROWS: f64 = 24.0;
/// A note's pin, where it was written, and the faint dot of it elsewhere:
/// how wide each is.
pub const PIN: f64 = 22.0;
pub const DOT: f64 = 8.0;
/// How tall a mark is (a count in the tree, a journey's step in the
/// overview), and how far it is set in from what it sits in.
pub const MARK_TALL: f64 = 18.0;
pub const MARK_INSET: f64 = 6.0;
/// The corners: of a sheet (a flow's box), of what is picked or ringed,
/// and of a mark, which is round.
pub const SHEET_RADIUS: f32 = 6.0;
pub const PICKED_RADIUS: f32 = 6.0;
pub const MARK_RADIUS: f32 = 9.0;
/// The corners of a card of the sidebar, and of a button on it.
pub const CARD_RADIUS: f32 = 8.0;
pub const BUTTON_RADIUS: f32 = 6.0;
/// What an assumption guessed, as the sidebar sets it: large, to be read
/// first.
pub const BIG: Font = Font { size: 20.0, weight: 600, line_height: 26.0 };
/// The space inside a bubble of a note's conversation, or a field, around
/// its text; how much of a card's width a bubble takes at most; and its
/// corners.
pub const INSET: f64 = 8.0;
pub const BUBBLE: f64 = 0.82;
pub const BUBBLE_RADIUS: f32 = 10.0;
/// How wide the name of a fact on a card is (what a node calls, its id,
/// where it is at a width), its value set beside it.
pub const FACT: f64 = 64.0;
/// A pane's scrollbar, drawn while what it holds is larger than it is and
/// never faded out: only its thumb, as wide as this, set in this far from
/// the pane's edge and from either end of its track, never shorter than
/// [`SCROLLBAR_LEAST`], round, in a translucent ink that shows on the desk
/// and on the tree's and the sidebar's surface alike. It runs in the space
/// the tree keeps right of its names, the sidebar around its cards, and
/// the pictures' pane under their labels.
pub const SCROLLBAR: Color = Color::from_rgba8(0x1A, 0x22, 0x33, 0x4D);
pub const SCROLLBAR_WIDE: f64 = 6.0;
pub const SCROLLBAR_INSET: f64 = 2.0;
pub const SCROLLBAR_LEAST: f64 = 24.0;
