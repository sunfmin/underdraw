//! A state's pages: the state drawn at each width, side by side, scaled
//! down together to fit the pane, never below [`SMALLEST`]: past that they
//! scroll across. Each is captioned, its nodes tagged with their labels at
//! their own size, and the node picked, the region picked and the node a
//! journey acts on next marked over it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use underdraw_layout::{Draw, Ink, Placed, Rect};
use vello::Scene;
use vello::kurbo::Affine;

use super::{Pane, Pin, Seen, Shows, Target};
use crate::area::{over, rect};
use crate::chrome::{self, CAPTION, DOT, MARGIN, PIN};
use crate::draw::{Handlers, Origin, Shown as State};
use crate::notes::Note;
use crate::reference::{Frame, On, Pointed, Reference, Region};
use crate::text::Typesetter;
use crate::window::{SMALLEST, caption, sheet};

/// One state drawn at each width, of the description in `dir`.
pub(crate) struct Pages {
    pub dir: PathBuf,
    /// The state drawn; none where the description has none.
    pub state: Option<State>,
    pub pages: Vec<Page>,
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
        let guessed = self.nodes.iter().rev().find(|node| node.guess_tag().is_some_and(|r| over(r, at)));
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

    /// Where `region` is drawn here: around the nodes it covers that are
    /// here, as the layout now places them; where none of them is, at its
    /// box, in a picture of the width it was dragged at; elsewhere, nowhere.
    pub fn region(&self, region: &Region) -> Option<Rect> {
        let found: Vec<Rect> =
            region.nodes.iter().filter_map(|origin| Some(self.placed(origin)?.rect)).collect();
        let Some(first) = found.first() else {
            let Frame { x, y, width, height } = region.area;
            let here = self.width as u32 == region.width;
            return here.then_some(Rect {
                x: x as f32,
                y: y as f32,
                width: width as f32,
                height: height as f32,
            });
        };
        let (mut x, mut y) = (first.x, first.y);
        let (mut right, mut bottom) = (first.x + first.width, first.y + first.height);
        for r in &found {
            (x, y) = (x.min(r.x), y.min(r.y));
            (right, bottom) = (right.max(r.x + r.width), bottom.max(r.y + r.height));
        }
        Some(Rect { x, y, width: right - x, height: bottom - y })
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
    pub fn origins(&self, pane: &Pane) -> Vec<(f64, f64)> {
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
    pub fn on_page(&self, at: (f64, f64), pane: &Pane) -> Option<(usize, &Page, (f64, f64))> {
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

    /// `r`, in the page at `at` drawn at `zoom`, in window points: its
    /// left, top, width and height.
    fn spot(r: Rect, (left, top): (f64, f64), zoom: f64) -> (f64, f64, f64, f64) {
        let (x, y) = (left + f64::from(r.x) * zoom, top + f64::from(r.y) * zoom);
        (x, y, f64::from(r.width) * zoom, f64::from(r.height) * zoom)
    }

    /// `r`, in the page at `at` drawn at `zoom`, in window points.
    fn window(r: Rect, at: (f64, f64), zoom: f64) -> Rect {
        let (x, y, w, h) = Self::spot(r, at, zoom);
        rect(x, y, w, h)
    }

    /// The labels the nodes of each page are tagged with, and the "?" of
    /// each guess, in window points: where the page puts them, scaled with
    /// it, but each at its own size, so that it reads as well at
    /// [`SMALLEST`] as at full size.
    fn labels(&self, pane: &Pane) -> Vec<Draw> {
        let zoom = self.zoom(pane.width);
        let mut labels = Vec::new();
        for (page, (left, top)) in self.pages.iter().zip(self.origins(pane)) {
            for tag in &page.tags {
                let Draw::Tag { rect: r, text, ink } = tag else { continue };
                let middle = (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0));
                let (x, y) = (left + middle.0 * zoom, top + middle.1 * zoom);
                let (w, h) = (f64::from(r.width), f64::from(r.height));
                labels.push(Draw::Tag {
                    rect: rect(x - w / 2.0, y - h / 2.0, w, h),
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
    fn size(&self, pane: &Pane, _: &mut Typesetter) -> (f64, f64) {
        let zoom = self.zoom(pane.width);
        let wide = self.pages.iter().map(|page| page.width * zoom + MARGIN).sum::<f64>() + MARGIN;
        let tallest = self.pages.iter().map(|p| p.height).fold(0.0, f64::max);
        (wide, MARGIN + CAPTION + tallest * zoom + MARGIN)
    }

    /// The innermost node under `at`, or a page's caption.
    fn target_at(&self, at: (f64, f64), pane: &Pane) -> Option<Target> {
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

    /// Each page on a sheet under its caption; over them, the node a
    /// journey acts on next ringed twice in the notes' ink, the node picked
    /// outlined, and the region picked and the one being dragged dashed.
    fn draw(&self, scene: &mut Scene, pane: &Pane, seen: &Seen, _: &mut Typesetter) -> Vec<Draw> {
        let (zoom, origins) = (self.zoom(pane.width), self.origins(pane));
        let mut over = self.labels(pane);
        for (page, &(x, y)) in self.pages.iter().zip(&origins) {
            over.push(caption(rect(x, y - CAPTION, page.width * zoom, 16.0), page.caption.clone()));
            sheet(scene, Affine::IDENTITY, rect(x, y, page.width * zoom, page.height * zoom), 0.0);
        }
        for (page, &(x, y)) in self.pages.iter().zip(&origins) {
            scene.append(&page.scene, Some(Affine::translate((x, y)) * Affine::scale(zoom)));
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
        if let Some(Target::Node { id, .. }) = &seen.picked {
            for (page, &at) in self.pages.iter().zip(&origins) {
                let Some(node) = page.nodes.iter().find(|n| &n.id == id) else { continue };
                let (x, y, w, h) = Self::spot(node.rect, at, zoom);
                over.push(Draw::Box {
                    rect: rect(x - 2.0, y - 2.0, w + 4.0, h + 4.0),
                    radius: chrome::PICKED_RADIUS / 2.0,
                    fill: None,
                    stroke: Some(Ink::Strong),
                    dashed: false,
                });
            }
        }
        let mut regions = Vec::new();
        if let Some(Target::Region { region, .. }) = &seen.picked {
            for (page, &at) in self.pages.iter().zip(&origins) {
                regions.extend(page.region(region).map(|r| Self::window(r, at, zoom)));
            }
        }
        regions.extend(seen.dragged);
        for around in regions {
            over.push(Draw::Box {
                rect: around,
                radius: 0.0,
                fill: None,
                stroke: Some(Ink::Strong),
                dashed: true,
            });
        }
        over
    }

    /// A node at the instance pointed at, a region, or the whole state.
    fn reference(&self, target: &Target) -> Option<Reference> {
        let state = self.state.clone()?;
        let on = match target {
            Target::Region { region, .. } => On::Region { state, region: region.clone() },
            Target::Node { id, pointed } => match self.pointed(id, *pointed) {
                Some(node) => On::Node { state, node },
                None => On::State { state },
            },
            Target::Caption => On::State { state },
            Target::Arrow(_) | Target::Pass { .. } | Target::State(_) | Target::Box(_) => return None,
        };
        Some(Reference { description: self.dir.clone(), on })
    }

    /// Each note on a node or a region a numbered pin at the top-right
    /// corner of its node, or of its region as it now is, in the page of
    /// the state and width it was written in, and a faint dot there in
    /// every other that holds the node, or a node the region covers; each
    /// on the whole state a pin at the right of the first page's caption.
    fn pins(&self, notes: &BTreeMap<PathBuf, Vec<Note>>, pane: &Pane) -> Vec<Pin> {
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
                    On::Region { state: written, region } => {
                        let Some(r) = page.region(region) else { continue };
                        (written, r, format!("{} {}", r.x + r.width, r.y), region.width)
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
                let region = (matches!(note.on, On::Region { .. }) && !faint)
                    .then(|| Self::window(r, (left, top), zoom));
                let side = if faint { DOT } else { PIN };
                let x = corner.0 - side / 2.0 - *before as f64 * (side + 2.0);
                *before += 1;
                let at = rect(x, corner.1 - side / 2.0, side, side);
                pins.push(Pin { region, ..Pin::of(&self.dir, note, at, faint) });
            }
        }
        pins
    }
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
        Pages { dir: PathBuf::from("shop"), state, pages: vec![page] }
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
    fn pages_draw_their_caption_and_outline_the_node_picked() {
        let pages = pages();
        let (wide, tall) = pages.size(&PANE, &mut Typesetter::new(None).unwrap());
        assert_eq!((wide, tall), (MARGIN + 390.0 + MARGIN, MARGIN + CAPTION + 600.0 + MARGIN));
        let picked = Some(Target::Node { id: "buy".to_string(), pointed: 0 });
        let drawn = pages.draw(&mut Scene::new(), &PANE, &seen(picked), &mut Typesetter::new(None).unwrap());
        assert!(drawn.iter().any(|d| matches!(d, Draw::Text { text, .. } if text == "default · 390")));
        let outline = drawn.iter().find_map(|d| match d {
            Draw::Box { rect, stroke: Some(Ink::Strong), dashed: false, .. } => Some(*rect),
            _ => None,
        });
        let (left, top) = (PANE.left + MARGIN, MARGIN + CAPTION);
        assert_eq!(
            outline,
            Some(rect(left + 18.0, top + 18.0, 54.0, 34.0)),
            "around the button, at both its edges"
        );
    }
}
