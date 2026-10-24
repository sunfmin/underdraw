//! One state of a screen description drawn: read, checked, laid out at a
//! width and built into a scene with its nodes, apart from what is done with
//! the pixels: `render` writes them to a PNG.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use underdraw_format::{Breakpoint, Lang, Model, Node, Screen, State, Transition, Undrawn};
use underdraw_layout::Page;
use vello::Scene;

use crate::pictures::Album;
use crate::{checked_inside, paint, pick, read, text};

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

/// A state of a description: one of its fixtures, or one of its phases;
/// written `{"fixture": <name>}` or `{"phase": <name>}`, as `render`
/// answers it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

/// What a node does when acted on: its id as the description writes it,
/// which a transition names it by, and its handlers by event, their cases
/// decided in the state shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Handlers {
    pub written: String,
    pub events: Map<String, Value>,
}

/// Where a node is written in the description: its id as written, where
/// in screen.json (a JSON Pointer), and for which element of each `each`
/// around it, the outermost first. Written with the id as written under
/// `id`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Origin {
    #[serde(rename = "id")]
    pub written: String,
    pub at: String,
    pub instance: Vec<usize>,
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
    /// What each node does when acted on, by id.
    pub handlers: BTreeMap<String, Handlers>,
    /// The id each node is first given in this state, by its id as the
    /// description writes it: a transition names a node so.
    pub written: BTreeMap<String, String>,
    /// Where each node is written, by id.
    pub origins: BTreeMap<String, Origin>,
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
    /// Where each node is written, by id.
    pub origins: BTreeMap<String, Origin>,
    /// What each node does when acted on, by id.
    pub handlers: BTreeMap<String, Handlers>,
    /// The wireframe.
    pub scene: Scene,
    /// Its marks, drawn over it: each label, and each guess's outline and
    /// "?".
    pub marks: Scene,
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
    Ok(Description::read(path)?.states())
}

/// A description read and checked once, as its files stood then, to lay
/// out any of its states from without checking it again: checking lays out
/// every state at each of its widths, so the gallery keeps one per description
/// until its files change.
pub struct Description {
    path: PathBuf,
    screen: Screen,
    dir: PathBuf,
    tokens: Value,
    checked: underdraw_format::Checked,
    /// Its project's root, under which the descriptions it draws are.
    root: PathBuf,
    /// The screens being drawn, outermost first, itself last: each draws
    /// the next inside it.
    drawing: Vec<String>,
}

impl Description {
    /// The description at `path` (a directory or its screen.json), read
    /// and checked; an error only when it cannot be read.
    pub fn read(path: &Path) -> anyhow::Result<Description> {
        Description::read_inside(path, &[])
    }

    /// The description at `path` as [`Description::read`] reads it, drawn
    /// inside each of the screens `outer` names in turn, outermost first:
    /// it draws none of them.
    pub fn read_inside(path: &Path, outer: &[String]) -> anyhow::Result<Description> {
        let (screen, dir) = read(path)?;
        let project = crate::project::Project::of(&dir)?;
        let checked = checked_inside(&screen, &dir, &underdraw_format::ScreenParams::new(), outer)?;
        let mut drawing = outer.to_vec();
        drawing.push(screen.name().to_string());
        let (tokens, root) = (project.tokens, project.root);
        Ok(Description { path: path.to_path_buf(), screen, dir, tokens, checked, root, drawing })
    }

    /// What `check` finds in it.
    pub fn findings(&self) -> &[underdraw_format::Finding] {
        &self.checked.findings
    }

    /// The widths its tokens name it drawn at, the first the one a single
    /// picture is drawn at: `underdraw_layout::widths`.
    pub fn widths(&self) -> Vec<u32> {
        underdraw_layout::widths(&self.tokens)
    }

    /// Every state it can be shown in, as [`states`] says.
    pub fn states(&self) -> Vec<Shown> {
        let fixtures = self.checked.fixtures.iter().cloned().map(Shown::Fixture);
        fixtures.chain(self.checked.phases.iter().cloned().map(Shown::Phase)).collect()
    }

    /// The directory the description is in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The screen's name, as its `screen` gives it.
    pub fn name(&self) -> &str {
        self.screen.name()
    }

    /// The state `wanted` names laid out; refused when the description has
    /// findings, since a picture of it would mislead. Given the screens
    /// `drawn` under its root, a node that navigates to any other is marked
    /// assumed; what underdraw assumes itself is said in `lang`.
    pub fn lay_out(
        &self,
        wanted: &Wanted,
        drawn: Option<&BTreeSet<String>>,
        lang: Lang,
    ) -> anyhow::Result<Laid> {
        let Description { screen, dir, tokens, checked, root, drawing, .. } = self;
        if !checked.ok {
            bail!(
                "the description has {} finding(s), so a picture of it would mislead: run `underdraw check {}` and fix them first",
                checked.findings.len(),
                self.path.display()
            );
        }
        let (state, name) = pick(checked, &wanted.fixture, &wanted.phase)?;
        let shown = match state {
            State::Fixture(_) => Shown::Fixture(name),
            State::Phase(_) => Shown::Phase(name),
        };
        let breakpoint = Breakpoint::for_width(tokens, wanted.width);
        let undrawn = drawn.map(|drawn| Undrawn { drawn, lang });
        let node = screen.instantiate(state, breakpoint, undrawn.as_ref()).map_err(anyhow::Error::msg)?;
        let locale = screen.description().get("locale").and_then(Value::as_str);
        let mut typesetter = text::Typesetter::new(locale).map_err(anyhow::Error::msg)?;
        let album = Album::new(root, drawing.clone(), lang);
        let page = underdraw_layout::layout(
            &node,
            tokens,
            screen,
            wanted.width as f32,
            lang,
            &mut typesetter,
            &album,
        )
        .map_err(anyhow::Error::msg)?;
        let mut calls = BTreeMap::new();
        called(&node, &mut calls);
        let mut handlers = BTreeMap::new();
        handled(&node, &mut handlers);
        let mut written = BTreeMap::new();
        ids(&node, &mut written);
        let mut origins = BTreeMap::new();
        originated(&node, &mut origins);
        Ok(Laid { dir: dir.clone(), shown, breakpoint, page, calls, handlers, written, origins, typesetter })
    }

    /// The transitions it declares, as [`transitions`] says.
    pub fn transitions(&self) -> anyhow::Result<Vec<Transition>> {
        match Model::read(&self.screen, self.checked.clone()) {
            Ok(model) => Ok(model.transitions.clone()),
            Err(checked) => bail!(
                "the description has {} finding(s): run `underdraw check {}` and fix them first",
                checked.findings.len(),
                self.path.display()
            ),
        }
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

/// Where `node` and each node in it are written, added to `origins` by id.
fn originated(node: &Node, origins: &mut BTreeMap<String, Origin>) {
    let origin = Origin {
        written: node.written_id.clone(),
        at: node.written_at.clone(),
        instance: node.instance.clone(),
    };
    origins.insert(node.id.clone(), origin);
    for child in &node.children {
        originated(child, origins);
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

/// What `node` and each node in it do when acted on, added to `handlers`
/// by id.
fn handled(node: &Node, handlers: &mut BTreeMap<String, Handlers>) {
    let events = match node.props.get("on") {
        Some(Value::Object(events)) => events.clone(),
        _ => Map::new(),
    };
    handlers.insert(node.id.clone(), Handlers { written: node.written_id.clone(), events });
    for child in &node.children {
        handled(child, handlers);
    }
}

impl Laid {
    /// The page painted at exactly `scale` pixels per point, its marks
    /// over it.
    pub fn scene(&mut self, scale: f64) -> Scene {
        let (mut scene, marks) = paint::scene(&self.page, &mut self.typesetter, scale);
        scene.append(&marks, None);
        scene
    }

    /// The scene of the page at `scale` pixels per point, or less when that
    /// would make a side longer than `max_side` pixels: a long page is drawn
    /// at a lower scale rather than not at all.
    pub fn draw(mut self, scale: u32, max_side: u32) -> Drawing {
        let page = &self.page;
        let tallest = f64::from(max_side) / f64::from(page.height.max(page.width).max(1.0));
        let scale = f64::from(scale).min(tallest);
        let pixels_wide = (f64::from(page.width) * scale).ceil() as u32;
        let pixels_high = (f64::from(page.height) * scale).ceil() as u32;
        let (scene, marks) = paint::scene(page, &mut self.typesetter, scale);
        Drawing {
            dir: self.dir,
            shown: self.shown,
            breakpoint: self.breakpoint,
            page: self.page,
            calls: self.calls,
            handlers: self.handlers,
            written: self.written,
            origins: self.origins,
            scene,
            marks,
            scale,
            pixels: [pixels_wide, pixels_high],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../format/tests/airbnb-listing");

    fn wanted(width: u32) -> Wanted {
        Wanted { fixture: None, phase: None, width, scale: 2 }
    }

    #[test]
    fn draws_a_state_without_a_gpu_or_a_file() {
        let drawing = Description::read(Path::new(DRAFT))
            .unwrap()
            .lay_out(&wanted(390), None, Lang::En)
            .unwrap()
            .draw(2, 16384);
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
        let laid =
            Description::read(Path::new(DRAFT)).unwrap().lay_out(&wanted(390), None, Lang::En).unwrap();
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
        let laid = Description::read(Path::new(DRAFT)).unwrap().lay_out(&wanted, None, Lang::En).unwrap();
        assert!(matches!(laid.shown, Shown::Phase(ref name) if name == "loading"));
    }

    #[test]
    fn says_which_operation_each_node_calls() {
        let laid =
            Description::read(Path::new(DRAFT)).unwrap().lay_out(&wanted(390), None, Lang::En).unwrap();
        assert!(!laid.calls.is_empty(), "the listing calls operations");
        for (id, operations) in &laid.calls {
            assert!(laid.page.nodes.iter().any(|n| &n.id == id), "{id} is a node placed");
            assert!(!operations.is_empty());
        }
    }
}
