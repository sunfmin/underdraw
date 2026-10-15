//! Why a description or a journey cannot be shown: a heading, then each
//! finding as `check` names it, where it is over what is wrong, listed down
//! the pane. Nothing on it is pressed or noted.

use std::collections::BTreeMap;
use std::path::PathBuf;

use underdraw_format::{Finding, Lang, say};
use underdraw_layout::{Draw, Ink, Measure};
use vello::Scene;

use super::{Pane, Pin, Seen, Shows, Target};
use crate::area::rect;
use crate::chrome::{self, CAPTION, GAP, MARGIN};
use crate::notes::Note;
use crate::reference::Reference;
use crate::text::Typesetter;
use crate::window::{moved, words};

/// The faults of the description, or the journey, at `within`.
#[derive(Debug)]
pub(crate) struct Faults {
    pub findings: Vec<Finding>,
    /// Where it is, under the root.
    pub within: String,
    /// Whether it is a journey's, which is not played rather than not drawn.
    pub journey: bool,
}

impl Faults {
    /// The faults listed in a pane `pane` wide, from its top, unscrolled,
    /// in `lang`. How far down the list ends.
    pub fn list(&self, pane: f64, lang: Lang, typesetter: &mut Typesetter) -> (Vec<Draw>, f64) {
        let wide = pane - 2.0 * MARGIN;
        let within = &self.within;
        let heading = match (self.journey, self.findings.len()) {
            (true, 1) => say!(lang, faults_not_played_one, within),
            (true, n) => say!(lang, faults_not_played_many, within, n),
            (false, 1) => say!(lang, faults_not_drawn_one, within),
            (false, n) => say!(lang, faults_not_drawn_many, within, n),
        };
        let mut list = vec![words(rect(0.0, MARGIN, wide, 18.0), heading, Ink::Text, chrome::HEADING)];
        let mut y = MARGIN + CAPTION + GAP;
        for fault in &self.findings {
            list.push(words(rect(0.0, y, wide, 16.0), fault.at.clone(), Ink::Muted, chrome::HEADING));
            y += 18.0;
            let tall = f64::from(typesetter.text(&fault.problem, chrome::BODY, Some(wide as f32)).height);
            let mut problem = words(rect(0.0, y, wide, tall), fault.problem.clone(), Ink::Text, 400);
            if let Draw::Text { max_lines, .. } = &mut problem {
                *max_lines = None;
            }
            list.push(problem);
            y += tall + GAP;
        }
        (list, y)
    }
}

impl Shows for Faults {
    fn size(&self, pane: &Pane, typesetter: &mut Typesetter) -> (f64, f64) {
        // The heading is the same height in every language.
        (0.0, self.list(pane.width, Lang::default(), typesetter).1 + MARGIN)
    }

    fn target_at(&self, _: (f64, f64), _: &Pane) -> Option<Target> {
        None
    }

    fn draw(&self, _: &mut Scene, pane: &Pane, seen: &Seen, typesetter: &mut Typesetter) -> Vec<Draw> {
        let (list, _) = self.list(pane.width, seen.lang, typesetter);
        list.into_iter().map(|draw| moved(draw, pane.left + MARGIN, -pane.scroll)).collect()
    }

    fn reference(&self, _: &Target) -> Option<Reference> {
        None
    }

    fn pins(&self, _: &BTreeMap<PathBuf, Vec<Note>>, _: &Pane) -> Vec<Pin> {
        Vec::new()
    }
}
