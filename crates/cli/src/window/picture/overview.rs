//! A description's overview: each of its states small, in rows as many as
//! the pane holds, captioned with its name, marked with the journeys that
//! pass through it; and each transition an arrow between two, labelled.

use std::collections::BTreeMap;
use std::path::PathBuf;

use underdraw_layout::{Align, Draw, Ink, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Rect as Area};
use vello::peniko::Fill;

use super::{Desk, Pin, Seen, Shows, Target, arrow_pins};
use crate::area::{over, rect};
use crate::chrome::{
    self, ARROW_FONT, CAPTION, MARGIN, MARK, MARK_INSET, MARK_TALL, PIN, SPACE, THUMB, THUMB_TALL,
};
use crate::draw::Shown as State;
use crate::notes::Note;
use crate::reference::{self, On, Reference};
use crate::text::Typesetter;
use crate::window::{edge, head, line, sheet, words};

/// The overview of the description in `dir`: each of its states, in the
/// order the tree lists them, and each transition it declares as an arrow
/// between two.
pub(crate) struct Overview {
    pub dir: PathBuf,
    pub thumbs: Vec<Thumb>,
    pub arrows: Vec<Arrow>,
}

/// One state in the overview, drawn at the overview's width at one pixel
/// per point.
pub(crate) struct Thumb {
    pub state: State,
    /// The tree's line of the state.
    pub line: usize,
    pub scene: Scene,
    pub width: f64,
    pub height: f64,
    /// The journeys that pass through the state, one mark for each step of
    /// one that is in it.
    pub journeys: Vec<Pass>,
}

/// One step of a journey in a state of the overview, marked over its
/// picture: picking the mark plays the journey from that step.
pub(crate) struct Pass {
    /// The tree's line of the journey.
    pub line: usize,
    /// The step, from 0.
    pub step: usize,
    pub label: String,
    /// How wide the label is set, in points.
    pub wide: f64,
}

/// One transition: from one state to another, by their places in
/// [`Overview::thumbs`], labelled with the label of the node acted on and,
/// for a call, the outcome.
pub(crate) struct Arrow {
    pub from: usize,
    pub to: usize,
    pub label: String,
    /// How wide the label is set, in points.
    pub wide: f64,
    /// The transition, as a reference names it.
    pub transition: reference::Transition,
}

impl Overview {
    /// How many states a row holds in a pane `pane` wide.
    fn columns(pane: f64) -> usize {
        ((pane - 2.0 * MARGIN + SPACE) / (THUMB + SPACE)).floor().max(1.0) as usize
    }

    /// Where each state's picture is, in window points: its left, top,
    /// width and height, the height no more than `THUMB_TALL`.
    pub fn cells(&self, pane: &Desk) -> Vec<(f64, f64, f64, f64)> {
        let columns = Self::columns(pane.width);
        self.thumbs
            .iter()
            .enumerate()
            .map(|(index, thumb)| {
                let (column, row) = ((index % columns) as f64, (index / columns) as f64);
                let x = pane.left + MARGIN + column * (THUMB + SPACE);
                let y = MARGIN - pane.scroll + CAPTION + row * (CAPTION + THUMB_TALL + SPACE);
                (x, y, THUMB, (thumb.height * THUMB / thumb.width).min(THUMB_TALL))
            })
            .collect()
    }

    /// The marks of the journeys through each state, in window points,
    /// stacked down from its picture's top left corner, each no wider than
    /// the picture; with the state's line.
    pub fn passes(&self, pane: &Desk) -> Vec<(Rect, &Pass, usize)> {
        let mut list = Vec::new();
        for (thumb, (x, y, w, _)) in self.thumbs.iter().zip(self.cells(pane)) {
            for (index, pass) in thumb.journeys.iter().enumerate() {
                let top = y + MARK_INSET + index as f64 * (MARK_TALL + MARK_INSET / 2.0);
                let wide = pass.wide.min(w - 2.0 * MARK_INSET);
                list.push((rect(x + MARK_INSET, top, wide, MARK_TALL), pass, thumb.line));
            }
        }
        list
    }

    /// The arrows, in window points: each as the stretches it is drawn
    /// with, and where its label is. An arrow runs from the edge of one
    /// picture to the edge of the other, each to the right of its way, so
    /// that two between the same states, either way, run apart, and is
    /// labelled nearer where it starts, so that their labels do too; one
    /// that stays in its state loops out of its right.
    pub fn paths(&self, pane: &Desk) -> Vec<(Vec<(f64, f64)>, Rect)> {
        let cells = self.cells(pane);
        let mut list = Vec::new();
        for (index, arrow) in self.arrows.iter().enumerate() {
            let before =
                self.arrows[..index].iter().filter(|a| (a.from, a.to) == (arrow.from, arrow.to)).count();
            let (x, y, w, h) = cells[arrow.from];
            let (path, middle) = if arrow.from == arrow.to {
                let top = y + 16.0 + before as f64 * 32.0;
                let out = x + w + 20.0;
                let path = vec![(x + w, top), (out, top), (out, top + 18.0), (x + w + 2.0, top + 18.0)];
                (path, (out + 4.0 + arrow.wide / 2.0, top + 9.0))
            } else {
                let to = cells[arrow.to];
                let (from_middle, to_middle) =
                    ((x + w / 2.0, y + h / 2.0), (to.0 + to.2 / 2.0, to.1 + to.3 / 2.0));
                let (dx, dy) = (to_middle.0 - from_middle.0, to_middle.1 - from_middle.1);
                let length = dx.hypot(dy).max(1.0);
                let way = (dx / length, dy / length);
                let off = 6.0 + before as f64 * 20.0;
                let right = (-way.1 * off, way.0 * off);
                let start = edge((x, y, w, h), (from_middle.0 + right.0, from_middle.1 + right.1), way, 3.0);
                let end = edge(to, (to_middle.0 + right.0, to_middle.1 + right.1), (-way.0, -way.1), 3.0);
                let along = 0.3 + before as f64 * 0.15;
                (vec![start, end], (start.0 + (end.0 - start.0) * along, start.1 + (end.1 - start.1) * along))
            };
            list.push((path, rect(middle.0 - arrow.wide / 2.0, middle.1 - 9.0, arrow.wide, 18.0)));
        }
        list
    }

    /// The arrows and their labels drawn, in window points; the one picked
    /// in the strong ink.
    fn arrow_draws(&self, pane: &Desk, picked: Option<usize>) -> Vec<Draw> {
        let mut list = Vec::new();
        for (index, (arrow, (path, at))) in self.arrows.iter().zip(self.paths(pane)).enumerate() {
            // The arrow picked stands out; the others are drawn quieter
            // than their labels, which are what a person reads.
            let (ink, edge, said) = if picked == Some(index) {
                (Ink::Strong, Ink::Strong, Ink::Strong)
            } else {
                (Ink::Muted, Ink::Line, Ink::Text)
            };
            for pair in path.windows(2) {
                list.push(line(pair[0], pair[1], ink));
            }
            list.extend(head(path[path.len() - 2], path[path.len() - 1], ink));
            list.push(Draw::Box {
                rect: at,
                radius: chrome::MARK_RADIUS,
                fill: Some(Ink::Paper),
                stroke: Some(edge),
                dashed: false,
            });
            list.push(Draw::Text {
                rect: rect(f64::from(at.x), f64::from(at.y) + 1.0, f64::from(at.width), 16.0),
                text: arrow.label.clone(),
                font: ARROW_FONT,
                ink: said,
                max_lines: Some(1),
                align: Align::Center,
                underline: false,
                strike: false,
            });
        }
        list
    }
}

impl Shows for Overview {
    fn size(&self, pane: &Desk, _: &mut Typesetter) -> (f64, f64) {
        let rows = self.thumbs.len().div_ceil(Self::columns(pane.width)) as f64;
        (0.0, MARGIN + rows * (CAPTION + THUMB_TALL + SPACE) - SPACE + MARGIN)
    }

    /// An arrow's label, then a journey's mark, then a state's picture or
    /// caption.
    fn target_at(&self, at: (f64, f64), pane: &Desk) -> Option<Target> {
        if let Some(arrow) = self.paths(pane).iter().position(|(_, label)| over(*label, at)) {
            return Some(Target::Arrow(arrow));
        }
        if let Some((_, pass, state)) = self.passes(pane).into_iter().find(|(r, _, _)| over(*r, at)) {
            return Some(Target::Pass { journey: pass.line, step: pass.step, state });
        }
        let cells = self.cells(pane);
        let index = cells.iter().position(|&(x, y, w, h)| over(rect(x, y - CAPTION, w, h + CAPTION), at))?;
        Some(Target::State(self.thumbs[index].line))
    }

    /// Each state on a sheet under its name, a tall one cut at the foot of
    /// its picture; over them the arrows, and the journeys' marks in the
    /// notes' ink, as the node a journey acts on is marked when it plays.
    fn draw(&self, scene: &mut Scene, pane: &Desk, seen: &Seen, _: &mut Typesetter) -> Vec<Draw> {
        let cells = self.cells(pane);
        let mut list = Vec::new();
        for (thumb, &(x, y, w, h)) in self.thumbs.iter().zip(&cells) {
            let ink = if matches!(thumb.state, State::Phase(_)) { Ink::Muted } else { Ink::Text };
            list.push(words(
                rect(x, y - CAPTION, w, 16.0),
                thumb.state.name().to_string(),
                ink,
                chrome::HEADING,
            ));
            sheet(scene, Affine::IDENTITY, rect(x, y, w, h), 0.0);
        }
        for (thumb, &(x, y, w, h)) in self.thumbs.iter().zip(&cells) {
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &Area::new(x, y, x + w, y + h));
            scene.append(&thumb.scene, Some(Affine::translate((x, y)) * Affine::scale(w / thumb.width)));
            scene.pop_layer();
        }
        let picked = match seen.picked {
            Some(Target::Arrow(arrow)) => Some(arrow),
            _ => None,
        };
        list.extend(self.arrow_draws(pane, picked));
        for (at, pass, _) in self.passes(pane) {
            list.push(Draw::Box {
                rect: at,
                radius: chrome::MARK_RADIUS,
                fill: Some(Ink::Note),
                stroke: None,
                dashed: false,
            });
            list.push(Draw::Text {
                rect: rect(f64::from(at.x) + 5.0, f64::from(at.y) + 2.0, f64::from(at.width) - 10.0, 14.0),
                text: pass.label.clone(),
                font: MARK,
                ink: Ink::Paper,
                max_lines: Some(1),
                align: Align::Start,
                underline: false,
                strike: false,
            });
        }
        list
    }

    /// A transition, or a whole state.
    fn reference(&self, target: &Target) -> Option<Reference> {
        let on = match target {
            Target::Arrow(arrow) => {
                On::Transition { transition: self.arrows.get(*arrow)?.transition.clone() }
            }
            Target::State(line) | Target::Pass { state: line, .. } => {
                On::State { state: self.thumbs.iter().find(|thumb| thumb.line == *line)?.state.clone() }
            }
            Target::Node { .. } | Target::Caption | Target::Box(_) => return None,
        };
        Some(Reference { description: self.dir.clone(), on })
    }

    /// Each note on a state a pin at the right of its caption, and each on
    /// a transition one beside its arrow's label.
    fn pins(&self, notes: &BTreeMap<PathBuf, Vec<Note>>, pane: &Desk) -> Vec<Pin> {
        let mut pins = Vec::new();
        if let Some(notes) = notes.get(&self.dir) {
            for (thumb, (x, y, w, _)) in self.thumbs.iter().zip(self.cells(pane)) {
                let on = notes
                    .iter()
                    .filter(|note| matches!(&note.on, On::State { state } if *state == thumb.state));
                for (before, note) in on.enumerate() {
                    let right = x + w - PIN / 2.0 - before as f64 * (PIN + 2.0);
                    let at = rect(right - PIN / 2.0, y - CAPTION / 2.0 - PIN / 2.0 - 2.0, PIN, PIN);
                    pins.push(Pin::of(&self.dir, note, at, false));
                }
            }
        }
        let spots = self
            .paths(pane)
            .into_iter()
            .map(|(_, at)| (f64::from(at.x + at.width) + PIN / 2.0 + 4.0, f64::from(at.y + at.height / 2.0)));
        let references = (0..self.arrows.len()).filter_map(|arrow| self.reference(&Target::Arrow(arrow)));
        pins.extend(arrow_pins(references.zip(spots), notes));
        pins
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{PANE, middle, seen};
    use super::*;

    /// A thumb of `state`, on the tree's line `line`, as tall as a page
    /// 390 wide and 780 tall drawn small.
    fn thumb(state: State, line: usize, journeys: Vec<Pass>) -> Thumb {
        Thumb { state, line, scene: Scene::new(), width: 390.0, height: 780.0, journeys }
    }

    /// Two states, `ready` (line 2, a journey's first step marked on it)
    /// and the phase `loading` (line 3), and one transition from the first
    /// to the second.
    fn overview() -> Overview {
        let pass = Pass { line: 9, step: 0, label: "buy 1".to_string(), wide: 60.0 };
        let thumbs = vec![
            thumb(State::Fixture("ready".to_string()), 2, vec![pass]),
            thumb(State::Phase("loading".to_string()), 3, Vec::new()),
        ];
        let transition = reference::Transition {
            from: "ready".to_string(),
            on: "buy.tap".to_string(),
            outcome: None,
            to: "loading".to_string(),
        };
        let arrow = Arrow { from: 0, to: 1, label: "1".to_string(), wide: 30.0, transition };
        Overview { dir: PathBuf::from("shop"), thumbs, arrows: vec![arrow] }
    }

    #[test]
    fn a_press_is_on_an_arrow_a_journeys_mark_or_a_state() {
        let overview = overview();
        let cells = overview.cells(&PANE);
        assert_eq!(cells[0], (PANE.left + MARGIN, MARGIN + CAPTION, THUMB, THUMB_TALL));
        assert_eq!(cells[1].0, PANE.left + MARGIN + THUMB + SPACE, "side by side");
        let (x, y, _, _) = cells[1];
        assert_eq!(overview.target_at((x + 80.0, y + 40.0), &PANE), Some(Target::State(3)));
        assert_eq!(
            overview.target_at((x + 4.0, y - CAPTION / 2.0), &PANE),
            Some(Target::State(3)),
            "its caption"
        );
        let (label, mark) = (overview.paths(&PANE)[0].1, overview.passes(&PANE)[0].0);
        assert_eq!(overview.target_at(middle(label), &PANE), Some(Target::Arrow(0)));
        let pass = overview.target_at(middle(mark), &PANE);
        assert_eq!(pass, Some(Target::Pass { journey: 9, step: 0, state: 2 }), "over its state");
        assert_eq!(overview.target_at((10.0, 10.0), &PANE), None);

        let transition = overview.reference(&Target::Arrow(0)).unwrap();
        assert!(matches!(&transition.on, On::Transition { transition } if transition.on == "buy.tap"));
        let state = overview.reference(&pass.unwrap()).unwrap();
        assert_eq!(
            state.on,
            On::State { state: State::Fixture("ready".to_string()) },
            "a mark notes its state"
        );
        assert_eq!(overview.reference(&Target::Arrow(1)), None, "past the last arrow");
    }

    #[test]
    fn an_overview_draws_each_state_named_and_its_arrow_picked_strong() {
        let overview = overview();
        let (_, tall) = overview.size(&PANE, &mut Typesetter::new(None).unwrap());
        assert_eq!(tall, MARGIN + CAPTION + THUMB_TALL + MARGIN, "one row");
        let picked = seen(Some(Target::Arrow(0)));
        let drawn = overview.draw(&mut Scene::new(), &PANE, &picked, &mut Typesetter::new(None).unwrap());
        let said = |ink: Ink| {
            drawn.iter().filter_map(move |d| match d {
                Draw::Text { text, ink: i, .. } if *i == ink => Some(text.as_str()),
                _ => None,
            })
        };
        assert_eq!(said(Ink::Muted).collect::<Vec<_>>(), ["loading"], "a phase quieter");
        assert!(said(Ink::Strong).any(|text| text == "1"), "the arrow picked");
        assert!(said(Ink::Paper).any(|text| text == "buy 1"), "the journey's mark");
    }
}
