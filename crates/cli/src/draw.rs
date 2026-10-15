//! One state of a screen description drawn: read, checked, laid out at a
//! width and built into a scene with its nodes, apart from what is done with
//! the pixels: `render` writes them to a PNG.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::bail;
use serde_json::Value;
use underdraw_format::{Breakpoint, Model, Node, State, Transition};
use underdraw_layout::Page;
use vello::Scene;

use crate::{checked, paint, pick, read, read_kinds, read_tokens, text};

/// The state to draw, and at what size.
pub struct Wanted {
    /// The fixture whose data to show; the first fixture when neither this
    /// nor `phase` is given.
    pub fixture: Option<String>,
    /// A phase to show instead of data.
    pub phase: Option<String>,
    /// The page width in points.
    pub width: u32,
    /// Pixels per point, at most.
    pub scale: u32,
}

/// A state of a description: one of its fixtures, or one of its phases.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    Fixture(String),
    Phase(String),
}

impl Shown {
    /// What to draw to show this state at `width`, at `scale`.
    pub fn wanted(&self, width: u32, scale: u32) -> Wanted {
        let (fixture, phase) = match self {
            Shown::Fixture(name) => (Some(name.clone()), None),
            Shown::Phase(name) => (None, Some(name.clone())),
        };
        Wanted { fixture, phase, width, scale }
    }

    /// The fixture's or phase's name.
    pub fn name(&self) -> &str {
        match self {
            Shown::Fixture(name) | Shown::Phase(name) => name,
        }
    }
}

/// One state laid out, the scene not built yet: everything that can be
/// wrong with the description is found by here, before a GPU is needed.
pub struct Laid {
    /// The directory the description is in.
    pub dir: PathBuf,
    pub shown: Shown,
    pub breakpoint: Breakpoint,
    /// The page, which holds where each node went.
    pub page: Page,
    /// The operations each node calls when acted on, by id.
    pub calls: BTreeMap<String, Vec<String>>,
    /// The id each node is first given in this state, by its id as the
    /// description writes it: a transition names a node so.
    pub written: BTreeMap<String, String>,
    typesetter: text::Typesetter,
}

/// One state drawn: its page, and the scene of it at `scale`.
pub struct Drawing {
    pub dir: PathBuf,
    pub shown: Shown,
    pub breakpoint: Breakpoint,
    /// The page, which holds where each node went.
    pub page: Page,
    /// The operations each node calls when acted on, by id.
    pub calls: BTreeMap<String, Vec<String>>,
    /// The id each node is first given in this state, by its id as written.
    pub written: BTreeMap<String, String>,
    pub scene: Scene,
    /// Pixels per point: the one asked for, or less when the page would be
    /// taller than the GPU draws.
    pub scale: f64,
    /// The scene's width and height in pixels.
    pub pixels: [u32; 2],
}

/// Every state the description at `path` can be shown in, as `check` finds
/// them: its fixtures, then its phases, each in the order written. Found
/// even when the description has findings, though none of them can be drawn
/// until they are fixed.
pub fn states(path: &Path) -> anyhow::Result<Vec<Shown>> {
    let (screen, dir) = read(path)?;
    let kinds = read_kinds(&dir)?;
    let checked = checked(&screen, &dir, kinds.as_ref())?;
    let fixtures = checked.fixtures.into_iter().map(Shown::Fixture);
    Ok(fixtures.chain(checked.phases.into_iter().map(Shown::Phase)).collect())
}

/// The state `wanted` names of the description at `path` (a directory or its
/// screen.json), laid out; refused when the description has findings, since
/// a picture of it would mislead. Given the screens `drawn` under its root,
/// a node that navigates to any other is marked assumed.
pub fn lay_out(path: &Path, wanted: &Wanted, drawn: Option<&BTreeSet<String>>) -> anyhow::Result<Laid> {
    let (screen, dir) = read(path)?;
    let tokens = read_tokens(&dir)?;
    let kinds = read_kinds(&dir)?;
    let checked = checked(&screen, &dir, kinds.as_ref())?;
    if !checked.ok {
        bail!(
            "the description has {} finding(s), so a picture of it would mislead: run `underdraw check {}` and fix them first",
            checked.findings.len(),
            path.display()
        );
    }
    let (state, name) = pick(&checked, &wanted.fixture, &wanted.phase)?;
    let shown = match state {
        State::Fixture(_) => Shown::Fixture(name),
        State::Phase(_) => Shown::Phase(name),
    };
    let breakpoint = Breakpoint::for_width(&tokens, wanted.width);
    let marked = drawn.map(|drawn| underdraw_format::assume_undrawn(&screen, drawn));
    let shows = marked.as_ref().unwrap_or(&screen);
    let root = underdraw_format::instantiate(shows, state, breakpoint).map_err(anyhow::Error::msg)?;
    let locale = screen.get("locale").and_then(Value::as_str);
    let mut typesetter = text::Typesetter::new(locale).map_err(anyhow::Error::msg)?;
    let page = underdraw_layout::layout(&root, &tokens, kinds.as_ref(), wanted.width as f32, &mut typesetter)
        .map_err(anyhow::Error::msg)?;
    let mut calls = BTreeMap::new();
    called(&root, &mut calls);
    let mut written = BTreeMap::new();
    ids(&root, &mut written);
    Ok(Laid { dir, shown, breakpoint, page, calls, written, typesetter })
}

/// The transitions the description at `path` declares, in the order
/// written; refused when the description has findings.
pub fn transitions(path: &Path) -> anyhow::Result<Vec<Transition>> {
    let (screen, dir) = read(path)?;
    let kinds = read_kinds(&dir)?;
    let checked = checked(&screen, &dir, kinds.as_ref())?;
    match Model::read(&screen, checked) {
        Ok(model) => Ok(model.transitions),
        Err(checked) => bail!(
            "the description has {} finding(s): run `underdraw check {}` and fix them first",
            checked.findings.len(),
            path.display()
        ),
    }
}

/// The id `node` and each node in it are given, added to `written` by the
/// id as written, where none was added before.
fn ids(node: &Node, written: &mut BTreeMap<String, String>) {
    written.entry(node.written_id.clone()).or_insert_with(|| node.id.clone());
    for child in &node.children {
        ids(child, written);
    }
}

/// The operations `node` and each node in it call, added to `calls` by id.
fn called(node: &Node, calls: &mut BTreeMap<String, Vec<String>>) {
    let operations: Vec<String> = node.calls().iter().map(|call| call.operation.to_string()).collect();
    if !operations.is_empty() {
        calls.entry(node.id.clone()).or_default().extend(operations);
    }
    for child in &node.children {
        called(child, calls);
    }
}

impl Laid {
    /// The scene of the page at `scale` pixels per point, or less when that
    /// would make a side longer than `max_side` pixels: a long page is drawn
    /// at a lower scale rather than not at all.
    pub fn draw(mut self, scale: u32, max_side: u32) -> Drawing {
        let page = &self.page;
        let tallest = f64::from(max_side) / f64::from(page.height.max(page.width).max(1.0));
        let scale = f64::from(scale).min(tallest);
        let pixels_wide = (f64::from(page.width) * scale).ceil() as u32;
        let pixels_high = (f64::from(page.height) * scale).ceil() as u32;
        let scene = paint::scene(page, &mut self.typesetter, scale);
        Drawing {
            dir: self.dir,
            shown: self.shown,
            breakpoint: self.breakpoint,
            page: self.page,
            calls: self.calls,
            written: self.written,
            scene,
            scale,
            pixels: [pixels_wide, pixels_high],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/airbnb-listing");

    fn wanted(width: u32) -> Wanted {
        Wanted { fixture: None, phase: None, width, scale: 2 }
    }

    #[test]
    fn draws_a_state_without_a_gpu_or_a_file() {
        let drawing = lay_out(Path::new(DRAFT), &wanted(390), None).unwrap().draw(2, 16384);
        assert_eq!(drawing.shown.name(), "default");
        assert!(matches!(drawing.shown, Shown::Fixture(_)));
        assert_eq!(drawing.breakpoint, Breakpoint::Compact);
        assert_eq!(drawing.page.width, 390.0);
        assert_eq!(drawing.scale, 2.0);
        assert_eq!(drawing.pixels, [780, (drawing.page.height * 2.0).ceil() as u32]);
        assert!(!drawing.scene.encoding().is_empty(), "the page is painted");
        assert!(drawing.page.nodes.iter().any(|n| n.id == "reserve-button"), "the nodes are placed");
    }

    #[test]
    fn a_page_taller_than_the_gpu_draws_is_drawn_smaller() {
        let laid = lay_out(Path::new(DRAFT), &wanted(390), None).unwrap();
        let side = laid.page.height.max(laid.page.width);
        let drawing = laid.draw(2, side as u32);
        assert_eq!(drawing.scale, 1.0, "one pixel per point fits");
        assert!(drawing.pixels[0].max(drawing.pixels[1]) <= side.ceil() as u32);
    }

    #[test]
    fn states_are_the_fixtures_then_the_phases_in_the_order_written() {
        let states = states(Path::new(DRAFT)).unwrap();
        let names: Vec<&str> = states.iter().map(Shown::name).collect();
        assert_eq!(names, ["default", "no-dates", "unavailable", "new-listing", "loading", "error"]);
        assert!(matches!(states[4], Shown::Phase(_)), "loading is a phase");
    }

    #[test]
    fn a_phase_is_shown_by_name() {
        let wanted = Wanted { phase: Some("loading".into()), ..wanted(390) };
        let laid = lay_out(Path::new(DRAFT), &wanted, None).unwrap();
        assert!(matches!(laid.shown, Shown::Phase(ref name) if name == "loading"));
    }

    #[test]
    fn says_which_operation_each_node_calls() {
        let laid = lay_out(Path::new(DRAFT), &wanted(390), None).unwrap();
        assert!(!laid.calls.is_empty(), "the listing calls operations");
        for (id, operations) in &laid.calls {
            assert!(laid.page.nodes.iter().any(|n| &n.id == id), "{id} is a node placed");
            assert!(!operations.is_empty());
        }
    }
}
