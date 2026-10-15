//! The gallery's window, run in a process of its own: the tree of
//! descriptions and their states on the left; for the state picked, its
//! picture drawn at a phone and a desktop width side by side in the middle,
//! where a node can be picked to read what it is, or acted on to follow what
//! the description says happens, as a prototype; and on the right the
//! sidebar ([`crate::sidebar`]), the cards of what is picked and the buttons
//! of what can be done with it; for a directory picked, the
//! flow between the screens under it; for a journey picked (ADR 0022), its
//! steps played one by one. The person writes notes (ADR 0019)
//! on its nodes, on regions dragged over it, on a whole state, and on the
//! arrows of an overview or a flow, each drawn as a numbered pin where it
//! was written; the window writes nothing else. The tree and panels are drawn by hand, in
//! the inks a wireframe is drawn in. The window is drawn from the files as
//! they now stand: as they change under its root, the tree, the state shown
//! and its notes follow.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use underdraw_format::{Finding, Lang};
use underdraw_layout::{Align, Draw, Font, Ink, Measure, Placed, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Rect as Area};
use vello::peniko::Fill;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::act::{self, Act};
use crate::draw::{self, Handlers, Origin, Shown as State, Wanted};
use crate::flow::{self, Map, Screen};
use crate::gallery::{self, Answering, Row};
use crate::inbox::{Guessed, Inbox, Item};
use crate::journeys::{self, Step};
use crate::notes::{self, Answer, Answers, By, Goes, Guess, Note, Proposal, Status};
use crate::reference::{self, Frame, Navigate, On, Pointed, Reference, Region};
use crate::sidebar::{self, Block, Card};
use crate::{gpu, links, paint, text};

/// The window's own words in the language it was opened in (`--lang`):
/// the same format string in English and in Chinese.
macro_rules! chrome {
    ($lang:expr, $en:literal, $zh:literal $(,)?) => {
        match $lang {
            Lang::En => format!($en),
            Lang::Zh => format!($zh),
        }
    };
    ($lang:expr, $en:literal, $zh:literal, $($arg:expr),+ $(,)?) => {
        match $lang {
            Lang::En => format!($en, $($arg),+),
            Lang::Zh => format!($zh, $($arg),+),
        }
    };
}

/// The widths each description is drawn at: a phone, and a desktop.
const WIDTHS: [u32; 2] = [390, 1120];

/// The tree's width, in points.
const TREE: f64 = 260.0;
/// One row of the tree, in points.
const ROW: f64 = 26.0;
/// How far in each directory level moves a row.
const INDENT: f64 = 14.0;
/// The space around the pictures, and between them.
const MARGIN: f64 = 24.0;
/// The caption over each picture.
const CAPTION: f64 = 26.0;
/// How far one notch of a mouse wheel scrolls.
const NOTCH: f64 = 40.0;
/// How small the pictures of a state get, fitting the width left, before
/// they scroll sideways instead.
const SMALLEST: f64 = 0.5;
/// The space between one finding and the next.
const GAP: f64 = 12.0;
/// The width each state is drawn at in a description's overview, and how
/// wide and, at most, how tall its picture there is.
const OVERVIEW: u32 = 390;
const THUMB: f64 = 160.0;
const THUMB_TALL: f64 = 320.0;
/// The space between two states in the overview, where the arrows run.
const SPACE: f64 = 72.0;
/// How far apart the arrows leaving, or reaching, one box of the flow are.
const ARROWS: f64 = 24.0;
/// A note's pin, where it was written, and the faint dot of it elsewhere:
/// how wide each is.
const PIN: f64 = 22.0;
const DOT: f64 = 8.0;
/// How far the pointer moves, held down, before a press is a drag over a
/// region rather than a click.
const DRAG: f64 = 6.0;

/// What the window is told from other threads.
enum Told {
    /// A later run of the command over the same root asks it forward.
    ComeForward,
    /// A file it is drawn from changed, appeared or went.
    Changed,
}

/// Opens the window over `root` and runs it until the person closes it.
/// Says on stdout, in one JSON line, whether the window opened, and nothing
/// after: the command that started this process reads that line and exits.
pub fn run(root: &Path, lang: Lang) {
    let event_loop = match EventLoop::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(e) => return report(&json!({ "error": e.to_string() })),
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    let typesetter = match text::Typesetter::new(None) {
        Ok(typesetter) => typesetter,
        Err(e) => return report(&json!({ "error": e })),
    };
    let mut gallery = Gallery::new(root, typesetter);
    gallery.lang = lang;
    gallery.proxy = Some(event_loop.create_proxy());
    if let Err(e) = event_loop.run_app(&mut gallery)
        && !gallery.reported
    {
        report(&json!({ "error": e.to_string() }));
    }
}

/// The one line the starting command reads.
fn report(line: &Value) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

struct Gallery {
    root: PathBuf,
    /// The language what underdraw assumes itself is said in.
    lang: Lang,
    lines: Vec<Line>,
    /// The line of the state shown, of the description when it has none, or
    /// of the directory whose flow is shown.
    picked: Option<usize>,
    /// The description and state the person last picked, by name, so that
    /// the files changing keeps showing them, or shows them again once
    /// they are back: a description gone unreadable lists no states.
    wanted: Option<Picked>,
    picture: Picture,
    /// The id of the node picked in the picture, kept from state to state
    /// while the state shown has it.
    node: Option<String>,
    /// The region dragged over a picture and picked, in place of a node,
    /// while the same state is shown.
    region: Option<Region>,
    /// Where the left button went down on a picture, while it is held: a
    /// drag from there picks a region.
    held: Option<(f64, f64)>,
    /// The picture, of those at each of `WIDTHS`, the node was picked in:
    /// the instance the person pointed at.
    pointed: usize,
    /// How far the pictures, the tree and the sidebar are scrolled down,
    /// and the pictures across, in points.
    scroll: f64,
    tree_scroll: f64,
    side_scroll: f64,
    across: f64,
    /// Whether the sidebar is shown: ⌘\ hides and shows it.
    sidebar: bool,
    /// Whether the card of the node picked is unfolded to its details: its
    /// id, and its label and box at each width.
    details: bool,
    /// Where the pointer is, in points.
    cursor: (f64, f64),
    /// Whether a click acts on the node under it, rather than picks it: ⌘,
    /// or Ctrl, is held.
    acting: bool,
    /// The states acting has come from, the last one last: where `back`
    /// returns to. Picking a state in the tree starts a new path.
    history: Vec<Picked>,
    /// What the last act on a node came to, while nothing else is done.
    acted: Option<Acted>,
    /// The window's width and height, in points.
    size: (f64, f64),
    /// Sets the tree's and the captions' text.
    typesetter: text::Typesetter,
    /// The window, once open.
    shown: Option<Shown>,
    /// Whether the starting command has been told the window opened.
    reported: bool,
    /// What brings the window forward, or says its files changed, from
    /// another thread.
    proxy: Option<EventLoopProxy<Told>>,
    /// Later runs of the command answered, once the window is open.
    answering: Option<Answering>,
    /// The clipboard ⌘C copies to, kept open once opened: on Linux what
    /// was copied goes with it.
    clipboard: Option<arboard::Clipboard>,
    /// Every description's notes, by its directory, as notes.json now
    /// holds them.
    notes: BTreeMap<PathBuf, Vec<Note>>,
    /// Whether the next click on a node, or drag over a region, starts a
    /// note on it: N was pressed.
    noting: bool,
    /// The note being written, while it is.
    draft: Option<Draft>,
    /// The arrow picked in the overview or the flow shown, by its place
    /// among their arrows.
    arrow: Option<usize>,
    /// The note whose pin was clicked: its description, and its id.
    note: Option<(PathBuf, String)>,
    /// The answered note whose pin the pointer is over, as `note` names it:
    /// its reply is shown.
    hovered: Option<(PathBuf, String)>,
    /// Whether Shift is held: Enter then breaks a line of a note.
    shift: bool,
    /// The journey picked, while one is played.
    playing: Option<Playing>,
    /// Each description drawn so far, read and checked once, by its
    /// directory, until the files change: checking one lays out every
    /// state, too slow to do on every pick.
    descriptions: RefCell<HashMap<PathBuf, Arc<draw::Description>>>,
    /// Each description's assumptions, as its states lay them out, by its
    /// directory, found once until the files change: for its inbox.
    guessed: RefCell<HashMap<PathBuf, Arc<Vec<Guessed>>>>,
    /// Whether the inbox lists the notes waiting on the agent, or only how
    /// many there are.
    waiting: bool,
}

/// A note being written: what it is for, and what has been typed.
struct Draft {
    on: Writing,
    text: String,
}

/// What a note being written is for.
enum Writing {
    /// A new note, on what this references.
    On(Reference),
    /// The line that reopens the note of this id, in the description in
    /// this directory.
    Reopening(PathBuf, String),
    /// A ✗ on the assumption of the node this references: what is right.
    Rejecting(Reference, Answers),
}

/// What a click on a button of the sidebar does.
#[derive(Clone, Debug, PartialEq)]
enum Do {
    /// Chooses the outcome the last act's operation answered with: the
    /// state it goes to shown.
    Choose { outcome: String, to: String },
    /// Writes a note on the node acted on, proposing where acting on it
    /// should go.
    Propose(Proposal),
    /// Starts writing a note on the node, region or arrow picked.
    Write,
    /// Closes the note picked: its description's directory, and its id.
    Close(PathBuf, String),
    /// Starts writing the line that reopens the note picked, named so.
    Reopen(PathBuf, String),
    /// Picks the note named so: one of a description's closed notes.
    Show(PathBuf, String),
    /// Answers the assumption on the node picked: ✓ writes the note at
    /// once; ✗ starts writing what is right.
    Judge(Answer),
    /// Shows the step of this place in the journey played.
    Step(usize),
    /// Folds or unfolds the details of the node picked.
    Details,
    /// Shows what an item of the inbox is on, picked.
    Visit(Visit),
    /// Folds or unfolds the inbox's notes waiting on the agent.
    Waiting,
}

/// An item of the inbox, as a click on it shows it.
#[derive(Clone, Debug, PartialEq)]
enum Visit {
    /// The assumption on the node of this id, in this state of the
    /// description in this directory.
    Guess(PathBuf, State, String),
    /// The note of this id, of the description in this directory.
    Note(PathBuf, String),
}

/// One note drawn on a picture: its pin, numbered, where it was written, or
/// a faint dot, in another state or width that holds its node, or the
/// nodes its region covers.
struct Pin {
    /// The directory of the description that keeps it.
    dir: PathBuf,
    id: String,
    /// Where it is, in window points.
    rect: Rect,
    ink: Ink,
    faint: bool,
    /// For a pin on a region, where it was written: the region, as it is
    /// now drawn, in window points.
    region: Option<Rect>,
}

/// Every description's notes on the lines of the tree over `root`, by its
/// directory, each that closes itself closed first; one whose notes.json
/// cannot be read has none (`check` says why).
fn notes_of(root: &Path, lines: &[Line]) -> BTreeMap<PathBuf, Vec<Note>> {
    lines
        .iter()
        .filter(|line| line.state.is_none())
        .filter_map(|line| line.row.description())
        .map(|dir| {
            let _ = notes::settle(dir, root);
            (dir.to_path_buf(), notes::read(dir).unwrap_or_default())
        })
        .collect()
}

/// What closed itself between the notes `before` and `after`, each with
/// why, as the panel says it once: `N3 closed: order-{index} is gone
/// (removed or renamed)`.
fn closed_itself(
    before: &BTreeMap<PathBuf, Vec<Note>>,
    after: &BTreeMap<PathBuf, Vec<Note>>,
    lang: Lang,
) -> Vec<String> {
    let mut said = Vec::new();
    for (dir, notes) in after {
        for note in notes {
            let Some(why) = &note.closed_because else { continue };
            let was_closed = before
                .get(dir)
                .and_then(|was| was.iter().find(|was| was.id == note.id))
                .is_none_or(|was| was.status == Status::Closed);
            if !was_closed {
                let id = &note.id;
                said.push(chrome!(lang, "{id} closed: {why}", "{id} 已关闭：{why}"));
            }
        }
    }
    said
}

/// One line of the tree: a directory, a description, one of a
/// description's states, listed under it, or a journey, listed under the
/// directory it is in.
struct Line {
    /// The directory or description; for a state, the description it is of,
    /// one level further in and named as the state is; for a journey, its
    /// file, named as the file is, with what `check` finds wrong in walking
    /// it.
    row: Row,
    state: Option<State>,
    journey: bool,
}

/// The lines of the tree over `root`: the root first, every other line in
/// from it, then each description followed by its states, as `check` finds
/// them, and each directory by the journeys under it, after its states
/// when it is a description. A description that cannot be read has no
/// states.
fn lines(root: &Path) -> Vec<Line> {
    let mut lines = Vec::new();
    let rows = gallery::tree(root);
    // A root that is no description is a line of its own, picked to show
    // the flow of, and the inbox of, everything under it.
    let within = usize::from(rows.first().is_some_and(|row| row.dir != root));
    if within == 1 {
        let name = root.file_name().map_or_else(|| ".".to_string(), |n| n.to_string_lossy().to_string());
        let row = Row {
            depth: 0,
            name,
            dir: root.to_path_buf(),
            described: false,
            faults: Vec::new(),
            states: Vec::new(),
        };
        lines.push(Line { row, state: None, journey: false });
        journey_lines(root, 1, &mut lines);
    }
    for mut row in rows {
        row.depth += within;
        let states = row.states.clone();
        let (depth, dir, faults) = (row.depth + 1, row.dir.clone(), row.faults.clone());
        lines.push(Line { row, state: None, journey: false });
        for state in states {
            let name = state.name().to_string();
            let states = Vec::new();
            let row = Row { depth, name, dir: dir.clone(), described: true, faults: faults.clone(), states };
            lines.push(Line { row, state: Some(state), journey: false });
        }
        journey_lines(&dir, depth, &mut lines);
    }
    lines
}

/// The journeys listed under `dir`, each a line `depth` in, added to
/// `lines`.
fn journey_lines(dir: &Path, depth: usize, lines: &mut Vec<Line>) {
    for file in journeys::listed(dir) {
        let name = file.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
        let faults = journeys::faults(&file);
        let row = Row { depth, name, dir: file, described: false, faults, states: Vec::new() };
        lines.push(Line { row, state: None, journey: true });
    }
}

/// A journey being played: its steps, and the one shown.
struct Playing {
    steps: Vec<Step>,
    at: usize,
    /// The state of the step shown, as its description lists it.
    state: State,
}

/// What an act on a node came to, as the panel says it at its top.
struct Acted {
    says: String,
    /// What the person chooses from, each a line of the panel and what a
    /// click on it does: the outcomes an operation can answer with, or,
    /// where nothing is declared, where acting should go; none once the
    /// act has gone somewhere.
    choices: Vec<(String, Do)>,
}

/// A line of the tree by name: its directory, a description's or not, and
/// its state's name.
#[derive(Clone)]
struct Picked {
    dir: PathBuf,
    state: Option<String>,
}

/// The window open, and the GPU drawing in it.
struct Shown {
    window: Arc<Window>,
    gpu: gpu::Gpu,
    surface: gpu::Window<'static>,
}

/// What the picked description shows.
enum Picture {
    /// One state at each of `WIDTHS`.
    Pages(Vec<Page>),
    /// Every state of the description, small, and its transitions.
    Overview(Overview),
    /// Why it cannot be drawn: what `check` finds wrong with it.
    Faults(Vec<Finding>),
    /// For a directory picked: the flow between the screens under it.
    Flow(Flow),
}

/// The flow between the screens under a directory: the map of them, and
/// what each of its boxes shows.
struct Flow {
    map: Map,
    /// One for each of the map's places, in their order.
    tiles: Vec<Tile>,
}

/// What one box of the flow shows under its name.
enum Tile {
    /// The description's first state, drawn at the phone's width.
    Picture { scene: Box<Scene>, width: f64 },
    /// A description that cannot be drawn: how many faults it has.
    Faulty(usize),
    /// A screen not drawn yet: a dashed box marked "?".
    Undrawn,
}

/// One state drawn at one width, at one pixel per point.
struct Page {
    caption: String,
    scene: Scene,
    width: f64,
    height: f64,
    /// Every node with an id, in the order drawn: one drawn later is over
    /// one drawn before, and an overlay is drawn after what it floats over.
    nodes: Vec<Placed>,
    /// The labels the nodes are tagged with, and the "?" of each guess, in
    /// the page's points: drawn apart from `scene`, at their own size.
    tags: Vec<Draw>,
    /// The operations each node calls when acted on, by id.
    calls: BTreeMap<String, Vec<String>>,
    /// What each node does when acted on, by id.
    handlers: BTreeMap<String, Handlers>,
    /// Where each node is written in the description, by id.
    origins: BTreeMap<String, Origin>,
}

/// A description's overview: each of its states, in the order the tree
/// lists them, and each transition it declares as an arrow between two.
struct Overview {
    thumbs: Vec<Thumb>,
    arrows: Vec<Arrow>,
}

/// One state in the overview, drawn at `OVERVIEW` at one pixel per point.
struct Thumb {
    name: String,
    phase: bool,
    /// The tree's line of the state.
    line: usize,
    scene: Scene,
    width: f64,
    height: f64,
    /// The journeys that pass through the state, one mark for each step of
    /// one that is in it.
    journeys: Vec<Pass>,
}

/// One step of a journey in a state of the overview, marked over its
/// picture: picking the mark plays the journey from that step.
struct Pass {
    /// The tree's line of the journey.
    line: usize,
    /// The step, from 0.
    step: usize,
    label: String,
    /// How wide the label is set, in points.
    wide: f64,
}

/// The font a journey's mark in the overview is labelled in.
const PASS_FONT: Font = Font { size: 11.0, weight: 600, line_height: 14.0 };
/// How tall a journey's mark in the overview is, and how far it is set in
/// from its picture's corner and from the mark above it.
const PASS_TALL: f64 = 18.0;
const PASS_INSET: f64 = 6.0;

/// One transition: from one state to another, by their places in
/// [`Overview::thumbs`], labelled with the label of the node acted on and,
/// for a call, the outcome.
struct Arrow {
    from: usize,
    to: usize,
    label: String,
    /// How wide the label is set, in points.
    wide: f64,
    /// The transition, as a reference names it.
    transition: reference::Transition,
}

/// The font the overview's arrows are labelled in.
const ARROW_FONT: Font = Font { size: 12.0, weight: 600, line_height: 16.0 };

impl Page {
    /// The node that a press at `at`, in the page's points, is on: the
    /// innermost there, an overlay before what it floats over.
    /// A press on a node's orange "?" is on that node, whatever is under
    /// it.
    fn node_at(&self, at: (f64, f64)) -> Option<&Placed> {
        let over = |r: Rect| {
            let (x, y) = (f64::from(r.x), f64::from(r.y));
            (x..=x + f64::from(r.width)).contains(&at.0) && (y..=y + f64::from(r.height)).contains(&at.1)
        };
        let guessed = self.nodes.iter().rev().find(|node| node.guess_tag().is_some_and(over));
        guessed.or_else(|| self.nodes.iter().rev().find(|node| over(node.rect)))
    }

    /// Whether a node of id `id` is in it.
    fn has(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }

    /// The node written where `origin` says, for the same instance, as it
    /// is placed here.
    fn placed(&self, origin: &Origin) -> Option<&Placed> {
        let id = self.origins.iter().find_map(|(id, at)| {
            (at.written == origin.written && at.instance == origin.instance).then_some(id)
        })?;
        self.nodes.iter().find(|node| &node.id == id)
    }

    /// Where `region` is drawn here: around the nodes it covers that are
    /// here, as the layout now places them; where none of them is, at its
    /// box, in a picture of the width it was dragged at; elsewhere, nowhere.
    fn region(&self, region: &Region) -> Option<Rect> {
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
}

impl Gallery {
    /// The tree of `root`, nothing picked and no window open yet.
    fn new(root: &Path, typesetter: text::Typesetter) -> Gallery {
        let lines = lines(root);
        Gallery {
            root: root.to_path_buf(),
            lang: Lang::default(),
            notes: notes_of(root, &lines),
            lines,
            picked: None,
            wanted: None,
            picture: Picture::Faults(Vec::new()),
            node: None,
            region: None,
            held: None,
            pointed: 0,
            scroll: 0.0,
            tree_scroll: 0.0,
            side_scroll: 0.0,
            across: 0.0,
            sidebar: true,
            details: false,
            cursor: (0.0, 0.0),
            acting: false,
            history: Vec::new(),
            acted: None,
            size: (0.0, 0.0),
            typesetter,
            shown: None,
            reported: false,
            proxy: None,
            answering: None,
            clipboard: None,
            noting: false,
            draft: None,
            arrow: None,
            note: None,
            hovered: None,
            shift: false,
            playing: None,
            descriptions: RefCell::new(HashMap::new()),
            guessed: RefCell::new(HashMap::new()),
            waiting: false,
        }
    }

    /// The description in `dir`, read and checked once until its files
    /// change.
    fn description(&self, dir: &Path) -> anyhow::Result<Arc<draw::Description>> {
        if let Some(description) = self.descriptions.borrow().get(dir) {
            return Ok(description.clone());
        }
        let description = Arc::new(draw::Description::read(dir)?);
        self.descriptions.borrow_mut().insert(dir.to_path_buf(), description.clone());
        Ok(description)
    }

    /// Scrolled `right` and `down` points, the pane under the pointer: the
    /// tree, the sidebar, or the pictures, which alone scroll across.
    fn wheel(&mut self, right: f64, down: f64) {
        if self.cursor.0 < TREE {
            self.tree_scroll += down;
        } else if self.cursor.0 >= self.pane_right() {
            self.side_scroll += down;
        } else {
            self.scroll += down;
            self.across += right;
        }
        self.clamp();
    }

    /// Where the pictures' pane ends on the right, in window points: where
    /// the sidebar starts while it is shown, else the window's edge.
    fn pane_right(&self) -> f64 {
        if self.sidebar { self.size.0 - sidebar::WIDTH } else { self.size.0 }
    }

    /// How wide the pictures' pane is, between the tree and the sidebar.
    fn pane(&self) -> f64 {
        self.pane_right() - TREE
    }

    /// The sidebar shown, or hidden, its width given to the pictures.
    fn toggle_sidebar(&mut self) {
        self.sidebar = !self.sidebar;
        self.clamp();
    }

    /// A press where the pointer is: on the tree, picks the line under it;
    /// on a button of the sidebar, does what it does; while N is pressed, starts
    /// a note there; on a pin, picks its note; on the pictures, picks the
    /// node under it, or none, or acts on it while [`Gallery::acting`]; on
    /// the overview, picks the arrow under it, or plays the journey whose
    /// mark is, from that step, or picks the state; on the flow,
    /// picks the arrow, or the description whose box is, under it. Whether
    /// that changed what is shown.
    fn press(&mut self) -> bool {
        if self.cursor.0 >= self.pane_right() {
            return match self.button_at(self.cursor) {
                Some(does) => self.click(does),
                None => false,
            };
        }
        if self.cursor.0 >= TREE {
            if self.acting && matches!(self.picture, Picture::Pages(_)) {
                return self.act();
            }
            if self.on_picture(self.cursor).is_some() && self.pin_at(self.cursor).is_none() {
                self.held = Some(self.cursor);
            }
            if self.noting {
                // On a picture, a click or a drag: the release says which.
                if self.held.is_some() {
                    return false;
                }
                return self.note_where_pressed();
            }
            if let Some(note) = self.pin_at(self.cursor) {
                let changed = Some(&note) != self.note.as_ref() || self.acted.is_some();
                self.note = Some(note);
                self.acted = None;
                return changed;
            }
            if let Some(arrow) = self.arrow_at(self.cursor) {
                let changed = Some(arrow) != self.arrow || self.acted.is_some() || self.note.is_some();
                self.arrow = Some(arrow);
                self.acted = None;
                self.note = None;
                return changed;
            }
            return match &self.picture {
                Picture::Flow(flow) => {
                    let at = self.in_flow(self.cursor);
                    let Some(Screen::Drawn(dir)) = flow.map.place_at(at).map(|place| &place.screen) else {
                        return self.unpick_arrow();
                    };
                    let Some(index) = self.description_line(dir) else {
                        return false;
                    };
                    self.pick(index);
                    true
                }
                Picture::Overview(_) => {
                    if let Some((line, step)) = self.pass_at(self.cursor) {
                        self.history.clear();
                        self.acted = None;
                        self.play(line, step);
                        return true;
                    }
                    let Some(line) = self.state_at(self.cursor) else {
                        return self.unpick_arrow();
                    };
                    self.history.clear();
                    self.acted = None;
                    self.pick(line);
                    true
                }
                Picture::Pages(_) | Picture::Faults(_) => {
                    let (node, pointed) = match self.hit(self.cursor) {
                        Some((pointed, _, node)) => (Some(node.id.clone()), pointed),
                        None => (None, 0),
                    };
                    let changed = node != self.node
                        || pointed != self.pointed
                        || self.acted.is_some()
                        || self.note.is_some()
                        || self.region.is_some();
                    self.node = node;
                    self.pointed = pointed;
                    self.acted = None;
                    self.note = None;
                    self.region = None;
                    changed
                }
            };
        }
        let line = ((self.cursor.1 + self.tree_scroll - MARGIN / 2.0) / ROW).floor();
        if line < 0.0 {
            return false;
        }
        match self.shows(line as usize) {
            Some(index) if Some(index) != self.picked => {
                self.history.clear();
                self.acted = None;
                self.pick(index);
                true
            }
            _ => false,
        }
    }

    /// No arrow picked, nor note, nor what was said: a press where none is.
    /// Whether that changed what is shown.
    fn unpick_arrow(&mut self) -> bool {
        let changed = self.arrow.is_some() || self.note.is_some() || self.acted.is_some();
        self.arrow = None;
        self.note = None;
        self.acted = None;
        changed
    }

    /// The left button let go: after a drag over a picture, the region
    /// dragged picked, and while N is pressed a note started on it; after a
    /// click while N is pressed, a note started on the node clicked.
    /// Whether that changed what is shown.
    fn release(&mut self) -> bool {
        let Some(from) = self.held.take() else {
            return false;
        };
        let moved = (self.cursor.0 - from.0).hypot(self.cursor.1 - from.1);
        if moved < DRAG {
            return self.noting && self.note_where_pressed();
        }
        let Some((pointed, region)) = self.region_between(from, self.cursor) else {
            return false;
        };
        self.node = None;
        self.pointed = pointed;
        self.region = Some(region);
        self.acted = None;
        self.note = None;
        if self.noting {
            self.noting = false;
            self.write();
        }
        true
    }

    /// The region between `from` and `to`, points in the window, on the
    /// picture `from` is on, cut at its edges: its box in that picture's
    /// points, and the outermost nodes wholly inside it, in the order laid
    /// out. With the picture's place among them.
    fn region_between(&self, from: (f64, f64), to: (f64, f64)) -> Option<(usize, Region)> {
        let (pointed, page, start) = self.on_picture(from)?;
        let (left, top) = self.origins()[pointed];
        let zoom = self.zoom(self.pane());
        let end =
            (((to.0 - left) / zoom).clamp(0.0, page.width), ((to.1 - top) / zoom).clamp(0.0, page.height));
        let (x, y) = (start.0.min(end.0).round(), start.1.min(end.1).round());
        let (right, bottom) = (start.0.max(end.0).round(), start.1.max(end.1).round());
        let area = Frame { x: x as i32, y: y as i32, width: (right - x) as u32, height: (bottom - y) as u32 };
        let dragged =
            Rect { x: x as f32, y: y as f32, width: (right - x) as f32, height: (bottom - y) as f32 };
        let mut kept: Vec<&Placed> = Vec::new();
        for node in page.nodes.iter().filter(|node| within(node.rect, dragged)) {
            if !kept.iter().any(|outer| within(node.rect, outer.rect)) {
                kept.push(node);
            }
        }
        let nodes = kept.iter().filter_map(|node| page.origins.get(&node.id).cloned()).collect();
        Some((pointed, Region { area, width: page.width as u32, nodes }))
    }

    /// The line picking line `index` shows: its own, a directory's (its
    /// flow), a description's (its overview) or a state's.
    fn shows(&self, index: usize) -> Option<usize> {
        self.lines.get(index)?;
        Some(index)
    }

    /// The tree read again from the files as they now stand, and the state
    /// or overview shown drawn again from them, scrolled as it was. Where
    /// that state went, its description's overview is shown; where its
    /// description went too, the first description's. A journey picked is
    /// played again at the step shown, while it is listed.
    fn reload(&mut self) {
        let (wanted, scroll, acted) = (self.wanted.take(), self.scroll, self.acted.take());
        let arrow = self.arrow.and_then(|_| self.reference());
        let journey = self.picked.and_then(|index| self.lines.get(index)).filter(|line| line.journey);
        let journey = journey.map(|line| (line.row.dir.clone(), self.playing.as_ref().map_or(0, |p| p.at)));
        self.lines = lines(&self.root);
        self.descriptions.borrow_mut().clear();
        self.guessed.borrow_mut().clear();
        let notes = notes_of(&self.root, &self.lines);
        let closed = closed_itself(&self.notes, &notes, self.lang);
        self.notes = notes;
        self.picked = None;
        self.picture = Picture::Faults(Vec::new());
        let playing = journey.and_then(|(file, at)| {
            Some((self.lines.iter().position(|line| line.journey && line.row.dir == file)?, at))
        });
        if let Some((index, at)) = playing {
            self.play(index, at);
            self.scroll = scroll;
            if !closed.is_empty() {
                self.say(self.reopen_from(&closed));
            }
            self.clamp();
            return;
        }
        let again = wanted.as_ref().and_then(|wanted| self.line_named(wanted));
        let description = wanted.as_ref().and_then(|wanted| {
            self.lines.iter().position(|line| line.state.is_none() && line.row.dir == wanted.dir)
        });
        let first = self.lines.iter().position(|line| line.row.description().is_some());
        if let Some(index) = again.or(description).or(first) {
            self.pick(index);
        }
        if again.is_some() {
            // What the panel said stands while the same state is shown: the
            // window's own note, written, comes back as a change.
            self.scroll = scroll;
            self.acted = acted;
            self.arrow = arrow.and_then(|arrow| self.arrow_references().iter().position(|r| *r == arrow));
        } else if description.is_some() {
            self.wanted = wanted;
        }
        if !closed.is_empty() {
            self.say(self.reopen_from(&closed));
        }
        self.clamp();
    }

    /// What the sidebar says of the notes that `closed` themselves.
    fn reopen_from(&self, closed: &[String]) -> String {
        let closed = closed.join(" · ");
        chrome!(
            self.lang,
            "{closed} · its description's overview lists it, to reopen",
            "{closed} · 可在它的描述的总览里重开"
        )
    }

    /// The line of the state `named` names, or of its directory when it
    /// names no state.
    fn line_named(&self, named: &Picked) -> Option<usize> {
        self.lines.iter().position(|line| {
            line.row.dir == named.dir
                && line.state.as_ref().map(|state| state.name()) == named.state.as_deref()
        })
    }

    /// Acts on the node under the pointer, as the description says: a
    /// transition declared from the state shown on the event a click raises
    /// goes to its state, or lists its outcomes to choose from; a navigate
    /// goes to the first state of the screen it names; back returns along
    /// the path taken. The panel says what came of it, and so where nothing
    /// is declared. Whether that changed what is shown.
    fn act(&mut self) -> bool {
        let Some((pointed, id, handlers, calls)) = self.hit(self.cursor).map(|(pointed, page, node)| {
            let handlers = page.handlers.get(&node.id).cloned().unwrap_or_default();
            (pointed, node.id.clone(), handlers, page.calls.get(&node.id).cloned().unwrap_or_default())
        }) else {
            let changed = self.node.is_some() || self.acted.is_some();
            self.node = None;
            self.acted = None;
            return changed;
        };
        self.node = Some(id);
        self.pointed = pointed;
        self.region = None;
        let Some(here) = self.wanted.clone() else {
            return true;
        };
        let state = here.state.clone().unwrap_or_default();
        let on = format!("{}.{}", handlers.written, act::event(&handlers.events));
        let lang = self.lang;
        let screen = crate::read(&here.dir).map(|(screen, _)| screen).unwrap_or(Value::Null);
        match act::act(&screen, &state, &handlers.written, &handlers.events) {
            Act::Go { to } => {
                let says = chrome!(lang, "{on} in {state} → {to}", "{on}（{state}）→ {to}");
                self.go(Picked { dir: here.dir, state: Some(to) }, says);
            }
            Act::Choose(choices) => {
                let calls = calls.join(", ");
                let says = chrome!(
                    lang,
                    "{on} in {state} calls {calls}: click how it answered",
                    "{on}（{state}）调用 {calls}：点它的回答"
                );
                let choices = choices
                    .into_iter()
                    .map(|(outcome, to)| (format!("{outcome} → {to}"), Do::Choose { outcome, to }))
                    .collect();
                self.acted = Some(Acted { says, choices });
            }
            Act::Navigate(screen) => match links::declared(&self.root).remove(&screen) {
                Some(description) => {
                    let says = chrome!(lang, "{on} navigates to {screen}", "{on} 跳转到 {screen}");
                    self.go(Picked { dir: description, state: None }, says);
                }
                None => {
                    self.say(format!("{on}: {}", underdraw_format::assumption::undrawn(&[screen], self.lang)))
                }
            },
            Act::Back => match self.history.pop() {
                Some(back) => {
                    let to = self.named(&back);
                    match self.line_named(&back) {
                        Some(index) => {
                            self.pick(index);
                            self.say(chrome!(lang, "{on} goes back to {to}", "{on} 返回到 {to}"));
                        }
                        None => self.say(chrome!(
                            lang,
                            "{on} goes back to {to}, which is gone",
                            "{on} 返回到 {to}，但它已不在"
                        )),
                    }
                }
                None => self.say(chrome!(
                    lang,
                    "{on} goes back, but no act led here: back returns along the path taken by acting",
                    "{on} 要返回，但没有操作通到这里：返回沿着操作走过的路退回"
                )),
            },
            Act::Nothing => {
                let says = chrome!(
                    lang,
                    "no transition from {state} on {on} · should go to:",
                    "{state} 上的 {on} 没有过渡 · 应该去："
                );
                let choices = self.proposals(&here.dir, &on);
                self.acted = Some(Acted { says, choices });
            }
        }
        true
    }

    /// Where acting on `on`, a node's id as written and its event, could be
    /// proposed to go from a state of the description in `dir`: each state
    /// of it, as the tree lists them, then each screen another description
    /// under the root declares, as a navigate; each a line of the panel.
    fn proposals(&self, dir: &Path, on: &str) -> Vec<(String, Do)> {
        let propose = |goes| Do::Propose(Proposal { on: on.to_string(), goes });
        let states = self
            .lines
            .iter()
            .filter(|line| line.row.dir == dir)
            .filter_map(|line| line.state.as_ref())
            .map(|state| (state.name().to_string(), propose(Goes::To(state.name().to_string()))));
        let screens = links::declared(&self.root).into_iter().filter(|(_, declared)| declared != dir).map(
            |(screen, _)| {
                let said = chrome!(self.lang, "navigate to {screen}", "跳转到 {screen}");
                (said, propose(Goes::Navigate(screen)))
            },
        );
        states.chain(screens).collect()
    }

    /// What a button of the sidebar does, done. Whether there was anything
    /// to do.
    fn click(&mut self, does: Do) -> bool {
        match does {
            Do::Choose { outcome, to } => self.choose(outcome, to),
            Do::Propose(proposal) => {
                self.propose(proposal);
                true
            }
            Do::Write => {
                self.write();
                true
            }
            Do::Close(dir, id) => {
                self.close(&dir, &id);
                true
            }
            Do::Reopen(dir, id) => {
                self.draft = Some(Draft { on: Writing::Reopening(dir, id), text: String::new() });
                true
            }
            Do::Show(dir, id) => {
                self.note = Some((dir, id));
                self.acted = None;
                true
            }
            Do::Judge(answer) => {
                self.judge(answer);
                true
            }
            Do::Step(at) => self.step(at),
            Do::Details => {
                self.details = !self.details;
                true
            }
            Do::Visit(visit) => self.visit(visit),
            Do::Waiting => {
                self.waiting = !self.waiting;
                true
            }
        }
    }

    /// The outcome the last act's operation answered with chosen: the
    /// state `to` it goes to shown. Whether a state was shown to go from.
    fn choose(&mut self, outcome: String, to: String) -> bool {
        let Some(here) = self.wanted.clone() else {
            return false;
        };
        let state = here.state.clone().unwrap_or_default();
        let says = chrome!(self.lang, "{outcome} in {state} → {to}", "{outcome}（{state}）→ {to}");
        self.go(Picked { dir: here.dir, state: Some(to.clone()) }, says);
        true
    }

    /// A note written on the node acted on, at the instance pointed at,
    /// holding `proposal`, for the agent to declare; the panel says so, or
    /// why it was not. The window writes the note only, never the
    /// description.
    fn propose(&mut self, proposal: Proposal) {
        let Some(on) = self.reference().filter(|on| matches!(on.on, On::Node { .. })) else {
            return self.say(chrome!(
                self.lang,
                "a proposal is a note on a node: act on one first",
                "提议是节点上的便签：先操作一个节点"
            ));
        };
        match notes::propose(&on, proposal) {
            Ok(id) => {
                self.notes = notes_of(&self.root, &self.lines);
                self.say(chrome!(
                    self.lang,
                    "{id} saved, proposing it for the agent: `underdraw notes` lists it",
                    "{id} 已保存，作为提议交给代理：`underdraw notes` 会列出它"
                ));
                self.note = Some((on.description, id));
            }
            Err(e) => self.say(chrome!(self.lang, "not saved: {e:#}", "没有保存：{e:#}")),
        }
    }

    /// The state `to` names shown, or its description's first state when
    /// it names none, as an act on a node led to it: where
    /// the person was is kept for `back`, and the panel says `says`.
    fn go(&mut self, to: Picked, says: String) {
        let Some(mut index) = self.line_named(&to) else {
            return self.say(chrome!(
                self.lang,
                "{says}, which the tree does not list",
                "{says}，但目录树里没有它"
            ));
        };
        // A description named alone: its first state, the line under it.
        if to.state.is_none()
            && self.lines.get(index + 1).is_some_and(|next| next.state.is_some() && next.row.dir == to.dir)
        {
            index += 1;
        }
        if let Some(here) = self.wanted.clone() {
            self.history.push(here);
        }
        self.pick(index);
        self.say(says);
    }

    /// The panel told `says`, and nothing to choose.
    fn say(&mut self, says: String) {
        self.acted = Some(Acted { says, choices: Vec::new() });
    }

    /// `picked` as the panel names it: its description, relative to the
    /// root, and its state.
    fn named(&self, picked: &Picked) -> String {
        let within = gallery::within(&self.root, &picked.dir);
        match &picked.state {
            Some(state) => format!("{within} · {state}"),
            None => within,
        }
    }

    /// What picking line `index` shows: a state drawn at each of `WIDTHS`,
    /// a description's overview, a directory's flow, or a journey's first
    /// step.
    fn pick(&mut self, index: usize) {
        let Some(index) = self.shows(index) else {
            return;
        };
        if self.lines[index].journey {
            return self.play(index, 0);
        }
        self.playing = None;
        let line = &self.lines[index];
        let (dir, described) = (line.row.dir.clone(), line.row.description().is_some());
        let (state, faults) = (line.state.clone(), line.row.faults.clone());
        self.picked = Some(index);
        self.arrow = None;
        // A note stays picked while what is shown holds it: its description,
        // or a flow over it.
        if self
            .note
            .as_ref()
            .is_some_and(|(of, _)| if described { *of != dir } else { !of.starts_with(&dir) })
        {
            self.note = None;
        }
        let name = state.as_ref().map(|state| state.name().to_string());
        if self.wanted.as_ref().is_none_or(|was| was.dir != dir || was.state != name) {
            self.region = None;
        }
        self.wanted = Some(Picked { dir: dir.clone(), state: name });
        (self.scroll, self.across) = (0.0, 0.0);
        if !described {
            self.node = None;
            self.picture = Picture::Flow(self.flow(&dir));
            return;
        }
        if !faults.is_empty() {
            self.picture = Picture::Faults(faults);
            return;
        }
        // The screens drawn under the root as they now stand: a navigate to
        // any other is a screen not drawn yet.
        let drawn = links::drawn(&self.root);
        if state.is_none() && self.lines.get(index + 1).is_some_and(|next| next.state.is_some()) {
            self.picture = match self.overview(index, &dir, &drawn) {
                Ok(overview) => Picture::Overview(overview),
                Err(e) => Picture::Faults(vec![Finding {
                    at: crate::SCREEN.to_string(),
                    problem: format!("{e:#}"),
                }]),
            };
            return;
        }
        self.show_pages(&dir, state.as_ref(), &drawn);
    }

    /// The state `state` of the description in `dir` drawn at each of
    /// `WIDTHS`, given the screens `drawn` under the root, and shown; or
    /// why it cannot be. The node picked stays picked while it is there.
    fn show_pages(&mut self, dir: &Path, state: Option<&State>, drawn: &std::collections::BTreeSet<String>) {
        let lang = self.lang;
        let laid: Vec<anyhow::Result<Page>> = match self.description(dir) {
            // Each width laid out on a thread of its own, at once.
            Ok(description) => std::thread::scope(|scope| {
                let laying: Vec<_> = WIDTHS
                    .map(|width| {
                        let description = &description;
                        scope.spawn(move || {
                            let wanted = match state {
                                Some(state) => state.wanted(width, 1),
                                // A description with no state: lay_out says why.
                                None => Wanted { fixture: None, phase: None, width, scale: 1 },
                            };
                            let mut laid = description.lay_out(&wanted, Some(drawn), lang)?;
                            // The labels, drawn by the window at their own
                            // size over a picture scaled down.
                            let (tags, draws) = std::mem::take(&mut laid.page.draws)
                                .into_iter()
                                .partition(|draw| matches!(draw, Draw::Tag { .. }));
                            laid.page.draws = draws;
                            let drawing = laid.draw(1, u32::MAX);
                            Ok(Page {
                                tags,
                                caption: format!("{} · {width}", drawing.shown.name()),
                                width: f64::from(drawing.page.width),
                                height: f64::from(drawing.page.height),
                                scene: drawing.scene,
                                nodes: drawing.page.nodes,
                                calls: drawing.calls,
                                handlers: drawing.handlers,
                                origins: drawing.origins,
                            })
                        })
                    })
                    .into_iter()
                    .collect();
                laying.into_iter().map(|laid| laid.join().expect("laying out a state panicked")).collect()
            }),
            Err(e) => vec![Err(e)],
        };
        let pages = match laid.into_iter().collect::<anyhow::Result<Vec<Page>>>() {
            Ok(pages) => pages,
            Err(e) => {
                let at = crate::SCREEN.to_string();
                self.picture = Picture::Faults(vec![Finding { at, problem: format!("{e:#}") }]);
                return;
            }
        };
        let kept = self.node.as_ref().is_some_and(|id| pages.iter().any(|page| page.has(id)));
        if !kept {
            self.node = None;
        }
        self.picture = Picture::Pages(pages);
    }

    /// The journey on line `index` played from its step `at` (its last, past
    /// the end): that step's state drawn at each of `WIDTHS`, the node it
    /// acts on marked. One `check` finds broken shows its faults instead,
    /// and is not played.
    fn play(&mut self, index: usize, at: usize) {
        let line = &self.lines[index];
        let (file, faults) = (line.row.dir.clone(), line.row.faults.clone());
        self.picked = Some(index);
        self.playing = None;
        (self.arrow, self.note, self.region, self.node) = (None, None, None, None);
        (self.scroll, self.across) = (0.0, 0.0);
        if !faults.is_empty() {
            self.wanted = None;
            self.picture = Picture::Faults(faults);
            return;
        }
        let steps = match journeys::steps(&file) {
            Ok(steps) => steps,
            Err(e) => {
                self.picture =
                    Picture::Faults(vec![Finding { at: String::new(), problem: format!("{e:#}") }]);
                return;
            }
        };
        let at = at.min(steps.len().saturating_sub(1));
        let Some(step) = steps.get(at) else {
            self.picture = Picture::Faults(Vec::new());
            return;
        };
        let state = draw::states(&step.dir)
            .ok()
            .and_then(|states| states.into_iter().find(|state| state.name() == step.state));
        let Some(state) = state else {
            let (within, state) = (gallery::within(&self.root, &step.dir), &step.state);
            let problem = chrome!(self.lang, "{within} has no state {state}", "{within} 没有状态 {state}");
            self.picture = Picture::Faults(vec![Finding { at: format!("steps.{at}"), problem }]);
            return;
        };
        // Acting on a node from here goes on as the prototype does, from
        // the step's state.
        self.wanted = Some(Picked { dir: step.dir.clone(), state: Some(step.state.clone()) });
        let dir = step.dir.clone();
        self.show_pages(&dir, Some(&state), &links::drawn(&self.root));
        self.playing = Some(Playing { steps, at, state });
    }

    /// The step `at` of the journey played shown. Whether one is played.
    fn step(&mut self, at: usize) -> bool {
        let (Some(index), Some(_)) = (self.picked, &self.playing) else {
            return false;
        };
        self.acted = None;
        self.play(index, at);
        true
    }

    /// The node the journey played acts on in the step shown, as placed in
    /// each picture: the first instance of its id as written; none in a
    /// picture that does not show it, or in the last step.
    fn marked(&self) -> Vec<Option<&Placed>> {
        let (Some(playing), Picture::Pages(pages)) = (&self.playing, &self.picture) else {
            return Vec::new();
        };
        let Some((written, _)) = &playing.steps[playing.at].on else {
            return Vec::new();
        };
        pages
            .iter()
            .map(|page| {
                page.nodes
                    .iter()
                    .find(|node| page.origins.get(&node.id).is_some_and(|o| &o.written == written))
            })
            .collect()
    }

    /// The marks around the node the journey played acts on next, in
    /// window points, in each picture that shows it.
    fn marks(&self) -> Vec<Rect> {
        let zoom = self.zoom(self.pane());
        self.marked()
            .into_iter()
            .zip(self.origins())
            .filter_map(|(node, (left, top))| {
                let r = node?.rect;
                let (x, y) = (left + f64::from(r.x) * zoom, top + f64::from(r.y) * zoom);
                let (w, h) = (f64::from(r.width) * zoom, f64::from(r.height) * zoom);
                Some(rect(x - 4.0, y - 4.0, w + 8.0, h + 8.0))
            })
            .collect()
    }

    /// The card of the journey played: which step is shown, of how many,
    /// its description and state, the act that leads on from it, and a
    /// button to each step beside it.
    fn playing_card(&self) -> Option<Card<Do>> {
        let (Some(playing), Some(index)) = (&self.playing, self.picked) else {
            return None;
        };
        let lang = self.lang;
        let (at, step) = (playing.at, &playing.steps[playing.at]);
        let (name, number, steps) = (&self.lines[index].row.name, at + 1, playing.steps.len());
        let (within, state) = (gallery::within(&self.root, &step.dir), &step.state);
        let title = chrome!(
            lang,
            "journey {name} · step {number} of {steps} · {within} · {state}",
            "旅程 {name} · 第 {number} 步，共 {steps} 步 · {within} · {state}"
        );
        let mut blocks = vec![Block::Text(match &step.on {
            Some((node, event)) => {
                let label = self.marked().into_iter().flatten().next().and_then(|node| node.label);
                let label = label
                    .map(|label| chrome!(lang, " (label {label})", "（编号 {label}）"))
                    .unwrap_or_default();
                let outcome = step
                    .outcome
                    .as_ref()
                    .map(|o| chrome!(lang, ", answered {o}", "，结果 {o}"))
                    .unwrap_or_default();
                chrome!(
                    lang,
                    "acts on {node}{label} · {event}{outcome} (marked)",
                    "作用于 {node}{label} · {event}{outcome}（已标出）"
                )
            }
            None => chrome!(lang, "the journey ends here", "旅程到此结束"),
        })];
        if at > 0 {
            blocks.push(Block::Button(chrome!(lang, "← previous step", "← 上一步"), Do::Step(at - 1)));
        }
        if at + 1 < playing.steps.len() {
            blocks.push(Block::Button(chrome!(lang, "→ next step", "→ 下一步"), Do::Step(at + 1)));
        }
        Some(Card { ink: Ink::Note, ..Card::new(title, blocks) })
    }

    /// The overview of the description on line `index`, in `dir`: its states
    /// on the lines under it, each drawn at `OVERVIEW`, and its transitions.
    fn overview(
        &mut self,
        index: usize,
        dir: &Path,
        drawn: &std::collections::BTreeSet<String>,
    ) -> anyhow::Result<Overview> {
        let states: Vec<(usize, &State)> = self
            .lines
            .iter()
            .enumerate()
            .skip(index + 1)
            .map_while(|(line, Line { state, .. })| Some((line, state.as_ref()?)))
            .collect();
        let lang = self.lang;
        let description = self.description(dir)?;
        let description = &description;
        // Each state laid out on a thread of its own, all at once.
        let drawings: Vec<anyhow::Result<draw::Drawing>> = std::thread::scope(|scope| {
            let laying: Vec<_> = states
                .iter()
                .map(|(_, state)| {
                    scope.spawn(move || {
                        Ok(description
                            .lay_out(&state.wanted(OVERVIEW, 1), Some(drawn), lang)?
                            .draw(1, u32::MAX))
                    })
                })
                .collect();
            laying.into_iter().map(|laid| laid.join().expect("laying out a state panicked")).collect()
        });
        let mut thumbs = Vec::new();
        // For each state, the label of each node by its id as written.
        let mut labels = Vec::new();
        for ((line, state), drawing) in states.into_iter().zip(drawings) {
            let drawing = drawing?;
            let labelled: BTreeMap<String, u32> = drawing
                .written
                .iter()
                .filter_map(|(written, id)| {
                    let label = drawing.page.nodes.iter().find(|node| &node.id == id)?.label?;
                    Some((written.clone(), label))
                })
                .collect();
            labels.push(labelled);
            thumbs.push(Thumb {
                name: state.name().to_string(),
                phase: matches!(state, State::Phase(_)),
                line,
                width: f64::from(drawing.page.width),
                height: f64::from(drawing.page.height),
                scene: drawing.scene,
                journeys: Vec::new(),
            });
        }
        // Each step of a journey listed in the tree that is in one of these
        // states, read as written: one that does not walk is marked too,
        // and picking it shows its faults.
        for (line, journey) in self.lines.iter().enumerate().filter(|(_, line)| line.journey) {
            for (step, on) in journeys::steps(&journey.row.dir).unwrap_or_default().iter().enumerate() {
                let thumb = thumbs.iter().position(|thumb| thumb.name == on.state);
                let Some(thumb) = thumb.filter(|_| on.dir == dir) else { continue };
                let (name, number) = (&journey.row.name, step + 1);
                let label = chrome!(lang, "→ {name} · step {number}", "→ {name} · 第 {number} 步");
                let wide = f64::from(self.typesetter.text(&label, PASS_FONT, None).width) + 10.0;
                thumbs[thumb].journeys.push(Pass { line, step, label, wide });
            }
        }
        let at = |name: &str| thumbs.iter().position(|thumb| thumb.name == name);
        let mut arrows = Vec::new();
        for transition in description.transitions()? {
            let (Some(from), Some(to)) = (at(&transition.from), at(&transition.to)) else { continue };
            let node = match labels[from].get(&transition.node) {
                Some(label) => label.to_string(),
                None => transition.node.clone(),
            };
            let label = match &transition.outcome {
                Some(outcome) => format!("{node} · {outcome}"),
                None => node,
            };
            let wide = f64::from(self.typesetter.text(&label, ARROW_FONT, None).width) + 10.0;
            let transition = reference::Transition {
                from: transition.from.clone(),
                on: format!("{}.{}", transition.node, transition.event),
                outcome: transition.outcome.clone(),
                to: transition.to.clone(),
            };
            arrows.push(Arrow { from, to, label, wide, transition });
        }
        Ok(Overview { thumbs, arrows })
    }

    /// How many states a row of the overview holds in a pane `pane` wide.
    fn columns(pane: f64) -> usize {
        ((pane - 2.0 * MARGIN + SPACE) / (THUMB + SPACE)).floor().max(1.0) as usize
    }

    /// Where each state's picture is in the overview, in window points:
    /// its left, top, width and height, the height no more than
    /// `THUMB_TALL`.
    fn cells(&self) -> Vec<(f64, f64, f64, f64)> {
        let Picture::Overview(overview) = &self.picture else {
            return Vec::new();
        };
        let columns = Self::columns(self.pane());
        overview
            .thumbs
            .iter()
            .enumerate()
            .map(|(index, thumb)| {
                let (column, row) = ((index % columns) as f64, (index / columns) as f64);
                let x = TREE + MARGIN + column * (THUMB + SPACE);
                let y = MARGIN - self.scroll + CAPTION + row * (CAPTION + THUMB_TALL + SPACE);
                (x, y, THUMB, (thumb.height * THUMB / thumb.width).min(THUMB_TALL))
            })
            .collect()
    }

    /// The marks of the journeys through each state of the overview, in
    /// window points, stacked down from its picture's top left corner, each
    /// no wider than the picture.
    fn passes(&self) -> Vec<(Rect, &Pass)> {
        let Picture::Overview(overview) = &self.picture else {
            return Vec::new();
        };
        let mut list = Vec::new();
        for (thumb, (x, y, w, _)) in overview.thumbs.iter().zip(self.cells()) {
            for (index, pass) in thumb.journeys.iter().enumerate() {
                let top = y + PASS_INSET + index as f64 * (PASS_TALL + PASS_INSET / 2.0);
                let wide = pass.wide.min(w - 2.0 * PASS_INSET);
                list.push((rect(x + PASS_INSET, top, wide, PASS_TALL), pass));
            }
        }
        list
    }

    /// The journey, by its tree line, and its step whose mark in the
    /// overview is under `at`, a point in the window.
    fn pass_at(&self, at: (f64, f64)) -> Option<(usize, usize)> {
        self.passes().into_iter().find_map(|(r, pass)| {
            let (x, y) = (f64::from(r.x), f64::from(r.y));
            let over =
                (x..=x + f64::from(r.width)).contains(&at.0) && (y..=y + f64::from(r.height)).contains(&at.1);
            over.then_some((pass.line, pass.step))
        })
    }

    /// The tree's line of the state whose picture in the overview is under
    /// `at`, a point in the window.
    fn state_at(&self, at: (f64, f64)) -> Option<usize> {
        let Picture::Overview(overview) = &self.picture else {
            return None;
        };
        let cells = self.cells();
        let index = cells
            .iter()
            .position(|&(x, y, w, h)| (x..=x + w).contains(&at.0) && (y - CAPTION..=y + h).contains(&at.1))?;
        Some(overview.thumbs[index].line)
    }

    /// The overview's arrows, in window points: each as the stretches it is
    /// drawn with, and where its label is. An arrow runs from the edge of
    /// one picture to the edge of the other, each to the right of its way,
    /// so that two between the same states, either way, run apart, and is
    /// labelled nearer where it starts, so that their labels do too; one
    /// that stays in its state loops out of its right.
    fn overview_arrows(&self) -> Vec<(Vec<(f64, f64)>, Rect)> {
        let Picture::Overview(overview) = &self.picture else {
            return Vec::new();
        };
        let cells = self.cells();
        let mut list = Vec::new();
        for (index, arrow) in overview.arrows.iter().enumerate() {
            let before =
                overview.arrows[..index].iter().filter(|a| (a.from, a.to) == (arrow.from, arrow.to)).count();
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

    /// The overview's arrows and their labels drawn, in window points; the
    /// one picked in the strong ink.
    fn arrows(&self) -> Vec<Draw> {
        let Picture::Overview(overview) = &self.picture else {
            return Vec::new();
        };
        let mut list = Vec::new();
        for (index, (arrow, (path, at))) in overview.arrows.iter().zip(self.overview_arrows()).enumerate() {
            let ink = if self.arrow == Some(index) { Ink::Strong } else { Ink::Text };
            for pair in path.windows(2) {
                list.push(line(pair[0], pair[1], ink));
            }
            list.extend(head(path[path.len() - 2], path[path.len() - 1], ink));
            list.push(Draw::Box {
                rect: at,
                radius: 9.0,
                fill: Some(Ink::Paper),
                stroke: Some(ink),
                dashed: false,
            });
            let label = Draw::Text {
                rect: rect(f64::from(at.x), f64::from(at.y) + 1.0, f64::from(at.width), 16.0),
                text: arrow.label.clone(),
                font: ARROW_FONT,
                ink,
                max_lines: Some(1),
                align: Align::Center,
                underline: false,
                strike: false,
            };
            list.push(label);
        }
        list
    }

    /// Where each arrow shown is labelled, in window points, and where its
    /// notes' pins go: each transition of the overview, or each navigate of
    /// the flow, in the order of [`Gallery::arrow_references`].
    fn arrow_spots(&self) -> Vec<(Rect, (f64, f64))> {
        match &self.picture {
            Picture::Overview(_) => self
                .overview_arrows()
                .into_iter()
                .map(|(_, at)| {
                    let pin =
                        (f64::from(at.x + at.width) + PIN / 2.0 + 4.0, f64::from(at.y + at.height / 2.0));
                    (at, pin)
                })
                .collect(),
            Picture::Flow(flow) => {
                let ((left, top), zoom) = (self.flow_origin(), self.zoom(self.pane()));
                let window = |(x, y): (f64, f64)| (left + x * zoom, top + y * zoom);
                flow_arrows(flow)
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
            Picture::Pages(_) | Picture::Faults(_) => Vec::new(),
        }
    }

    /// The arrow whose label is under `at`, a point in the window.
    fn arrow_at(&self, at: (f64, f64)) -> Option<usize> {
        self.arrow_spots().iter().position(|(r, _)| {
            let (x, y) = (f64::from(r.x), f64::from(r.y));
            (x..=x + f64::from(r.width)).contains(&at.0) && (y..=y + f64::from(r.height)).contains(&at.1)
        })
    }

    /// The reference to each arrow shown: each transition of the overview,
    /// in its description, or each navigate of the flow, in the description
    /// it is written in.
    fn arrow_references(&self) -> Vec<Reference> {
        match (&self.picture, &self.wanted) {
            (Picture::Overview(overview), Some(wanted)) => overview
                .arrows
                .iter()
                .map(|arrow| Reference {
                    description: wanted.dir.clone(),
                    on: On::Transition { transition: arrow.transition.clone() },
                })
                .collect(),
            (Picture::Flow(flow), _) => flow
                .map
                .arrows
                .iter()
                .map(|arrow| {
                    let link = &arrow.link;
                    let navigate =
                        Navigate { from: link.from.clone(), node: link.node.clone(), to: link.to.clone() };
                    Reference { description: link.description.clone(), on: On::Navigate { navigate } }
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The line of the description in `dir`.
    fn description_line(&self, dir: &Path) -> Option<usize> {
        self.lines.iter().position(|line| line.state.is_none() && line.row.description() == Some(dir))
    }

    /// The flow between the descriptions under `dir`, from the links `check
    /// <root>` reports, each box showing its description's first state.
    fn flow(&self, dir: &Path) -> Flow {
        let under: Vec<PathBuf> = self
            .lines
            .iter()
            .filter(|line| line.state.is_none())
            .filter_map(|line| line.row.description())
            .filter(|description| description.starts_with(dir))
            .map(Path::to_path_buf)
            .collect();
        let map = flow::map(&under, &links::links(&self.root));
        let drawn = links::drawn(&self.root);
        let tiles = map
            .places
            .iter()
            .map(|place| match &place.screen {
                Screen::Drawn(dir) => self.tile(dir, &drawn),
                Screen::Undrawn(_) => Tile::Undrawn,
            })
            .collect();
        Flow { map, tiles }
    }

    /// The box of the description in `dir`: its first state at the phone's
    /// width, or how many faults keep it from being drawn.
    fn tile(&self, dir: &Path, drawn: &std::collections::BTreeSet<String>) -> Tile {
        let line = self.description_line(dir);
        let faults = line.map_or(0, |index| self.lines[index].row.faults.len());
        if faults > 0 {
            return Tile::Faulty(faults);
        }
        let first = line.and_then(|index| self.lines.get(index + 1)).and_then(|next| next.state.clone());
        let wanted = match first {
            Some(state) => state.wanted(WIDTHS[0], 1),
            None => Wanted { fixture: None, phase: None, width: WIDTHS[0], scale: 1 },
        };
        match self
            .description(dir)
            .and_then(|description| description.lay_out(&wanted, Some(drawn), self.lang))
        {
            Ok(laid) => {
                let drawing = laid.draw(1, u32::MAX);
                Tile::Picture { scene: Box::new(drawing.scene), width: f64::from(drawing.page.width) }
            }
            Err(_) => Tile::Faulty(1),
        }
    }

    /// Where the flow's top-left corner is in the window, in points.
    fn flow_origin(&self) -> (f64, f64) {
        (TREE + MARGIN, MARGIN - self.scroll)
    }

    /// `at`, a point in the window, in the flow's points.
    fn in_flow(&self, at: (f64, f64)) -> (f64, f64) {
        let ((left, top), zoom) = (self.flow_origin(), self.zoom(self.pane()));
        ((at.0 - left) / zoom, (at.1 - top) / zoom)
    }

    /// Where each picture's top-left corner is in the window, in points.
    fn origins(&self) -> Vec<(f64, f64)> {
        let Picture::Pages(pages) = &self.picture else {
            return Vec::new();
        };
        let zoom = self.zoom(self.pane());
        let mut x = TREE + MARGIN - self.across;
        let top = MARGIN - self.scroll + CAPTION;
        pages
            .iter()
            .map(|page| {
                let at = (x, top);
                x += page.width * zoom + MARGIN;
                at
            })
            .collect()
    }

    /// The node under `at`, a point in the window, and the picture it is
    /// in, with its place among them; none off the pictures or where no
    /// node is.
    fn hit(&self, at: (f64, f64)) -> Option<(usize, &Page, &Placed)> {
        let (index, page, at) = self.on_picture(at)?;
        page.node_at(at).map(|node| (index, page, node))
    }

    /// The picture under `at`, a point in the window, with its place among
    /// them and `at` in its points; none off the pictures.
    fn on_picture(&self, at: (f64, f64)) -> Option<(usize, &Page, (f64, f64))> {
        let Picture::Pages(pages) = &self.picture else {
            return None;
        };
        let zoom = self.zoom(self.pane());
        pages.iter().zip(self.origins()).enumerate().find_map(|(index, (page, (left, top)))| {
            let (x, y) = ((at.0 - left) / zoom, (at.1 - top) / zoom);
            ((0.0..=page.width).contains(&x) && (0.0..=page.height).contains(&y)).then_some((
                index,
                page,
                (x, y),
            ))
        })
    }

    /// What ⌘C copies: the region picked, its box and the nodes it covers;
    /// or the node picked, as the description writes it, and the instance
    /// of it pointed at (in the picture it was picked in, or the first that
    /// has it); or, with neither picked, the state shown; or the arrow
    /// picked in an overview or a flow. None where nothing is.
    fn reference(&self) -> Option<Reference> {
        if let Picture::Overview(_) | Picture::Flow(_) = self.picture {
            return self.arrow_references().into_iter().nth(self.arrow?);
        }
        let (Picture::Pages(pages), Some(wanted)) = (&self.picture, &self.wanted) else {
            return None;
        };
        let state = self.state_shown()?;
        let node = self.node.as_ref().and_then(|id| {
            let has = |page: &&Page| page.has(id);
            let page = pages.get(self.pointed).filter(has).or_else(|| pages.iter().find(has))?;
            let placed = page.nodes.iter().find(|node| &node.id == id)?;
            let origin = page.origins.get(id)?.clone();
            Some(Pointed { origin, label: placed.label, width: page.width as u32 })
        });
        let on = match (self.region.clone(), node) {
            (Some(region), _) => On::Region { state, region },
            (None, Some(node)) => On::Node { state, node },
            (None, None) => On::State { state },
        };
        Some(Reference { description: wanted.dir.clone(), on })
    }

    /// The reference to what is picked put on the clipboard, and the panel
    /// says so; or says why nothing was.
    fn copy(&mut self) {
        let Some(reference) = self.reference() else {
            return self.say(chrome!(
                self.lang,
                "nothing to copy: pick a state, a node or region in it, or an arrow, first",
                "没有可复制的：先选中一个状态、其中的节点或区域，或一个箭头"
            ));
        };
        let (line, lang) = (reference.line(), self.lang);
        let copied = match self.clipboard() {
            Some(clipboard) => clipboard.set_text(line.clone()).map_err(|e| e.to_string()),
            None => Err(chrome!(lang, "there is no clipboard to copy to", "没有剪贴板可复制到")),
        };
        match copied {
            Ok(()) => self.say(chrome!(lang, "copied  {line}", "已复制  {line}")),
            Err(e) => self.say(chrome!(lang, "could not copy ({e}): {line}", "无法复制（{e}）：{line}")),
        }
    }

    /// The state whose pictures are shown: the one picked, or the step's of
    /// the journey played.
    fn state_shown(&self) -> Option<State> {
        if let Some(playing) = &self.playing {
            return Some(playing.state.clone());
        }
        self.picked.and_then(|index| self.lines.get(index)?.state.clone())
    }

    /// The clipboard, opened the first time it is asked for.
    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }
        self.clipboard.as_mut()
    }

    /// The answered note whose pin the pointer is now over, kept to show
    /// its reply. Whether that changed.
    fn hover(&mut self) -> bool {
        let over = self.pin_at(self.cursor).filter(|(dir, id)| {
            self.notes.get(dir).is_some_and(|notes| {
                notes.iter().any(|note| &note.id == id && note.status == Status::Answered)
            })
        });
        let changed = over != self.hovered;
        self.hovered = over;
        changed
    }

    /// What the button of the sidebar under `at`, a point in the window,
    /// does; none off its buttons, or while it is hidden.
    fn button_at(&mut self, at: (f64, f64)) -> Option<Do> {
        if !self.sidebar {
            return None;
        }
        let cards = self.sidebar_cards();
        let laid = sidebar::lay_out(&cards, self.pane_right(), -self.side_scroll, &mut self.typesetter);
        laid.at(at).cloned()
    }

    /// The labels the nodes of each picture are tagged with, and the "?" of
    /// each guess, in window points: where the picture puts them, scaled
    /// with it, but each at its own size, so that it reads as well at
    /// [`SMALLEST`] as at full size.
    fn labels(&self) -> Vec<Draw> {
        let Picture::Pages(pages) = &self.picture else {
            return Vec::new();
        };
        let zoom = self.zoom(self.pane());
        let mut labels = Vec::new();
        for (page, (left, top)) in pages.iter().zip(self.origins()) {
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

    /// The outline around the node picked, in window points, in each
    /// picture that has it.
    fn outlines(&self) -> Vec<Rect> {
        let (Some(id), Picture::Pages(pages)) = (&self.node, &self.picture) else {
            return Vec::new();
        };
        let zoom = self.zoom(self.pane());
        pages
            .iter()
            .zip(self.origins())
            .filter_map(|(page, (left, top))| {
                let r = page.nodes.iter().find(|n| &n.id == id)?.rect;
                let (x, y) = (left + f64::from(r.x) * zoom, top + f64::from(r.y) * zoom);
                let (w, h) = (f64::from(r.width) * zoom, f64::from(r.height) * zoom);
                Some(rect(x - 2.0, y - 2.0, w + 4.0, h + 4.0))
            })
            .collect()
    }

    /// The dashed boxes over the pictures, in window points: the region
    /// picked, where it is now drawn, and the one being dragged, from where
    /// the button went down to the pointer.
    fn regions(&self) -> Vec<Rect> {
        let mut boxes = Vec::new();
        if let (Some(region), Picture::Pages(pages)) = (&self.region, &self.picture) {
            let zoom = self.zoom(self.pane());
            for (page, (left, top)) in pages.iter().zip(self.origins()) {
                if let Some(r) = page.region(region) {
                    let (x, y) = (left + f64::from(r.x) * zoom, top + f64::from(r.y) * zoom);
                    boxes.push(rect(x, y, f64::from(r.width) * zoom, f64::from(r.height) * zoom));
                }
            }
        }
        if let Some(from) = self.held {
            let to = self.cursor;
            if (to.0 - from.0).hypot(to.1 - from.1) >= DRAG {
                let (x, y) = (from.0.min(to.0), from.1.min(to.1));
                boxes.push(rect(x, y, (to.0 - from.0).abs(), (to.1 - from.1).abs()));
            }
        }
        boxes
    }

    /// What the sidebar's cards say, one string each: a card's title, then
    /// each of its blocks.
    #[cfg(test)]
    fn panel(&self) -> Vec<String> {
        self.cards()
            .into_iter()
            .flat_map(|card| {
                let blocks = card.blocks.iter().map(|block| block.says().to_string()).collect::<Vec<_>>();
                std::iter::once(card.title).chain(blocks)
            })
            .collect()
    }

    /// The sidebar's cards, top to bottom, with a button for each thing a
    /// click does: the journey played, its step and the steps beside it;
    /// what the last act on a node came to, and any outcomes to choose
    /// from; then the note being written, or how to start one; then the
    /// note picked; then the node picked (what it is, what it calls, what
    /// was assumed there, and, folded into "details", its id and its label
    /// and box at each width), the region or the arrow picked, with a button
    /// to write a note on it; then a description's closed notes. None when
    /// none of these is there.
    fn cards(&self) -> Vec<Card<Do>> {
        let lang = self.lang;
        let mut cards: Vec<Card<Do>> = self.playing_card().into_iter().collect();
        if let Some(Acted { says, choices }) = &self.acted {
            let buttons =
                choices.iter().map(|(said, does)| Block::Button(said.clone(), does.clone())).collect();
            cards.push(Card::new(says.clone(), buttons));
        }
        if let Some(draft) = &self.draft {
            let title = match &draft.on {
                Writing::On(on) => chrome!(lang, "note {}", "便签：{}", on.on.said(lang)),
                Writing::Reopening(_, id) => {
                    chrome!(lang, "reopening {id}: say what is still to do", "重开 {id}：说说还有什么要做")
                }
                Writing::Rejecting(on, _) => chrome!(
                    lang,
                    "✗ wrong {}: say what is right, for the agent",
                    "✗ 不对 {}：告诉代理什么是对的",
                    on.on.said(lang)
                ),
            };
            let keys = chrome!(
                lang,
                "Enter saves, Shift+Enter breaks a line, Esc cancels",
                "回车保存，Shift+回车换行，Esc 放弃"
            );
            let blocks = vec![Block::Text(format!("{}▏", draft.text)), Block::Muted(keys)];
            cards.push(Card { ink: Ink::Note, ..Card::new(title, blocks) });
        } else if self.noting {
            let click = match (&self.picture, lang) {
                (Picture::Overview(_), Lang::En) => "click a transition's arrow, or a state, it is on",
                (Picture::Overview(_), Lang::Zh) => "点它所在的过渡箭头，或某个状态",
                (Picture::Flow(_), Lang::En) => "click the navigate's arrow it is on",
                (Picture::Flow(_), Lang::Zh) => "点它所在的跳转箭头",
                (Picture::Pages(_) | Picture::Faults(_), Lang::En) => {
                    "click the node it is on or a picture's caption, or drag over the region"
                }
                (Picture::Pages(_) | Picture::Faults(_), Lang::Zh) => {
                    "点它所在的节点或画面的标题，或拖出一块区域"
                }
            };
            let blocks =
                vec![Block::Text(click.to_string()), Block::Muted(chrome!(lang, "Esc cancels", "Esc 放弃"))];
            cards.push(Card {
                ink: Ink::Note, ..Card::new(chrome!(lang, "write a note", "写便签"), blocks)
            });
        }
        cards.extend(self.note_card());
        let picked = self.node_card().or_else(|| self.region_card()).or_else(|| self.arrow_card());
        if let Some(mut card) = picked {
            if self.draft.is_none() && !self.noting {
                card.blocks.extend(self.assumption_blocks());
                card.blocks.push(Block::Button(
                    chrome!(lang, "write a note on it  (N)", "在这里写便签  (N)"),
                    Do::Write,
                ));
            }
            cards.push(card);
        }
        if self.note.is_none() && self.draft.is_none() && !self.noting {
            cards.extend(self.closed_card());
        }
        cards
    }

    /// What the sidebar shows: its cards, then the inbox; and, while nothing
    /// is picked, what a person can do in the window.
    fn sidebar_cards(&self) -> Vec<Card<Do>> {
        let mut cards = self.cards();
        let picked = !cards.is_empty();
        cards.extend(self.inbox_card());
        if picked {
            return cards;
        }
        let lang = self.lang;
        let blocks = vec![
            Block::Muted(chrome!(
                lang,
                "Click a node to see what it is; drag over a picture to pick a region.",
                "点一个节点看它是什么；在画面上拖动可选中一块区域。"
            )),
            Block::Muted(chrome!(
                lang,
                "⌘-click a node to act on it, as a prototype.",
                "按住 ⌘ 点节点，像原型一样操作它。"
            )),
            Block::Muted(chrome!(
                lang,
                "N writes a note · ⌘C copies a reference · ⌘\\ hides this sidebar",
                "N 写便签 · ⌘C 复制引用 · ⌘\\ 隐藏侧栏"
            )),
        ];
        cards.push(Card {
            ink: Ink::Muted,
            ..Card::new(chrome!(lang, "Nothing picked", "未选中任何东西"), blocks)
        });
        cards
    }

    /// The descriptions whose inbox the sidebar shows: the one picked, or
    /// whose state is, or each under the directory picked, in the order the
    /// tree lists them. None while a node, a region or an arrow is picked,
    /// a note is written, or a journey played.
    fn inboxed(&self) -> Vec<PathBuf> {
        // The node picked is kept through an overview, for the next state:
        // it is picked only while a state is drawn.
        let drawn = matches!(self.picture, Picture::Pages(_));
        let busy = drawn && (self.node.is_some() || self.region.is_some()) || self.arrow.is_some();
        if busy || self.draft.is_some() || self.noting || self.playing.is_some() {
            return Vec::new();
        }
        let Some(line) = self.picked.and_then(|index| self.lines.get(index)).filter(|line| !line.journey)
        else {
            return Vec::new();
        };
        if let Some(dir) = line.row.description() {
            return vec![dir.to_path_buf()];
        }
        self.lines
            .iter()
            .filter(|line| line.state.is_none() && !line.journey)
            .filter_map(|line| line.row.description())
            .filter(|dir| dir.starts_with(&line.row.dir))
            .map(Path::to_path_buf)
            .collect()
    }

    /// The assumptions of the description in `dir`, each once, where it is
    /// first seen: its states laid out at the overview's width, in the
    /// order the tree lists them, among the screens `drawn` under the root
    /// (found the first time they are needed). None where it has faults.
    fn guessed(
        &self,
        dir: &Path,
        drawn: &mut Option<std::collections::BTreeSet<String>>,
    ) -> Arc<Vec<Guessed>> {
        if let Some(guessed) = self.guessed.borrow().get(dir) {
            return guessed.clone();
        }
        let drawn = drawn.get_or_insert_with(|| links::drawn(&self.root));
        let mut guessed: Vec<Guessed> = Vec::new();
        if let Ok(description) = self.description(dir) {
            let states =
                self.lines.iter().filter(|line| line.row.dir == dir).filter_map(|line| line.state.as_ref());
            for state in states {
                let wanted = state.wanted(OVERVIEW, 1);
                let Ok(laid) = description.lay_out(&wanted, Some(drawn), self.lang) else { continue };
                for node in &laid.page.nodes {
                    let (Some(assumed), Some(origin)) = (&node.assumed, laid.origins.get(&node.id)) else {
                        continue;
                    };
                    if !guessed.iter().any(|guess| guess.written == origin.written) {
                        guessed.push(Guessed {
                            state: state.clone(),
                            id: node.id.clone(),
                            written: origin.written.clone(),
                            assumed: assumed.clone(),
                        });
                    }
                }
            }
        }
        let guessed = Arc::new(guessed);
        self.guessed.borrow_mut().insert(dir.to_path_buf(), guessed.clone());
        guessed
    }

    /// The inbox's card: what waits on the person under what is picked, a
    /// button each, grouped by description for a directory; then, folded
    /// to how many there are, the notes still waiting on the agent. None
    /// where nothing waits, or [`Gallery::inboxed`] lists no description.
    fn inbox_card(&self) -> Option<Card<Do>> {
        let dirs = self.inboxed();
        let grouped = self
            .picked
            .and_then(|index| self.lines.get(index))
            .is_some_and(|line| line.row.description().is_none());
        let mut drawn = None;
        let guessed: Vec<(PathBuf, Arc<Vec<Guessed>>)> =
            dirs.into_iter().map(|dir| (dir.clone(), self.guessed(&dir, &mut drawn))).collect();
        let none = Vec::new();
        let inboxes: Vec<(&PathBuf, Inbox)> = guessed
            .iter()
            .map(|(dir, guessed)| (dir, Inbox::of(guessed, self.notes.get(dir).unwrap_or(&none))))
            .filter(|(_, inbox)| !inbox.is_empty())
            .collect();
        if inboxes.is_empty() {
            return None;
        }
        let lang = self.lang;
        let mut blocks = Vec::new();
        for (dir, inbox) in inboxes.iter().filter(|(_, inbox)| !inbox.yours.is_empty()) {
            if grouped {
                blocks.push(Block::Muted(gallery::within(&self.root, dir)));
            }
            for item in &inbox.yours {
                blocks.push(match item {
                    Item::Guess(guess) => {
                        let (written, state, (guessed, _)) =
                            (&guess.written, guess.state.name(), parts(&guess.assumed));
                        Block::Button(
                            chrome!(
                                lang,
                                "? {written} in {state}: {guessed}",
                                "? {written}（{state}）：{guessed}"
                            ),
                            Do::Visit(Visit::Guess((*dir).clone(), guess.state.clone(), guess.id.clone())),
                        )
                    }
                    Item::Answered(note) => {
                        let (id, on, reply) =
                            (&note.id, note.on.said(lang), note.reply().unwrap_or_default());
                        Block::Button(
                            chrome!(lang, "{id} · {on} · agent: {reply}", "{id} · {on} · 代理：{reply}"),
                            Do::Visit(Visit::Note((*dir).clone(), note.id.clone())),
                        )
                    }
                });
            }
        }
        let yours: usize = inboxes.iter().map(|(_, inbox)| inbox.yours.len()).sum();
        if yours == 0 {
            blocks.push(Block::Muted(chrome!(lang, "nothing waits on you", "没有等你处理的")));
        }
        let agents: usize = inboxes.iter().map(|(_, inbox)| inbox.agents.len()).sum();
        if agents > 0 {
            let fold = if self.waiting { "▾" } else { "▸" };
            blocks.push(Block::Button(
                chrome!(lang, "{fold} waiting on the agent · {agents}", "{fold} 等代理处理 · {agents}"),
                Do::Waiting,
            ));
        }
        if self.waiting {
            for (dir, inbox) in &inboxes {
                for note in &inbox.agents {
                    let (id, on) = (&note.id, note.on.said(lang));
                    let said = if grouped {
                        format!("{} · {id} · {on}", gallery::within(&self.root, dir))
                    } else {
                        format!("{id} · {on}")
                    };
                    blocks.push(Block::Button(said, Do::Visit(Visit::Note((*dir).clone(), note.id.clone()))));
                }
            }
        }
        Some(Card {
            ink: Ink::Note,
            ..Card::new(chrome!(lang, "waits on you · {yours}", "等你处理 · {yours}"), blocks)
        })
    }

    /// What an item of the inbox is on shown: an assumption's state, its
    /// node picked; a note's state with its node or region picked, its
    /// description's overview with its transition picked, or the flow its
    /// navigate is drawn in with that picked; and the note picked, with its
    /// card. Whether there was anything to show.
    fn visit(&mut self, visit: Visit) -> bool {
        self.history.clear();
        self.acted = None;
        let (dir, on, note) = match visit {
            Visit::Guess(dir, state, id) => {
                let Some(index) = self.line_named(&Picked { dir, state: Some(state.name().to_string()) })
                else {
                    return false;
                };
                self.note = None;
                self.pick(index);
                (self.region, self.arrow) = (None, None);
                self.pick_node(|page| page.has(&id).then(|| id.clone()), None);
                return true;
            }
            Visit::Note(dir, id) => {
                let Some(note) =
                    self.notes.get(&dir).and_then(|notes| notes.iter().find(|note| note.id == id))
                else {
                    return false;
                };
                (dir, note.on.clone(), id)
            }
        };
        match &on {
            On::Node { state, .. } | On::Region { state, .. } | On::State { state } => {
                let Some(index) =
                    self.line_named(&Picked { dir: dir.clone(), state: Some(state.name().to_string()) })
                else {
                    return false;
                };
                self.pick(index);
                (self.node, self.region, self.arrow) = (None, None, None);
                match &on {
                    On::Node { node, .. } => {
                        let origin = node.origin.clone();
                        self.pick_node(
                            |page| page.placed(&origin).map(|placed| placed.id.clone()),
                            Some(node.width),
                        );
                    }
                    On::Region { region, .. } => {
                        if let Picture::Pages(pages) = &self.picture {
                            self.pointed =
                                pages.iter().position(|page| page.width as u32 == region.width).unwrap_or(0);
                        }
                        self.region = Some(region.clone());
                    }
                    _ => {}
                }
            }
            On::Transition { .. } | On::Navigate { .. } => {
                // A transition's arrow is in its description's overview; a
                // navigate's, in the flow of a directory above it.
                let shown_in: Vec<usize> = match &on {
                    On::Transition { .. } => self.description_line(&dir).into_iter().collect(),
                    _ => dir
                        .ancestors()
                        .skip(1)
                        .filter_map(|above| {
                            self.lines.iter().position(|line| {
                                line.state.is_none()
                                    && !line.journey
                                    && line.row.description().is_none()
                                    && line.row.dir == above
                            })
                        })
                        .chain(self.description_line(&dir))
                        .collect(),
                };
                let wanted = Reference { description: dir.clone(), on: on.clone() };
                for index in shown_in {
                    self.pick(index);
                    (self.node, self.region) = (None, None);
                    self.arrow = self.arrow_references().iter().position(|arrow| *arrow == wanted);
                    if self.arrow.is_some() {
                        break;
                    }
                }
            }
        }
        self.note = Some((dir, note));
        true
    }

    /// The node `found` finds on a picture shown picked, in the picture of
    /// `width` before the others: the instance pointed at.
    fn pick_node(&mut self, found: impl Fn(&Page) -> Option<String>, width: Option<u32>) {
        let Picture::Pages(pages) = &self.picture else {
            return;
        };
        let mut order: Vec<usize> = (0..pages.len()).collect();
        order.sort_by_key(|&at| Some(pages[at].width as u32) != width);
        if let Some((pointed, id)) = order.into_iter().find_map(|at| Some((at, found(&pages[at])?))) {
            (self.node, self.pointed) = (Some(id), pointed);
        }
    }

    /// How tall the sidebar's cards are, unscrolled; nothing while it is
    /// hidden.
    fn sidebar_height(&mut self) -> f64 {
        if !self.sidebar {
            return 0.0;
        }
        let cards = self.sidebar_cards();
        sidebar::lay_out(&cards, 0.0, 0.0, &mut self.typesetter).height
    }

    /// The card of the arrow picked: what it is.
    fn arrow_card(&self) -> Option<Card<Do>> {
        self.arrow?;
        let said = self.reference()?.on.said(self.lang);
        Some(Card::new(said.strip_prefix("on ").unwrap_or(&said).to_string(), Vec::new()))
    }

    /// What the node's card says of the assumption on the node picked, past
    /// what was guessed and what else it could be (its own blocks say
    /// those): the notes that already answer it, then ✓ and ✗, each saying
    /// what it means. Nothing when the node picked has none.
    fn assumption_blocks(&self) -> Vec<Block<Do>> {
        let Some((on, _)) = self.assumption() else {
            return Vec::new();
        };
        let lang = self.lang;
        let mut blocks = Vec::new();
        if let (On::Node { node, .. }, Some(notes)) = (&on.on, self.notes.get(&on.description)) {
            for note in notes.iter().filter(|note| note.status != Status::Closed) {
                let (Some(answers), On::Node { node: on, .. }) = (&note.answers, &note.on) else { continue };
                if on.origin.written == node.origin.written {
                    let (id, mark, status) =
                        (&note.id, answered(answers.answer, lang), note_status(note, lang));
                    blocks.push(Block::Muted(chrome!(
                        lang,
                        "{id} answers it {mark} · {status}",
                        "{id} 已回答 {mark} · {status}"
                    )));
                }
            }
        }
        blocks.push(Block::Button(
            chrome!(
                lang,
                "✓ right, keep it: the agent takes the guess out, and the note closes itself",
                "✓ 对，保留：代理去掉这个猜测，便签自动关闭"
            ),
            Do::Judge(Answer::Right),
        ));
        blocks.push(Block::Button(
            chrome!(
                lang,
                "✗ wrong, tell the agent what is right: write it, for the agent to change and answer",
                "✗ 不对，告诉代理什么是对的：写下来，由代理修改并回答"
            ),
            Do::Judge(Answer::Wrong),
        ));
        blocks
    }

    /// The node picked, as a note on it references it, and what was assumed
    /// there; none when nothing was.
    fn assumption(&self) -> Option<(Reference, String)> {
        let (Some(id), Picture::Pages(pages)) = (&self.node, &self.picture) else {
            return None;
        };
        let assumed =
            pages.iter().find_map(|page| page.nodes.iter().find(|n| &n.id == id)?.assumed.clone())?;
        let on = self.reference().filter(|on| matches!(on.on, On::Node { .. }))?;
        Some((on, assumed))
    }

    /// The assumption on the node picked answered `answer`: ✓ writes its
    /// note at once; ✗ starts writing what is right. The window writes the
    /// note only, never the description.
    fn judge(&mut self, answer: Answer) {
        let Some((on, assumed)) = self.assumption() else {
            return self.say(chrome!(
                self.lang,
                "nothing is assumed there: pick a node with an orange \"?\" first",
                "那里没有假定：先选中一个带橙色「?」的节点"
            ));
        };
        let (On::Node { node, .. }, Picture::Pages(pages)) = (&on.on, &self.picture) else {
            return;
        };
        let screen = crate::read(&on.description).map(|(screen, _)| screen).unwrap_or(Value::Null);
        let id = self.node.clone().unwrap_or_default();
        let component =
            pages.iter().find_map(|page| page.nodes.iter().find(|n| n.id == id)).map(|n| n.component.clone());
        let guess = Guess::of(
            &screen,
            &node.origin.written,
            &component.unwrap_or_default(),
            &links::drawn(&self.root),
        );
        let answers = Answers { assumed, guess, answer };
        match answer {
            Answer::Right => match notes::judge(&on, answers, "") {
                Ok(id) => {
                    self.notes = notes_of(&self.root, &self.lines);
                    self.say(chrome!(
                        self.lang,
                        "{id} saved ✓, for the agent to take the guess out: the note then closes itself",
                        "{id} 已保存 ✓，由代理去掉这个猜测：之后便签自动关闭"
                    ));
                    self.note = Some((on.description, id));
                }
                Err(e) => self.say(chrome!(self.lang, "not saved: {e:#}", "没有保存：{e:#}")),
            },
            Answer::Wrong => {
                self.acted = None;
                self.note = None;
                self.draft = Some(Draft { on: Writing::Rejecting(on, answers), text: String::new() });
            }
        }
    }

    /// The card of the closed notes of the description whose overview is
    /// shown, each with why it closed, a click on one picking it, to reopen;
    /// none elsewhere, or when it has none.
    fn closed_card(&self) -> Option<Card<Do>> {
        let (Picture::Overview(_), Some(wanted)) = (&self.picture, &self.wanted) else {
            return None;
        };
        let lang = self.lang;
        let notes = self.notes.get(&wanted.dir)?;
        let closed: Vec<&Note> = notes.iter().filter(|note| note.status == Status::Closed).collect();
        if closed.is_empty() {
            return None;
        }
        let blocks = closed
            .into_iter()
            .map(|note| {
                let why =
                    note.closed_because.clone().unwrap_or_else(|| chrome!(lang, "closed by you", "你关闭的"));
                Block::Button(
                    format!("{} · {why} · {}", note.id, note.on.said(lang)),
                    Do::Show(wanted.dir.clone(), note.id.clone()),
                )
            })
            .collect();
        Some(Card::new(
            chrome!(lang, "closed notes · click one to read or reopen it", "已关闭的便签 · 点一条查看或重开"),
            blocks,
        ))
    }

    /// The note picked, if the description it names has it.
    fn picked_note(&self) -> Option<&Note> {
        let (dir, id) = self.note.as_ref()?;
        self.notes.get(dir)?.iter().find(|note| &note.id == id)
    }

    /// The card of the note picked: where it is, its thread, and what the
    /// person can do with it. None when none is.
    fn note_card(&self) -> Option<Card<Do>> {
        let (Some(note), Some((dir, _))) = (self.picked_note(), &self.note) else {
            return None;
        };
        let lang = self.lang;
        let status = note_status(note, lang);
        let answers = match &note.answers {
            Some(answers) => {
                let mark = answered(answers.answer, lang);
                chrome!(lang, " · answers the assumption {mark}", " · 回答假定 {mark}")
            }
            None => String::new(),
        };
        let title = format!("{} · {status} · {}{answers}", note.id, note.on.said(lang));
        let mut blocks: Vec<Block<Do>> = note
            .thread
            .iter()
            .map(|said| {
                let who = match (said.by, lang) {
                    (By::Person, Lang::En) => "you",
                    (By::Person, Lang::Zh) => "你",
                    (By::Agent, Lang::En) => "agent",
                    (By::Agent, Lang::Zh) => "代理",
                };
                Block::Text(format!("{who}: {}", said.says))
            })
            .collect();
        let (dir, id) = (dir.clone(), note.id.clone());
        let reopen = chrome!(lang, "reopen it, saying what is still to do", "重开，说说还有什么要做");
        match note.status {
            Status::Open => blocks.push(Block::Button(chrome!(lang, "close it", "关闭"), Do::Close(dir, id))),
            Status::Answered => {
                blocks.push(Block::Button(
                    chrome!(lang, "close it: the answer did it", "关闭：回答解决了"),
                    Do::Close(dir.clone(), id.clone()),
                ));
                blocks.push(Block::Button(reopen, Do::Reopen(dir, id)));
            }
            Status::Closed if note.closed_because.is_some() => blocks.push(Block::Button(
                chrome!(
                    lang,
                    "reopen it on the whole state, saying what is still to do",
                    "在整个状态上重开，说说还有什么要做"
                ),
                Do::Reopen(dir, id),
            )),
            Status::Closed => blocks.push(Block::Button(reopen, Do::Reopen(dir, id))),
        }
        let ink = if note.status == Status::Answered { Ink::Answered } else { Ink::Note };
        Some(Card { ink, ..Card::new(title, blocks) })
    }

    /// The next click on a node, a state's caption or an arrow starts a note
    /// on it; the panel says so. Not where a description's faults are
    /// shown. Whether that changed what is shown.
    fn start_noting(&mut self) -> bool {
        if matches!(self.picture, Picture::Faults(_)) {
            self.say(chrome!(
                self.lang,
                "a note is written on a state, an overview or a flow: pick one in the tree, then press N",
                "便签写在状态、总览或流程上：先在目录树里选中一个，再按 N"
            ));
            return true;
        }
        self.noting = true;
        self.note = None;
        self.acted = None;
        true
    }

    /// A note started on what is under the pointer: a node, a picture's
    /// caption (the whole state), a state or a transition's arrow in the
    /// overview, or a navigate's arrow in the flow, picked; or, where none
    /// is, the panel says so. Whether that changed what is shown.
    fn note_where_pressed(&mut self) -> bool {
        self.noting = false;
        let on = match &self.picture {
            Picture::Pages(_) => {
                self.region = None;
                match self.hit(self.cursor).map(|(pointed, _, node)| (pointed, node.id.clone())) {
                    Some((pointed, id)) => {
                        self.node = Some(id);
                        self.pointed = pointed;
                        self.reference()
                    }
                    None if self.caption_at(self.cursor) => {
                        self.node = None;
                        self.reference()
                    }
                    None => None,
                }
            }
            Picture::Overview(_) => match self.arrow_at(self.cursor) {
                Some(arrow) => {
                    self.arrow = Some(arrow);
                    self.reference()
                }
                None => self.state_at(self.cursor).and_then(|line| {
                    let line = &self.lines[line];
                    let state = line.state.clone()?;
                    Some(Reference { description: line.row.dir.clone(), on: On::State { state } })
                }),
            },
            Picture::Flow(_) => self.arrow_at(self.cursor).and_then(|arrow| {
                self.arrow = Some(arrow);
                self.reference()
            }),
            Picture::Faults(_) => None,
        };
        match on {
            Some(on) => self.draft_on(on),
            None => self.say(chrome!(
                self.lang,
                "nothing a note can be on there: press N, then click a node, a picture's caption or an arrow",
                "那里没有可以写便签的东西：按 N，再点一个节点、画面的标题或一个箭头"
            )),
        }
        true
    }

    /// Whether `at`, a point in the window, is on a picture's caption.
    fn caption_at(&self, at: (f64, f64)) -> bool {
        let Picture::Pages(pages) = &self.picture else {
            return false;
        };
        let zoom = self.zoom(self.pane());
        pages.iter().zip(self.origins()).any(|(page, (left, top))| {
            (left..=left + page.width * zoom).contains(&at.0) && (top - CAPTION..top).contains(&at.1)
        })
    }

    /// A note started on the node or arrow picked, at the instance pointed
    /// at.
    fn write(&mut self) {
        match self.reference() {
            Some(on) => self.draft_on(on),
            None => self.say(chrome!(
                self.lang,
                "a note is on a node, a state or an arrow: click one first",
                "便签写在节点、状态或箭头上：先点一个"
            )),
        }
    }

    /// A note started on what `on` references.
    fn draft_on(&mut self, on: Reference) {
        self.acted = None;
        self.note = None;
        self.draft = Some(Draft { on: Writing::On(on), text: String::new() });
    }

    /// The note being written saved to notes.json, and its pin drawn; or,
    /// where it cannot be, the panel says why and the note stays to edit.
    fn save(&mut self) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        let lang = self.lang;
        let saved = match &draft.on {
            Writing::On(on) => notes::add(on, &draft.text)
                .map(|id| (on.description.clone(), id, chrome!(lang, "saved", "已保存"))),
            Writing::Reopening(dir, id) => notes::reopen(dir, id, &draft.text)
                .map(|()| (dir.clone(), id.clone(), chrome!(lang, "reopened", "已重开"))),
            Writing::Rejecting(on, answers) => notes::judge(on, answers.clone(), &draft.text)
                .map(|id| (on.description.clone(), id, chrome!(lang, "saved ✗", "已保存 ✗"))),
        };
        match saved {
            Ok((dir, id, done)) => {
                self.notes = notes_of(&self.root, &self.lines);
                self.say(chrome!(
                    lang,
                    "{id} {done}, for the agent: `underdraw notes` lists it",
                    "{id} {done}，交给代理：`underdraw notes` 会列出它"
                ));
                self.note = Some((dir, id));
            }
            Err(e) => {
                self.say(chrome!(lang, "not saved: {e:#}", "没有保存：{e:#}"));
                self.draft = Some(draft);
            }
        }
    }

    /// The note of id `id`, in the description in `dir`, closed.
    fn close(&mut self, dir: &Path, id: &str) {
        match notes::close(dir, id) {
            Ok(()) => {
                self.notes = notes_of(&self.root, &self.lines);
                self.note = None;
                self.say(chrome!(self.lang, "{id} closed", "{id} 已关闭"));
            }
            Err(e) => self.say(chrome!(self.lang, "not closed: {e:#}", "没有关闭：{e:#}")),
        }
    }

    /// A key pressed, ⌘ not held, as `key` names it and with the `text` it
    /// types: while a note is written, typed into it, Enter saving it,
    /// Shift+Enter breaking a line and Esc dropping it; otherwise N starts
    /// a note and Esc stops one. Whether that changed what is shown.
    fn key(&mut self, key: &Key, text: Option<&str>) -> bool {
        if let Some(draft) = &mut self.draft {
            match key {
                Key::Named(NamedKey::Enter) if self.shift => draft.text.push('\n'),
                Key::Named(NamedKey::Enter) => self.save(),
                Key::Named(NamedKey::Escape) => self.draft = None,
                Key::Named(NamedKey::Backspace) => {
                    draft.text.pop();
                }
                _ => match text {
                    Some(text) if !text.chars().any(char::is_control) => draft.text.push_str(text),
                    _ => return false,
                },
            }
            return true;
        }
        match key {
            Key::Named(NamedKey::Escape) if self.noting || self.note.is_some() || self.region.is_some() => {
                self.noting = false;
                self.note = None;
                self.region = None;
                true
            }
            Key::Character(c) if c.eq_ignore_ascii_case("n") => self.start_noting(),
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowLeft) => {
                let Some(playing) = &self.playing else { return false };
                let at = match key {
                    Key::Named(NamedKey::ArrowRight) => playing.at + 1,
                    _ => playing.at.wrapping_sub(1),
                };
                at < playing.steps.len() && self.step(at)
            }
            _ => false,
        }
    }

    /// `text` typed into the note being written: what the input method
    /// composed, or what was pasted. Whether there was one.
    fn type_in(&mut self, text: &str) -> bool {
        let Some(draft) = &mut self.draft else {
            return false;
        };
        draft.text.push_str(text);
        true
    }

    /// The notes on what is shown, not closed, drawn on it; blue while open,
    /// green once answered. On a state's pictures, each on a node or a
    /// region a numbered pin at the top-right corner of its node, or of its
    /// region as it now is, in the picture of the state and width it was
    /// written in, and a faint dot there in every other that holds the
    /// node, or a node the region covers; each on the whole state a pin at
    /// the right of the first picture's caption. In the overview, each on a
    /// state a pin at the right of its caption, and each on a transition one
    /// beside its arrow's label; in the flow, each on a navigate one under
    /// its arrow, where it leaves its box.
    fn pins(&self) -> Vec<Pin> {
        let mut pins = match &self.picture {
            Picture::Pages(_) => self.page_pins(),
            Picture::Overview(_) => self.state_pins(),
            Picture::Flow(_) | Picture::Faults(_) => Vec::new(),
        };
        let shown = |note: &&Note| note.status != Status::Closed;
        for (reference, (_, (x, y))) in self.arrow_references().into_iter().zip(self.arrow_spots()) {
            let Some(notes) = self.notes.get(&reference.description) else { continue };
            let on = notes.iter().filter(shown).filter(|note| note.on == reference.on);
            for (before, note) in on.enumerate() {
                let x = x + before as f64 * (PIN + 2.0);
                pins.push(self.pin(
                    &reference.description,
                    note,
                    rect(x - PIN / 2.0, y - PIN / 2.0, PIN, PIN),
                    false,
                ));
            }
        }
        pins
    }

    /// Note `note`, of the description in `dir`, drawn in `rect`: faint, or
    /// its pin.
    fn pin(&self, dir: &Path, note: &Note, rect: Rect, faint: bool) -> Pin {
        let ink = if note.status == Status::Answered { Ink::Answered } else { Ink::Note };
        Pin { dir: dir.to_path_buf(), id: note.id.clone(), rect, ink, faint, region: None }
    }

    /// The pins of the notes on the state shown, on its pictures: on its
    /// nodes, and on the whole of it.
    fn page_pins(&self) -> Vec<Pin> {
        let (Picture::Pages(pages), Some(wanted)) = (&self.picture, &self.wanted) else {
            return Vec::new();
        };
        let Some(state) = self.state_shown() else {
            return Vec::new();
        };
        let Some(notes) = self.notes.get(&wanted.dir) else {
            return Vec::new();
        };
        let zoom = self.zoom(self.pane());
        let mut pins: Vec<Pin> = Vec::new();
        for (index, (page, (left, top))) in pages.iter().zip(self.origins()).enumerate() {
            // How many pins and dots each node has had so far, to set the
            // next one beside them; and the whole state.
            let mut on: BTreeMap<String, usize> = BTreeMap::new();
            let mut whole = 0;
            for note in notes.iter().filter(|note| note.status != Status::Closed) {
                let (written, r, key, width) = match &note.on {
                    On::Node { state: written, node } => {
                        let Some(placed) = page.placed(&node.origin) else { continue };
                        (written, placed.rect, placed.id.clone(), node.width)
                    }
                    On::Region { state: written, region } => {
                        let Some(r) = page.region(region) else { continue };
                        (written, r, format!("{} {}", r.x + r.width, r.y), region.width)
                    }
                    On::State { state: written } if *written == state && index == 0 => {
                        let right = left + page.width * zoom - PIN / 2.0 - whole as f64 * (PIN + 2.0);
                        whole += 1;
                        let at = rect(right - PIN / 2.0, top - CAPTION / 2.0 - PIN / 2.0 - 2.0, PIN, PIN);
                        pins.push(self.pin(&wanted.dir, note, at, false));
                        continue;
                    }
                    _ => continue,
                };
                let corner = (left + f64::from(r.x + r.width) * zoom, top + f64::from(r.y) * zoom);
                let before = on.entry(key).or_default();
                let faint = *written != state || page.width as u32 != width;
                let region = (matches!(note.on, On::Region { .. }) && !faint).then(|| {
                    let (x, y) = (left + f64::from(r.x) * zoom, top + f64::from(r.y) * zoom);
                    rect(x, y, f64::from(r.width) * zoom, f64::from(r.height) * zoom)
                });
                let side = if faint { DOT } else { PIN };
                let x = corner.0 - side / 2.0 - *before as f64 * (side + 2.0);
                *before += 1;
                let at = rect(x, corner.1 - side / 2.0, side, side);
                pins.push(Pin { region, ..self.pin(&wanted.dir, note, at, faint) });
            }
        }
        pins
    }

    /// The pins of the notes on the whole of each state of the overview, at
    /// the right of its caption.
    fn state_pins(&self) -> Vec<Pin> {
        let (Picture::Overview(overview), Some(wanted)) = (&self.picture, &self.wanted) else {
            return Vec::new();
        };
        let Some(notes) = self.notes.get(&wanted.dir) else {
            return Vec::new();
        };
        let mut pins = Vec::new();
        for (thumb, (x, y, w, _)) in overview.thumbs.iter().zip(self.cells()) {
            let state = self.lines[thumb.line].state.as_ref();
            let on = notes.iter().filter(|note| {
                note.status != Status::Closed
                    && matches!(&note.on, On::State { state: s } if Some(s) == state)
            });
            for (before, note) in on.enumerate() {
                let right = x + w - PIN / 2.0 - before as f64 * (PIN + 2.0);
                let at = rect(right - PIN / 2.0, y - CAPTION / 2.0 - PIN / 2.0 - 2.0, PIN, PIN);
                pins.push(self.pin(&wanted.dir, note, at, false));
            }
        }
        pins
    }

    /// The note whose pin, not a faint dot, is under `at`, a point in the
    /// window: its description's directory, and its id.
    fn pin_at(&self, at: (f64, f64)) -> Option<(PathBuf, String)> {
        self.pins().into_iter().rev().find_map(|pin| {
            let r = pin.rect;
            let (x, y) = (f64::from(r.x), f64::from(r.y));
            let over =
                (x..=x + f64::from(r.width)).contains(&at.0) && (y..=y + f64::from(r.height)).contains(&at.1);
            (over && !pin.faint).then_some((pin.dir, pin.id))
        })
    }

    /// The card of the region picked: its box and width, then each node it
    /// covers, as written. None when none is.
    fn region_card(&self) -> Option<Card<Do>> {
        let region = self.region.as_ref()?;
        let lang = self.lang;
        let (Frame { x, y, width, height }, at) = (region.area, region.width);
        let title = chrome!(
            lang,
            "region  {x}, {y} · {width} × {height} at {at}",
            "区域  {x}, {y} · {width} × {height}，宽 {at}"
        );
        let mut blocks = Vec::new();
        if region.nodes.is_empty() {
            blocks.push(Block::Muted(chrome!(lang, "covers  no node", "覆盖  没有节点")));
        }
        for origin in &region.nodes {
            let node = format!("{}{}", origin.written, reference::instance(&origin.instance));
            blocks.push(Block::Muted(chrome!(lang, "covers  {node}", "覆盖  {node}")));
        }
        Some(Card::new(title, blocks))
    }

    /// The card of the node picked: what it is (its component), the
    /// operation it calls, what was assumed there, and a button that folds
    /// out its details: its id, and its label and box at each width. None
    /// when none is.
    fn node_card(&self) -> Option<Card<Do>> {
        let (Some(id), Picture::Pages(pages)) = (&self.node, &self.picture) else {
            return None;
        };
        let lang = self.lang;
        let (page, node) =
            pages.iter().find_map(|page| Some((page, page.nodes.iter().find(|n| &n.id == id)?)))?;
        let mut blocks = vec![Block::Text(match page.calls.get(id) {
            Some(operations) => {
                let operations = operations.join(", ");
                chrome!(lang, "calls  {operations}", "调用  {operations}")
            }
            None => chrome!(lang, "calls  nothing", "调用  无"),
        })];
        if let Some(assumed) = node.assumed.as_deref() {
            let (guessed, other) = parts(assumed);
            blocks.push(Block::Text(chrome!(lang, "assumed  {guessed}", "假定  {guessed}")));
            blocks.push(Block::Muted(match other {
                Some(other) => chrome!(lang, "or else  {other}", "也可能  {other}"),
                None => {
                    chrome!(lang, "or else  not said: ✗ says what it is", "也可能  没说：用 ✗ 说明是什么")
                }
            }));
        }
        let (fold, details) = match lang {
            Lang::En => (if self.details { "▾" } else { "▸" }, "details"),
            Lang::Zh => (if self.details { "▾" } else { "▸" }, "详情"),
        };
        blocks.push(Block::Button(format!("{fold} {details}"), Do::Details));
        if self.details {
            blocks.push(Block::Muted(chrome!(lang, "id  {id}", "标识  {id}")));
            for page in pages {
                let width = page.width;
                blocks.push(Block::Muted(match page.nodes.iter().find(|n| &n.id == id) {
                    Some(node) => {
                        let r = node.rect;
                        let label = match (node.label, lang) {
                            (Some(n), Lang::En) => format!("label {n}"),
                            (Some(n), Lang::Zh) => format!("编号 {n}"),
                            (None, Lang::En) => "no label".to_string(),
                            (None, Lang::Zh) => "无编号".to_string(),
                        };
                        let (x, y, w, h) = (r.x, r.y, r.width, r.height);
                        chrome!(
                            lang,
                            "at {width}  {label} · box {x}, {y} · {w} × {h}",
                            "宽 {width}  {label} · 框 {x}, {y} · {w} × {h}"
                        )
                    }
                    None => chrome!(lang, "at {width}  not in this state", "宽 {width}  这个状态里没有"),
                }));
            }
        }
        Some(Card::new(node.component.clone(), blocks))
    }

    /// How many notes are open on the tree's line of `row` and `state`: on
    /// the whole description, or on that state of it.
    fn open_notes(&self, row: &Row, state: Option<&State>) -> usize {
        let Some(notes) = row.description().and_then(|dir| self.notes.get(dir)) else {
            return 0;
        };
        let open = notes.iter().filter(|note| note.status == Status::Open);
        match state {
            Some(state) => open.filter(|note| note.on.state() == Some(state)).count(),
            None => open.count(),
        }
    }

    /// The pins drawn: each numbered, the one picked ringed; each faint dot
    /// a small ring.
    fn pin_draws(&self) -> Vec<Draw> {
        let picked = self.note.as_ref().filter(|_| self.picked_note().is_some());
        let mut list = Vec::new();
        for pin in self.pins() {
            if let Some(around) = pin.region {
                list.push(Draw::Box {
                    rect: around,
                    radius: 0.0,
                    fill: None,
                    stroke: Some(pin.ink),
                    dashed: true,
                });
            }
            if pin.faint {
                let radius = pin.rect.width / 2.0;
                list.push(Draw::Box {
                    rect: pin.rect,
                    radius,
                    fill: Some(Ink::Paper),
                    stroke: Some(pin.ink),
                    dashed: false,
                });
                continue;
            }
            if picked.is_some_and(|(dir, id)| *dir == pin.dir && *id == pin.id) {
                let r = pin.rect;
                let around = rect(f64::from(r.x) - 3.0, f64::from(r.y) - 3.0, PIN + 6.0, PIN + 6.0);
                let radius = around.width / 2.0;
                list.push(Draw::Box {
                    rect: around,
                    radius,
                    fill: None,
                    stroke: Some(pin.ink),
                    dashed: false,
                });
            }
            list.push(Draw::Tag { rect: pin.rect, text: pin.id, ink: pin.ink });
        }
        list
    }

    /// The reply of the answered note whose pin the pointer is over, under
    /// its pin: nothing when it is over none.
    fn reply_shown(&mut self) -> Vec<Draw> {
        let Some((dir, id)) = self.hovered.clone() else {
            return Vec::new();
        };
        let Some(pin) = self.pins().into_iter().find(|pin| pin.dir == dir && pin.id == id && !pin.faint)
        else {
            return Vec::new();
        };
        let reply = self
            .notes
            .get(&dir)
            .and_then(|notes| notes.iter().find(|note| note.id == id)?.reply())
            .map(|reply| chrome!(self.lang, "{id} answered: {reply}", "{id} 已回答：{reply}"));
        let Some(said) = reply else {
            return Vec::new();
        };
        let font = Font { size: 13.0, weight: 400, line_height: 16.0 };
        let set = self.typesetter.text(&said, font, Some(320.0));
        let (wide, tall) = (f64::from(set.width) + 1.0, f64::from(set.height));
        let (x, y) = (f64::from(pin.rect.x), f64::from(pin.rect.y) + PIN + 4.0);
        let mut text = words(rect(x + 8.0, y + 6.0, wide, tall), said, Ink::Text, 400);
        if let Draw::Text { max_lines, .. } = &mut text {
            *max_lines = None;
        }
        vec![
            Draw::Box {
                rect: rect(x, y, wide + 16.0, tall + 12.0),
                radius: 4.0,
                fill: Some(Ink::Paper),
                stroke: Some(Ink::Answered),
                dashed: false,
            },
            text,
        ]
    }

    /// How much the pictures are scaled down to fit side by side in a pane
    /// `pane` points wide; never up, and a state's never below
    /// [`SMALLEST`]: past that they scroll across.
    fn zoom(&self, pane: f64) -> f64 {
        match &self.picture {
            Picture::Pages(pages) => {
                let wide: f64 = pages.iter().map(|p| p.width).sum();
                let gaps = MARGIN * (pages.len() + 1) as f64;
                ((pane - gaps) / wide).clamp(SMALLEST, 1.0)
            }
            Picture::Overview(_) | Picture::Faults(_) => 1.0,
            Picture::Flow(flow) => ((pane - 2.0 * MARGIN) / flow.map.width.max(1.0)).clamp(0.1, 1.0),
        }
    }

    /// How tall the pictures' pane is, all of it, in a pane `pane` wide.
    fn pictures_height(&mut self, pane: f64) -> f64 {
        match &self.picture {
            Picture::Pages(pages) => {
                let tallest = pages.iter().map(|p| p.height).fold(0.0, f64::max);
                MARGIN + CAPTION + tallest * self.zoom(pane) + MARGIN
            }
            Picture::Overview(overview) => {
                let rows = overview.thumbs.len().div_ceil(Self::columns(pane)) as f64;
                MARGIN + rows * (CAPTION + THUMB_TALL + SPACE) - SPACE + MARGIN
            }
            Picture::Faults(_) => self.faults(pane).1 + MARGIN,
            Picture::Flow(flow) => MARGIN + flow.map.height * self.zoom(pane) + MARGIN,
        }
    }

    /// How wide a state's pictures are, side by side, in a pane `pane`
    /// wide, the space around them included: wider than the pane only at
    /// [`SMALLEST`]. Nothing else is shown wider than its pane.
    fn pictures_width(&self, pane: f64) -> f64 {
        let Picture::Pages(pages) = &self.picture else {
            return 0.0;
        };
        let zoom = self.zoom(pane);
        pages.iter().map(|page| page.width * zoom + MARGIN).sum::<f64>() + MARGIN
    }

    /// The picked description's faults listed in a pane `pane` wide, from
    /// its top, unscrolled: a heading, then each finding as `check` names
    /// it, where it is over what is wrong. How far down the list ends.
    fn faults(&mut self, pane: f64) -> (Vec<Draw>, f64) {
        let Picture::Faults(faults) = &self.picture else {
            return (Vec::new(), 0.0);
        };
        let wide = pane - 2.0 * MARGIN;
        let path = self.picked.and_then(|i| Some(self.lines.get(i)?.row.dir.clone())).unwrap_or_default();
        let within = path.strip_prefix(&self.root).unwrap_or(&path).display().to_string();
        let journey = self.picked.and_then(|i| self.lines.get(i)).is_some_and(|line| line.journey);
        let lang = self.lang;
        let heading = match (journey, faults.len()) {
            (true, 1) => chrome!(
                lang,
                "{within} has a fault, so it is not played: `underdraw check` its root",
                "{within} 有 1 个错误，所以不能播放：对它的根目录运行 `underdraw check`"
            ),
            (true, n) => chrome!(
                lang,
                "{within} has {n} faults, so it is not played: `underdraw check` its root",
                "{within} 有 {n} 个错误，所以不能播放：对它的根目录运行 `underdraw check`"
            ),
            (false, 1) => chrome!(
                lang,
                "{within} has a fault, so it is not drawn: `underdraw check` it",
                "{within} 有 1 个错误，所以画不出来：对它运行 `underdraw check`"
            ),
            (false, n) => chrome!(
                lang,
                "{within} has {n} faults, so it is not drawn: `underdraw check` it",
                "{within} 有 {n} 个错误，所以画不出来：对它运行 `underdraw check`"
            ),
        };
        let mut list = vec![words(rect(0.0, MARGIN, wide, 18.0), heading, Ink::Text, 600)];
        let mut y = MARGIN + CAPTION + GAP;
        for fault in faults {
            list.push(words(rect(0.0, y, wide, 16.0), fault.at.clone(), Ink::Muted, 600));
            y += 18.0;
            let font = Font { size: 13.0, weight: 400, line_height: 16.0 };
            let tall = f64::from(self.typesetter.text(&fault.problem, font, Some(wide as f32)).height);
            let mut problem = words(rect(0.0, y, wide, tall), fault.problem.clone(), Ink::Text, 400);
            if let Draw::Text { max_lines, .. } = &mut problem {
                *max_lines = None;
            }
            list.push(problem);
            y += tall + GAP;
        }
        (list, y)
    }

    /// The scrolls kept inside what there is to scroll.
    fn clamp(&mut self) {
        let height = self.size.1;
        let below = (self.pictures_height(self.pane()) - height).max(0.0);
        self.scroll = self.scroll.clamp(0.0, below);
        let beyond = (self.pictures_width(self.pane()) - self.pane()).max(0.0);
        self.across = self.across.clamp(0.0, beyond);
        let below = (self.sidebar_height() - height).max(0.0);
        self.side_scroll = self.side_scroll.clamp(0.0, below);
        let below = (self.lines.len() as f64 * ROW + MARGIN - height).max(0.0);
        self.tree_scroll = self.tree_scroll.clamp(0.0, below);
    }

    /// The whole window, in points.
    fn frame(&mut self) -> Scene {
        let (width, height) = self.size;
        let mut list =
            vec![Draw::Line { from: [TREE as f32, 0.0], to: [TREE as f32, height as f32], ink: Ink::Line }];
        for (index, line) in self.lines.iter().enumerate() {
            let Line { row, state, .. } = line;
            let y = MARGIN / 2.0 + index as f64 * ROW - self.tree_scroll;
            if self.picked == Some(index) {
                list.push(Draw::Box {
                    rect: rect(4.0, y, TREE - 8.0, ROW),
                    radius: 4.0,
                    fill: Some(Ink::Fill),
                    stroke: None,
                    dashed: false,
                });
            }
            let x = 12.0 + row.depth as f64 * INDENT;
            let (name, ink, weight) = match (row.description(), state) {
                _ if line.journey => (format!("→ {}", row.name), Ink::Text, 400),
                (None, _) => (format!("{}/", row.name), Ink::Muted, 600),
                (Some(_), None) => (row.name.clone(), Ink::Text, 600),
                (Some(_), Some(State::Fixture(_))) => (row.name.clone(), Ink::Text, 400),
                (Some(_), Some(State::Phase(_))) => (row.name.clone(), Ink::Muted, 400),
            };
            let mut right = TREE - 8.0;
            let open = self.open_notes(row, state.as_ref());
            if open > 0 {
                let mark = match open {
                    1 => chrome!(self.lang, "1 note", "1 条便签"),
                    n => chrome!(self.lang, "{n} notes", "{n} 条便签"),
                };
                let mut tag = words(rect(x, y + 5.0, right - 4.0 - x, 16.0), mark, Ink::Note, 600);
                if let Draw::Text { align, .. } = &mut tag {
                    *align = Align::End;
                }
                list.push(tag);
                right -= 60.0;
            }
            if state.is_none() && !row.faults.is_empty() {
                let mark = match row.faults.len() {
                    1 => chrome!(self.lang, "1 fault", "1 个错误"),
                    n => chrome!(self.lang, "{n} faults", "{n} 个错误"),
                };
                let mut tag = words(rect(x, y + 5.0, right - 4.0 - x, 16.0), mark, Ink::Strong, 600);
                if let Draw::Text { align, .. } = &mut tag {
                    *align = Align::End;
                }
                list.push(tag);
                right -= 64.0;
            }
            list.push(words(rect(x, y + 5.0, right - x, 16.0), name, ink, weight));
        }
        let left = TREE + MARGIN;
        let (faults, _) = self.faults(self.pane());
        let mut pane: Vec<Draw> = faults.into_iter().map(|draw| moved(draw, left, -self.scroll)).collect();
        let zoom = self.zoom(self.pane());
        let mut pages = Vec::new();
        let labels = self.labels();
        let mut thumbs = Vec::new();
        let (origins, outlines) = (self.origins(), self.outlines());
        let marks = self.marks();
        let regions = self.regions();
        let (pins, reply) = (self.pin_draws(), self.reply_shown());
        let (cells, arrows) = (self.cells(), self.arrows());
        match &self.picture {
            Picture::Pages(shots) => {
                for (shot, at) in shots.iter().zip(origins) {
                    pane.push(words(
                        rect(at.0, at.1 - CAPTION, shot.width * zoom, 16.0),
                        shot.caption.clone(),
                        Ink::Muted,
                        400,
                    ));
                    pane.push(Draw::Box {
                        rect: rect(at.0 - 0.5, at.1 - 0.5, shot.width * zoom + 1.0, shot.height * zoom + 1.0),
                        radius: 0.0,
                        fill: None,
                        stroke: Some(Ink::Line),
                        dashed: false,
                    });
                    pages.push((at, &shot.scene));
                }
                // The node a journey acts on next, ringed twice in the
                // notes' ink: no wireframe draws in it.
                for around in marks {
                    for inset in [0.0, 1.5] {
                        pane.push(Draw::Box {
                            rect: rect(
                                f64::from(around.x) + inset,
                                f64::from(around.y) + inset,
                                f64::from(around.width) - 2.0 * inset,
                                f64::from(around.height) - 2.0 * inset,
                            ),
                            radius: 5.0,
                            fill: None,
                            stroke: Some(Ink::Note),
                            dashed: false,
                        });
                    }
                }
                for around in outlines {
                    pane.push(Draw::Box {
                        rect: around,
                        radius: 3.0,
                        fill: None,
                        stroke: Some(Ink::Strong),
                        dashed: false,
                    });
                }
                for around in regions {
                    pane.push(Draw::Box {
                        rect: around,
                        radius: 0.0,
                        fill: None,
                        stroke: Some(Ink::Strong),
                        dashed: true,
                    });
                }
            }
            Picture::Overview(overview) => {
                for (thumb, (x, y, w, h)) in overview.thumbs.iter().zip(cells) {
                    let ink = if thumb.phase { Ink::Muted } else { Ink::Text };
                    pane.push(words(rect(x, y - CAPTION, w, 16.0), thumb.name.clone(), ink, 600));
                    pane.push(Draw::Box {
                        rect: rect(x - 0.5, y - 0.5, w + 1.0, h + 1.0),
                        radius: 0.0,
                        fill: None,
                        stroke: Some(Ink::Line),
                        dashed: false,
                    });
                    thumbs.push(((x, y, w, h), w / thumb.width, &thumb.scene));
                }
                pane.extend(arrows);
                // Over the pictures, in the notes' ink, as the node a
                // journey acts on is marked when it plays.
                for (at, pass) in self.passes() {
                    pane.push(Draw::Box {
                        rect: at,
                        radius: 9.0,
                        fill: Some(Ink::Paper),
                        stroke: Some(Ink::Note),
                        dashed: false,
                    });
                    pane.push(Draw::Text {
                        rect: rect(
                            f64::from(at.x) + 5.0,
                            f64::from(at.y) + 2.0,
                            f64::from(at.width) - 10.0,
                            14.0,
                        ),
                        text: pass.label.clone(),
                        font: PASS_FONT,
                        ink: Ink::Note,
                        max_lines: Some(1),
                        align: Align::Start,
                        underline: false,
                        strike: false,
                    });
                }
            }
            Picture::Faults(_) | Picture::Flow(_) => {}
        }
        pane.extend(pins);
        pane.extend(reply);
        let right = self.pane_right();
        let side = if self.sidebar {
            let cards = self.sidebar_cards();
            let mut side = vec![
                Draw::Box {
                    rect: rect(right, 0.0, width - right, height),
                    radius: 0.0,
                    fill: Some(Ink::Fill),
                    stroke: None,
                    dashed: false,
                },
                Draw::Line { from: [right as f32, 0.0], to: [right as f32, height as f32], ink: Ink::Line },
            ];
            side.extend(sidebar::lay_out(&cards, right, -self.side_scroll, &mut self.typesetter).draws);
            side
        } else {
            Vec::new()
        };

        let mut scene = Scene::new();
        let at = Affine::IDENTITY;
        let paper = Area::new(0.0, 0.0, width, height);
        scene.fill(Fill::NonZero, at, vello::peniko::Color::WHITE, None, &paper);
        scene.push_clip_layer(Fill::NonZero, at, &Area::new(TREE, 0.0, right, height));
        for ((x, y), page) in pages {
            scene.append(page, Some(Affine::translate((x, y)) * Affine::scale(zoom)));
        }
        // A tall state is cut at the foot of its picture.
        for ((x, y, w, h), zoom, thumb) in thumbs {
            scene.push_clip_layer(Fill::NonZero, at, &Area::new(x, y, x + w, y + h));
            scene.append(thumb, Some(Affine::translate((x, y)) * Affine::scale(zoom)));
            scene.pop_layer();
        }
        if let Picture::Flow(flow) = &self.picture {
            let at = Affine::translate(self.flow_origin()) * Affine::scale(zoom);
            for (place, card) in flow.map.places.iter().zip(&flow.tiles) {
                let Tile::Picture { scene: picture, width } = card else { continue };
                let (x, y) = (place.x, place.y + flow::TITLE);
                scene.push_clip_layer(
                    Fill::NonZero,
                    at,
                    &Area::new(x, y, x + flow::WIDE, place.y + flow::TALL),
                );
                let small = Affine::translate((x, y)) * Affine::scale(flow::WIDE / width);
                scene.append(picture, Some(at * small));
                scene.pop_layer();
            }
            paint::draws(&mut scene, at, &flow_draws(flow, self.arrow, self.lang), &mut self.typesetter);
        }
        // The labels under what the window draws over the pictures: the
        // outline, the pins.
        let mut over = labels;
        over.extend(pane);
        paint::draws(&mut scene, at, &over, &mut self.typesetter);
        scene.pop_layer();
        paint::draws(&mut scene, at, &side, &mut self.typesetter);
        paint::draws(&mut scene, at, &list, &mut self.typesetter);
        scene
    }

    /// The window drawn as it now stands.
    fn redraw(&mut self) {
        self.clamp();
        let frame = self.frame();
        let Some(shown) = &mut self.shown else {
            return;
        };
        let scene = {
            let mut scene = Scene::new();
            scene.append(&frame, Some(Affine::scale(shown.window.scale_factor())));
            scene
        };
        // Nowhere to say it: the window shows the last frame drawn.
        let _ = shown.gpu.present(&shown.surface, &scene);
    }
}

/// The flow's arrows, in its own points: each as the stretches it is drawn
/// with, and where it is named, where it leaves its box, so that the arrows
/// from one box, each leaving at its own height, are named apart.
fn flow_arrows(flow: &Flow) -> Vec<(Vec<(f64, f64)>, Rect)> {
    let (places, arrows) = (&flow.map.places, &flow.map.arrows);
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

/// The flow drawn, in its own points, but for the pictures in its boxes:
/// each box with its name over it, and each navigate an arrow labelled with
/// its node, in orange to a screen not drawn yet; the one at `picked` in the
/// strong ink.
fn flow_draws(flow: &Flow, picked: Option<usize>, lang: Lang) -> Vec<Draw> {
    let mut list = Vec::new();
    let (places, arrows) = (&flow.map.places, &flow.map.arrows);
    for (index, (arrow, (path, at))) in arrows.iter().zip(flow_arrows(flow)).enumerate() {
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
            font.size = 11.0;
            if path[1].0 < path[0].0 {
                *align = Align::End;
            }
        }
        list.push(label);
    }
    for (place, card) in places.iter().zip(&flow.tiles) {
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
            radius: 6.0,
            fill: None,
            stroke: Some(if undrawn { Ink::Assumed } else { Ink::Line }),
            dashed: undrawn,
        });
        list.push(words(rect(x + 8.0, y + 4.0, flow::WIDE - 16.0, 16.0), name, ink, 600));
        list.push(line((x, y + flow::TITLE), (x + flow::WIDE, y + flow::TITLE), Ink::Line));
        let middle = (x + flow::WIDE / 2.0, y + (flow::TITLE + flow::TALL) / 2.0);
        match card {
            Tile::Picture { .. } => {}
            Tile::Faulty(faults) => {
                let mark = match faults {
                    1 => chrome!(lang, "faulty: 1 fault, not drawn", "有错：1 个错误，没有画"),
                    n => chrome!(lang, "faulty: {n} faults, not drawn", "有错：{n} 个错误，没有画"),
                };
                let mut said =
                    words(rect(x + 8.0, middle.1 - 8.0, flow::WIDE - 16.0, 16.0), mark, Ink::Strong, 600);
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

/// A line from `from` to `to`.
/// What an assumption says, in its two parts: what was guessed, and what
/// else it could be, split where it first says "; " (or "；"); the second
/// none when it does not.
fn parts(assumed: &str) -> (&str, Option<&str>) {
    match assumed.find([';', '；']) {
        Some(at) => {
            let split = assumed[at..].chars().next().map_or(1, char::len_utf8);
            let other = assumed[at + split..].trim();
            (assumed[..at].trim(), (!other.is_empty()).then_some(other))
        }
        None => (assumed.trim(), None),
    }
}

/// An answer to an assumption, as the sidebar marks it in `lang`.
fn answered(answer: Answer, lang: Lang) -> &'static str {
    match (answer, lang) {
        (Answer::Right, Lang::En) => "✓ right",
        (Answer::Right, Lang::Zh) => "✓ 对",
        (Answer::Wrong, Lang::En) => "✗ wrong",
        (Answer::Wrong, Lang::Zh) => "✗ 不对",
    }
}

/// Where `note` is in its life, as the sidebar says it in `lang`.
fn note_status(note: &Note, lang: Lang) -> String {
    match (&note.status, &note.closed_because) {
        (Status::Open, _) => chrome!(lang, "open, for the agent to answer", "待代理回答"),
        (Status::Answered, _) => chrome!(lang, "answered", "已回答"),
        (Status::Closed, Some(why)) => chrome!(lang, "closed: {why}", "已关闭：{why}"),
        (Status::Closed, None) => chrome!(lang, "closed", "已关闭"),
    }
}

fn line(from: (f64, f64), to: (f64, f64), ink: Ink) -> Draw {
    Draw::Line { from: [from.0 as f32, from.1 as f32], to: [to.0 as f32, to.1 as f32], ink }
}

/// The head of an arrow whose last stretch runs from `before` to `tip`.
fn head(before: (f64, f64), tip: (f64, f64), ink: Ink) -> [Draw; 2] {
    let (dx, dy) = (tip.0 - before.0, tip.1 - before.1);
    let length = dx.hypot(dy).max(1.0);
    let (ux, uy) = (dx / length, dy / length);
    [-1.0, 1.0].map(|side| {
        let wing = (tip.0 - 8.0 * ux - side * 4.0 * uy, tip.1 - 8.0 * uy + side * 4.0 * ux);
        line(wing, tip, ink)
    })
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x: x as f32, y: y as f32, width: width.max(0.0) as f32, height: height.max(0.0) as f32 }
}

/// Where a way from `from`, inside `cell` (left, top, width, height), going
/// `way` (of length 1) leaves it, `gap` further on.
fn edge(cell: (f64, f64, f64, f64), from: (f64, f64), way: (f64, f64), gap: f64) -> (f64, f64) {
    let (x, y, w, h) = cell;
    let across = |at: f64, low: f64, high: f64, by: f64| {
        if by > 0.0 {
            (high - at) / by
        } else if by < 0.0 {
            (low - at) / by
        } else {
            f64::INFINITY
        }
    };
    let t = across(from.0, x, x + w, way.0).min(across(from.1, y, y + h, way.1)).max(0.0) + gap;
    (from.0 + way.0 * t, from.1 + way.1 * t)
}

/// `draw`, a text, moved `dx` across and `dy` down.
fn moved(mut draw: Draw, dx: f64, dy: f64) -> Draw {
    if let Draw::Text { rect, .. } = &mut draw {
        rect.x += dx as f32;
        rect.y += dy as f32;
    }
    draw
}

/// `text` on one line in `rect`, in the interface's size.
fn words(rect: Rect, text: String, ink: Ink, weight: u16) -> Draw {
    Draw::Text {
        rect,
        text,
        font: Font { size: 13.0, weight, line_height: 16.0 },
        ink,
        max_lines: Some(1),
        align: Align::Start,
        underline: false,
        strike: false,
    }
}

impl ApplicationHandler<Told> for Gallery {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shown.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(chrome!(
                self.lang,
                "underdraw gallery — {}",
                "underdraw 画廊 — {}",
                self.root.display()
            ))
            .with_inner_size(LogicalSize::new(1440.0, 900.0));
        let opened = event_loop.create_window(attributes).map_err(anyhow::Error::from).and_then(|window| {
            let window = Arc::new(window);
            let size = window.inner_size();
            let (gpu, surface) =
                gpu::Gpu::open_for_window(window.clone(), size.width.max(1), size.height.max(1))?;
            Ok(Shown { window, gpu, surface })
        });
        match opened {
            Ok(shown) => {
                let size = shown.window.inner_size().to_logical::<f64>(shown.window.scale_factor());
                self.size = (size.width, size.height);
                if let Some(proxy) = self.proxy.clone() {
                    let forward = proxy.clone();
                    self.answering =
                        Answering::start(&self.root, move || forward.send_event(Told::ComeForward).is_ok());
                    gallery::watch(&self.root, move || proxy.send_event(Told::Changed).is_ok());
                }
                report(&json!({ "opened": true }));
                self.reported = true;
                // Drawn once the command has been told, so it answers at once.
                if let Some(first) = self.lines.iter().position(|line| line.row.description().is_some()) {
                    self.pick(first);
                }

                shown.window.request_redraw();
                self.shown = Some(shown);
            }
            Err(e) => {
                report(&json!({ "error": format!("{e:#}") }));
                self.reported = true;
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, told: Told) {
        match told {
            Told::ComeForward => {
                if let Some(shown) = &self.shown {
                    shown.window.set_minimized(false);
                    shown.window.set_visible(true);
                    shown.window.focus_window();
                }
            }
            // Drawn again in place: the window is not brought forward.
            Told::Changed => {
                self.reload();
                if let Some(shown) = &self.shown {
                    shown.window.request_redraw();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(shown) = &mut self.shown else {
            return;
        };
        let scale = shown.window.scale_factor();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    shown.gpu.resize(&mut shown.surface, size.width, size.height);
                }
                let points = size.to_logical::<f64>(scale);
                self.size = (points.width, points.height);
                shown.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let at = position.to_logical::<f64>(scale);
                self.cursor = (at.x, at.y);
                let window = shown.window.clone();
                if self.hover() || self.held.is_some() {
                    window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let held = modifiers.state();
                self.acting = held.super_key() || held.control_key();
                self.shift = held.shift_key();
            }
            WindowEvent::KeyboardInput { event, .. } if self.acting && is_key(&event, "v", KeyCode::KeyV) => {
                let window = shown.window.clone();
                let pasted = self.clipboard().and_then(|clipboard| clipboard.get_text().ok());
                if pasted.is_some_and(|text| self.type_in(&text)) {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if self.acting && is_key(&event, "\\", KeyCode::Backslash) =>
            {
                let window = shown.window.clone();
                self.toggle_sidebar();
                window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } if self.acting && copies(&event) => {
                let window = shown.window.clone();
                self.copy();
                window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. }
                if !self.acting && event.state == ElementState::Pressed =>
            {
                let window = shown.window.clone();
                if self.key(&event.logical_key, event.text.as_deref()) {
                    window.request_redraw();
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                let window = shown.window.clone();
                if self.type_in(&text) {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (right, down) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-f64::from(x) * NOTCH, -f64::from(y) * NOTCH),
                    MouseScrollDelta::PixelDelta(moved) => {
                        let moved = moved.to_logical::<f64>(scale);
                        (-moved.x, -moved.y)
                    }
                };
                // A wheel with no sideways turn scrolls across with Shift.
                let (right, down) = if self.shift && right == 0.0 { (down, 0.0) } else { (right, down) };
                shown.window.request_redraw();
                self.wheel(right, down);
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let window = shown.window.clone();
                if self.press() {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                let window = shown.window.clone();
                if self.release() {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
        // What an input method composes goes into a note, and only there.
        if let Some(shown) = &self.shown {
            shown.window.set_ime_allowed(self.draft.is_some());
        }
    }
}

/// Whether `inner` lies wholly within `outer`, edges included.
fn within(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// Whether `event` is the C key pressed: ⌘C (Ctrl+C) copies, on any
/// keyboard layout, or where the layout has no C, on the key where C is.
fn copies(event: &KeyEvent) -> bool {
    is_key(event, "c", KeyCode::KeyC)
}

/// Whether `event` is the key of the letter `letter` pressed, on any
/// keyboard layout, or where the layout has no such letter, on the key
/// `code`, where it is.
fn is_key(event: &KeyEvent, letter: &str, code: KeyCode) -> bool {
    if event.state != ElementState::Pressed || event.repeat {
        return false;
    }
    match &event.logical_key {
        Key::Character(c) if c.chars().all(|c| c.is_ascii_alphabetic()) => c.eq_ignore_ascii_case(letter),
        _ => event.physical_key == PhysicalKey::Code(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAFTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts");

    /// The gallery over the drafts, in a window of 1440 × 900 points that
    /// is never opened.
    fn gallery() -> Gallery {
        let mut gallery = Gallery::new(Path::new(DRAFTS), text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        gallery
    }

    /// The pointer moved over row `index` of the tree.
    fn over_row(gallery: &mut Gallery, index: usize) {
        gallery.cursor = (40.0, MARGIN / 2.0 + (index as f64 + 0.5) * ROW);
    }

    /// The line of the description named `name`.
    fn line_of(gallery: &Gallery, name: &str) -> usize {
        gallery.lines.iter().position(|line| line.row.name == name && line.state.is_none()).unwrap()
    }

    /// The line of the state named `name`.
    fn state_of(gallery: &Gallery, name: &str) -> usize {
        gallery.lines.iter().position(|line| line.row.name == name && line.state.is_some()).unwrap()
    }

    /// The names of the pictures drawn, each with its width.
    fn captions(gallery: &Gallery) -> Vec<&str> {
        match &gallery.picture {
            Picture::Pages(pages) => pages.iter().map(|page| page.caption.as_str()).collect(),
            Picture::Faults(faults) => panic!("no picture: it has faults: {faults:?}"),
            Picture::Overview(_) => panic!("the overview, not a state"),
            Picture::Flow(_) => panic!("no picture: a flow is shown"),
        }
    }

    #[test]
    fn each_description_lists_its_fixtures_then_its_phases_as_written() {
        let gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        let states: Vec<(&str, usize)> = gallery.lines[listing + 1..]
            .iter()
            .take_while(|line| line.state.is_some())
            .map(|line| (line.row.name.as_str(), line.row.depth))
            .collect();
        let depth = gallery.lines[listing].row.depth + 1;
        assert_eq!(
            states,
            ["default", "no-dates", "unavailable", "new-listing", "loading", "error"]
                .map(|name| (name, depth))
        );
        assert!(matches!(gallery.lines[listing + 5].state, Some(State::Phase(_))), "loading is a phase");
    }

    #[test]
    fn picking_a_phase_draws_its_tree() {
        let mut gallery = gallery();
        let loading = line_of(&gallery, "airbnb-listing") + 5;
        over_row(&mut gallery, loading);
        assert!(gallery.press());
        assert_eq!(gallery.picked, Some(loading));
        assert_eq!(captions(&gallery), ["loading · 390", "loading · 1120"]);
    }

    #[test]
    fn picking_a_state_draws_it_at_both_widths() {
        let mut gallery = gallery();
        let docs = line_of(&gallery, "docs");
        over_row(&mut gallery, docs + 1);
        assert!(gallery.press(), "the state is picked");
        assert_eq!(gallery.picked, Some(docs + 1));
        let Picture::Pages(pages) = &gallery.picture else { panic!("docs has no faults") };
        let widths: Vec<f64> = pages.iter().map(|page| page.width).collect();
        assert_eq!(widths, vec![390.0, 1120.0]);
        assert_eq!(captions(&gallery), ["default · 390", "default · 1120"]);
        assert!(!gallery.press(), "picking it again changes nothing");
        gallery.cursor = (TREE + 100.0, 100.0);
        gallery.press();
        assert_eq!(gallery.picked, Some(docs + 1), "a press on the pictures leaves the state picked");
    }

    /// The overview shown: each state's name, and each arrow as (from, to,
    /// label).
    fn overview(gallery: &Gallery) -> (Vec<&str>, Vec<(&str, &str, &str)>) {
        let Picture::Overview(overview) = &gallery.picture else { panic!("not an overview") };
        let names = overview.thumbs.iter().map(|thumb| thumb.name.as_str()).collect();
        let arrows = overview
            .arrows
            .iter()
            .map(|arrow| {
                let name = |index: usize| overview.thumbs[index].name.as_str();
                (name(arrow.from), name(arrow.to), arrow.label.as_str())
            })
            .collect();
        (names, arrows)
    }

    #[test]
    fn picking_a_description_shows_every_state_and_the_transitions_between_them() {
        let mut gallery = gallery();
        let orders = line_of(&gallery, "orders");
        over_row(&mut gallery, orders);
        assert!(gallery.press(), "the description is picked");
        assert_eq!(gallery.picked, Some(orders), "the description itself, not a state of it");
        let listed: Vec<&str> = gallery.lines[orders + 1..]
            .iter()
            .take_while(|line| line.state.is_some())
            .map(|line| line.row.name.as_str())
            .collect();
        let (names, arrows) = overview(&gallery);
        assert_eq!(names, listed, "every state, in the order the tree lists them");
        assert!(names.contains(&"loading"), "a state no transition reaches or leaves is still shown");
        let screen: Value = serde_json::from_str(
            &std::fs::read_to_string(Path::new(DRAFTS).join("orders/screen.json")).unwrap(),
        )
        .unwrap();
        let declared = screen["transitions"].as_array().unwrap();
        assert_eq!(arrows.len(), declared.len(), "an arrow for each transition declared");
        for ((from, to, label), declared) in arrows.iter().zip(declared) {
            assert_eq!((*from, *to), (declared["from"].as_str().unwrap(), declared["to"].as_str().unwrap()));
            let (number, outcome) = label.split_once(" · ").map_or((*label, None), |(n, o)| (n, Some(o)));
            assert!(number.parse::<u32>().is_ok(), "labelled with the node's label: {label}");
            assert_eq!(outcome, declared["outcome"].as_str(), "and, for a call, the outcome");
        }
        assert!(
            arrows.iter().any(|(from, to, label)| (*from, *to) == ("cancel-ready", "cancel-ready")
                && label.ends_with(" · shipped")),
            "one that stays in its state too: {arrows:?}"
        );

        // Each label is the one the node is tagged with in its from state.
        let Picture::Overview(shown) = &gallery.picture else { unreachable!() };
        let retry = shown.arrows.iter().find(|arrow| shown.thumbs[arrow.from].name == "error").unwrap();
        let error = orders + 1 + names.iter().position(|name| *name == "error").unwrap();
        let drawing = draw::lay_out(
            &Path::new(DRAFTS).join("orders"),
            &gallery.lines[error].state.as_ref().unwrap().wanted(OVERVIEW, 1),
            None,
            Lang::En,
        )
        .unwrap();
        let label = drawing.page.nodes.iter().find(|node| node.id == "retry").unwrap().label.unwrap();
        assert_eq!(retry.label, format!("{label} · ok"));

        let cells = gallery.cells();
        assert_eq!(cells.len(), names.len(), "each state has its picture");
        let error_cell = cells[error - orders - 1];
        gallery.cursor = (error_cell.0 + 10.0, error_cell.1 + 10.0);
        assert!(gallery.press(), "clicking a state's picture picks it");
        assert_eq!(gallery.picked, Some(error));
        assert_eq!(captions(&gallery), ["error · 390", "error · 1120"]);
    }

    #[test]
    fn a_tall_page_scrolls_as_far_as_its_end() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        over_row(&mut gallery, listing + 1);
        gallery.press();
        let end = gallery.pictures_height(gallery.pane()) - 900.0;
        assert!(end > 0.0, "the listing is taller than the window");
        gallery.cursor = (TREE + 100.0, 400.0);
        gallery.wheel(0.0, 120.0);
        assert_eq!(gallery.scroll, 120.0);
        gallery.wheel(0.0, 1e6);
        assert_eq!(gallery.scroll, end, "no further than the end");
        gallery.wheel(0.0, -1e6);
        assert_eq!(gallery.scroll, 0.0, "no further than the top");
        over_row(&mut gallery, 0);
        gallery.wheel(0.0, 500.0);
        assert_eq!((gallery.scroll, gallery.tree_scroll), (0.0, 500.0), "over the tree, the tree scrolls");
        gallery.wheel(0.0, 1e6);
        let end = gallery.lines.len() as f64 * ROW + MARGIN - 900.0;
        assert_eq!(gallery.tree_scroll, end, "as far as its last line");
    }

    /// The point in the window at the middle of node `id` in picture `page`.
    fn on_node(gallery: &Gallery, page: usize, id: &str) -> (f64, f64) {
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture: it has faults") };
        let node = pages[page].nodes.iter().find(|n| n.id == id).unwrap();
        let (left, top) = gallery.origins()[page];
        let zoom = gallery.zoom(gallery.pane());
        let r = node.rect;
        let middle = (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0));
        (left + middle.0 * zoom, top + middle.1 * zoom)
    }

    /// The line of the state named `name` of the description on line
    /// `description`.
    fn state_of_in(gallery: &Gallery, description: usize, name: &str) -> usize {
        let mut states = gallery.lines[description + 1..].iter().take_while(|line| line.state.is_some());
        description + 1 + states.position(|line| line.row.name == name).unwrap()
    }

    /// The gallery showing line `index` of the tree.
    fn showing(gallery: &mut Gallery, index: usize) {
        over_row(gallery, index);
        gallery.press();
    }

    #[test]
    fn a_node_picked_in_the_picture_is_outlined_and_says_what_it_is() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.cursor = on_node(&gallery, 0, "nav-save");
        assert!(gallery.press(), "a node is picked");
        assert_eq!(gallery.node.as_deref(), Some("nav-save"), "the innermost, not the bar it sits in");
        assert_eq!(
            gallery.panel(),
            ["IconButton", "calls  toggleSaved", "▸ details", "write a note on it  (N)"],
            "what it is and what it calls; its id, label and boxes folded away"
        );
        assert!(!gallery.press(), "picking it again changes nothing");
        assert_eq!(gallery.outlines().len(), 2, "it is outlined at both widths");
        assert!(click_panel_line(&mut gallery, "▸ details"), "the details unfolded");
        let panel = gallery.panel();
        assert_eq!(
            panel[..5],
            [
                "IconButton",
                "calls  toggleSaved",
                "▾ details",
                "id  nav-save",
                "at 390  label 3 · box 334, 8 · 40 × 40"
            ]
        );
        assert!(panel[5].starts_with("at 1120  label "), "and where it is at the other width: {}", panel[5]);
        assert!(click_panel_line(&mut gallery, "▾ details"), "and folded again");
        assert!(!gallery.panel().iter().any(|line| line.starts_with("id  ")));
    }

    #[test]
    fn a_node_in_a_repeat_is_referenced_by_its_template_and_the_instance_pointed_at() {
        let mut gallery = gallery();
        let orders = line_of(&gallery, "orders");
        showing(&mut gallery, orders + 1);
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture") };
        let id = pages[1].nodes.iter().map(|n| &n.id).find(|id| id.ends_with("-cancel")).unwrap().clone();
        gallery.cursor = on_node(&gallery, 1, &id);
        assert!(gallery.press(), "the cancel button is picked, in the desktop picture");
        let reference = gallery.reference().unwrap();
        assert_eq!(reference.description, Path::new(DRAFTS).join("orders"));
        let On::Node { state, node } = reference.on.clone() else { panic!("not a node: {reference:?}") };
        assert_eq!(state, State::Fixture("default".into()));
        assert_eq!(node.origin.written, "order-{index}-cancel", "the template, not {id}");
        assert_eq!(node.origin.instance.len(), 1, "one each around it");
        assert_eq!(id, format!("order-{}-cancel", node.origin.instance[0]), "the row pointed at");
        assert_eq!(node.width, 1120, "the width it was picked at");
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture") };
        assert_eq!(node.label, pages[1].nodes.iter().find(|n| n.id == id).unwrap().label);
        let screen: Value = serde_json::from_str(
            &std::fs::read_to_string(reference.description.join("screen.json")).unwrap(),
        )
        .unwrap();
        let written =
            screen.pointer(&node.origin.at).unwrap_or_else(|| panic!("{} names nothing", node.origin.at));
        assert_eq!(written["id"], "cancel", "the pointer names that node in screen.json, inside OrderRow");
        let line = reference.line();
        assert!(
            line.contains(&format!("order-{{index}}-cancel at {} in screen.json", node.origin.at)),
            "{line}"
        );
        assert!(line.contains(&format!("pointed at [{}], label ", node.origin.instance[0])), "{line}");
        assert!(line.ends_with("width 1120"), "{line}");
    }

    #[test]
    fn with_no_node_picked_the_state_shown_is_referenced() {
        let mut gallery = gallery();
        let orders = line_of(&gallery, "orders");
        assert!(gallery.reference().is_none(), "an overview is no state");
        gallery.copy();
        assert_eq!(
            gallery.panel()[0],
            "nothing to copy: pick a state, a node or region in it, or an arrow, first"
        );
        let loading = state_of_in(&gallery, orders, "loading");
        showing(&mut gallery, loading);
        let Picture::Pages(_) = &gallery.picture else { panic!("no picture") };
        // A click on a picture's caption picks no node: the state itself.
        let (left, top) = gallery.origins()[0];
        gallery.cursor = (left + 4.0, top - CAPTION / 2.0);
        gallery.press();
        let reference = gallery.reference().unwrap();
        assert_eq!(reference.on, On::State { state: State::Phase("loading".into()) });
        assert_eq!(
            reference.line(),
            format!("{} · phase loading", Path::new(DRAFTS).join("orders").display())
        );
    }

    #[test]
    fn what_was_assumed_is_said() {
        let mut gallery = gallery();
        let checkout = line_of(&gallery, "checkout");
        let states = gallery.lines[checkout + 1..].iter().take_while(|line| line.state.is_some()).count();
        let shown = (1..=states).find(|offset| {
            showing(&mut gallery, checkout + offset);
            let Picture::Pages(pages) = &gallery.picture else { return false };
            pages[0].has("card-number")
        });
        assert!(shown.is_some(), "a state of checkout shows the card form");
        gallery.cursor = on_node(&gallery, 0, "card-number");
        gallery.press();
        assert!(gallery.panel().iter().any(|line| line.starts_with("assumed  Card fields are drawn here")));
    }

    #[test]
    fn an_overlay_is_picked_before_what_it_floats_over() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.cursor = on_node(&gallery, 1, "photos-show-all");
        gallery.press();
        assert_eq!(gallery.node.as_deref(), Some("photos-show-all"), "not the photos under it");
    }

    #[test]
    fn switching_state_keeps_the_node_picked_while_the_state_has_it() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.cursor = on_node(&gallery, 0, "nav-back");
        gallery.press();
        showing(&mut gallery, listing + 2);
        assert_eq!(gallery.node.as_deref(), Some("nav-back"), "another fixture has it");
        showing(&mut gallery, listing + 5);
        assert_eq!(gallery.node.as_deref(), Some("nav-back"), "and so does the loading phase");
        gallery.cursor = on_node(&gallery, 0, "loading-skeleton");
        gallery.press();
        showing(&mut gallery, listing + 1);
        assert_eq!(gallery.node, None, "the data has no skeleton");
        assert!(gallery.panel().is_empty());
    }

    #[test]
    fn a_press_where_no_node_is_clears_the_pick() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.cursor = on_node(&gallery, 0, "nav-save");
        gallery.press();
        let (left, top) = gallery.origins()[1];
        gallery.cursor = (left - MARGIN / 2.0, top + 10.0);
        assert!(gallery.press(), "between the pictures");
        assert_eq!(gallery.node, None);
        assert!(gallery.panel().is_empty(), "the panel is gone");
    }

    #[test]
    fn the_sidebar_is_on_the_right_and_command_backslash_gives_its_width_to_the_pictures() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        assert_eq!(gallery.pane_right(), 1440.0 - sidebar::WIDTH, "the sidebar is shown by default");
        let shown = gallery.sidebar_cards();
        assert_eq!(shown[0].title, "Nothing picked", "with nothing picked, it says what can be done");
        gallery.cursor = on_node(&gallery, 0, "nav-save");
        gallery.press();
        let narrow = gallery.zoom(gallery.pane());
        let (left, _) = gallery.origins()[1];
        assert!(left + 1120.0 * narrow <= gallery.pane_right(), "the pictures end before the sidebar");
        gallery.cursor = (1440.0 - sidebar::WIDTH / 2.0, 890.0);
        assert!(!gallery.press(), "a press in the sidebar off its buttons does nothing");
        assert_eq!(gallery.node.as_deref(), Some("nav-save"), "and leaves the node picked");

        gallery.toggle_sidebar();
        assert_eq!(gallery.pane_right(), 1440.0, "hidden: the pictures take the width");
        assert!(gallery.zoom(gallery.pane()) > narrow, "and are drawn larger");
        gallery.cursor = on_node(&gallery, 1, "nav-save");
        assert!(gallery.press(), "a node near the window's right edge is picked, no sidebar there");
        assert_eq!(gallery.pointed, 1);
        gallery.toggle_sidebar();
        assert_eq!(gallery.pane_right(), 1440.0 - sidebar::WIDTH, "and shown again");
    }

    #[test]
    fn in_a_narrow_window_the_pictures_stop_at_half_size_and_scroll_across() {
        let mut gallery = gallery();
        gallery.size = (1000.0, 900.0);
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        assert_eq!(gallery.zoom(gallery.pane()), SMALLEST, "no smaller than half");
        let beyond = gallery.pictures_width(gallery.pane()) - gallery.pane();
        assert!(beyond > 0.0, "wider than the pane");
        gallery.cursor = (TREE + 100.0, 400.0);
        gallery.wheel(1e6, 0.0);
        assert_eq!(gallery.across, beyond, "scrolled across as far as the last picture's end");
        gallery.cursor = on_node(&gallery, 1, "nav-save");
        assert!(gallery.cursor.0 < gallery.pane_right(), "the desktop picture's save button in view");
        assert!(gallery.press());
        assert_eq!(
            (gallery.node.as_deref(), gallery.pointed),
            (Some("nav-save"), 1),
            "the right node, scaled and scrolled"
        );

        let tag = |draw: &Draw| match draw {
            Draw::Tag { rect, text, .. } => Some((rect.width, text.clone())),
            _ => None,
        };
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture") };
        let tagged: Vec<(f32, String)> = pages.iter().flat_map(|page| &page.tags).filter_map(tag).collect();
        assert!(!tagged.is_empty(), "the labels are kept apart from the scene");
        let drawn: Vec<(f32, String)> = gallery.labels().iter().filter_map(tag).collect();
        assert_eq!(drawn, tagged, "each drawn at its own size, at half size too");
    }

    #[test]
    fn with_lang_zh_the_sidebar_says_nothing_in_english() {
        let mut gallery = gallery();
        gallery.lang = Lang::Zh;
        assert_eq!(gallery.sidebar_cards()[0].title, "未选中任何东西");
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.cursor = on_node(&gallery, 0, "nav-save");
        gallery.press();
        assert_eq!(gallery.panel(), ["IconButton", "调用  toggleSaved", "▸ 详情", "在这里写便签  (N)"]);
        assert!(click_panel_line(&mut gallery, "▸ 详情"));
        assert_eq!(gallery.panel()[3..5], ["标识  nav-save", "宽 390  编号 3 · 框 334, 8 · 40 × 40"]);
        press_keys(&mut gallery, &["n"]);
        assert_eq!(gallery.panel()[0], "写便签");
        press_keys(&mut gallery, &["Esc"]);
        gallery.copy();
        assert!(gallery.panel()[0].starts_with("已复制  "), "{:?}", gallery.panel());
        // Nothing the window says itself is in English: what is left is the
        // description's own names.
        let english = |line: &String| {
            line.split(|c: char| !c.is_ascii_alphabetic())
                .any(|word| ["the", "to", "on", "in", "at", "a", "of", "it"].contains(&word))
        };
        gallery.copy();
        let said: Vec<String> =
            gallery.panel().into_iter().filter(|line| !line.starts_with("已复制")).collect();
        assert!(!said.iter().any(english), "{said:?}");
    }

    #[test]
    fn a_description_with_faults_shows_them() {
        let root = std::env::temp_dir().join(format!("underdraw-window-faults-{}", std::process::id()));
        std::fs::create_dir_all(root.join("broken")).unwrap();
        std::fs::write(root.join("broken").join(crate::SCREEN), "{}").unwrap();
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let broken = &gallery.lines[1].row;
        assert!(!broken.faults.is_empty(), "the tree marks it");
        let found = broken.faults.clone();
        gallery.pick(1);
        let Picture::Faults(shown) = &gallery.picture else {
            panic!("a description with faults is not drawn")
        };
        assert_eq!(shown, &found, "each finding as `check` names it");
        let (list, _) = gallery.faults(gallery.pane());
        let texts: Vec<&str> = list
            .iter()
            .filter_map(|d| if let Draw::Text { text, .. } = d { Some(text.as_str()) } else { None })
            .collect();
        for fault in &found {
            assert!(
                texts.contains(&fault.at.as_str()) && texts.contains(&fault.problem.as_str()),
                "{texts:?}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A root of its own named `name`, holding a copy of the todo draft
    /// at `todo/`.
    fn todo_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("underdraw-window-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("todo")).unwrap();
        for file in [crate::SCREEN, crate::TOKENS] {
            std::fs::copy(Path::new(DRAFTS).join("todo").join(file), root.join("todo").join(file)).unwrap();
        }
        root
    }

    /// The todo description under `root`, as JSON.
    fn todo_screen(root: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(root.join("todo").join(crate::SCREEN)).unwrap())
            .unwrap()
    }

    fn write_todo_screen(root: &Path, screen: &Value) {
        std::fs::write(root.join("todo").join(crate::SCREEN), screen.to_string()).unwrap();
    }

    /// How tall each picture is.
    fn heights(gallery: &Gallery) -> Vec<f64> {
        match &gallery.picture {
            Picture::Pages(pages) => pages.iter().map(|page| page.height).collect(),
            Picture::Faults(faults) => panic!("no picture: it has faults: {faults:?}"),
            Picture::Overview(_) => panic!("the overview, not a state"),
            Picture::Flow(_) => panic!("no picture: a flow is shown"),
        }
    }

    #[test]
    fn an_edited_description_is_drawn_again_in_the_state_shown() {
        let root = todo_root("edited");
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 120.0);
        let typing = state_of(&gallery, "typing");
        gallery.pick(typing);
        let before = heights(&gallery);
        gallery.cursor = (TREE + 100.0, 100.0);
        gallery.wheel(0.0, 40.0);
        assert_eq!(gallery.scroll, 40.0);

        let mut screen = todo_screen(&root);
        let todos = screen["fixtures"]["default"]["todos"].as_array_mut().unwrap();
        for id in 4..12 {
            todos.push(json!({ "id": id.to_string(), "title": format!("第 {id} 项") }));
        }
        // A state added above the one shown.
        let fixtures = screen["fixtures"].as_object_mut().unwrap();
        let empty = fixtures["empty"].clone();
        fixtures.shift_insert(0, "first".to_string(), empty);
        write_todo_screen(&root, &screen);
        gallery.reload();

        let typing = state_of(&gallery, "typing");
        assert_eq!(gallery.picked, Some(typing), "the same state, though a state was added above it");
        assert_eq!(gallery.lines[typing - 2].row.name, "first", "the tree lists the new state");
        assert_eq!(captions(&gallery), ["typing · 390", "typing · 1120"]);
        let after = heights(&gallery);
        assert!(
            after[0] > before[0] && after[1] > before[1],
            "drawn again with more todos: {before:?} {after:?}"
        );
        assert_eq!(gallery.scroll, 40.0, "scrolled as it was");

        let mut screen = todo_screen(&root);
        let fixtures = screen["fixtures"].as_object_mut().unwrap();
        let typing = fixtures.shift_remove("typing").unwrap();
        fixtures.insert("typed".to_string(), typing);
        write_todo_screen(&root, &screen);
        gallery.reload();
        let todo = line_of(&gallery, "todo");
        assert_eq!(gallery.picked, Some(todo), "a state gone: its description's overview");
        assert!(matches!(gallery.picture, Picture::Overview(_)));
        assert_eq!(gallery.scroll, 0.0, "from its top");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_description_added_or_removed_comes_into_the_tree_or_leaves_it() {
        let root = todo_root("added");
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let typing = state_of(&gallery, "typing");
        gallery.pick(typing);

        let added = root.join("a-flow").join("sign-in");
        std::fs::create_dir_all(&added).unwrap();
        for file in [crate::SCREEN, crate::TOKENS] {
            std::fs::copy(root.join("todo").join(file), added.join(file)).unwrap();
        }
        gallery.reload();
        let names: Vec<&str> = gallery
            .lines
            .iter()
            .filter(|line| line.state.is_none())
            .map(|line| line.row.name.as_str())
            .collect();
        assert_eq!(names[1..], ["a-flow", "sign-in", "todo"], "the new one, under its directory");
        let typing = line_of(&gallery, "todo") + 2;
        assert_eq!(gallery.lines[typing].row.name, "typing");
        assert_eq!(gallery.picked, Some(typing), "still the state shown");
        assert_eq!(gallery.lines[gallery.picked.unwrap()].row.description(), Some(&*root.join("todo")));

        std::fs::remove_dir_all(root.join("todo")).unwrap();
        gallery.reload();
        let names: Vec<&str> = gallery
            .lines
            .iter()
            .filter(|line| line.state.is_none())
            .map(|line| line.row.name.as_str())
            .collect();
        assert_eq!(names[1..], ["a-flow", "sign-in"], "the removed one has left");
        assert_eq!(gallery.picked, Some(2), "its description gone: the first description's overview");
        assert_eq!(overview(&gallery).0[0], "default");

        std::fs::remove_dir_all(root.join("a-flow")).unwrap();
        gallery.reload();
        assert!(gallery.lines.is_empty(), "nothing left to list");
        assert_eq!(gallery.picked, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_description_that_gains_faults_shows_them_and_its_picture_once_fixed() {
        let root = todo_root("gains");
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let typing = state_of(&gallery, "typing");
        gallery.pick(typing);
        let fine = todo_screen(&root);

        let mut broken = fine.clone();
        broken["data"]["stray"] = json!("nonsense");
        write_todo_screen(&root, &broken);
        gallery.reload();
        assert!(!gallery.lines[1].row.faults.is_empty(), "the tree marks it");
        assert_eq!(gallery.picked, Some(typing), "the same state");
        assert!(matches!(&gallery.picture, Picture::Faults(shown) if shown == &gallery.lines[1].row.faults));

        std::fs::write(root.join("todo").join(crate::KINDS), "{}").unwrap();
        gallery.reload();
        let Picture::Faults(shown) = &gallery.picture else { panic!("a kinds.json left behind is a fault") };
        assert!(shown.iter().any(|f| f.problem.contains("kinds.json is no longer read")), "{shown:?}");

        std::fs::remove_file(root.join("todo").join(crate::KINDS)).unwrap();
        write_todo_screen(&root, &fine);
        gallery.reload();
        assert!(gallery.lines[1].row.faults.is_empty(), "fixed");
        assert_eq!(captions(&gallery), ["typing · 390", "typing · 1120"], "drawn again");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_state_of_a_description_with_faults_shows_them_too() {
        let root = std::env::temp_dir().join(format!("underdraw-window-state-faults-{}", std::process::id()));
        let todo = Path::new(DRAFTS).join("todo");
        std::fs::create_dir_all(root.join("todo")).unwrap();
        let mut screen: Value =
            serde_json::from_str(&std::fs::read_to_string(todo.join(crate::SCREEN)).unwrap()).unwrap();
        screen["data"]["stray"] = json!("nonsense");
        std::fs::write(root.join("todo").join(crate::SCREEN), screen.to_string()).unwrap();
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        assert!(!gallery.lines[2].row.faults.is_empty(), "the stray field is a fault");
        assert!(gallery.lines[2].state.is_some(), "its states are still listed");
        gallery.pick(2);
        assert!(matches!(&gallery.picture, Picture::Faults(shown) if shown == &gallery.lines[2].row.faults));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The line of state `state` of the description named `description`.
    fn state_in(gallery: &Gallery, description: &str, state: &str) -> usize {
        let at = line_of(gallery, description);
        at + 1
            + gallery.lines[at + 1..]
                .iter()
                .take_while(|l| l.state.is_some())
                .position(|l| l.row.name == state)
                .unwrap()
    }

    /// The gallery showing state `state` of the description `description`.
    fn show(gallery: &mut Gallery, description: &str, state: &str) {
        let line = state_in(gallery, description, state);
        showing(gallery, line);
    }

    /// The state shown, as "<description> · <state>".
    fn shown(gallery: &Gallery) -> String {
        gallery.named(gallery.wanted.as_ref().unwrap())
    }

    /// Node `id` acted on, ⌘ held, in the phone's picture.
    fn act_on(gallery: &mut Gallery, id: &str) -> bool {
        gallery.cursor = on_node(gallery, 0, id);
        gallery.acting = true;
        let changed = gallery.press();
        gallery.acting = false;
        changed
    }

    #[test]
    fn acting_on_a_node_follows_its_transition_and_a_plain_click_only_picks_it() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        gallery.cursor = on_node(&gallery, 0, "order-0-cancel");
        assert!(gallery.press());
        assert_eq!(shown(&gallery), "orders · default", "a plain click picks the node, and stays");
        assert!(act_on(&mut gallery, "order-0-cancel"));
        assert_eq!(shown(&gallery), "orders · cancel-dialog", "a row's button, by its id as written");
        assert_eq!(gallery.panel()[0], "order-{index}-cancel.tap in default → cancel-dialog");
    }

    #[test]
    fn several_outcomes_are_chosen_in_the_panel() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "cancel-ready");
        act_on(&mut gallery, "cd-confirm");
        assert_eq!(shown(&gallery), "orders · cancel-ready", "nothing is shown until an outcome is chosen");
        let panel = gallery.panel();
        assert_eq!(panel[0], "cd-confirm.tap in cancel-ready calls cancelOrder: click how it answered");
        assert_eq!(panel[1..3], ["ok → default", "shipped → cancel-ready"]);
        assert!(click_panel_line(&mut gallery, "ok → default"), "the first outcome clicked");
        assert_eq!(shown(&gallery), "orders · default");
        assert_eq!(gallery.panel()[0], "ok in cancel-ready → default");
    }

    #[test]
    fn navigate_goes_to_the_screens_first_state_and_back_returns_along_the_path() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        act_on(&mut gallery, "order-0-cancel");
        act_on(&mut gallery, "nav-export");
        let export = line_of(&gallery, "export");
        assert_eq!(gallery.picked, Some(export + 1), "the export screen's first state");
        assert_eq!(gallery.panel()[0], "nav-export.tap navigates to ExportDocuments");
        act_on(&mut gallery, "nav-back");
        assert_eq!(shown(&gallery), "orders · cancel-dialog", "back across screens, to where it came from");
        assert_eq!(gallery.panel()[0], "nav-back.tap goes back to orders · cancel-dialog");
        act_on(&mut gallery, "nav-back");
        assert_eq!(shown(&gallery), "orders · default");
        act_on(&mut gallery, "nav-back");
        assert_eq!(shown(&gallery), "orders · default", "nowhere further back");
        assert!(gallery.panel()[0].contains("no act led here"), "{}", gallery.panel()[0]);
    }

    #[test]
    fn a_navigate_to_a_screen_not_drawn_yet_says_so() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        act_on(&mut gallery, "order-0");
        assert_eq!(shown(&gallery), "orders · default");
        assert_eq!(
            gallery.panel()[0],
            format!(
                "order-{{index}}.tap: {}",
                underdraw_format::assumption::undrawn(&["OrderDetail".to_string()], Lang::En)
            )
        );
    }

    #[test]
    fn a_node_with_nothing_declared_says_so() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "cancel-unpaid");
        act_on(&mut gallery, "cd-keep");
        assert_eq!(shown(&gallery), "orders · cancel-unpaid", "nothing declared: it stays");
        let panel = gallery.panel();
        assert_eq!(panel[0], "no transition from cancel-unpaid on cd-keep.tap · should go to:");
        let ids = panel.iter().position(|line| line == "Button").expect("and the node is picked");
        let offered: Vec<&str> = panel[1..ids].iter().map(|line| line.trim()).collect();
        assert_eq!(offered[0], "default", "the description's states, as the tree lists them: {offered:?}");
        assert!(offered.contains(&"cancel-unpaid") && offered.contains(&"loading"), "{offered:?}");
        assert!(
            offered.contains(&"navigate to ExportDocuments"),
            "and the screens under the root: {offered:?}"
        );
        assert!(!offered.iter().any(|line| line.ends_with("OrderList")), "but not its own: {offered:?}");
        act_on(&mut gallery, "nav-export");
        let todo = line_of(&gallery, "todo");
        over_row(&mut gallery, todo);
        gallery.press();
        assert!(
            gallery.history.is_empty() && gallery.acted.is_none(),
            "picking in the tree starts a new path"
        );
    }

    #[test]
    fn choosing_where_nothing_declared_should_go_writes_a_note_and_never_the_description() {
        let root = root_of("propose", &[("orders", "orders"), ("export", "export")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let dir = root.join("orders");
        let screen = std::fs::read_to_string(dir.join(crate::SCREEN)).unwrap();
        show(&mut gallery, "orders", "cancel-unpaid");
        act_on(&mut gallery, "cd-keep");
        assert!(click_panel_line(&mut gallery, "default"), "a state chosen");
        assert_eq!(
            shown(&gallery),
            "orders · cancel-unpaid",
            "a proposal goes nowhere: the agent declares it"
        );
        assert!(gallery.panel()[0].starts_with("N1 saved"), "{:?}", gallery.panel());
        act_on(&mut gallery, "cd-keep");
        assert!(click_panel_line(&mut gallery, "navigate to ExportDocuments"), "a screen chosen");

        let written = notes::read(&dir).unwrap();
        let proposed: Vec<(String, Option<&Proposal>, &str)> = written
            .iter()
            .map(|note| {
                let On::Node { state, node } = &note.on else { panic!("not on a node: {note:?}") };
                (node.origin.written.clone(), note.proposes.as_ref(), state.name())
            })
            .collect();
        let on = "cd-keep.tap".to_string();
        assert_eq!(
            proposed,
            [
                (
                    "cd-keep".to_string(),
                    Some(&Proposal { on: on.clone(), goes: Goes::To("default".into()) }),
                    "cancel-unpaid"
                ),
                (
                    "cd-keep".to_string(),
                    Some(&Proposal { on, goes: Goes::Navigate("ExportDocuments".into()) }),
                    "cancel-unpaid"
                ),
            ]
        );
        assert_eq!(
            written[0].thread[0].says,
            "cd-keep.tap in cancel-unpaid should go to default: declare the transition"
        );
        let listed = notes::notes(&root, None, None, false, false).unwrap();
        assert_eq!(
            listed["notes"][1]["proposes"],
            json!({ "on": "cd-keep.tap", "navigate": "ExportDocuments" })
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(crate::SCREEN)).unwrap(),
            screen,
            "screen.json untouched"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A root of its own named `name`, holding copies of the drafts `copied`
    /// at the paths beside each.
    fn root_of(name: &str, copied: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("underdraw-window-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (draft, at) in copied {
            std::fs::create_dir_all(root.join(at)).unwrap();
            for file in [crate::SCREEN, crate::TOKENS] {
                let from = Path::new(DRAFTS).join(draft).join(file);
                if from.is_file() {
                    std::fs::copy(from, root.join(at).join(file)).unwrap();
                }
            }
        }
        root
    }

    /// The flow shown.
    fn flow_shown(gallery: &Gallery) -> &Flow {
        match &gallery.picture {
            Picture::Flow(flow) => flow,
            _ => panic!("no flow is shown"),
        }
    }

    /// The texts drawn in the flow.
    fn flow_texts(flow: &Flow) -> Vec<String> {
        flow_draws(flow, None, Lang::En)
            .into_iter()
            .filter_map(|d| if let Draw::Text { text, .. } = d { Some(text) } else { None })
            .collect()
    }

    #[test]
    fn picking_a_directory_draws_the_flow_between_its_screens() {
        let root = root_of(
            "flow",
            &[("airbnb-listing", "shop/listing"), ("checkout", "shop/checkout"), ("todo", "todo")],
        );
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let shop = gallery.lines.iter().position(|line| line.row.name == "shop").unwrap();
        over_row(&mut gallery, shop);
        assert!(gallery.press(), "a directory can be picked");
        assert_eq!(gallery.picked, Some(shop));
        let flow = flow_shown(&gallery);
        let screens: Vec<&Screen> = flow.map.places.iter().map(|place| &place.screen).collect();
        let undrawn = |name: &str| Screen::Undrawn(name.to_string());
        assert_eq!(
            screens,
            [
                &Screen::Drawn(root.join("shop/checkout")),
                &Screen::Drawn(root.join("shop/listing")),
                &undrawn("AddressBook"),
                &undrawn("AddressForm"),
                &undrawn("PhotoGallery"),
                &undrawn("FullMap"),
            ],
            "its screens, todo not among them, and a box for each screen not drawn yet"
        );
        // From the links `check <root>` reports, each an arrow.
        let reported: Vec<(PathBuf, String, Screen)> = links::links(&root)
            .into_iter()
            .filter(|link| link.description.starts_with(root.join("shop")))
            .map(|link| {
                (link.description, link.node, link.target.map_or(Screen::Undrawn(link.to), Screen::Drawn))
            })
            .collect();
        let drawn: Vec<(PathBuf, String, Screen)> = flow
            .map
            .arrows
            .iter()
            .map(|arrow| {
                let Screen::Drawn(from) = &flow.map.places[arrow.from].screen else {
                    panic!("from a description")
                };
                (from.clone(), arrow.link.node.clone(), flow.map.places[arrow.to].screen.clone())
            })
            .collect();
        assert_eq!(drawn, reported);
        assert!(drawn.contains(&(
            root.join("shop/listing"),
            "reserve-button".to_string(),
            Screen::Drawn(root.join("shop/checkout"))
        )));
        assert!(
            matches!(flow.tiles[..2], [Tile::Picture { .. }, Tile::Picture { .. }]),
            "each with its first state"
        );
        assert!(flow.tiles[2..].iter().all(|card| matches!(card, Tile::Undrawn)));
        let texts = flow_texts(flow);
        for name in ["checkout", "listing", "PhotoGallery", "reserve-button", "photos-show-all"] {
            assert!(texts.iter().any(|text| text == name), "{name} in {texts:?}");
        }
        let draws = flow_draws(flow, None, Lang::En);
        let dashed = draws
            .iter()
            .filter(|d| matches!(d, Draw::Box { dashed: true, stroke: Some(Ink::Assumed), .. }))
            .count();
        let marks = draws.iter().filter(|d| matches!(d, Draw::Tag { text, .. } if text == "?")).count();
        assert_eq!((dashed, marks), (4, 4), "a dashed box marked \"?\" for each screen not drawn yet");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clicking_a_box_of_the_flow_picks_its_description() {
        let root =
            root_of("flow-click", &[("airbnb-listing", "shop/listing"), ("checkout", "shop/checkout")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        showing(&mut gallery, 0);
        let flow = flow_shown(&gallery);
        let place =
            flow.map.places.iter().find(|p| p.screen == Screen::Drawn(root.join("shop/listing"))).unwrap();
        let ((left, top), zoom) = (gallery.flow_origin(), gallery.zoom(gallery.pane()));
        let undrawn = flow.map.places.iter().find(|p| matches!(p.screen, Screen::Undrawn(_))).unwrap();
        let on = |p: &flow::Place| (left + (p.x + 20.0) * zoom, top + (p.y + 40.0) * zoom);
        let (on_listing, on_undrawn) = (on(place), on(undrawn));
        gallery.cursor = on_undrawn;
        assert!(!gallery.press(), "a screen not drawn yet has no description to pick");
        gallery.cursor = on_listing;
        assert!(gallery.press(), "its box picks the description");
        let listing = line_of(&gallery, "listing");
        assert_eq!(gallery.picked, Some(listing), "the description itself");
        assert_eq!(overview(&gallery).0[0], "default", "showing its overview");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_description_with_faults_is_a_box_marked_faulty_without_a_picture() {
        let root = root_of("flow-faulty", &[("todo", "flow/todo")]);
        std::fs::create_dir_all(root.join("flow/broken")).unwrap();
        std::fs::write(root.join("flow/broken").join(crate::SCREEN), "{}").unwrap();
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        showing(&mut gallery, 0);
        let flow = flow_shown(&gallery);
        let faults = gallery.lines[line_of(&gallery, "broken")].row.faults.len();
        assert!(matches!(flow.tiles[..], [Tile::Faulty(n), Tile::Picture { .. }] if n == faults));
        let mark = if faults == 1 {
            "faulty: 1 fault, not drawn".to_string()
        } else {
            format!("faulty: {faults} faults, not drawn")
        };
        assert!(flow_texts(flow).contains(&mark));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_flow_shown_is_drawn_again_as_the_files_change() {
        let root = root_of("flow-reload", &[("orders", "app/orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        showing(&mut gallery, 1);
        let undrawn = |gallery: &Gallery| {
            flow_shown(gallery).tiles.iter().filter(|c| matches!(c, Tile::Undrawn)).count()
        };
        assert_eq!(undrawn(&gallery), 2, "OrderDetail and ExportDocuments are not drawn yet");
        let export = root_of("flow-reload-export", &[("export", "export")]);
        std::fs::rename(export.join("export"), root.join("app/export")).unwrap();
        std::fs::remove_dir_all(export).unwrap();
        gallery.reload();
        assert_eq!(gallery.picked, Some(1), "the directory is still picked");
        assert_eq!(undrawn(&gallery), 1, "ExportDocuments is drawn now");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Keys pressed in the window, ⌘ not held: each a letter, or one of
    /// Enter, Shift+Enter (written "⇧Enter"), Backspace and Esc.
    fn press_keys(gallery: &mut Gallery, keys: &[&str]) {
        for key in keys {
            let named = |named| Key::Named(named);
            gallery.shift = *key == "⇧Enter";
            let (pressed, text) = match *key {
                "Enter" | "⇧Enter" => (named(NamedKey::Enter), Some("\r")),
                "Backspace" => (named(NamedKey::Backspace), None),
                "Esc" => (named(NamedKey::Escape), None),
                " " => (named(NamedKey::Space), Some(" ")),
                "→" => (named(NamedKey::ArrowRight), None),
                "←" => (named(NamedKey::ArrowLeft), None),
                letter => (Key::Character(letter.into()), Some(letter)),
            };
            gallery.key(&pressed, text);
            gallery.shift = false;
        }
    }

    /// The text typed, a key for each character.
    fn typed(text: &str) -> Vec<String> {
        text.chars().map(String::from).collect()
    }

    fn type_text(gallery: &mut Gallery, text: &str) {
        let keys = typed(text);
        press_keys(gallery, &keys.iter().map(String::as_str).collect::<Vec<_>>());
    }

    /// The pins drawn, as (id, picture, ink, faint), the picture by its
    /// place among the pictures.
    fn pins_shown(gallery: &Gallery) -> Vec<(String, usize, Ink, bool)> {
        let origins = gallery.origins();
        gallery
            .pins()
            .into_iter()
            .map(|pin| {
                let x = f64::from(pin.rect.x);
                let picture = origins.iter().rposition(|(left, _)| *left <= x + PIN).unwrap_or(0);
                (pin.id, picture, pin.ink, pin.faint)
            })
            .collect()
    }

    /// The middle of the pin of note `id`, in the window.
    fn on_pin(gallery: &Gallery, id: &str) -> (f64, f64) {
        let pin = gallery.pins().into_iter().find(|pin| pin.id == id && !pin.faint).unwrap();
        (f64::from(pin.rect.x) + PIN / 2.0, f64::from(pin.rect.y) + PIN / 2.0)
    }

    /// The sidebar's button that reads `text`, clicked in the middle of
    /// its box.
    fn click_panel_line(gallery: &mut Gallery, text: &str) -> bool {
        let cards = gallery.sidebar_cards();
        let buttons =
            cards.iter().flat_map(|card| &card.blocks).filter(|block| matches!(block, Block::Button(..)));
        let index = buttons
            .clone()
            .position(|block| block.says() == text)
            .unwrap_or_else(|| panic!("no button {text:?}: {:?}", gallery.panel()));
        let laid =
            sidebar::lay_out(&cards, gallery.pane_right(), -gallery.side_scroll, &mut gallery.typesetter);
        let r = laid.buttons[index].0;
        gallery.cursor = (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0));
        gallery.press()
    }

    #[test]
    fn a_note_written_on_a_node_is_kept_in_notes_json_and_pinned_where_it_was_written() {
        let root = root_of("notes-write", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        press_keys(&mut gallery, &["n"]);
        assert_eq!(
            gallery.panel()[..3],
            [
                "write a note",
                "click the node it is on or a picture's caption, or drag over the region",
                "Esc cancels"
            ]
        );
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture") };
        let id = pages[1].nodes.iter().map(|n| &n.id).find(|id| id.ends_with("-cancel")).unwrap().clone();
        gallery.cursor = on_node(&gallery, 1, &id);
        gallery.press();
        assert!(gallery.release(), "the node clicked is the one the note is on");
        assert_eq!(gallery.panel()[0], "note on order-{index}-cancel in default at 1120");
        type_text(&mut gallery, "价格太长");
        press_keys(&mut gallery, &["⇧Enter", "a", "x", "Backspace", " ", "b"]);
        assert_eq!(gallery.panel()[1], "价格太长\na b▏", "Shift+Enter breaks a line");
        assert!(!root.join("orders").join(notes::NOTES).exists(), "nothing is written while typing");
        press_keys(&mut gallery, &["Enter"]);
        assert!(gallery.draft.is_none(), "Enter saves");
        assert!(gallery.panel()[0].starts_with("N1 saved"), "{:?}", gallery.panel());

        let written = notes::read(&root.join("orders")).unwrap();
        assert_eq!(written.len(), 1);
        let note = &written[0];
        assert_eq!((note.id.as_str(), note.status), ("N1", Status::Open));
        assert_eq!(note.thread[0].says, "价格太长\na b");
        let On::Node { state, node } = &note.on else { panic!("not on a node: {note:?}") };
        assert_eq!(*state, State::Fixture("default".into()));
        let reference = gallery.reference().unwrap();
        assert_eq!(note.reference(&root.join("orders")), reference, "pinned as ⌘C references it");
        assert_eq!(node.origin.written, "order-{index}-cancel");
        assert_eq!(node.width, 1120);

        assert_eq!(
            pins_shown(&gallery),
            [("N1".to_string(), 0, Ink::Note, true), ("N1".to_string(), 1, Ink::Note, false)],
            "a pin where it was written, at 1120, and a faint dot on the node at 390"
        );
        let orders = line_of(&gallery, "orders");
        let default = state_of_in(&gallery, orders, "default");
        let loading = state_of_in(&gallery, orders, "loading");
        let count = |gallery: &Gallery, line: usize| {
            let Line { row, state, .. } = &gallery.lines[line];
            gallery.open_notes(row, state.as_ref())
        };
        assert_eq!((count(&gallery, orders), count(&gallery, default), count(&gallery, loading)), (1, 1, 0));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn esc_drops_a_note_and_writing_one_from_the_panel_needs_no_n() {
        let root = root_of("notes-esc", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        gallery.cursor = on_node(&gallery, 0, "nav-export");
        gallery.press();
        assert!(click_panel_line(&mut gallery, "write a note on it  (N)"), "the node's panel starts one");
        assert!(gallery.panel()[0].starts_with("note on nav-export in default at 390"));
        type_text(&mut gallery, "x");
        press_keys(&mut gallery, &["Esc"]);
        assert!(gallery.draft.is_none());
        assert!(!root.join("orders").join(notes::NOTES).exists(), "Esc writes nothing");
        press_keys(&mut gallery, &["n", "Esc"]);
        assert!(!gallery.noting, "Esc stops N too");
        gallery.cursor = on_node(&gallery, 0, "nav-export");
        press_keys(&mut gallery, &["n"]);
        gallery.press();
        gallery.release();
        press_keys(&mut gallery, &["Enter"]);
        assert!(gallery.panel()[0].starts_with("not saved: a note says something"), "{:?}", gallery.panel());
        assert!(gallery.draft.is_some(), "an empty note stays to be written");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_answered_note_turns_green_shows_its_reply_and_is_closed_or_reopened_by_the_person() {
        let root = root_of("notes-answered", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        for (id, says) in [("nav-export", "rename it"), ("order-0-cancel", "too red")] {
            press_keys(&mut gallery, &["n"]);
            gallery.cursor = on_node(&gallery, 0, id);
            gallery.press();
            gallery.release();
            type_text(&mut gallery, says);
            press_keys(&mut gallery, &["Enter"]);
        }
        let dir = root.join("orders");
        notes::answer(&dir, "N1", "renamed it Export").unwrap();
        gallery.reload();
        let inks: Vec<(String, Ink)> =
            gallery.pins().into_iter().filter(|pin| !pin.faint).map(|pin| (pin.id, pin.ink)).collect();
        assert_eq!(inks, [("N1".to_string(), Ink::Answered), ("N2".to_string(), Ink::Note)], "drawn again");

        gallery.cursor = on_pin(&gallery, "N1");
        assert!(gallery.hover(), "the pointer over an answered pin");
        let texts: Vec<String> = gallery
            .reply_shown()
            .into_iter()
            .filter_map(|d| if let Draw::Text { text, .. } = d { Some(text) } else { None })
            .collect();
        assert_eq!(texts, ["N1 answered: renamed it Export"]);
        assert!(!gallery.hover(), "still over it: nothing changed");
        gallery.cursor = on_pin(&gallery, "N2");
        assert!(gallery.hover() && gallery.reply_shown().is_empty(), "an open note has no reply to show");

        gallery.cursor = on_pin(&gallery, "N1");
        assert!(gallery.press(), "clicking a pin picks its note");
        let panel = gallery.panel();
        assert_eq!(panel[0], "N1 · answered · on nav-export in default at 390");
        assert_eq!(panel[1..3], ["you: rename it", "agent: renamed it Export"]);
        assert!(click_panel_line(&mut gallery, "reopen it, saying what is still to do"));
        type_text(&mut gallery, "and the icon");
        press_keys(&mut gallery, &["Enter"]);
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.status, Status::Open, "reopened");
        assert_eq!(note.thread.last().unwrap().says, "and the icon");

        gallery.cursor = on_pin(&gallery, "N2");
        gallery.press();
        assert!(click_panel_line(&mut gallery, "close it"));
        assert_eq!(notes::read(&dir).unwrap()[1].status, Status::Closed);
        let left: Vec<String> = gallery.pins().into_iter().map(|pin| pin.id).collect();
        assert!(left.iter().all(|id| id == "N1"), "a closed note is drawn no more: {left:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_note_whose_node_is_renamed_closes_itself_says_so_once_and_reopens_on_the_whole_state() {
        let root = root_of("notes-gone", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        press_keys(&mut gallery, &["n"]);
        gallery.cursor = on_node(&gallery, 0, "nav-export");
        gallery.press();
        gallery.release();
        type_text(&mut gallery, "say export what");
        press_keys(&mut gallery, &["Enter"]);
        let dir = root.join("orders");
        notes::answer(&dir, "N1", "it says Export").unwrap();

        let file = dir.join(crate::SCREEN);
        let screen =
            std::fs::read_to_string(&file).unwrap().replace("\"id\": \"export\"", "\"id\": \"download\"");
        std::fs::write(&file, screen).unwrap();
        gallery.reload();
        assert_eq!(
            gallery.panel()[0],
            "N1 closed: nav-export is gone (removed or renamed) · its description's overview lists it, to reopen"
        );
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.status, Status::Closed, "an answered note closes itself too");
        assert!(gallery.pins().is_empty(), "and is drawn no more");
        assert_eq!(
            gallery.panel()[1],
            "N1 · closed: nav-export is gone (removed or renamed) · on nav-export in default at 390",
            "the note picked says why"
        );
        (gallery.acted, gallery.note) = (None, None);
        gallery.reload();
        assert!(gallery.panel().is_empty(), "said once: {:?}", gallery.panel());

        let orders = line_of(&gallery, "orders");
        gallery.pick(orders);
        assert_eq!(
            gallery.panel(),
            [
                "closed notes · click one to read or reopen it",
                "N1 · nav-export is gone (removed or renamed) · on nav-export in default at 390",
            ]
        );
        assert!(click_panel_line(
            &mut gallery,
            "N1 · nav-export is gone (removed or renamed) · on nav-export in default at 390"
        ));
        assert_eq!(
            gallery.panel()[0],
            "N1 · closed: nav-export is gone (removed or renamed) · on nav-export in default at 390"
        );
        assert!(click_panel_line(&mut gallery, "reopen it on the whole state, saying what is still to do"));
        type_text(&mut gallery, "the download link then");
        press_keys(&mut gallery, &["Enter"]);
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.status, Status::Open);
        assert_eq!(note.on, On::State { state: State::Fixture("default".into()) }, "on the whole state");
        assert_eq!(note.closed_because, None);
        assert_eq!(note.thread.len(), 3, "its thread kept, the new line added");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The middle of the orange "?" of node `id`, in picture `page`.
    fn on_guess(gallery: &Gallery, page: usize, id: &str) -> (f64, f64) {
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture: it has faults") };
        let node = pages[page].nodes.iter().find(|n| n.id == id).unwrap();
        let r = node.guess_tag().expect("it is guessed");
        let (left, top) = gallery.origins()[page];
        let zoom = gallery.zoom(gallery.pane());
        let middle = (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0));
        (left + middle.0 * zoom, top + middle.1 * zoom)
    }

    #[test]
    fn an_assumption_is_answered_with_a_note_and_a_confirmed_one_closes_once_taken_out() {
        let root = root_of("assume", &[("checkout", "checkout")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let checkout = line_of(&gallery, "checkout");
        let states = gallery.lines[checkout + 1..].iter().take_while(|line| line.state.is_some()).count();
        let shown = (1..=states).find(|offset| {
            showing(&mut gallery, checkout + offset);
            let Picture::Pages(pages) = &gallery.picture else { return false };
            pages[0].has("card-number")
        });
        assert!(shown.is_some(), "a state of checkout shows the card form");
        gallery.cursor = on_guess(&gallery, 0, "card-number");
        gallery.press();
        assert_eq!(gallery.node.as_deref(), Some("card-number"), "a click on its \"?\" picks it");
        let panel = gallery.panel();
        for part in [
            "assumed  Card fields are drawn here",
            "or else  the real build may use the payment provider's hosted fields",
            "✓ right, keep it: the agent takes the guess out, and the note closes itself",
            "✗ wrong, tell the agent what is right: write it, for the agent to change and answer",
        ] {
            assert!(panel.iter().any(|line| line == part), "{part:?} in {panel:?}");
        }

        assert!(click_panel_line(
            &mut gallery,
            "✗ wrong, tell the agent what is right: write it, for the agent to change and answer"
        ));
        press_keys(&mut gallery, &["Enter"]);
        assert!(gallery.panel()[0].starts_with("not saved: ✗ says what is right"), "{:?}", gallery.panel());
        assert!(gallery.draft.is_some(), "the ✗ stays to write");
        type_text(&mut gallery, "use the hosted fields");
        press_keys(&mut gallery, &["Enter"]);
        let dir = root.join("checkout");
        let wrong = notes::read(&dir).unwrap().remove(0);
        assert_eq!(wrong.status, Status::Open);
        assert_eq!(wrong.thread[0].says, "use the hosted fields");
        let answers = wrong.answers.clone().unwrap();
        assert_eq!((answers.answer, answers.guess), (Answer::Wrong, Guess::Author));
        assert!(answers.assumed.starts_with("Card fields are drawn here;"));

        gallery.note = None;
        assert!(click_panel_line(
            &mut gallery,
            "✓ right, keep it: the agent takes the guess out, and the note closes itself"
        ));
        let right = notes::read(&dir).unwrap().remove(1);
        assert_eq!(right.answers.as_ref().map(|answers| answers.answer), Some(Answer::Right));
        assert_eq!(right.thread[0].says, notes::RIGHT, "✓ needs no line");
        assert!(
            gallery
                .panel()
                .iter()
                .any(|line| line == "N1 answers it ✗ wrong · open, for the agent to answer")
        );

        notes::answer(&dir, "N1", "kept them drawn: hosted fields are for the build").unwrap();
        let file = dir.join(crate::SCREEN);
        let screen = std::fs::read_to_string(&file).unwrap().replace(
            ",\n                        \"assumed\": \"Card fields are drawn here; the real build may use the payment provider's hosted fields\"",
            "",
        );
        assert!(!screen.contains("Card fields are drawn here"), "the guess taken out");
        std::fs::write(&file, screen).unwrap();
        gallery.reload();
        assert_eq!(
            gallery.panel()[0],
            "N2 closed: the assumption on card-number, confirmed, is taken out · its description's overview lists it, to reopen"
        );
        let notes = notes::read(&dir).unwrap();
        assert_eq!(notes[1].status, Status::Closed, "the ✓ closes itself");
        assert_eq!(notes[0].status, Status::Answered, "the ✗ waits for the person, as any note does");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_confirmed_screen_not_drawn_yet_closes_once_it_is_drawn() {
        let root = root_of("assume-undrawn", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        gallery.cursor = on_guess(&gallery, 0, "nav-export");
        gallery.press();
        let panel = gallery.panel();
        assert!(
            panel.iter().any(|line| line.starts_with("assumed  This navigates to ExportDocuments")),
            "{panel:?}"
        );
        assert!(
            panel.iter().any(|line| line.starts_with("or else  it could instead be a misspelt name")),
            "{panel:?}"
        );
        assert!(click_panel_line(
            &mut gallery,
            "✓ right, keep it: the agent takes the guess out, and the note closes itself"
        ));
        let dir = root.join("orders");
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.answers.unwrap().guess, Guess::Undrawn(vec!["ExportDocuments".into()]));
        gallery.reload();
        assert_eq!(notes::read(&dir).unwrap()[0].status, Status::Open, "not drawn yet: it stands");

        let drawn = root_of("assume-undrawn-export", &[("export", "export")]);
        std::fs::rename(drawn.join("export"), root.join("export")).unwrap();
        gallery.reload();
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.status, Status::Closed);
        assert_eq!(
            note.closed_because.as_deref(),
            Some("the assumption on nav-export, confirmed, is taken out")
        );
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(drawn).unwrap();
    }

    #[test]
    fn a_note_shows_as_a_faint_dot_in_another_state_that_holds_its_node() {
        let root = root_of("notes-dot", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        press_keys(&mut gallery, &["n"]);
        gallery.cursor = on_node(&gallery, 0, "order-0-cancel");
        gallery.press();
        gallery.release();
        type_text(&mut gallery, "too red");
        press_keys(&mut gallery, &["Enter"]);
        show(&mut gallery, "orders", "cancel-dialog");
        let shown = pins_shown(&gallery);
        assert!(!shown.is_empty() && shown.iter().all(|(_, _, _, faint)| *faint), "{shown:?}");
        let orders = line_of(&gallery, "orders");
        let dialog = state_of_in(&gallery, orders, "cancel-dialog");
        let Line { row, state, .. } = &gallery.lines[dialog];
        assert_eq!(gallery.open_notes(row, state.as_ref()), 0, "a state counts the notes written in it");
        show(&mut gallery, "orders", "loading");
        assert!(pins_shown(&gallery).is_empty(), "loading has no rows");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The middle of the label of arrow `index`, of the overview or flow
    /// shown, in the window.
    fn on_arrow(gallery: &Gallery, index: usize) -> (f64, f64) {
        let (r, _) = gallery.arrow_spots()[index];
        (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0))
    }

    /// Note `text` written with N, then a click at `at`, then Enter.
    fn note_at(gallery: &mut Gallery, at: (f64, f64), text: &str) {
        press_keys(gallery, &["n"]);
        gallery.cursor = at;
        gallery.press();
        type_text(gallery, text);
        press_keys(gallery, &["Enter"]);
    }

    #[test]
    fn a_note_on_a_whole_state_is_written_from_a_caption_and_pinned_there() {
        let root = root_of("notes-state", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        press_keys(&mut gallery, &["n"]);
        let (left, top) = gallery.origins()[1];
        gallery.cursor = (left + 4.0, top - CAPTION / 2.0);
        assert!(gallery.press());
        assert_eq!(
            gallery.panel()[..3],
            ["note on the whole of default", "▏", "Enter saves, Shift+Enter breaks a line, Esc cancels"]
        );
        type_text(&mut gallery, "too busy");
        press_keys(&mut gallery, &["Enter"]);
        let dir = root.join("orders");
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.on, On::State { state: State::Fixture("default".into()) });
        let pins = pins_shown(&gallery);
        assert_eq!(pins, [("N1".to_string(), 0, Ink::Note, false)], "one pin, by the first caption");
        gallery.cursor = on_pin(&gallery, "N1");
        gallery.note = None;
        assert!(gallery.press(), "clicking it picks its note");
        assert_eq!(gallery.panel()[0], "N1 · open, for the agent to answer · on the whole of default");
        let orders = line_of(&gallery, "orders");
        let default = state_of_in(&gallery, orders, "default");
        let Line { row, state, .. } = &gallery.lines[default];
        assert_eq!(gallery.open_notes(row, state.as_ref()), 1, "counted on its state");

        showing(&mut gallery, orders);
        let pins: Vec<String> = gallery.pins().into_iter().map(|pin| pin.id).collect();
        assert_eq!(pins, ["N1"], "pinned on its state in the overview too");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_transition_arrow_is_picked_copied_and_noted_in_the_overview() {
        let root = root_of("notes-transition", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let orders = line_of(&gallery, "orders");
        showing(&mut gallery, orders);
        let dir = root.join("orders");
        let screen: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(crate::SCREEN)).unwrap()).unwrap();
        let declared = screen["transitions"].as_array().unwrap().clone();

        gallery.cursor = on_arrow(&gallery, 0);
        assert!(gallery.press(), "a click on its label picks the arrow");
        assert_eq!(gallery.arrow, Some(0));
        let reference = gallery.reference().unwrap();
        let On::Transition { transition } = &reference.on else { panic!("not a transition: {reference:?}") };
        assert_eq!(serde_json::to_value(transition).unwrap(), declared[0], "as screen.json writes it");
        let line = reference.line();
        assert!(line.starts_with(&format!("{} · transition from ", dir.display())), "{line}");
        assert!(
            gallery.panel().iter().any(|row| row.trim() == "write a note on it  (N)"),
            "{:?}",
            gallery.panel()
        );
        gallery.copy();
        assert!(gallery.panel()[0].ends_with(&line), "⌘C copies it: {:?}", gallery.panel());

        let last = declared.len() - 1;
        let at = on_arrow(&gallery, last);
        note_at(&mut gallery, at, "should it say why?");
        let note = notes::read(&dir).unwrap().remove(0);
        let On::Transition { transition } = &note.on else { panic!("not a transition: {note:?}") };
        assert_eq!(serde_json::to_value(transition).unwrap(), declared[last]);
        assert_eq!(gallery.pins().len(), 1, "a pin beside its arrow");
        gallery.note = None;
        gallery.cursor = on_pin(&gallery, "N1");
        assert!(gallery.press());
        assert!(gallery.panel()[0].starts_with("N1 · open, for the agent to answer · on transition from "));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_navigate_arrow_is_picked_copied_and_noted_in_the_flow() {
        let root = root_of("notes-navigate", &[("orders", "app/orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        showing(&mut gallery, 0);
        let dir = root.join("app/orders");
        let link = flow_shown(&gallery).map.arrows[0].link.clone();

        gallery.cursor = on_arrow(&gallery, 0);
        assert!(gallery.press(), "a click on its name picks the arrow");
        let line = gallery.reference().unwrap().line();
        assert_eq!(
            line,
            format!("{} · navigate from {} on {} to {}", dir.display(), link.from, link.node, link.to)
        );
        gallery.copy();
        assert!(gallery.panel()[0].ends_with(&line), "⌘C copies it: {:?}", gallery.panel());

        let at = on_arrow(&gallery, 0);
        note_at(&mut gallery, at, "is this the right screen?");
        let file: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(notes::NOTES)).unwrap()).unwrap();
        assert_eq!(
            file["notes"][0]["navigate"],
            json!({ "from": link.from, "node": link.node, "to": link.to }),
            "kept by the description it is written in"
        );
        assert_eq!(gallery.pins().len(), 1, "a pin under its arrow");
        gallery.note = None;
        gallery.cursor = on_pin(&gallery, "N1");
        assert!(gallery.press());
        assert_eq!(
            gallery.panel()[0],
            format!(
                "N1 · open, for the agent to answer · on navigate from {} on {} to {}",
                link.from, link.node, link.to
            )
        );
        let Line { row, state, .. } = &gallery.lines[line_of(&gallery, "orders")];
        assert_eq!(gallery.open_notes(row, state.as_ref()), 1, "counted on its description");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// `at`, a point in picture `page`'s points, in the window.
    fn on_page(gallery: &Gallery, page: usize, at: (f64, f64)) -> (f64, f64) {
        let (left, top) = gallery.origins()[page];
        let zoom = gallery.zoom(gallery.pane());
        (left + at.0 * zoom, top + at.1 * zoom)
    }

    /// Whether two points in the window are the same, but for rounding.
    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
    }

    /// The left button pressed at `from` and let go at `to`, points in
    /// picture `page`'s points. Whether letting go changed what is shown.
    fn drag(gallery: &mut Gallery, page: usize, from: (f64, f64), to: (f64, f64)) -> bool {
        gallery.cursor = on_page(gallery, page, from);
        gallery.press();
        gallery.cursor = on_page(gallery, page, to);
        gallery.release()
    }

    #[test]
    fn dragging_in_note_mode_writes_a_region_note_with_its_box_and_the_nodes_it_covered() {
        let root = root_of("notes-region", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        press_keys(&mut gallery, &["n"]);
        assert!(drag(&mut gallery, 0, (10.0, 145.0), (380.0, 175.0)), "a drag picks a region");
        assert!(!gallery.regions().is_empty() && gallery.node.is_none(), "drawn dashed, and no node picked");
        assert!(
            gallery.panel()[0]
                .starts_with("note on region 10, 145 · 370 × 30 covering 1 node in default at 390"),
            "{:?}",
            gallery.panel()
        );
        type_text(&mut gallery, "too much air");
        press_keys(&mut gallery, &["Enter"]);
        let dir = root.join("orders");
        let note = notes::read(&dir).unwrap().remove(0);
        let On::Region { region, .. } = &note.on else { panic!("not on a region: {:?}", note.on) };
        assert_eq!(region.area, Frame { x: 10, y: 145, width: 370, height: 30 });
        assert_eq!(region.width, 390);
        let covered: Vec<&str> = region.nodes.iter().map(|origin| origin.written.as_str()).collect();
        assert_eq!(covered, ["match-count"], "only the node wholly inside");
        assert_eq!(region.nodes[0].at, "/root/children/1/children/2");
        let shown = pins_shown(&gallery);
        assert_eq!(
            shown,
            [("N1".to_string(), 0, Ink::Note, false), ("N1".to_string(), 1, Ink::Note, true)],
            "a pin where it was dragged, and a faint dot over its node at 1120"
        );
        let pin = gallery.pins().into_iter().find(|pin| !pin.faint).unwrap();
        let around = pin.region.expect("its region is outlined where it was written");
        let at = on_page(&gallery, 0, (16.0, 152.0));
        assert!(near((f64::from(around.x), f64::from(around.y)), at), "around the node it covered");
        gallery.cursor = on_pin(&gallery, "N1");
        assert!(gallery.press(), "its pin picks it");
        assert_eq!(
            gallery.panel()[0],
            "N1 · open, for the agent to answer · on region 10, 145 · 370 × 30 covering 1 node in default at 390"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn after_a_relayout_a_region_follows_its_nodes_and_one_over_nothing_stays_put() {
        let root = root_of("notes-region-move", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        for (from, to, says) in
            [((10.0, 145.0), (380.0, 175.0), "too much air"), ((2.0, 575.0), (12.0, 590.0), "too empty")]
        {
            press_keys(&mut gallery, &["n"]);
            drag(&mut gallery, 0, from, to);
            type_text(&mut gallery, says);
            press_keys(&mut gallery, &["Enter"]);
        }
        let dir = root.join("orders");
        let empty = &notes::read(&dir).unwrap()[1];
        let On::Region { region, .. } = &empty.on else { panic!("not on a region") };
        assert!(region.nodes.is_empty(), "over nothing: {:?}", region.nodes);
        let corner = |gallery: &Gallery, id: &str| {
            let pin = gallery.pins().into_iter().find(|pin| pin.id == id && !pin.faint).unwrap();
            let r = pin.region.unwrap();
            (f64::from(r.x), f64::from(r.y))
        };
        let (before, still) = (corner(&gallery, "N1"), corner(&gallery, "N2"));
        assert!(near(still, on_page(&gallery, 0, (2.0, 575.0))), "at its box");

        // The search field taken out: what was under it moves up.
        let file = dir.join(crate::SCREEN);
        let mut screen: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        screen.pointer_mut("/root/children/1/children").unwrap().as_array_mut().unwrap().remove(0);
        std::fs::write(&file, screen.to_string()).unwrap();
        gallery.reload();
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture: {:?}", gallery.panel()) };
        let count = pages[0].nodes.iter().find(|node| node.id == "match-count").unwrap().rect;
        let after = corner(&gallery, "N1");
        assert!(after.1 < before.1, "the region moved up with its node: {before:?} → {after:?}");
        assert!(near(after, on_page(&gallery, 0, (f64::from(count.x), f64::from(count.y)))));
        assert_eq!(corner(&gallery, "N2"), still, "a region over nothing stays at its box");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_region_dragged_is_picked_and_copied_as_its_reference() {
        let root = root_of("region-copy", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        gallery.cursor = on_node(&gallery, 0, "nav-export");
        gallery.press();
        assert!(!gallery.release(), "a click is no drag");
        assert_eq!(gallery.node.as_deref(), Some("nav-export"));
        assert!(drag(&mut gallery, 0, (0.0, 0.0), (390.0, 56.0)));
        assert_eq!(gallery.node, None, "a region is picked in place of the node");
        let Some(Reference { on: On::Region { region, .. }, .. }) = gallery.reference() else {
            panic!("no region referenced")
        };
        let covered: Vec<&str> = region.nodes.iter().map(|origin| origin.written.as_str()).collect();
        assert_eq!(covered, ["nav"], "the outermost: the nodes in it are in it already");
        assert!(gallery.panel().contains(&"covers  nav".to_string()), "{:?}", gallery.panel());
        gallery.copy();
        let line = gallery.reference().unwrap().line();
        assert!(
            line.ends_with("· region 0, 0 · 390 × 56 at width 390 · covering nav at /components/Nav/nodes"),
            "{line}"
        );
        assert!(gallery.panel()[0].ends_with(&line), "⌘C copies it: {:?}", gallery.panel());
        assert!(click_panel_line(&mut gallery, "write a note on it  (N)"), "and a note can be written on it");
        assert!(gallery.panel()[0].starts_with("note on region 0, 0 · 390 × 56 covering 1 node"));
        press_keys(&mut gallery, &["Esc", "Esc"]);
        assert!(gallery.region.is_none() && gallery.regions().is_empty(), "Esc lets it go");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The line of the journey named `name`.
    fn journey_of(gallery: &Gallery, name: &str) -> usize {
        gallery.lines.iter().position(|line| line.journey && line.row.name == name).unwrap()
    }

    /// The step of the journey played, from 1, and the id as written of the
    /// node marked in each picture.
    fn played(gallery: &Gallery) -> (usize, Vec<Option<String>>) {
        let at = gallery.playing.as_ref().expect("a journey is played").at;
        let Picture::Pages(pages) = &gallery.picture else { panic!("no pictures") };
        let marked = gallery
            .marked()
            .into_iter()
            .zip(pages)
            .map(|(node, page)| node.map(|node| page.origins[&node.id].written.clone()))
            .collect();
        (at + 1, marked)
    }

    #[test]
    fn the_shops_journey_is_listed_under_its_directory_and_played_step_by_step() {
        let mut gallery = gallery();
        let journey = journey_of(&gallery, "cancel-an-order");
        let shop = &gallery.lines[journey - 1];
        assert_eq!((shop.row.name.as_str(), shop.row.depth + 1), ("shop", gallery.lines[journey].row.depth));
        assert!(shop.row.description().is_none(), "listed under the directory it is in");
        over_row(&mut gallery, journey);
        assert!(gallery.press(), "the journey is picked");
        assert_eq!(captions(&gallery), ["default · 390", "default · 1120"]);
        let marked = Some("order-{index}".to_string());
        assert_eq!(played(&gallery), (1, vec![marked.clone(), marked]));
        let panel = gallery.panel();
        assert_eq!(panel[0], "journey cancel-an-order · step 1 of 3 · shop/orders · default");
        assert!(panel[1].trim().starts_with("acts on order-{index} (label "), "{panel:?}");
        assert!(panel[1].ends_with("· tap (marked)"), "{panel:?}");
        assert!(!panel.iter().any(|line| line.trim() == "← previous step"), "no step before the first");

        press_keys(&mut gallery, &["→"]);
        assert_eq!(captions(&gallery), ["open · 390", "open · 1120"]);
        let marked = Some("summary-cancel".to_string());
        assert_eq!(played(&gallery), (2, vec![marked.clone(), marked]));
        assert!(gallery.panel()[1].contains("· tap, answered ok (marked)"), "{:?}", gallery.panel());

        assert!(click_panel_line(&mut gallery, "→ next step"));
        assert_eq!(captions(&gallery), ["cancelled · 390", "cancelled · 1120"]);
        assert_eq!(played(&gallery), (3, Vec::new()), "the last step acts on nothing");
        let panel = gallery.panel();
        assert_eq!(panel[1].trim(), "the journey ends here");
        assert!(!panel.iter().any(|line| line.trim() == "→ next step"), "no step after the last");
        assert!(!gallery.key(&Key::Named(NamedKey::ArrowRight), None), "nothing past the last");

        press_keys(&mut gallery, &["←", "←"]);
        assert_eq!(played(&gallery).0, 1, "back to the first step");
        assert!(click_panel_line(&mut gallery, "→ next step"));
        assert_eq!(played(&gallery).0, 2);
        assert!(click_panel_line(&mut gallery, "← previous step"));
        assert_eq!(played(&gallery).0, 1);
        assert_eq!(gallery.picked, Some(journey), "the journey stays picked as it plays");

        let orders = line_of(&gallery, "orders");
        over_row(&mut gallery, orders);
        gallery.press();
        assert!(gallery.playing.is_none(), "picking another line stops playing");
    }

    /// The line of the description in `dir`, a directory of the drafts.
    fn description_in(gallery: &Gallery, dir: &str) -> usize {
        let dir = Path::new(DRAFTS).join(dir);
        gallery
            .lines
            .iter()
            .position(|line| line.state.is_none() && !line.journey && line.row.dir == dir)
            .unwrap()
    }

    /// The labels of the journeys marked on each state of the overview
    /// shown, by the state's name.
    fn passes_shown(gallery: &Gallery) -> Vec<(&str, Vec<&str>)> {
        let Picture::Overview(overview) = &gallery.picture else { panic!("not an overview") };
        let marked = overview.thumbs.iter().filter(|thumb| !thumb.journeys.is_empty());
        marked
            .map(|thumb| (thumb.name.as_str(), thumb.journeys.iter().map(|p| p.label.as_str()).collect()))
            .collect()
    }

    #[test]
    fn an_overview_marks_the_journeys_through_each_state_and_plays_one_from_its_mark() {
        let mut gallery = gallery();
        let (orders, order) =
            (description_in(&gallery, "shop/orders"), description_in(&gallery, "shop/order"));
        showing(&mut gallery, orders);
        assert_eq!(passes_shown(&gallery), [("default", vec!["→ cancel-an-order · step 1"])]);
        showing(&mut gallery, order);
        assert_eq!(
            passes_shown(&gallery),
            [("open", vec!["→ cancel-an-order · step 2"]), ("cancelled", vec!["→ cancel-an-order · step 3"])]
        );
        let other = description_in(&gallery, "orders");
        showing(&mut gallery, other);
        assert!(passes_shown(&gallery).is_empty(), "no journey passes through the other orders");

        showing(&mut gallery, order);
        let passes = gallery.passes();
        let (open, cell) = (
            passes[0].0,
            gallery.cells()[gallery.lines[order + 1..].iter().position(|l| l.row.name == "open").unwrap()],
        );
        assert!(open.x as f64 > cell.0 && (open.y as f64) > cell.1, "over the state's picture");
        gallery.cursor = (f64::from(open.x) + 4.0, f64::from(open.y) + 4.0);
        assert!(gallery.press(), "picking the mark plays the journey");
        assert_eq!(gallery.picked, Some(journey_of(&gallery, "cancel-an-order")));
        assert_eq!(
            played(&gallery),
            (2, vec![Some("summary-cancel".to_string()), Some("summary-cancel".to_string())])
        );
        assert_eq!(captions(&gallery), ["open · 390", "open · 1120"]);
    }

    /// A copy of the shop draft in a directory of its own, named `name`.
    fn shop(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("underdraw-window-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let shop = Path::new(DRAFTS).join("shop");
        for file in [
            "domain.json",
            "tokens.json",
            "orders/screen.json",
            "order/screen.json",
            "journeys/cancel-an-order.json",
            "components/OrderRow.json",
            "components/OrderStatus.json",
        ] {
            std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
            std::fs::copy(shop.join(file), root.join(file)).unwrap();
        }
        root
    }

    #[test]
    fn the_shared_components_are_listed_on_their_own_drawn_in_each_fixture_and_noted() {
        let root = shop("components");
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let components = line_of(&gallery, "components");
        let depth = gallery.lines[components].row.depth;
        let section: Vec<(&str, bool)> = gallery.lines[components + 1..]
            .iter()
            .take_while(|line| line.row.depth > depth)
            .map(|line| (line.row.name.as_str(), line.state.is_some()))
            .collect();
        assert_eq!(
            section,
            [
                ("OrderRow", false),
                ("pending", true),
                ("shipped", true),
                ("OrderStatus", false),
                ("pending", true),
                ("cancelled", true)
            ],
            "a section of their own, each in its fixtures"
        );
        show(&mut gallery, "OrderRow", "shipped");
        assert_eq!(captions(&gallery), ["shipped · 390", "shipped · 1120"]);
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture") };
        assert!(pages[0].has("order-row-status") && !pages[0].has("order-row-cancel"), "shipped: no cancel");
        show(&mut gallery, "OrderStatus", "cancelled");
        assert_eq!(captions(&gallery), ["cancelled · 390", "cancelled · 1120"]);

        show(&mut gallery, "OrderRow", "pending");
        press_keys(&mut gallery, &["n"]);
        gallery.cursor = on_node(&gallery, 0, "order-row-cancel");
        gallery.press();
        gallery.release();
        assert!(
            gallery.panel()[0].starts_with("note on order-row-cancel in pending at 390"),
            "{:?}",
            gallery.panel()
        );
        type_text(&mut gallery, "say which order");
        press_keys(&mut gallery, &["Enter"]);
        let dir = root.join("components/OrderRow");
        let note = notes::read(&dir).unwrap().remove(0);
        let On::Node { node, .. } = &note.on else { panic!("not on a node: {note:?}") };
        assert_eq!(node.origin.written, "order-row-cancel");
        let listed = notes::notes(&root, None, None, false, false).unwrap();
        assert_eq!(listed["notes"][0]["description"], "components/OrderRow", "{listed}");

        let file = root.join("components/OrderRow.json");
        let renamed = std::fs::read_to_string(&file).unwrap().replace(r#""id": "cancel""#, r#""id": "drop""#);
        std::fs::write(&file, renamed).unwrap();
        gallery.reload();
        assert_eq!(
            gallery.panel()[0],
            "N1 closed: order-row-cancel is gone (removed or renamed) · its description's overview lists it, to reopen"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_broken_journey_shows_its_fault_and_is_played_once_fixed() {
        let root = shop("broken-journey");
        let order = root.join("order").join(crate::SCREEN);
        let walks = std::fs::read_to_string(&order).unwrap();
        std::fs::write(&order, walks.replacen(r#""to": "cancelled""#, r#""to": "shipped""#, 1)).unwrap();
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let journey = journey_of(&gallery, "cancel-an-order");
        assert_eq!(gallery.lines[journey].row.depth, 1, "listed under the root, one in from its line");
        over_row(&mut gallery, journey);
        assert!(gallery.press());
        assert!(gallery.playing.is_none(), "not played");
        let Picture::Faults(faults) = &gallery.picture else { panic!("its faults are shown") };
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert_eq!(faults[0].at, "steps.1.on");
        assert!(faults[0].problem.contains("does not reach the next step"), "{faults:?}");
        let (list, _) = gallery.faults(gallery.pane());
        let Draw::Text { text, .. } = &list[0] else { panic!("a heading") };
        assert!(text.contains("has a fault, so it is not played"), "{text}");

        std::fs::write(&order, walks).unwrap();
        gallery.reload();
        assert_eq!(gallery.picked, Some(journey), "the journey stays picked");
        assert_eq!(played(&gallery).0, 1, "fixed, it is played");
        press_keys(&mut gallery, &["→"]);
        gallery.reload();
        assert_eq!(played(&gallery).0, 2, "a change keeps the step shown");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// What the inbox's card says: its title, then each of its blocks;
    /// nothing when it is not shown.
    fn inbox(gallery: &Gallery) -> Vec<String> {
        gallery
            .inbox_card()
            .into_iter()
            .flat_map(|card| {
                let blocks = card.blocks.iter().map(|block| block.says().to_string()).collect::<Vec<_>>();
                std::iter::once(card.title).chain(blocks)
            })
            .collect()
    }

    #[test]
    fn the_inbox_lists_what_waits_on_the_person_and_shows_each_item_picked() {
        let root = root_of("inbox", &[("orders", "orders"), ("checkout", "pay/checkout")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let orders = line_of(&gallery, "orders");
        gallery.pick(orders);
        let guesses = [
            "? nav-export in default: This navigates to ExportDocuments, which no description under the root declares, so it is guessed that ExportDocuments is a screen not drawn yet",
            "? order-{index} in default: This navigates to OrderDetail, which no description under the root declares, so it is guessed that OrderDetail is a screen not drawn yet",
        ];
        assert_eq!(inbox(&gallery), ["waits on you · 2", guesses[0], guesses[1]]);

        assert!(click_panel_line(&mut gallery, guesses[0]));
        assert_eq!(shown(&gallery), "orders · default", "clicking an item shows its state");
        assert_eq!(gallery.node.as_deref(), Some("nav-export"), "with its node picked");
        assert_eq!(gallery.panel()[0], "IconButton", "and its card");
        assert!(inbox(&gallery).is_empty(), "a node picked: the inbox gives way to its card");

        let dir = root.join("orders");
        let on_export = gallery.reference().unwrap();
        notes::add(&on_export, "say Export").unwrap();
        notes::answer(&dir, "N1", "it says Export now").unwrap();
        let whole =
            Reference { description: dir.clone(), on: On::State { state: State::Fixture("default".into()) } };
        notes::add(&whole, "too dense").unwrap();
        gallery.reload();
        gallery.pick(orders);
        let answered = "N1 · on nav-export in default at 390 · agent: it says Export now";
        assert_eq!(
            inbox(&gallery),
            ["waits on you · 3", guesses[0], guesses[1], answered, "▸ waiting on the agent · 1"],
            "two assumptions unanswered and one note answered; the open note folded to its count"
        );
        assert!(click_panel_line(&mut gallery, "▸ waiting on the agent · 1"));
        assert_eq!(inbox(&gallery)[4..], ["▾ waiting on the agent · 1", "N2 · on the whole of default"]);

        assert!(click_panel_line(&mut gallery, answered));
        assert_eq!(shown(&gallery), "orders · default");
        assert_eq!(gallery.node.as_deref(), Some("nav-export"), "the note's node picked");
        assert_eq!(
            gallery.panel()[0],
            "N1 · answered · on nav-export in default at 390",
            "with the note's card"
        );

        let top = gallery.lines.iter().position(|line| line.row.dir == root).unwrap();
        assert_eq!(top, 0, "the root is the tree's first line");
        gallery.pick(top);
        assert!(matches!(gallery.picture, Picture::Flow(_)), "the flow of everything under it");
        let everything = inbox(&gallery);
        let (at_orders, at_checkout) = (
            everything.iter().position(|line| line == "orders").unwrap(),
            everything.iter().position(|line| line == "pay/checkout").unwrap(),
        );
        assert_eq!(
            everything[at_orders + 1..at_orders + 4],
            [guesses[0], guesses[1], answered],
            "{everything:?}"
        );
        assert!(at_orders < at_checkout, "grouped by description, in the tree's order");
        assert!(
            everything[at_checkout + 1..].iter().any(|line| line.starts_with("? card-number in ")),
            "{everything:?}"
        );
        let pay = gallery.lines.iter().position(|line| line.row.dir == root.join("pay")).unwrap();
        gallery.pick(pay);
        assert!(!inbox(&gallery).contains(&"orders".to_string()), "a directory's is what is under it");

        notes::close(&dir, "N1").unwrap();
        let assumed = guesses[0].split_once(": ").unwrap().1.to_string();
        let guess = Guess::Undrawn(vec!["ExportDocuments".into()]);
        notes::judge(&on_export, Answers { assumed, guess, answer: Answer::Right }, "").unwrap();
        gallery.reload();
        gallery.pick(orders);
        assert_eq!(
            inbox(&gallery),
            [
                "waits on you · 1",
                guesses[1],
                "▾ waiting on the agent · 2",
                "N2 · on the whole of default",
                "N3 · on nav-export in default at 390",
            ],
            "closed, and answered ✓, they leave as the files change"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Each surface of the window drawn to a PNG and held to a golden one.
    mod frames;
}
