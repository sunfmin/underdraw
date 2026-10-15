//! A state's pages: the state drawn at each width, side by side, scaled
//! down together to fit the pane, never below [`SMALLEST`]: past that they
//! scroll across. Each is captioned, its nodes tagged with their labels at
//! their own size, each node that acting on goes somewhere marked in the
//! interaction ink, and the node picked and the node a journey acts on next
//! marked over it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use underdraw_layout::{Align, Draw, Ink, Measure, Placed, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Rect as Area, Stroke, Vec2};
use vello::peniko::Fill;

use super::{Desk, Offer, Pin, Seen, Shows, Target};
use crate::act::{Leads, Way};
use crate::area::{over, rect};
use crate::chrome::{self, BADGE, CAPTION, DOT, MARGIN, OFFER_GAP, OFFER_PAD, OFFER_TALL, PIN};
use crate::draw::{Handlers, Origin, Shown as State};
use crate::notes::Note;
use crate::paint;
use crate::reference::{On, Pointed, Reference};
use crate::text::Typesetter;
use crate::window::{SMALLEST, caption, picked, sheet};

/// One state drawn at each width, of the description in `dir`.
pub(crate) struct Pages {
    pub dir: PathBuf,
    /// The state drawn; none where the description has none.
    pub state: Option<State>,
    pub pages: Vec<Page>,
    /// Where acting on each node goes from the state drawn, by id: a node
    /// that goes nowhere is not in it.
    pub ways: BTreeMap<String, Vec<Way>>,
}

/// A node that acting on goes somewhere, as a page shows it, in window
/// points.
#[derive(Debug, PartialEq)]
pub(crate) struct Leading {
    pub id: String,
    /// The page it is on, by its place among them.
    pub page: usize,
    pub node: Area,
    pub leads: Leads,
    /// Its badge, ↗ or →, [`BADGE`] wide at any scale.
    pub badge: Area,
}

/// One state drawn at one width, at one pixel per point.
pub(crate) struct Page {
    pub caption: String,
    pub scene: Scene,
    pub width: f64,
    pub height: f64,
    /// Every node with an id, in the order drawn: one drawn later is over
    /// one drawn before, and an overlay is drawn after what it floats over.
    pub nodes: Vec<Placed>,
    /// The labels the nodes are tagged with, and the "?" of each guess, in
    /// the page's points: drawn apart from `scene`, at their own size.
    pub tags: Vec<Draw>,
    /// The operations each node calls when acted on, by id.
    pub calls: BTreeMap<String, Vec<String>>,
    /// What each node does when acted on, by id.
    pub handlers: BTreeMap<String, Handlers>,
    /// Where each node is written in the description, by id.
    pub origins: BTreeMap<String, Origin>,
}

impl Page {
    /// The node that a press at `at`, in the page's points, is on: the
    /// innermost there, an overlay before what it floats over.
    /// A press on a node's orange "?" is on that node, whatever is under
    /// it.
    pub fn node_at(&self, at: (f64, f64)) -> Option<&Placed> {
        let guessed = self.nodes.iter().rev().find(|node| node.guess_tag.is_some_and(|r| over(r, at)));
        guessed.or_else(|| self.nodes.iter().rev().find(|node| over(node.rect, at)))
    }

    /// Whether a node of id `id` is in it.
    pub fn has(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }

    /// The node written where `origin` says, for the same instance, as it
    /// is placed here.
    pub fn placed(&self, origin: &Origin) -> Option<&Placed> {
        let id = self.origins.iter().find_map(|(id, at)| {
            (at.written == origin.written && at.instance == origin.instance).then_some(id)
        })?;
        self.nodes.iter().find(|node| &node.id == id)
    }

    /// The first node written as `written`: the one a journey's step acts
    /// on.
    fn written(&self, written: &str) -> Option<&Placed> {
        self.nodes.iter().find(|node| self.origins.get(&node.id).is_some_and(|o| o.written == written))
    }
}

impl Pages {
    /// How much the pages are scaled down to fit side by side in a pane
    /// `pane` points wide; never up, and never below [`SMALLEST`].
    pub fn zoom(&self, pane: f64) -> f64 {
        let wide: f64 = self.pages.iter().map(|p| p.width).sum();
        let gaps = MARGIN * (self.pages.len() + 1) as f64;
        ((pane - gaps) / wide).clamp(SMALLEST, 1.0)
    }

    /// Where each page's top-left corner is in the window.
    pub fn origins(&self, pane: &Desk) -> Vec<(f64, f64)> {
        let zoom = self.zoom(pane.width);
        let mut x = pane.left + MARGIN - pane.across;
        let top = MARGIN - pane.scroll + CAPTION;
        self.pages
            .iter()
            .map(|page| {
                let at = (x, top);
                x += page.width * zoom + MARGIN;
                at
            })
            .collect()
    }

    /// The page under `at`, a point in the window, with its place among
    /// them and `at` in its points; none off the pages.
    pub fn on_page(&self, at: (f64, f64), pane: &Desk) -> Option<(usize, &Page, (f64, f64))> {
        let zoom = self.zoom(pane.width);
        self.pages.iter().zip(self.origins(pane)).enumerate().find_map(|(index, (page, (left, top)))| {
            let (x, y) = ((at.0 - left) / zoom, (at.1 - top) / zoom);
            over(rect(0.0, 0.0, page.width, page.height), (x, y)).then_some((index, page, (x, y)))
        })
    }

    /// The node written as `written` in each page: none in one that does
    /// not show it.
    pub fn marked(&self, written: &str) -> Vec<Option<&Placed>> {
        self.pages.iter().map(|page| page.written(written)).collect()
    }

    /// The node `id` on each page that shows it, in window points.
    pub fn picked(&self, id: &str, pane: &Desk) -> Vec<Area> {
        let zoom = self.zoom(pane.width);
        self.pages
            .iter()
            .zip(self.origins(pane))
            .filter_map(|(page, at)| {
                let node = page.nodes.iter().find(|n| n.id == id)?;
                let (x, y, w, h) = Self::spot(node.rect, at, zoom);
                Some(Area::new(x, y, x + w, y + h))
            })
            .collect()
    }

    /// Each node, on each page, that acting on goes somewhere, in the
    /// order drawn; not one a person cannot see, cut away or covered by an
    /// overlay, which layout leaves unlabelled as it does every node no one
    /// can act on there. Its badge sits over its top-right corner,
    /// moved right past its label and its "?" where it would cover either.
    pub fn leading(&self, pane: &Desk) -> Vec<Leading> {
        let zoom = self.zoom(pane.width);
        let mut leading = Vec::new();
        for (index, (page, at)) in self.pages.iter().zip(self.origins(pane)).enumerate() {
            for node in &page.nodes {
                let leads = self.ways.get(&node.id).and_then(|ways| Some(ways.first()?.leads()));
                let (Some(leads), Some(label)) = (leads, node.label) else {
                    continue;
                };
                let (x, y, w, h) = Self::spot(node.rect, at, zoom);
                let label = page.tags.iter().find_map(|tag| match tag {
                    Draw::Tag { rect, text, ink: Ink::Label } if *text == label.to_string() => Some(*rect),
                    _ => None,
                });
                let mut covered: Vec<Area> =
                    label.into_iter().chain(node.guess_tag).map(|r| Self::tag_spot(r, at, zoom)).collect();
                covered.sort_by(|a, b| a.x0.total_cmp(&b.x0));
                let (right, top) = (x + w - 2.0, y + 2.0);
                let mut badge =
                    Area::new(right - BADGE / 2.0, top - BADGE / 2.0, right + BADGE / 2.0, top + BADGE / 2.0);
                for tag in covered {
                    if !badge.intersect(tag.inflate(1.0, 1.0)).is_zero_area() {
                        badge = badge + Vec2::new(tag.x1 + 2.0 - badge.x0, 0.0);
                    }
                }
                let (id, node) = (node.id.clone(), Area::new(x, y, x + w, y + h));
                leading.push(Leading { id, page: index, node, leads, badge });
            }
        }
        leading
    }

    /// A tag at `r`, in the page at `at` drawn at `zoom`, in window points:
    /// where the page puts it, scaled with it, but at its own size.
    fn tag_spot(r: Rect, (left, top): (f64, f64), zoom: f64) -> Area {
        let middle = (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0));
        let (x, y) = (left + middle.0 * zoom, top + middle.1 * zoom);
        let (w, h) = (f64::from(r.width), f64::from(r.height));
        Area::new(x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0)
    }

    /// Where the buttons floating beside node `id` are, each as wide as
    /// `wide` says, in window points: in rows under the node on page
    /// `pointed`, or on the first page that shows it, from the node's left
    /// edge, a row going on under it where the next would pass the page's
    /// right edge, and each row kept inside that edge; over the node where
    /// the page has no room under it. At their own size at any scale, as
    /// the labels are. None where no page shows the node.
    pub fn offered(&self, id: &str, pointed: usize, wide: &[f64], pane: &Desk) -> Vec<Area> {
        let (zoom, origins) = (self.zoom(pane.width), self.origins(pane));
        let has = |index: &usize| self.pages.get(*index).is_some_and(|page| page.has(id));
        let Some(index) = Some(pointed).filter(has).or_else(|| (0..self.pages.len()).find(has)) else {
            return Vec::new();
        };
        let (page, at) = (&self.pages[index], origins[index]);
        let Some(node) = page.nodes.iter().find(|n| n.id == id) else {
            return Vec::new();
        };
        let (x, y, _, h) = Self::spot(node.rect, at, zoom);
        let right = at.0 + page.width * zoom;
        // Each row's buttons, by their place in `wide`.
        let mut rows: Vec<Vec<usize>> = Vec::new();
        let mut end = f64::INFINITY;
        for (index, w) in wide.iter().enumerate() {
            match rows.last_mut() {
                Some(row) if end + OFFER_GAP + w <= right => {
                    row.push(index);
                    end += OFFER_GAP + w;
                }
                _ => {
                    rows.push(vec![index]);
                    end = x + w;
                }
            }
        }
        let tall = rows.len() as f64 * (OFFER_TALL + OFFER_GAP) - OFFER_GAP;
        let under = y + h + OFFER_GAP;
        let top = if under + tall <= at.1 + page.height * zoom { under } else { y - OFFER_GAP - tall };
        let mut offered = Vec::new();
        for (number, row) in rows.iter().enumerate() {
            let long = row.iter().map(|&index| wide[index]).sum::<f64>() + OFFER_GAP * (row.len() - 1) as f64;
            let mut left = x.min(right - long).max(at.0);
            let top = top + number as f64 * (OFFER_TALL + OFFER_GAP);
            for &index in row {
                offered.push(Area::new(left, top, left + wide[index], top + OFFER_TALL));
                left += wide[index] + OFFER_GAP;
            }
        }
        offered
    }

    /// `r`, in the page at `at` drawn at `zoom`, in window points: its
    /// left, top, width and height.
    fn spot(r: Rect, (left, top): (f64, f64), zoom: f64) -> (f64, f64, f64, f64) {
        let (x, y) = (left + f64::from(r.x) * zoom, top + f64::from(r.y) * zoom);
        (x, y, f64::from(r.width) * zoom, f64::from(r.height) * zoom)
    }

    /// The labels the nodes of each page are tagged with, and the "?" of
    /// each guess, in window points: where the page puts them, scaled with
    /// it, but each at its own size, so that it reads as well at
    /// [`SMALLEST`] as at full size.
    fn labels(&self, pane: &Desk) -> Vec<Draw> {
        let zoom = self.zoom(pane.width);
        let mut labels = Vec::new();
        for (page, (left, top)) in self.pages.iter().zip(self.origins(pane)) {
            for tag in &page.tags {
                let Draw::Tag { rect: r, text, ink } = tag else { continue };
                let at = Self::tag_spot(*r, (left, top), zoom);
                labels.push(Draw::Tag {
                    rect: rect(at.x0, at.y0, at.width(), at.height()),
                    text: text.clone(),
                    ink: *ink,
                });
            }
        }
        labels
    }

    /// The reference to node `id`, at its instance in page `pointed`, or in
    /// the first page that has it.
    fn pointed(&self, id: &str, pointed: usize) -> Option<Pointed> {
        let has = |page: &&Page| page.has(id);
        let page = self.pages.get(pointed).filter(has).or_else(|| self.pages.iter().find(has))?;
        let placed = page.nodes.iter().find(|node| node.id == id)?;
        let origin = page.origins.get(id)?.clone();
        Some(Pointed { origin, label: placed.label, width: page.width as u32 })
    }
}

impl Shows for Pages {
    fn size(&self, pane: &Desk, _: &mut Typesetter) -> (f64, f64) {
        let zoom = self.zoom(pane.width);
        let wide = self.pages.iter().map(|page| page.width * zoom + MARGIN).sum::<f64>() + MARGIN;
        let tallest = self.pages.iter().map(|p| p.height).fold(0.0, f64::max);
        (wide, MARGIN + CAPTION + tallest * zoom + MARGIN)
    }

    /// The innermost node under `at`, or a page's caption.
    fn target_at(&self, at: (f64, f64), pane: &Desk) -> Option<Target> {
        if let Some((pointed, page, at)) = self.on_page(at, pane) {
            return page.node_at(at).map(|node| Target::Node { id: node.id.clone(), pointed });
        }
        let zoom = self.zoom(pane.width);
        let on_caption = self
            .pages
            .iter()
            .zip(self.origins(pane))
            .any(|(page, (left, top))| over(rect(left, top - CAPTION, page.width * zoom, CAPTION), at));
        on_caption.then_some(Target::Caption)
    }

    /// Each page on a sheet under its caption, each node that acting on
    /// goes somewhere and the node picked marked in the interaction ink on
    /// each page that shows it; over them, the node a journey acts on next
    /// ringed twice in the notes' ink.
    fn draw(&self, scene: &mut Scene, pane: &Desk, seen: &Seen, typesetter: &mut Typesetter) -> Vec<Draw> {
        let (zoom, origins) = (self.zoom(pane.width), self.origins(pane));
        let mut over = self.labels(pane);
        for (page, &(x, y)) in self.pages.iter().zip(&origins) {
            over.push(caption(rect(x, y - CAPTION, page.width * zoom, 16.0), page.caption.clone()));
            sheet(scene, Affine::IDENTITY, rect(x, y, page.width * zoom, page.height * zoom), 0.0);
        }
        for (page, &(x, y)) in self.pages.iter().zip(&origins) {
            scene.append(&page.scene, Some(Affine::translate((x, y)) * Affine::scale(zoom)));
        }
        for leading in self.leading(pane) {
            lead(scene, &leading, seen.lit, typesetter);
        }
        // No wireframe draws in the notes' ink.
        let marked = seen.marked.as_deref().map(|written| self.marked(written)).unwrap_or_default();
        for (node, &at) in marked.into_iter().zip(&origins) {
            let Some(node) = node else { continue };
            let (x, y, w, h) = Self::spot(node.rect, at, zoom);
            let around = rect(x - 4.0, y - 4.0, w + 8.0, h + 8.0);
            for inset in [0.0, 1.5] {
                over.push(Draw::Box {
                    rect: rect(
                        f64::from(around.x) + inset,
                        f64::from(around.y) + inset,
                        f64::from(around.width) - 2.0 * inset,
                        f64::from(around.height) - 2.0 * inset,
                    ),
                    radius: chrome::PICKED_RADIUS - inset as f32,
                    fill: None,
                    stroke: Some(Ink::Note),
                    dashed: false,
                });
            }
        }
        if let Some(Target::Node { id, pointed }) = &seen.picked {
            for node in self.picked(id, pane) {
                picked(scene, node, chrome::PICKED_RADIUS / 2.0);
            }
            let wide = offer_widths(&seen.offers, typesetter);
            for (offer, area) in seen.offers.iter().zip(self.offered(id, *pointed, &wide, pane)) {
                over.extend(offer_draws(offer, area));
            }
        }
        over
    }

    /// A node at the instance pointed at, or the whole state.
    fn reference(&self, target: &Target) -> Option<Reference> {
        let state = self.state.clone()?;
        let on = match target {
            Target::Node { id, pointed } => match self.pointed(id, *pointed) {
                Some(node) => On::Node { state, node },
                None => On::State { state },
            },
            Target::Caption => On::State { state },
            Target::Arrow(_) | Target::Pass { .. } | Target::State(_) | Target::Box(_) => return None,
        };
        Some(Reference { description: self.dir.clone(), on })
    }

    /// Each note on a node a numbered pin at the top-right corner of its
    /// node, in the page of the state and width it was written in, and a
    /// faint dot there in every other that holds the node; each on the
    /// whole state a pin at the right of the first page's caption.
    fn pins(&self, notes: &BTreeMap<PathBuf, Vec<Note>>, pane: &Desk) -> Vec<Pin> {
        let (Some(state), Some(notes)) = (&self.state, notes.get(&self.dir)) else {
            return Vec::new();
        };
        let zoom = self.zoom(pane.width);
        let mut pins: Vec<Pin> = Vec::new();
        for (index, (page, (left, top))) in self.pages.iter().zip(self.origins(pane)).enumerate() {
            // How many pins and dots each node has had so far, to set the
            // next one beside them; and the whole state.
            let mut on: BTreeMap<String, usize> = BTreeMap::new();
            let mut whole = 0;
            for note in notes {
                let (written, r, key, width) = match &note.on {
                    On::Node { state: written, node } => {
                        let Some(placed) = page.placed(&node.origin) else { continue };
                        (written, placed.rect, placed.id.clone(), node.width)
                    }
                    On::State { state: written } if written == state && index == 0 => {
                        let right = left + page.width * zoom - PIN / 2.0 - whole as f64 * (PIN + 2.0);
                        whole += 1;
                        let at = rect(right - PIN / 2.0, top - CAPTION / 2.0 - PIN / 2.0 - 2.0, PIN, PIN);
                        pins.push(Pin::of(&self.dir, note, at, false));
                        continue;
                    }
                    _ => continue,
                };
                let corner = (left + f64::from(r.x + r.width) * zoom, top + f64::from(r.y) * zoom);
                let before = on.entry(key).or_default();
                let faint = written != state || page.width as u32 != width;
                let side = if faint { DOT } else { PIN };
                let x = corner.0 - side / 2.0 - *before as f64 * (side + 2.0);
                *before += 1;
                let at = rect(x, corner.1 - side / 2.0, side, side);
                pins.push(Pin::of(&self.dir, note, at, faint));
            }
        }
        pins
    }
}

/// A node that acting on goes somewhere, marked in the interaction ink: a
/// thin dashed outline round it and its badge; washed and outlined solid
/// instead while `lit`, ⌘ held or a flash. In window points, so it reads
/// at any scale.
fn lead(scene: &mut Scene, leading: &Leading, lit: bool, typesetter: &mut Typesetter) {
    let at = Affine::IDENTITY;
    let around = leading.node.inflate(1.5, 1.5).to_rounded_rect(2.0);
    if lit {
        scene.fill(Fill::NonZero, at, chrome::INTERACTION_FILL, None, &around);
        scene.stroke(&Stroke::new(2.0), at, chrome::INTERACTION, None, &around);
    } else {
        scene.stroke(&Stroke::new(1.0).with_dashes(0.0, [3.0, 2.0]), at, chrome::INTERACTION, None, &around);
    }
    let b = leading.badge;
    let badge = Draw::Tag {
        rect: rect(b.x0, b.y0, b.width(), b.height()),
        text: leading.leads.badge().to_string(),
        ink: Ink::Interaction,
    };
    paint::draws(scene, at, &[badge], chrome::colour, typesetter);
}

/// How wide each of `offers` is drawn: its text, set in the buttons' type,
/// and the space either side of it.
pub(crate) fn offer_widths(offers: &[Offer], typesetter: &mut Typesetter) -> Vec<f64> {
    offers
        .iter()
        .map(|offer| f64::from(typesetter.text(&offer.says, chrome::BUTTON, None).width) + 2.0 * OFFER_PAD)
        .collect()
}

/// `offer` drawn at `area`: filled with the interaction ink where it goes
/// there, its text on it in the paper's; grayed where it only says where it
/// would.
fn offer_draws(offer: &Offer, area: Area) -> [Draw; 2] {
    let (fill, stroke, ink) = match offer.live {
        true => (Ink::Interaction, None, Ink::Paper),
        false => (Ink::Paper, Some(Ink::Line), Ink::Muted),
    };
    let tall = f64::from(chrome::BUTTON.line_height);
    [
        Draw::Box {
            rect: rect(area.x0, area.y0, area.width(), area.height()),
            radius: chrome::BUTTON_RADIUS,
            fill: Some(fill),
            stroke,
            dashed: false,
        },
        Draw::Text {
            rect: rect(area.x0, area.y0 + (area.height() - tall) / 2.0, area.width(), tall),
            text: offer.says.clone(),
            font: chrome::BUTTON,
            ink,
            max_lines: Some(1),
            align: Align::Center,
            underline: false,
            strike: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::super::tests::{PANE, seen};
    use super::*;

    /// A node `id` written as `written`, at `r`, in page points.
    fn node(id: &str, (x, y, width, height): (f32, f32, f32, f32)) -> Placed {
        Placed {
            id: id.to_string(),
            component: "Box".to_string(),
            rect: Rect { x, y, width, height },
            label: None,
            assumed: None,
            guess_tag: None,
        }
    }

    /// One state on one page 390 wide: a card, and a button inside it.
    fn pages() -> Pages {
        let nodes = vec![node("card", (10.0, 10.0, 200.0, 100.0)), node("buy", (20.0, 20.0, 50.0, 30.0))];
        let origins = nodes
            .iter()
            .map(|n| {
                (n.id.clone(), Origin { written: n.id.clone(), at: String::new(), instance: Vec::new() })
            })
            .collect();
        let page = Page {
            caption: "default · 390".to_string(),
            scene: Scene::new(),
            width: 390.0,
            height: 600.0,
            nodes,
            tags: Vec::new(),
            calls: BTreeMap::new(),
            handlers: BTreeMap::new(),
            origins,
        };
        let state = Some(State::Fixture("default".to_string()));
        let ways =
            BTreeMap::from([("buy".to_string(), vec![Way::To { outcome: None, to: "paid".to_string() }])]);
        Pages { dir: PathBuf::from("shop"), state, pages: vec![page], ways }
    }

    #[test]
    fn a_press_is_on_the_innermost_node_or_the_caption_and_references_it() {
        let pages = pages();
        // Unscaled in a pane this wide, under its caption.
        let (left, top) = (PANE.left + MARGIN, MARGIN + CAPTION);
        assert_eq!(pages.origins(&PANE), vec![(left, top)]);
        let on = |x: f64, y: f64| pages.target_at((left + x, top + y), &PANE);
        let buy = Target::Node { id: "buy".to_string(), pointed: 0 };
        assert_eq!(on(25.0, 25.0), Some(buy.clone()), "the innermost");
        assert_eq!(on(150.0, 50.0), Some(Target::Node { id: "card".to_string(), pointed: 0 }));
        assert_eq!(on(300.0, 300.0), None, "on the page, on no node");
        assert_eq!(on(4.0, -CAPTION / 2.0), Some(Target::Caption));
        assert_eq!(on(-2.0 * MARGIN, 10.0), None, "off the page");

        let reference = pages.reference(&buy).unwrap();
        assert_eq!(reference.description, PathBuf::from("shop"));
        assert!(
            matches!(&reference.on, On::Node { node, .. } if node.origin.written == "buy"),
            "{reference:?}"
        );
        let whole = pages.reference(&Target::Caption).unwrap();
        assert!(matches!(whole.on, On::State { .. }), "the caption is the whole state");
        assert_eq!(pages.reference(&Target::Arrow(0)), None, "pages have no arrows");
    }

    #[test]
    fn pages_draw_their_caption_and_mark_the_node_picked() {
        let pages = pages();
        let (wide, tall) = pages.size(&PANE, &mut Typesetter::new(None).unwrap());
        assert_eq!((wide, tall), (MARGIN + 390.0 + MARGIN, MARGIN + CAPTION + 600.0 + MARGIN));
        let picked = Some(Target::Node { id: "buy".to_string(), pointed: 0 });
        let drawn = pages.draw(&mut Scene::new(), &PANE, &seen(picked), &mut Typesetter::new(None).unwrap());
        assert!(drawn.iter().any(|d| matches!(d, Draw::Text { text, .. } if text == "default · 390")));
        assert!(
            !drawn.iter().any(|d| matches!(d, Draw::Box { stroke: Some(Ink::Strong), .. })),
            "not in the wireframe's strong ink"
        );
        let (left, top) = (PANE.left + MARGIN, MARGIN + CAPTION);
        assert_eq!(
            pages.picked("buy", &PANE),
            vec![Area::new(left + 20.0, top + 20.0, left + 70.0, top + 50.0)],
            "the button, on the page that shows it"
        );
        assert!(pages.picked("gone", &PANE).is_empty());
    }

    #[test]
    fn a_node_that_goes_somewhere_is_badged_at_its_top_right_clear_of_its_label_and_its_guess() {
        let mut pages = pages();
        let (left, top) = (PANE.left + MARGIN, MARGIN + CAPTION);
        assert!(pages.leading(&PANE).is_empty(), "unlabelled, covered by an overlay: not marked");
        let page = &mut pages.pages[0];
        page.nodes[1].label = Some(1);
        page.tags.push(Draw::Tag {
            rect: Rect { x: 14.0, y: 14.0, width: 18.0, height: 18.0 },
            text: "1".to_string(),
            ink: Ink::Label,
        });
        let leading = pages.leading(&PANE);
        assert_eq!(leading.len(), 1, "only the button goes somewhere");
        let buy = &leading[0];
        assert_eq!((buy.id.as_str(), buy.page, buy.leads), ("buy", 0, Leads::State));
        assert_eq!(buy.node, Area::new(left + 20.0, top + 20.0, left + 70.0, top + 50.0));
        let corner = (left + 68.0, top + 22.0);
        assert_eq!(buy.badge.center(), corner.into(), "over its top-right corner");
        assert_eq!(buy.badge.width(), BADGE, "at its own size");

        // Its "?" where the badge would go, and its label too in a node so
        // narrow its label reaches its right: the badge moved right of both.
        let page = &mut pages.pages[0];
        let tag = Rect { x: 58.0, y: 14.0, width: 18.0, height: 18.0 };
        page.nodes[1].guess_tag = Some(tag);
        let leading = pages.leading(&PANE);
        let badge = leading[0].badge;
        assert_eq!(badge.x0, left + 76.0 + 2.0, "right of the \"?\"");
        assert_eq!(badge.center().y, corner.1, "at the same height");
        let page = &mut pages.pages[0];
        page.nodes[1].guess_tag = None;
        page.nodes[1].rect.width = 10.0;
        let badge = pages.leading(&PANE)[0].badge;
        assert_eq!(badge.x0, left + 32.0 + 2.0, "right of the label");
    }
}
