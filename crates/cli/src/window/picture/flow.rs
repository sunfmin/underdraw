//! A directory's flow: the map of the screens under it ([`crate::flow`]),
//! each box showing its description's first state, and each navigate an
//! arrow labelled with its node; scaled down to fit the pane's width.

use std::collections::BTreeMap;
use std::path::PathBuf;

use underdraw_format::{Lang, say};
use underdraw_layout::{Align, Draw, Font, Ink, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Rect as Area};
use vello::peniko::Fill;

use super::{Desk, Pin, Seen, Shows, Target, arrow_pins};
use crate::area::{over, rect};
use crate::chrome::{self, ARROWS, MARGIN, MARK, PIN};
use crate::flow::{self, Map, Screen};
use crate::notes::Note;
use crate::paint;
use crate::reference::{Navigate, On, Reference};
use crate::text::Typesetter;
use crate::window::{head, line, sheet, words};

/// The flow between the screens under a directory: the map of them, and
/// what each of its boxes shows.
pub(crate) struct Flow {
    pub map: Map,
    /// One for each of the map's places, in their order.
    pub tiles: Vec<Tile>,
}

/// What one box of the flow shows under its name.
pub(crate) enum Tile {
    /// The description's first state, drawn at the phone's width.
    Picture { scene: Box<Scene>, width: f64 },
    /// A description that cannot be drawn: how many faults it has.
    Faulty(usize),
    /// A screen not drawn yet: a dashed box marked "?".
    Undrawn,
}

impl Flow {
    /// How much it is scaled down to fit a pane `pane` wide; never up.
    pub fn zoom(&self, pane: f64) -> f64 {
        ((pane - 2.0 * MARGIN) / self.map.width.max(1.0)).clamp(0.1, 1.0)
    }

    /// Where its top-left corner is in the window.
    pub fn origin(pane: &Desk) -> (f64, f64) {
        (pane.left + MARGIN, MARGIN - pane.scroll)
    }

    /// Its arrows, in its own points: each as the stretches it is drawn
    /// with, and where it is named, where it leaves its box, so that the
    /// arrows from one box, each leaving at its own height, are named apart.
    fn arrows(&self) -> Vec<(Vec<(f64, f64)>, Rect)> {
        let (places, arrows) = (&self.map.places, &self.map.arrows);
        let mut list = Vec::new();
        for (index, arrow) in arrows.iter().enumerate() {
            let (from, to) = (&places[arrow.from], &places[arrow.to]);
            let leaving = arrows[..index].iter().filter(|a| a.from == arrow.from).count() as f64;
            let reaching = arrows[..index].iter().filter(|a| a.to == arrow.to).count() as f64;
            let out = from.y + flow::TITLE + ARROWS + leaving * ARROWS;
            let into = to.y + flow::TITLE + ARROWS + reaching * ARROWS;
            let path = if arrow.from == arrow.to {
                let (side, low) = (from.x + flow::WIDE, out + ARROWS / 2.0);
                vec![(side, out), (side + 30.0, out), (side + 30.0, low), (side, low)]
            } else if to.x > from.x {
                vec![(from.x + flow::WIDE, out), (to.x, into)]
            } else if to.x < from.x {
                vec![(from.x, out), (to.x + flow::WIDE, into)]
            } else if to.y > from.y {
                vec![(from.x + flow::WIDE / 2.0, from.y + flow::TALL), (to.x + flow::WIDE / 2.0, to.y)]
            } else {
                vec![(from.x + flow::WIDE / 2.0, from.y), (to.x + flow::WIDE / 2.0, to.y + flow::TALL)]
            };
            let start = path[0];
            let x = if path[1].0 < start.0 { start.0 - 134.0 } else { start.0 + 6.0 };
            list.push((path, rect(x, start.1 - 17.0, 128.0, 16.0)));
        }
        list
    }

    /// Where each arrow is named, in window points, and where its notes'
    /// pins start: under it, where it leaves its box.
    pub fn spots(&self, pane: &Desk) -> Vec<(Rect, (f64, f64))> {
        let ((left, top), zoom) = (Self::origin(pane), self.zoom(pane.width));
        let window = |(x, y): (f64, f64)| (left + x * zoom, top + y * zoom);
        self.arrows()
            .into_iter()
            .map(|(path, at)| {
                let (x, y) = window((f64::from(at.x), f64::from(at.y)));
                let label = rect(x, y, f64::from(at.width) * zoom, f64::from(at.height) * zoom);
                let (start, next) = (window(path[0]), window(path[1]));
                let middle = ((start.0 + next.0) / 2.0, (start.1 + next.1) / 2.0 + PIN / 2.0 + 3.0);
                (label, middle)
            })
            .collect()
    }

    /// It drawn, in its own points, but for the pictures in its boxes:
    /// each box with its name over it, and each navigate an arrow labelled
    /// with its node, in orange to a screen not drawn yet; the one at
    /// `picked` in the strong ink.
    pub fn draws(&self, picked: Option<usize>, lang: Lang) -> Vec<Draw> {
        let mut list = Vec::new();
        let (places, arrows) = (&self.map.places, &self.map.arrows);
        for (index, (arrow, (path, at))) in arrows.iter().zip(self.arrows()).enumerate() {
            let ink = if picked == Some(index) {
                Ink::Strong
            } else if matches!(places[arrow.to].screen, Screen::Undrawn(_)) {
                Ink::Assumed
            } else {
                Ink::Muted
            };
            for pair in path.windows(2) {
                list.push(line(pair[0], pair[1], ink));
            }
            list.extend(head(path[path.len() - 2], path[path.len() - 1], ink));
            let mut label = words(at, arrow.link.node.clone(), ink, 400);
            if let Draw::Text { align, font, .. } = &mut label {
                *font = Font { weight: 500, ..MARK };
                if path[1].0 < path[0].0 {
                    *align = Align::End;
                }
            }
            list.push(label);
        }
        for (place, card) in places.iter().zip(&self.tiles) {
            let (x, y) = (place.x, place.y);
            let (name, ink) = match &place.screen {
                Screen::Drawn(dir) => {
                    let name =
                        dir.file_name().map_or_else(|| ".".to_string(), |n| n.to_string_lossy().to_string());
                    (name, Ink::Text)
                }
                Screen::Undrawn(screen) => (screen.clone(), Ink::Assumed),
            };
            let undrawn = matches!(card, Tile::Undrawn);
            list.push(Draw::Box {
                rect: rect(x, y, flow::WIDE, flow::TALL),
                radius: chrome::SHEET_RADIUS,
                fill: None,
                stroke: undrawn.then_some(Ink::Assumed),
                dashed: undrawn,
            });
            list.push(words(rect(x + 8.0, y + 4.0, flow::WIDE - 16.0, 16.0), name, ink, chrome::HEADING));
            list.push(line((x, y + flow::TITLE), (x + flow::WIDE, y + flow::TITLE), Ink::Line));
            let middle = (x + flow::WIDE / 2.0, y + (flow::TITLE + flow::TALL) / 2.0);
            match card {
                Tile::Picture { .. } => {}
                Tile::Faulty(faults) => {
                    let mark = match faults {
                        1 => say!(lang, faulty_one),
                        n => say!(lang, faulty_many, n),
                    };
                    let mut said = words(
                        rect(x + 8.0, middle.1 - 8.0, flow::WIDE - 16.0, 16.0),
                        mark,
                        Ink::Strong,
                        chrome::HEADING,
                    );
                    if let Draw::Text { align, .. } = &mut said {
                        *align = Align::Center;
                    }
                    list.push(said);
                }
                Tile::Undrawn => list.push(Draw::Tag {
                    rect: rect(middle.0 - 14.0, middle.1 - 14.0, 28.0, 28.0),
                    text: "?".to_string(),
                    ink: Ink::Assumed,
                }),
            }
        }
        list
    }
}

impl Shows for Flow {
    fn size(&self, pane: &Desk, _: &mut Typesetter) -> (f64, f64) {
        (0.0, MARGIN + self.map.height * self.zoom(pane.width) + MARGIN)
    }

    /// An arrow's label, then the box of a description drawn.
    fn target_at(&self, at: (f64, f64), pane: &Desk) -> Option<Target> {
        if let Some(arrow) = self.spots(pane).iter().position(|(label, _)| over(*label, at)) {
            return Some(Target::Arrow(arrow));
        }
        let ((left, top), zoom) = (Self::origin(pane), self.zoom(pane.width));
        let at = ((at.0 - left) / zoom, (at.1 - top) / zoom);
        match &self.map.place_at(at)?.screen {
            Screen::Drawn(dir) => Some(Target::Box(dir.clone())),
            Screen::Undrawn(_) => None,
        }
    }

    /// Each box of a description on a sheet, its picture cut at its foot;
    /// then the boxes' names and the arrows, all in the flow's own points
    /// scaled to the pane.
    fn draw(&self, scene: &mut Scene, pane: &Desk, seen: &Seen, typesetter: &mut Typesetter) -> Vec<Draw> {
        let at = Affine::translate(Self::origin(pane)) * Affine::scale(self.zoom(pane.width));
        for (place, tile) in self.map.places.iter().zip(&self.tiles) {
            if !matches!(tile, Tile::Undrawn) {
                sheet(scene, at, rect(place.x, place.y, flow::WIDE, flow::TALL), chrome::SHEET_RADIUS);
            }
        }
        for (place, tile) in self.map.places.iter().zip(&self.tiles) {
            let Tile::Picture { scene: picture, width } = tile else { continue };
            let (x, y) = (place.x, place.y + flow::TITLE);
            scene.push_clip_layer(Fill::NonZero, at, &Area::new(x, y, x + flow::WIDE, place.y + flow::TALL));
            let small = Affine::translate((x, y)) * Affine::scale(flow::WIDE / width);
            scene.append(picture, Some(at * small));
            scene.pop_layer();
        }
        let picked = match seen.picked {
            Some(Target::Arrow(arrow)) => Some(arrow),
            _ => None,
        };
        paint::draws(scene, at, &self.draws(picked, seen.lang), chrome::colour, typesetter);
        Vec::new()
    }

    /// A navigate, in the description it is written in.
    fn reference(&self, target: &Target) -> Option<Reference> {
        let Target::Arrow(arrow) = target else {
            return None;
        };
        let link = &self.map.arrows.get(*arrow)?.link;
        let navigate = Navigate { from: link.from.clone(), node: link.node.clone(), to: link.to.clone() };
        Some(Reference { description: link.description.clone(), on: On::Navigate { navigate } })
    }

    /// Each note on a navigate a pin under its arrow, where it leaves its
    /// box.
    fn pins(&self, notes: &BTreeMap<PathBuf, Vec<Note>>, pane: &Desk) -> Vec<Pin> {
        let references = (0..self.map.arrows.len()).filter_map(|arrow| self.reference(&Target::Arrow(arrow)));
        let spots = self.spots(pane).into_iter().map(|(_, pin)| pin);
        arrow_pins(references.zip(spots), notes)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{PANE, middle, seen};
    use super::*;
    use crate::flow::{Arrow, Place};
    use crate::links::Link;

    /// The screens `home` and `cart`, side by side, and `pay`, not drawn
    /// yet, under `cart`; `home` navigates to `cart`.
    fn flow() -> Flow {
        let place = |screen, x, y| Place { screen, x, y };
        let places = vec![
            place(Screen::Drawn(PathBuf::from("shop/home")), 0.0, 0.0),
            place(Screen::Drawn(PathBuf::from("shop/cart")), 320.0, 0.0),
            place(Screen::Undrawn("Pay".to_string()), 320.0, 340.0),
        ];
        let link = Link {
            description: PathBuf::from("shop/home"),
            from: "Home".to_string(),
            node: "open-cart".to_string(),
            to: "Cart".to_string(),
            target: Some(PathBuf::from("shop/cart")),
        };
        let map = Map { places, arrows: vec![Arrow { from: 0, to: 1, link }], width: 500.0, height: 640.0 };
        Flow { map, tiles: vec![Tile::Faulty(1), Tile::Faulty(2), Tile::Undrawn] }
    }

    #[test]
    fn a_press_is_on_an_arrow_or_the_box_of_a_description_drawn() {
        let flow = flow();
        let (left, top) = Flow::origin(&PANE);
        assert_eq!((left, top), (PANE.left + MARGIN, MARGIN));
        let on = |x: f64, y: f64| flow.target_at((left + x, top + y), &PANE);
        assert_eq!(on(90.0, 150.0), Some(Target::Box(PathBuf::from("shop/home"))));
        assert_eq!(on(410.0, 500.0), None, "a screen not drawn yet");
        let label = flow.spots(&PANE)[0].0;
        assert_eq!(flow.target_at(middle(label), &PANE), Some(Target::Arrow(0)));

        let navigate = flow.reference(&Target::Arrow(0)).unwrap();
        assert_eq!(navigate.description, PathBuf::from("shop/home"), "where it is written");
        assert!(matches!(&navigate.on, On::Navigate { navigate } if navigate.node == "open-cart"));
        assert_eq!(flow.reference(&Target::Box(PathBuf::from("shop/home"))), None, "a box is not noted");
    }

    #[test]
    fn a_flow_draws_its_boxes_and_its_arrow_picked_strong() {
        let flow = flow();
        let mut typesetter = Typesetter::new(None).unwrap();
        assert_eq!(flow.size(&PANE, &mut typesetter), (0.0, MARGIN + 640.0 + MARGIN), "never scaled up");
        let picked = seen(Some(Target::Arrow(0)));
        assert!(flow.draw(&mut Scene::new(), &PANE, &picked, &mut typesetter).is_empty(), "all in its scene");
        let drawn = flow.draws(Some(0), Lang::En);
        let texts: Vec<(&str, Ink)> = drawn
            .iter()
            .filter_map(|d| match d {
                Draw::Text { text, ink, .. } => Some((text.as_str(), *ink)),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&("open-cart", Ink::Strong)), "{texts:?}");
        assert!(texts.contains(&("Pay", Ink::Assumed)), "{texts:?}");
    }
}
