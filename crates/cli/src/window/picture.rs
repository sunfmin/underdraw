//! What the pictures' pane shows, one kind of picture at a time: a state's
//! pages, a description's overview, its faults, or a directory's flow.
//! Each kind owns its geometry behind [`Shows`]: how large it is in the
//! pane, which [`Target`] is at a point, how it is drawn, and the
//! reference a note on a target is written on. The window presses, notes
//! and picks by the target, never by the kind and where it is.

mod faults;
mod flow;
mod overview;
mod pages;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use underdraw_format::Lang;
use underdraw_layout::{Draw, Ink, Rect};
use vello::Scene;

pub(crate) use self::faults::Faults;
pub(crate) use self::flow::{Flow, Tile};
pub(crate) use self::overview::{Arrow, Overview, Pass, Thumb};
pub(crate) use self::pages::{Page, Pages, offer_widths};
use crate::area::rect;
use crate::chrome::PIN;
use crate::notes::{Note, Stands};
use crate::reference::Reference;
use crate::text::Typesetter;

/// What the picked line shows in the pictures' pane.
pub(crate) enum Picture {
    /// One state at each of the widths.
    Pages(Pages),
    /// Every state of the description, small, and its transitions.
    Overview(Overview),
    /// Why it cannot be drawn: what `check` finds wrong with it.
    Faults(Faults),
    /// For a directory picked: the flow between the screens under it.
    Flow(Flow),
}

impl Picture {
    /// The picture, as the window sizes, presses, draws and notes it.
    pub fn shows(&self) -> &dyn Shows {
        match self {
            Picture::Pages(pages) => pages,
            Picture::Overview(overview) => overview,
            Picture::Faults(faults) => faults,
            Picture::Flow(flow) => flow,
        }
    }

    /// The reference to each arrow shown, in their order: each transition
    /// of an overview, or each navigate of a flow.
    pub fn arrow_references(&self) -> Vec<Reference> {
        (0..).map_while(|index| self.shows().reference(&Target::Arrow(index))).collect()
    }
}

/// One kind of picture: all the window knows of it.
pub(crate) trait Shows {
    /// How wide and tall it is drawn in `pane`, the space around it
    /// included; a width of nothing when it never scrolls across.
    fn size(&self, pane: &Desk, typesetter: &mut Typesetter) -> (f64, f64);

    /// What is at `at`, a point in the window, drawn in `pane`: the one on
    /// top where several are; none where nothing a press picks is.
    fn target_at(&self, at: (f64, f64), pane: &Desk) -> Option<Target>;

    /// It drawn in `pane`, as `seen` says: its pictures into `scene`, and
    /// what goes over them, in window points, handed back to be drawn above
    /// the pane's scrollbars.
    fn draw(&self, scene: &mut Scene, pane: &Desk, seen: &Seen, typesetter: &mut Typesetter) -> Vec<Draw>;

    /// What a note on `target` is on, or ⌘C copies of it; none where it
    /// names nothing here. [`Target::Caption`] is the whole of what is shown.
    fn reference(&self, target: &Target) -> Option<Reference>;

    /// The pins of `notes`, by their descriptions' directories, on it drawn
    /// in `pane`.
    fn pins(&self, notes: &BTreeMap<PathBuf, Vec<Note>>, pane: &Desk) -> Vec<Pin>;
}

/// The desk the pictures lie on, the pictures' pane: where it is, and how
/// far it is scrolled, in window points.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Desk {
    /// Its left edge.
    pub left: f64,
    pub width: f64,
    /// How far it is scrolled down, and across.
    pub scroll: f64,
    pub across: f64,
}

/// What a press on a picture is on, or what is picked there.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// A node of a state's page `pointed`, by its id.
    Node { id: String, pointed: usize },
    /// A page's caption: the whole state shown.
    Caption,
    /// An arrow's label, by its place among the arrows shown.
    Arrow(usize),
    /// A journey's mark on a state of the overview: the journey's line of
    /// the tree, its step, and the state's line.
    Pass { journey: usize, step: usize, state: usize },
    /// A state of the overview, its picture or its caption, by its line.
    State(usize),
    /// A box of the flow, by the directory of the description it shows.
    Box(PathBuf),
}

/// What the window marks on a picture as it draws it.
pub(crate) struct Seen {
    /// What is picked, as a target.
    pub picked: Option<Target>,
    /// The id, as written, of the node the journey played acts on next.
    pub marked: Option<String>,
    /// Where acting on the node picked goes, each a button floating
    /// beside it: none while it goes nowhere.
    pub offers: Vec<Offer>,
    /// Whether every node that acting on goes somewhere is lit up, not
    /// only outlined: while ⌘ (Ctrl) is held, and in the flash after a
    /// ⌘-click that goes nowhere.
    pub lit: bool,
    /// Whether the marks over the pictures are shown: the labels, the
    /// guesses, the interaction marks (shown too while `lit`) and the
    /// journeys'. The node picked, and what acting on it offers, are
    /// marked either way.
    pub marks: bool,
    pub lang: Lang,
}

/// A button floating beside the node picked, offering one place acting on
/// it goes: what it says, and whether it goes there, in the interaction
/// ink, or only says where it would, grayed (a screen not drawn yet).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Offer {
    pub says: String,
    pub live: bool,
}

/// One note drawn on a picture: its pin, numbered, where it was written, or
/// a faint dot, in another state or width that holds its node.
pub(crate) struct Pin {
    /// The directory of the description that keeps it.
    pub dir: PathBuf,
    pub id: String,
    /// Where it is, in window points.
    pub rect: Rect,
    pub ink: Ink,
    pub faint: bool,
}

impl Pin {
    /// Note `note`, of the description in `dir`, drawn in `rect`: faint, or
    /// its pin, in the ink of where it stands: one that waits on the agent
    /// in the note's, one with an answer unread in the answer's, and one
    /// answered and read quiet.
    fn of(dir: &Path, note: &Note, rect: Rect, faint: bool) -> Pin {
        let ink = match note.stands() {
            Stands::Waiting | Stands::Taken => Ink::Note,
            Stands::Unread => Ink::Answered,
            Stands::Read => Ink::Line,
        };
        Pin { dir: dir.to_path_buf(), id: note.id.clone(), rect, ink, faint }
    }
}

/// The pins of the notes on each arrow, given by its reference and the
/// point its pins start from, set one beside the other.
fn arrow_pins(
    arrows: impl IntoIterator<Item = (Reference, (f64, f64))>,
    notes: &BTreeMap<PathBuf, Vec<Note>>,
) -> Vec<Pin> {
    let mut pins = Vec::new();
    for (reference, (x, y)) in arrows {
        let Some(notes) = notes.get(&reference.description) else { continue };
        let on = notes.iter().filter(|note| note.on == reference.on);
        for (before, note) in on.enumerate() {
            let x = x + before as f64 * (PIN + 2.0);
            pins.push(Pin::of(
                &reference.description,
                note,
                rect(x - PIN / 2.0, y - PIN / 2.0, PIN, PIN),
                false,
            ));
        }
    }
    pins
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pane 1000 wide right of a tree 100 wide, unscrolled: each kind's
    /// tests draw in it.
    pub(super) const PANE: Desk = Desk { left: 100.0, width: 1000.0, scroll: 0.0, across: 0.0 };

    /// What the window marks with `picked` picked, in English.
    pub(super) fn seen(picked: Option<Target>) -> Seen {
        Seen { picked, marked: None, offers: Vec::new(), lit: false, marks: true, lang: Lang::En }
    }

    /// The middle of `r`.
    pub(super) fn middle(r: Rect) -> (f64, f64) {
        (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0))
    }
}
