//! The gallery's window, run in a process of its own: the tree of
//! descriptions and their states on the left; for the state picked, its
//! picture drawn at a phone and a desktop width side by side in the middle,
//! where a node can be picked to read what it is, or acted on to follow what
//! the description says happens, as a prototype; and on the right the
//! sidebar ([`crate::sidebar`]), the cards of what is picked and the buttons
//! of what can be done with it; for a directory picked, the
//! flow between the screens under it; for a journey picked (ADR 0022), its
//! steps played one by one. The person talks to the agent about what they
//! picked (ADR 0023): a node, a whole state, or an arrow of an overview or
//! a flow has one note, a conversation shown in the sidebar under its
//! card, said more in from the box at its foot, and drawn as a numbered pin
//! where it was written; the window writes nothing else. The tree and panels are drawn by hand, in
//! the chrome's look ([`chrome`]). The window is drawn from the files as
//! they now stand: as they change under its root, the tree, the state shown
//! and its notes follow.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use underdraw_format::{Finding, Lang, say};
use underdraw_layout::{Align, Draw, Font, Ink, Measure, Placed, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Rect as Area};
use vello::peniko::Fill;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::act::{self, Act};
use crate::chrome::{
    ARROW_FONT, ARROWS, CAPTION, DOT, GAP, INDENT, MARGIN, MARK, MARK_INSET, MARK_TALL, PIN, ROW, SIDEBAR,
    SIDEBAR_LEAST, SPACE, THUMB, THUMB_TALL, TREE, TREE_LEAST,
};
use crate::draw::{self, Handlers, Origin, Shown as State, Wanted};
use crate::field::{Edit, Field, Motion};
use crate::flow::{self, Map, Screen};
use crate::gallery::{self, Answering, Row};
use crate::inbox::{Guessed, Inbox, Item};
use crate::journeys::{self, Step};
use crate::notes::{self, Answer, Answers, Goes, Guess, Note, Proposal, Stands};
use crate::reference::{self, Navigate, On, Pointed, Reference};
use crate::scrollbar::{Axis, Bar, Part};
use crate::sidebar::Item as Listed;
use crate::sidebar::{self, Block, Card, Foot};
use crate::{chrome, gpu, links, paint, text};

/// The widths each description is drawn at: a phone, and a desktop.
const WIDTHS: [u32; 2] = [390, 1120];
/// The width each state is drawn at in a description's overview.
const OVERVIEW: u32 = 390;
/// How far one notch of a mouse wheel scrolls.
const NOTCH: f64 = 40.0;
/// How small the pictures of a state get, fitting the width left, before
/// they scroll sideways instead.
const SMALLEST: f64 = 0.5;
/// How long a ✓ on an assumption can be undone before it is written.
const UNDO: Duration = Duration::from_secs(5);

/// What the window is told from other threads.
enum Told {
    /// A later run of the command over the same root asks it forward.
    ComeForward,
    /// A file it is drawn from changed, appeared or went.
    Changed,
}

/// What the person does to the window, as its events are told to the
/// gallery: the gallery decides what each means, from these alone, and the
/// window only turns its events into them ([`Gallery::handle`]).
#[derive(Clone, Debug)]
enum Input {
    /// The window is now this wide and tall, in points.
    Resized((f64, f64)),
    /// The pointer moved to here, in window points.
    Moved((f64, f64)),
    /// The left button went down where the pointer is.
    Pressed,
    /// The left button let go.
    Released,
    /// The wheel turned this far across and down, as the system says it:
    /// a turn toward the person is down, negative.
    Wheel { x: f64, y: f64, unit: Unit },
    /// The modifier keys now held.
    Modifiers(ModifiersState),
    /// A key pressed: what it means on the keyboard's layout, the key it
    /// is on the keyboard, the text it types, and whether it repeats
    /// because it is held.
    Key { key: Key, code: PhysicalKey, text: Option<String>, repeat: bool },
    /// What an input method says.
    Ime(Ime),
    /// The person closes the window.
    Close,
}

/// What a turn of the wheel is counted in.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    /// Notches of a mouse wheel, each [`NOTCH`] points.
    Lines,
    /// Points, as a trackpad scrolls.
    Points,
}

/// What the window does once the gallery has handled an [`Input`].
#[derive(Debug, Default, PartialEq)]
struct Effects {
    /// Whether what is shown changed, and is drawn again.
    redraw: bool,
    /// How the pointer looks where it now is, once it moved.
    cursor: Option<CursorIcon>,
    /// Whether an input method may compose: only into a note being written.
    ime: bool,
    /// Where the caret of the note being written is, in window points: an
    /// input method shows what it offers beside it.
    caret: Option<Rect>,
    /// Whether the window closes.
    exit: bool,
}

/// The clipboard ⌘C and ⌘X copy to and ⌘V pastes from. The window lends
/// the system's; tests lend one of their own.
trait Clipboard {
    /// The text on it; none where it holds none, or there is no clipboard.
    fn text(&mut self) -> Option<String>;
    /// `text` put on it; where it could not be, why, or none where there
    /// is no clipboard.
    fn put(&mut self, text: String) -> Result<(), Option<String>>;
}

/// The system's clipboard, opened the first time it is asked for and
/// kept open: on Linux what was copied goes with it.
#[derive(Default)]
struct System(Option<arboard::Clipboard>);

impl System {
    fn open(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.0.is_none() {
            self.0 = arboard::Clipboard::new().ok();
        }
        self.0.as_mut()
    }
}

impl Clipboard for System {
    fn text(&mut self) -> Option<String> {
        self.open()?.get_text().ok()
    }

    fn put(&mut self, text: String) -> Result<(), Option<String>> {
        self.open().ok_or(None)?.set_text(text).map_err(|e| Some(e.to_string()))
    }
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
    if let Err(e) = gallery.speak(lang) {
        return report(&json!({ "error": e }));
    }
    let mut app = App {
        gallery,
        shown: None,
        reported: false,
        proxy: Some(event_loop.create_proxy()),
        answering: None,
        clipboard: System::default(),
    };
    if let Err(e) = event_loop.run_app(&mut app)
        && !app.reported
    {
        report(&json!({ "error": e.to_string() }));
    }
}

/// The window the gallery is drawn in: it turns the window's events into
/// [`Input`]s for the gallery and does what the [`Effects`] say, and holds
/// what the gallery has no need of to decide: the window, the GPU, the
/// clipboard, and what answers from other threads.
struct App {
    gallery: Gallery,
    /// The window, once open.
    shown: Option<Shown>,
    /// Whether the starting command has been told the window opened.
    reported: bool,
    /// What brings the window forward, or says its files changed, from
    /// another thread.
    proxy: Option<EventLoopProxy<Told>>,
    /// Later runs of the command answered, once the window is open.
    answering: Option<Answering>,
    /// The clipboard lent to the gallery.
    clipboard: System,
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
    line: Option<usize>,
    /// The description and state the person last picked, by name, so that
    /// the files changing keeps showing them, or shows them again once
    /// they are back: a description gone unreadable lists no states.
    wanted: Option<Named>,
    picture: Picture,
    /// What the person picked in what is shown: one thing at a time.
    picked: Picked,
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
    /// The tree's width, in points: where the line between it and the
    /// pictures is, and the pictures' pane starts. Dragging the line moves
    /// it, held to [`Gallery::bounds`].
    tree: f64,
    /// The sidebar's width, in points, kept while it is hidden: where the
    /// line between the pictures and it is, from the window's right edge.
    /// Dragging the line moves it, held to [`Gallery::bounds`].
    side: f64,
    /// The line between two panes held, being dragged; none while no line is.
    resizing: Option<Edge>,
    /// The scrollbar's thumb held, being dragged; none while none is.
    grabbed: Option<Grab>,
    /// The scrollbar whose thumb the pointer is over, drawn darker.
    hot: Option<Scrolls>,
    /// Where the pointer is, in points.
    cursor: (f64, f64),
    /// Whether a click acts on the node under it, rather than picks it: ⌘,
    /// or Ctrl, is held.
    acting: bool,
    /// The states acting has come from, the last one last: where `back`
    /// returns to. Picking a state in the tree starts a new path.
    history: Vec<Named>,
    /// What the last act on a node came to, while nothing else is done.
    acted: Option<Acted>,
    /// The window's width and height, in points.
    size: (f64, f64),
    /// Sets the tree's and the captions' text.
    typesetter: text::Typesetter,
    /// Every description's notes, by its directory, as notes.json now
    /// holds them.
    notes: BTreeMap<PathBuf, Vec<Note>>,
    /// The box at the sidebar's foot, where the person writes to the
    /// conversation on what is picked, kept as typed from one pick to the
    /// next.
    talk: Field,
    /// A ✗ on an assumption being written, in place of the box, while it
    /// is.
    draft: Option<Draft>,
    /// A ✓ on an assumption waiting to be written, while it can be undone.
    confirming: Option<Confirming>,
    /// Where the caret of the field written in was last drawn, in window
    /// points: an input method shows what it offers beside it.
    caret: Option<Rect>,
    /// Where the text of the field written in was, in window points,
    /// while the left button, pressed on it, is held: a drag selects.
    selecting: Option<(f64, f64)>,
    /// The answered note whose pin the pointer is over, as `note` names it:
    /// its reply is shown.
    hovered: Option<(PathBuf, String)>,
    /// Whether Shift is held: Enter then breaks a line of what is written.
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

/// What the person picked in what is shown, to read what it is, copy it
/// or talk about it: one thing, or nothing, at a time.
#[derive(Clone, Debug, PartialEq)]
enum Picked {
    /// Nothing: the state, description or directory shown, as a whole.
    Nothing,
    /// A node of the state shown, by its id, kept from state to state while
    /// the state shown has it; and the picture, of those at each of
    /// `WIDTHS`, it was picked in: the instance the person pointed at.
    Node { id: String, pointed: usize },
    /// An arrow of the overview or the flow shown, by its place among their
    /// arrows.
    Arrow(usize),
}

/// A ✗ on the assumption of the node `on` references being written: what
/// it answers, and the field what is right is typed in.
struct Draft {
    on: Reference,
    answers: Answers,
    text: Field,
}

/// A ✓ on an assumption, not yet written: notes.json gets it once
/// [`UNDO`] has passed without the person undoing it, so an agent waiting
/// on the notes never acts on an answer taken back.
struct Confirming {
    on: Reference,
    answers: Answers,
    /// When it is written.
    at: Instant,
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
    /// Sends what the box at the sidebar's foot holds to the conversation
    /// on what is picked.
    Send,
    /// Answers the assumption on the node picked: ✓ is written once it can
    /// no longer be undone; ✗ starts writing what is right.
    Judge(Answer),
    /// Takes back the ✓ waiting to be written.
    Undo,
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
/// a faint dot, in another state or width that holds its node.
struct Pin {
    /// The directory of the description that keeps it.
    dir: PathBuf,
    id: String,
    /// Where it is, in window points.
    rect: Rect,
    ink: Ink,
    faint: bool,
}

/// Every description's notes on the lines of the tree, by its directory,
/// each on what is gone joined its state's note first; one whose
/// notes.json cannot be read has none (`check` says why).
fn notes_of(lines: &[Line]) -> BTreeMap<PathBuf, Vec<Note>> {
    lines
        .iter()
        .filter(|line| line.state.is_none())
        .filter_map(|line| line.row.description())
        .map(|dir| {
            let _ = notes::settle(dir);
            (dir.to_path_buf(), notes::read(dir).unwrap_or_default())
        })
        .collect()
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
struct Named {
    dir: PathBuf,
    state: Option<String>,
}

/// The window open, and the GPU drawing in it.
struct Shown {
    window: Arc<Window>,
    gpu: gpu::Gpu,
    surface: gpu::Window<'static>,
}

/// A line between two panes, dragged to widen or narrow the pane beside
/// the pictures it bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Edge {
    /// Between the tree and the pictures.
    Tree,
    /// Between the pictures and the sidebar.
    Side,
}

/// A pane's scrollbar, by the scroll it moves: the tree's, the pictures'
/// down and across, the sidebar's.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Scrolls {
    Tree,
    Down,
    Across,
    Side,
}

/// A scrollbar's thumb taken hold of, being dragged: whose, how far along
/// its track the pointer was then, and how far its pane was scrolled.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Grab {
    scrolls: Scrolls,
    from: f64,
    scrolled: f64,
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
}

impl Gallery {
    /// Says its own words in `lang`, set in the fonts of its writing: the
    /// same Han character is drawn one way in Japanese, another in Chinese.
    fn speak(&mut self, lang: Lang) -> Result<(), String> {
        self.typesetter = text::Typesetter::new(lang.locale())?;
        self.lang = lang;
        Ok(())
    }

    /// The window's title, naming its root.
    fn title(&self) -> String {
        say!(self.lang, window_title, root = self.root.display())
    }

    /// The tree of `root`, nothing picked and no window open yet.
    fn new(root: &Path, typesetter: text::Typesetter) -> Gallery {
        let lines = lines(root);
        let talk = Field::new(&typesetter, sidebar::field_width(SIDEBAR));
        Gallery {
            root: root.to_path_buf(),
            lang: Lang::default(),
            notes: notes_of(&lines),
            lines,
            line: None,
            wanted: None,
            picture: Picture::Faults(Vec::new()),
            picked: Picked::Nothing,
            scroll: 0.0,
            tree_scroll: 0.0,
            side_scroll: 0.0,
            across: 0.0,
            sidebar: true,
            details: false,
            tree: TREE,
            side: SIDEBAR,
            resizing: None,
            grabbed: None,
            hot: None,
            cursor: (0.0, 0.0),
            acting: false,
            history: Vec::new(),
            acted: None,
            size: (0.0, 0.0),
            typesetter,
            talk,
            draft: None,
            confirming: None,
            caret: None,
            selecting: None,
            hovered: None,
            shift: false,
            playing: None,
            descriptions: RefCell::new(HashMap::new()),
            guessed: RefCell::new(HashMap::new()),
            waiting: false,
        }
    }

    /// What the person did, `input`, done: the one way in for all the
    /// window is told, the clipboard lent for ⌘C, ⌘X and ⌘V. What the
    /// window does after.
    fn handle(&mut self, input: Input, clipboard: &mut dyn Clipboard) -> Effects {
        let mut effects = Effects::default();
        match input {
            Input::Resized(size) => {
                self.size = size;
                effects.redraw = true;
            }
            Input::Moved(at) => {
                self.cursor = at;
                let grip = self.resizing.is_some() || self.edge_at().is_some();
                effects.cursor = Some(if grip { CursorIcon::EwResize } else { CursorIcon::Default });
                let resized = self.resize() || self.drag_thumb();
                effects.redraw = resized || self.hover() || self.drag_field();
            }
            Input::Pressed => effects.redraw = self.press(),
            Input::Released => effects.redraw = self.release(),
            Input::Wheel { x, y, unit } => {
                let (right, down) = match unit {
                    Unit::Lines => (-x * NOTCH, -y * NOTCH),
                    Unit::Points => (-x, -y),
                };
                // A wheel with no sideways turn scrolls across with Shift.
                let (right, down) = if self.shift && right == 0.0 { (down, 0.0) } else { (right, down) };
                self.wheel(right, down);
                effects.redraw = true;
            }
            Input::Modifiers(held) => {
                self.acting = held.super_key() || held.control_key();
                self.shift = held.shift_key();
            }
            Input::Key { key, code, text, repeat } => {
                effects.redraw = self.key_pressed(&key, code, text.as_deref(), repeat, clipboard);
            }
            Input::Ime(ime) => effects.redraw = self.ime(ime),
            Input::Close => {
                self.leave();
                effects.exit = true;
            }
        }
        (effects.ime, effects.caret) = self.composing();
        effects
    }

    /// Whether an input method may compose, only into a field keys type
    /// into ([`Gallery::field_mut`]), and where the caret it shows what it
    /// offers beside is.
    fn composing(&self) -> (bool, Option<Rect>) {
        let typing = self.typing();
        (typing, self.caret.filter(|_| typing))
    }

    /// A key pressed: with ⌘ (or Ctrl) held, ⌘V pastes into the note being
    /// written, ⌘A selects all of it, ⌘X cuts what is selected in it, ⌘C
    /// copies that, or with nothing selected the reference to what is
    /// picked, and ⌘\ hides or shows the sidebar; without, what
    /// [`Gallery::key`] says. Each letter on any keyboard layout, or where
    /// the layout has no such letter, on the key where it is. Whether that
    /// changed what is shown.
    fn key_pressed(
        &mut self,
        key: &Key,
        code: PhysicalKey,
        text: Option<&str>,
        repeat: bool,
        clipboard: &mut dyn Clipboard,
    ) -> bool {
        if !self.acting {
            return self.key(key, text);
        }
        let chord = |letter: &str, on: KeyCode| {
            !repeat
                && match key {
                    Key::Character(c) if c.chars().all(|c| c.is_ascii_alphabetic()) => {
                        c.eq_ignore_ascii_case(letter)
                    }
                    _ => code == PhysicalKey::Code(on),
                }
        };
        if chord("v", KeyCode::KeyV) {
            clipboard.text().is_some_and(|text| self.type_in(&text))
        } else if chord("a", KeyCode::KeyA) {
            self.select_all()
        } else if chord("\\", KeyCode::Backslash) {
            self.toggle_sidebar();
            true
        } else if chord("x", KeyCode::KeyX) {
            self.copy_selected(true, clipboard)
        } else if chord("c", KeyCode::KeyC) {
            if !self.copy_selected(false, clipboard) {
                self.copy(clipboard);
            }
            true
        } else {
            false
        }
    }

    /// The window open: its first description shown.
    fn show_first(&mut self) {
        if let Some(first) = self.lines.iter().position(|line| line.row.description().is_some()) {
            self.pick(first);
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
        if self.cursor.0 < self.tree {
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
        if self.sidebar { self.size.0 - self.side } else { self.size.0 }
    }

    /// How wide the pictures' pane is, between the tree and the sidebar.
    fn pane(&self) -> f64 {
        self.pane_right() - self.tree
    }

    /// The least and the most the width of the pane `edge` bounds can be:
    /// [`TREE_LEAST`] or [`SIDEBAR_LEAST`], and what leaves the pictures'
    /// pane, beside the other pane, room for the phone's picture at
    /// [`SMALLEST`], with the space around it. In a window too narrow for
    /// both, the least.
    fn bounds(&self, edge: Edge) -> (f64, f64) {
        let picture = f64::from(WIDTHS[0]) * SMALLEST + 2.0 * MARGIN;
        let (least, beside) = match edge {
            Edge::Tree => (TREE_LEAST, self.size.0 - self.pane_right()),
            Edge::Side => (SIDEBAR_LEAST, self.tree),
        };
        (least, (self.size.0 - beside - picture).max(least))
    }

    /// Where `edge`'s line is, in window points; none for the sidebar's
    /// while it is hidden.
    fn edge_x(&self, edge: Edge) -> Option<f64> {
        match edge {
            Edge::Tree => Some(self.tree),
            Edge::Side => self.sidebar.then(|| self.pane_right()),
        }
    }

    /// The width of the pane `edge` bounds: the tree's, or the sidebar's.
    fn width_mut(&mut self, edge: Edge) -> &mut f64 {
        match edge {
            Edge::Tree => &mut self.tree,
            Edge::Side => &mut self.side,
        }
    }

    /// The line between two panes the pointer is on, near enough to take
    /// hold of it.
    fn edge_at(&self) -> Option<Edge> {
        [Edge::Tree, Edge::Side]
            .into_iter()
            .find(|&edge| self.edge_x(edge).is_some_and(|x| (self.cursor.0 - x).abs() <= chrome::GRIP))
    }

    /// The pointer moved, a line held: the pane it bounds as wide as
    /// reaches where the pointer is, within its bounds, and the pictures
    /// fitted to what is left; the other pane keeps its width. Whether a
    /// line is held.
    fn resize(&mut self) -> bool {
        let Some(edge) = self.resizing else {
            return false;
        };
        let (least, most) = self.bounds(edge);
        let reach = match edge {
            Edge::Tree => self.cursor.0,
            Edge::Side => self.size.0 - self.cursor.0,
        };
        *self.width_mut(edge) = reach.clamp(least, most);
        self.clamp();
        true
    }

    /// The pointer moved, a scrollbar's thumb held: its pane scrolled as
    /// far as the thumb has moved with the pointer along its track, held
    /// within what there is to scroll. Whether a thumb is held.
    fn drag_thumb(&mut self) -> bool {
        let Some(grab) = self.grabbed else {
            return false;
        };
        if let Some((_, bar)) = self.bars().into_iter().find(|(scrolls, _)| *scrolls == grab.scrolls) {
            *self.scrolled_mut(grab.scrolls) = bar.dragged(grab.scrolled, bar.along(self.cursor) - grab.from);
            self.clamp();
        }
        true
    }

    /// How far the pane `scrolls` scrolls is scrolled.
    fn scrolled_mut(&mut self, scrolls: Scrolls) -> &mut f64 {
        match scrolls {
            Scrolls::Tree => &mut self.tree_scroll,
            Scrolls::Down => &mut self.scroll,
            Scrolls::Across => &mut self.across,
            Scrolls::Side => &mut self.side_scroll,
        }
    }

    /// The sidebar shown, or hidden, its width given to the pictures.
    fn toggle_sidebar(&mut self) {
        self.sidebar = !self.sidebar;
        self.clamp();
    }

    /// A press where the pointer is: on the tree, picks the line under it;
    /// on a button of the sidebar, does what it does; on a pin, picks what
    /// its note is on; on the pictures, picks the
    /// node under it, or none, or acts on it while [`Gallery::acting`]; on
    /// the overview, picks the arrow under it, or plays the journey whose
    /// mark is, from that step, or picks the state; on the flow,
    /// picks the arrow, or the description whose box is, under it. Whether
    /// that changed what is shown.
    fn press(&mut self) -> bool {
        // On a line between two panes, only taking hold of it: no line of
        // the tree, nor node, is picked, nor button pressed.
        if let Some(edge) = self.edge_at() {
            self.resizing = Some(edge);
            return false;
        }
        // On a scrollbar, only scrolling its pane: its thumb taken hold
        // of, or a page toward the press on its track.
        if let Some((scrolls, bar, part)) = self.bar_at(self.cursor) {
            let page = bar.page();
            match part {
                Part::Thumb => {
                    let (from, scrolled) = (bar.along(self.cursor), bar.scrolled);
                    self.grabbed = Some(Grab { scrolls, from, scrolled });
                }
                Part::Before => *self.scrolled_mut(scrolls) -= page,
                Part::After => *self.scrolled_mut(scrolls) += page,
            }
            self.clamp();
            return true;
        }
        if self.cursor.0 >= self.pane_right() {
            if self.press_field() {
                return true;
            }
            return match self.button_at(self.cursor) {
                Some(does) => self.click(does),
                None => false,
            };
        }
        if self.cursor.0 >= self.tree {
            if self.acting && matches!(self.picture, Picture::Pages(_)) {
                return self.act();
            }
            if let Some((dir, id)) = self.pin_at(self.cursor) {
                return self.visit(Visit::Note(dir, id));
            }
            if let Some(arrow) = self.arrow_at(self.cursor) {
                return self.pick_thing(Picked::Arrow(arrow));
            }
            return match &self.picture {
                Picture::Flow(flow) => {
                    let at = self.in_flow(self.cursor);
                    let Some(Screen::Drawn(dir)) = flow.map.place_at(at).map(|place| &place.screen) else {
                        return self.pick_thing(Picked::Nothing);
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
                        return self.pick_thing(Picked::Nothing);
                    };
                    self.history.clear();
                    self.acted = None;
                    self.pick(line);
                    true
                }
                Picture::Pages(_) | Picture::Faults(_) => {
                    let picked = match self.hit(self.cursor) {
                        Some((pointed, _, node)) => Picked::Node { id: node.id.clone(), pointed },
                        None => Picked::Nothing,
                    };
                    self.pick_thing(picked)
                }
            };
        }
        let line = ((self.cursor.1 + self.tree_scroll - MARGIN / 2.0) / ROW).floor();
        if line < 0.0 {
            return false;
        }
        match self.shows(line as usize) {
            Some(index) if Some(index) != self.line => {
                self.history.clear();
                self.acted = None;
                self.pick(index);
                true
            }
            _ => false,
        }
    }

    /// `picked` picked by the person, in place of what was, and what the
    /// last act on a node came to no longer said: the one way what the
    /// person does changes what is picked. Whether that changed what is
    /// shown.
    fn pick_thing(&mut self, picked: Picked) -> bool {
        let changed = picked != self.picked || self.acted.is_some();
        self.picked = picked;
        self.acted = None;
        changed
    }

    /// What is still picked once the line `dir` is on is shown, `described`
    /// when it is a description's: a node while the state shown has it, or
    /// kept through an overview or faults for the next state; never an
    /// arrow, which is of the picture before.
    fn still_picked(&self, described: bool) -> Picked {
        let kept = match &self.picked {
            Picked::Nothing | Picked::Arrow(_) => false,
            Picked::Node { id, .. } => match &self.picture {
                Picture::Pages(pages) => pages.iter().any(|page| page.has(id)),
                Picture::Overview(_) | Picture::Faults(_) => described,
                Picture::Flow(_) => false,
            },
        };
        if kept { self.picked.clone() } else { Picked::Nothing }
    }

    /// The node picked: its id, and the picture it was picked in.
    fn node(&self) -> Option<(&str, usize)> {
        match &self.picked {
            Picked::Node { id, pointed } => Some((id, *pointed)),
            _ => None,
        }
    }

    /// The arrow picked.
    fn arrow(&self) -> Option<usize> {
        match self.picked {
            Picked::Arrow(arrow) => Some(arrow),
            _ => None,
        }
    }

    /// The left button let go: a line between panes, a scrollbar's thumb
    /// or the text of a field no longer held. Whether that changed what is
    /// shown.
    fn release(&mut self) -> bool {
        self.selecting = None;
        self.resizing = None;
        self.grabbed.take().is_some()
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
        let arrow = self.arrow().and_then(|_| self.reference());
        let journey = self.line.and_then(|index| self.lines.get(index)).filter(|line| line.journey);
        let journey = journey.map(|line| (line.row.dir.clone(), self.playing.as_ref().map_or(0, |p| p.at)));
        self.lines = lines(&self.root);
        self.descriptions.borrow_mut().clear();
        self.guessed.borrow_mut().clear();
        self.notes = notes_of(&self.lines);
        self.line = None;
        self.picture = Picture::Faults(Vec::new());
        let playing = journey.and_then(|(file, at)| {
            Some((self.lines.iter().position(|line| line.journey && line.row.dir == file)?, at))
        });
        if let Some((index, at)) = playing {
            self.play(index, at);
            self.scroll = scroll;
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
            if let Some(at) = arrow.and_then(|arrow| self.arrow_references().iter().position(|r| *r == arrow))
            {
                self.picked = Picked::Arrow(at);
            }
        } else if description.is_some() {
            self.wanted = wanted;
        }
        self.clamp();
    }

    /// The line of the state `named` names, or of its directory when it
    /// names no state.
    fn line_named(&self, named: &Named) -> Option<usize> {
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
            return self.pick_thing(Picked::Nothing);
        };
        self.pick_thing(Picked::Node { id, pointed });
        let Some(here) = self.wanted.clone() else {
            return true;
        };
        let state = here.state.clone().unwrap_or_default();
        let on = format!("{}.{}", handlers.written, act::event(&handlers.events));
        let lang = self.lang;
        let screen = crate::read(&here.dir).map(|(screen, _)| screen).unwrap_or(Value::Null);
        match act::act(&screen, &state, &handlers.written, &handlers.events) {
            Act::Go { to } => {
                let says = say!(lang, acted_to_state, on, state, to);
                self.go(Named { dir: here.dir, state: Some(to) }, says);
            }
            Act::Choose(choices) => {
                let calls = calls.join(", ");
                let says = say!(lang, acted_calls, on, state, calls);
                let choices = choices
                    .into_iter()
                    .map(|(outcome, to)| (format!("{outcome} → {to}"), Do::Choose { outcome, to }))
                    .collect();
                self.acted = Some(Acted { says, choices });
            }
            Act::Navigate(screen) => match links::declared(&self.root).remove(&screen) {
                Some(description) => {
                    let says = say!(lang, acted_navigates, on, screen);
                    self.go(Named { dir: description, state: None }, says);
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
                            self.say(say!(lang, acted_goes_back, on, to));
                        }
                        None => self.say(say!(lang, acted_goes_back_gone, on, to)),
                    }
                }
                None => self.say(say!(lang, acted_goes_back_nowhere, on)),
            },
            Act::Nothing => {
                let says = say!(lang, no_transition, state, on);
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
                let said = say!(self.lang, propose_navigate, screen);
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
            Do::Send => {
                self.send();
                true
            }
            Do::Judge(answer) => {
                self.judge(answer, Instant::now());
                true
            }
            Do::Undo => {
                self.confirming = None;
                self.say(say!(self.lang, undone));
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
        let says = say!(self.lang, acted_outcome, outcome, state, to);
        self.go(Named { dir: here.dir, state: Some(to.clone()) }, says);
        true
    }

    /// A note written on the node acted on, at the instance pointed at,
    /// holding `proposal`, for the agent to declare; the panel says so, or
    /// why it was not. The window writes the note only, never the
    /// description.
    fn propose(&mut self, proposal: Proposal) {
        let Some(on) = self.reference().filter(|on| matches!(on.on, On::Node { .. })) else {
            return self.say(say!(self.lang, proposal_needs_node));
        };
        match notes::propose(&on, proposal) {
            Ok(id) => {
                self.notes = notes_of(&self.lines);
                self.say(say!(self.lang, proposal_saved, id));
            }
            Err(e) => self.say(say!(self.lang, not_saved, e = format!("{e:#}"))),
        }
    }

    /// The state `to` names shown, or its description's first state when
    /// it names none, as an act on a node led to it: where
    /// the person was is kept for `back`, and the panel says `says`.
    fn go(&mut self, to: Named, says: String) {
        let Some(mut index) = self.line_named(&to) else {
            return self.say(say!(self.lang, gone_from_tree, says));
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
    fn named(&self, picked: &Named) -> String {
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
        self.line = Some(index);
        let name = state.as_ref().map(|state| state.name().to_string());
        self.wanted = Some(Named { dir: dir.clone(), state: name });
        (self.scroll, self.across) = (0.0, 0.0);
        self.picture = if !described {
            Picture::Flow(self.flow(&dir))
        } else if !faults.is_empty() {
            Picture::Faults(faults)
        } else if state.is_none() && self.lines.get(index + 1).is_some_and(|next| next.state.is_some()) {
            // The screens drawn under the root as they now stand: a
            // navigate to any other is a screen not drawn yet.
            match self.overview(index, &dir, &links::drawn(&self.root)) {
                Ok(overview) => Picture::Overview(overview),
                Err(e) => Picture::Faults(vec![Finding {
                    at: crate::SCREEN.to_string(),
                    problem: format!("{e:#}"),
                }]),
            }
        } else {
            self.lay_out_pages(&dir, state.as_ref(), &links::drawn(&self.root))
        };
        self.picked = self.still_picked(described);
    }

    /// The state `state` of the description in `dir` drawn at each of
    /// `WIDTHS`, given the screens `drawn` under the root; or why it
    /// cannot be.
    fn lay_out_pages(
        &self,
        dir: &Path,
        state: Option<&State>,
        drawn: &std::collections::BTreeSet<String>,
    ) -> Picture {
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
        match laid.into_iter().collect::<anyhow::Result<Vec<Page>>>() {
            Ok(pages) => Picture::Pages(pages),
            Err(e) => {
                Picture::Faults(vec![Finding { at: crate::SCREEN.to_string(), problem: format!("{e:#}") }])
            }
        }
    }

    /// The journey on line `index` played from its step `at` (its last, past
    /// the end): that step's state drawn at each of `WIDTHS`, the node it
    /// acts on marked. One `check` finds broken shows its faults instead,
    /// and is not played.
    fn play(&mut self, index: usize, at: usize) {
        let line = &self.lines[index];
        let (file, faults) = (line.row.dir.clone(), line.row.faults.clone());
        self.line = Some(index);
        self.playing = None;
        self.picked = Picked::Nothing;
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
            let problem = say!(self.lang, no_such_state, within, state);
            self.picture = Picture::Faults(vec![Finding { at: format!("steps.{at}"), problem }]);
            return;
        };
        // Acting on a node from here goes on as the prototype does, from
        // the step's state.
        self.wanted = Some(Named { dir: step.dir.clone(), state: Some(step.state.clone()) });
        let dir = step.dir.clone();
        self.picture = self.lay_out_pages(&dir, Some(&state), &links::drawn(&self.root));
        self.playing = Some(Playing { steps, at, state });
    }

    /// The step `at` of the journey played shown. Whether one is played.
    fn step(&mut self, at: usize) -> bool {
        let (Some(index), Some(_)) = (self.line, &self.playing) else {
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
        let (Some(playing), Some(index)) = (&self.playing, self.line) else {
            return None;
        };
        let lang = self.lang;
        let (at, step) = (playing.at, &playing.steps[playing.at]);
        let (name, number, steps) = (&self.lines[index].row.name, at + 1, playing.steps.len());
        let (within, state) = (gallery::within(&self.root, &step.dir), &step.state);
        let title = say!(lang, journey_title, name, number, steps, within, state);
        let mut blocks = vec![Block::Text(match &step.on {
            Some((node, event)) => {
                let label = self.marked().into_iter().flatten().next().and_then(|node| node.label);
                let label = label.map(|label| say!(lang, journey_label, label)).unwrap_or_default();
                let outcome =
                    step.outcome.as_ref().map(|o| say!(lang, journey_answered, o)).unwrap_or_default();
                say!(lang, journey_acts_on, node, label, event, outcome)
            }
            None => say!(lang, journey_ends),
        })];
        if at > 0 {
            blocks.push(Block::Button(say!(lang, previous_step), Do::Step(at - 1)));
        }
        if at + 1 < playing.steps.len() {
            blocks.push(Block::Button(say!(lang, next_step), Do::Step(at + 1)));
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
                let label = say!(lang, journey_mark, name, number);
                let wide = f64::from(self.typesetter.text(&label, MARK, None).width) + 10.0;
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
                let x = self.tree + MARGIN + column * (THUMB + SPACE);
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
                let top = y + MARK_INSET + index as f64 * (MARK_TALL + MARK_INSET / 2.0);
                let wide = pass.wide.min(w - 2.0 * MARK_INSET);
                list.push((rect(x + MARK_INSET, top, wide, MARK_TALL), pass));
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
            // The arrow picked stands out; the others are drawn quieter
            // than their labels, which are what a person reads.
            let (ink, edge, said) = if self.arrow() == Some(index) {
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
            let label = Draw::Text {
                rect: rect(f64::from(at.x), f64::from(at.y) + 1.0, f64::from(at.width), 16.0),
                text: arrow.label.clone(),
                font: ARROW_FONT,
                ink: said,
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
        (self.tree + MARGIN, MARGIN - self.scroll)
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
        let mut x = self.tree + MARGIN - self.across;
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

    /// What ⌘C copies, and what the conversation in the sidebar is on: the
    /// node picked, as the description writes it, and the instance of it
    /// pointed at (in the picture it was picked in, or the first that has
    /// it); or, with none picked, the state shown; or the arrow picked in
    /// an overview or a flow. None where nothing is.
    fn reference(&self) -> Option<Reference> {
        if let Picture::Overview(_) | Picture::Flow(_) = self.picture {
            return self.arrow_references().into_iter().nth(self.arrow()?);
        }
        let (Picture::Pages(pages), Some(wanted)) = (&self.picture, &self.wanted) else {
            return None;
        };
        let state = self.state_shown()?;
        let node = self.node().and_then(|(id, pointed)| {
            let has = |page: &&Page| page.has(id);
            let page = pages.get(pointed).filter(has).or_else(|| pages.iter().find(has))?;
            let placed = page.nodes.iter().find(|node| node.id == id)?;
            let origin = page.origins.get(id)?.clone();
            Some(Pointed { origin, label: placed.label, width: page.width as u32 })
        });
        let on = match node {
            Some(node) => On::Node { state, node },
            None => On::State { state },
        };
        Some(Reference { description: wanted.dir.clone(), on })
    }

    /// The reference to what is picked put on the clipboard, and the panel
    /// says so; or says why nothing was.
    fn copy(&mut self, clipboard: &mut dyn Clipboard) {
        let Some(reference) = self.reference() else {
            return self.say(say!(self.lang, nothing_to_copy));
        };
        let (line, lang) = (reference.line(), self.lang);
        let copied = clipboard.put(line.clone()).map_err(|e| e.unwrap_or_else(|| say!(lang, no_clipboard)));
        match copied {
            Ok(()) => self.say(say!(lang, copied, line)),
            Err(e) => self.say(say!(lang, could_not_copy, e, line)),
        }
    }

    /// The state whose pictures are shown: the one picked, or the step's of
    /// the journey played.
    fn state_shown(&self) -> Option<State> {
        if let Some(playing) = &self.playing {
            return Some(playing.state.clone());
        }
        self.line.and_then(|index| self.lines.get(index)?.state.clone())
    }

    /// The answered note whose pin the pointer is now over, kept to show
    /// its reply, and the scrollbar whose thumb it is over, drawn darker.
    /// Whether either changed.
    fn hover(&mut self) -> bool {
        let hot =
            self.bar_at(self.cursor).and_then(|(scrolls, _, part)| (part == Part::Thumb).then_some(scrolls));
        let warmed = hot != self.hot;
        self.hot = hot;
        let over = self.pin_at(self.cursor).filter(|(dir, id)| {
            self.notes.get(dir).is_some_and(|notes| notes.iter().any(|note| &note.id == id && !note.waits()))
        });
        let changed = over != self.hovered;
        self.hovered = over;
        changed || warmed
    }

    /// What the button of the sidebar under `at`, a point in the window,
    /// does; none off its buttons, or while it is hidden.
    fn button_at(&mut self, at: (f64, f64)) -> Option<Do> {
        if !self.sidebar {
            return None;
        }
        let (cards, foot) = (self.sidebar_cards(), self.foot());
        let (right, bottom) = (self.pane_right(), self.size.1);
        let top = -self.side_scroll;
        let laid =
            sidebar::lay_out(&cards, foot.as_ref(), (right, self.side), (top, bottom), &mut self.typesetter);
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
        let (Some((id, _)), Picture::Pages(pages)) = (self.node(), &self.picture) else {
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

    /// What the sidebar's cards say, one string each: a card's title, then
    /// each of its blocks.
    #[cfg(test)]
    fn panel(&self) -> Vec<String> {
        self.cards()
            .into_iter()
            .flat_map(|card| {
                let blocks = card.blocks.iter().flat_map(Block::says).collect::<Vec<_>>();
                std::iter::once(card.title).chain(blocks)
            })
            .collect()
    }

    /// The sidebar's cards, top to bottom, with a button for each thing a
    /// click does: the journey played, its step and the steps beside it;
    /// what the last act on a node came to, and any outcomes to choose
    /// from; a ✓ waiting to be written, to undo; the assumption on the node
    /// picked, to answer; then what is picked: the node (what it is, what
    /// it calls, and, folded into "details", its id and its label and box
    /// at each width), the arrow, or, with neither, the state shown; then
    /// its note, as a conversation, newest at the bottom. None when none of
    /// these is there.
    fn cards(&self) -> Vec<Card<Do>> {
        let mut cards: Vec<Card<Do>> = self.playing_card().into_iter().collect();
        if let Some(Acted { says, choices }) = &self.acted {
            let buttons =
                choices.iter().map(|(said, does)| Block::Button(said.clone(), does.clone())).collect();
            cards.push(Card::new(says.clone(), buttons));
        }
        cards.extend(self.confirming_card(Instant::now()));
        cards.extend(self.assumption_card());
        cards.extend(self.node_card().or_else(|| self.arrow_card()).or_else(|| self.state_card()));
        cards.extend(self.conversation_card());
        cards
    }

    /// What the sidebar shows: its cards, then the inbox; and, while nothing
    /// is picked to talk about, what a person can do in the window.
    fn sidebar_cards(&self) -> Vec<Card<Do>> {
        let mut cards = self.cards();
        let picked = !cards.is_empty();
        cards.extend(self.inbox_card());
        if picked {
            return cards;
        }
        let lang = self.lang;
        let blocks = vec![
            Block::Muted(say!(lang, hint_click)),
            Block::Muted(say!(lang, hint_act)),
            Block::Muted(say!(lang, hint_keys)),
        ];
        cards.push(Card { ink: Ink::Muted, ..Card::new(say!(lang, nothing_picked), blocks) });
        cards
    }

    /// The descriptions whose inbox the sidebar shows: the one picked, or
    /// whose state is, or each under the directory picked, in the order the
    /// tree lists them. None while a node or an arrow is picked, a ✗ is
    /// written, or a journey played.
    fn inboxed(&self) -> Vec<PathBuf> {
        // The node picked is kept through an overview, for the next state:
        // it is picked only while a state is drawn.
        let drawn = matches!(self.picture, Picture::Pages(_));
        let busy = drawn && self.node().is_some() || self.arrow().is_some();
        if busy || self.draft.is_some() || self.playing.is_some() {
            return Vec::new();
        }
        let Some(line) = self.line.and_then(|index| self.lines.get(index)).filter(|line| !line.journey)
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
            .line
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
                blocks.push(Block::Item(match item {
                    Item::Guess(guess) => {
                        let (written, state, (guessed, _)) =
                            (&guess.written, guess.state.name(), parts(&guess.assumed));
                        Listed {
                            mark: "?".to_string(),
                            mark_ink: Ink::Assumed,
                            on: say!(lang, guess_on, written, state),
                            says: guessed.to_string(),
                            ink: Ink::Muted,
                            does: Do::Visit(Visit::Guess(
                                (*dir).clone(),
                                guess.state.clone(),
                                guess.id.clone(),
                            )),
                        }
                    }
                    Item::Answered(note) => {
                        let reply = note.reply().unwrap_or_default();
                        Listed {
                            mark: note.id.clone(),
                            mark_ink: Ink::Note,
                            on: note.on.said(lang),
                            says: say!(lang, agent_reply, reply),
                            ink: Ink::Answered,
                            does: Do::Visit(Visit::Note((*dir).clone(), note.id.clone())),
                        }
                    }
                }));
            }
        }
        let yours: usize = inboxes.iter().map(|(_, inbox)| inbox.yours.len()).sum();
        if yours == 0 {
            blocks.push(Block::Muted(say!(lang, nothing_waits)));
        }
        let agents: usize = inboxes.iter().map(|(_, inbox)| inbox.agents.len()).sum();
        if agents > 0 {
            let fold = if self.waiting { "▾" } else { "▸" };
            blocks.push(Block::Fold(say!(lang, waiting_on_agent, fold, agents), Do::Waiting));
        }
        if self.waiting {
            for (dir, inbox) in &inboxes {
                for note in &inbox.agents {
                    let on = note.on.said(lang);
                    let on =
                        if grouped { format!("{} · {on}", gallery::within(&self.root, dir)) } else { on };
                    blocks.push(Block::Item(Listed {
                        mark: note.id.clone(),
                        mark_ink: Ink::Note,
                        on,
                        says: String::new(),
                        ink: Ink::Muted,
                        does: Do::Visit(Visit::Note((*dir).clone(), note.id.clone())),
                    }));
                }
            }
        }
        Some(Card { ink: Ink::Note, ..Card::new(say!(lang, waits_on_you, yours), blocks) })
    }

    /// What an item of the inbox, or a pin, is on shown: an assumption's
    /// state, its node picked; a note's state with its node picked, its
    /// description's overview with its transition picked, or the flow its
    /// navigate is drawn in with that picked; and so its conversation in
    /// the sidebar. The state shown already is not drawn again. Whether
    /// there was anything to show.
    fn visit(&mut self, visit: Visit) -> bool {
        self.history.clear();
        self.acted = None;
        let (dir, on) = match visit {
            Visit::Guess(dir, state, id) => {
                let Some(index) = self.line_named(&Named { dir, state: Some(state.name().to_string()) })
                else {
                    return false;
                };
                self.pick(index);
                self.picked = Picked::Nothing;
                self.pick_node(|page| page.has(&id).then(|| id.clone()), None);
                return true;
            }
            Visit::Note(dir, id) => {
                let Some(note) =
                    self.notes.get(&dir).and_then(|notes| notes.iter().find(|note| note.id == id))
                else {
                    return false;
                };
                (dir, note.on.clone())
            }
        };
        match &on {
            On::Node { state, .. } | On::State { state } => {
                let Some(index) =
                    self.line_named(&Named { dir: dir.clone(), state: Some(state.name().to_string()) })
                else {
                    return false;
                };
                if self.line != Some(index) {
                    self.pick(index);
                }
                self.picked = Picked::Nothing;
                if let On::Node { node, .. } = &on {
                    let origin = node.origin.clone();
                    self.pick_node(
                        |page| page.placed(&origin).map(|placed| placed.id.clone()),
                        Some(node.width),
                    );
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
                    if let Some(at) = self.arrow_references().iter().position(|arrow| *arrow == wanted) {
                        self.picked = Picked::Arrow(at);
                        break;
                    }
                }
            }
        }
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
            self.picked = Picked::Node { id, pointed };
        }
    }

    /// How tall the sidebar's cards are, unscrolled; nothing while it is
    /// hidden.
    fn sidebar_height(&mut self) -> f64 {
        if !self.sidebar {
            return 0.0;
        }
        let cards = self.sidebar_cards();
        sidebar::lay_out(&cards, None, (0.0, self.side), (0.0, self.size.1), &mut self.typesetter).height
    }

    /// Where the box at the sidebar's foot starts, from the window's top:
    /// the foot of what the sidebar's cards scroll in; the window's
    /// bottom while there is no box, or no sidebar.
    fn sidebar_foot(&mut self) -> f64 {
        let foot = self.foot();
        if !self.sidebar || foot.is_none() {
            return self.size.1;
        }
        let (right, bottom) = (self.pane_right(), self.size.1);
        sidebar::lay_out(&[], foot.as_ref(), (right, self.side), (0.0, bottom), &mut self.typesetter).foot
    }

    /// The card of the arrow picked: what it is.
    fn arrow_card(&self) -> Option<Card<Do>> {
        self.arrow()?;
        let said = self.reference()?.on.said(self.lang);
        Some(Card::new(said.strip_prefix("on ").unwrap_or(&said).to_string(), Vec::new()))
    }

    /// The card of the assumption on the node picked, for the person to
    /// answer: what was guessed, in large type, what else it could be, the
    /// notes that already answer it, then ✓ and ✗; or, once ✗ is clicked,
    /// the field for what is right, in their place. None when the node
    /// picked has none.
    fn assumption_card(&self) -> Option<Card<Do>> {
        let (on, assumed) = self.assumption()?;
        let lang = self.lang;
        let (guessed, other) = parts(&assumed);
        let mut blocks = vec![
            Block::Big(guessed.to_string()),
            Block::Fact(
                say!(lang, or_else),
                match other {
                    Some(other) => other.to_string(),
                    None => say!(lang, or_else_not_said),
                },
            ),
        ];
        if let (On::Node { node, .. }, Some(notes)) = (&on.on, self.notes.get(&on.description)) {
            for note in notes {
                let (Some(answers), On::Node { node: on, .. }) = (&note.answers, &note.on) else { continue };
                if on.origin.written == node.origin.written {
                    let (id, mark, status) =
                        (&note.id, answered(answers.answer, lang), note_status(note, lang));
                    blocks.push(Block::Muted(say!(lang, answers_it, id, mark, status)));
                }
            }
        }
        if self.draft.is_some() {
            blocks.extend(self.field_blocks());
        } else if self.confirming.as_ref().is_some_and(|confirming| confirming.on == on) {
            // Its ✓ waits, to be undone, in the card above.
        } else {
            blocks.push(Block::Row(vec![
                (say!(lang, keep_it), Do::Judge(Answer::Right)),
                (say!(lang, wrong_it), Do::Judge(Answer::Wrong)),
            ]));
            blocks.push(Block::Muted(say!(lang, judge_hint)));
        }
        let title = say!(lang, assumed_title);
        Some(Card { ink: Ink::Assumed, ..Card::new(title, blocks) })
    }

    /// The node picked, as a note on it references it, and what was assumed
    /// there; none when nothing was.
    fn assumption(&self) -> Option<(Reference, String)> {
        let (Some((id, _)), Picture::Pages(pages)) = (self.node(), &self.picture) else {
            return None;
        };
        let assumed =
            pages.iter().find_map(|page| page.nodes.iter().find(|n| &n.id == id)?.assumed.clone())?;
        let on = self.reference().filter(|on| matches!(on.on, On::Node { .. }))?;
        Some((on, assumed))
    }

    /// The assumption on the node picked answered `answer`: ✓ waits
    /// [`UNDO`] to be written, from `now`, while it can be undone; ✗ opens
    /// a field, in place, for what is right. The window writes the note
    /// only, never the description.
    fn judge(&mut self, answer: Answer, now: Instant) {
        let Some((on, assumed)) = self.assumption() else {
            return self.say(say!(self.lang, nothing_assumed));
        };
        let (On::Node { node, .. }, Picture::Pages(pages)) = (&on.on, &self.picture) else {
            return;
        };
        let screen = crate::read(&on.description).map(|(screen, _)| screen).unwrap_or(Value::Null);
        let id = self.node().map(|(id, _)| id.to_string()).unwrap_or_default();
        let component =
            pages.iter().find_map(|page| page.nodes.iter().find(|n| n.id == id)).map(|n| n.component.clone());
        let guess = Guess::of(
            &screen,
            &node.origin.written,
            &component.unwrap_or_default(),
            &links::drawn(&self.root),
        );
        let answers = Answers { assumed, guess, answer };
        self.acted = None;
        match answer {
            Answer::Right => {
                // One ✓ waits at a time: the one before it is written now.
                if let Some(at) = self.confirming.as_ref().map(|before| before.at) {
                    self.confirm(at);
                }
                self.confirming = Some(Confirming { on, answers, at: now + UNDO });
            }
            Answer::Wrong => {
                let text = Field::new(&self.typesetter, sidebar::field_width(self.side));
                self.draft = Some(Draft { on, answers, text });
            }
        }
    }

    /// The ✓ waiting to be written written, once it is time at `now`:
    /// notes.json gets it, and the sidebar says so. Whether it was.
    fn confirm(&mut self, now: Instant) -> bool {
        if self.confirming.as_ref().is_none_or(|confirming| now < confirming.at) {
            return false;
        }
        let Some(Confirming { on, answers, .. }) = self.confirming.take() else {
            return false;
        };
        match notes::judge(&on, answers, "") {
            Ok(id) => {
                self.notes = notes_of(&self.lines);
                self.say(say!(self.lang, judged_right_saved, id));
            }
            Err(e) => self.say(say!(self.lang, not_saved, e = format!("{e:#}"))),
        }
        true
    }

    /// The window closing: a ✓ still to be written is the person's answer
    /// all the same, and is written now.
    fn leave(&mut self) {
        if let Some(at) = self.confirming.as_ref().map(|confirming| confirming.at) {
            self.confirm(at);
        }
    }

    /// The card of the ✓ waiting to be written, at `now`: on what, how long
    /// until it is, and a button that takes it back.
    fn confirming_card(&self, now: Instant) -> Option<Card<Do>> {
        let confirming = self.confirming.as_ref()?;
        let lang = self.lang;
        let left = confirming.at.saturating_duration_since(now).as_secs_f64().ceil();
        let on = confirming.on.on.said(lang);
        let blocks =
            vec![Block::Muted(say!(lang, confirming_in, left)), Block::Button(say!(lang, undo), Do::Undo)];
        Some(Card { ink: Ink::Answered, ..Card::new(say!(lang, confirming_title, on), blocks) })
    }

    /// The notes on what the conversation in the sidebar is on
    /// ([`Gallery::reference`]), in the order written: its note, and the
    /// notes answering an assumption on it or proposing where acting on it
    /// should go, each a part of the one conversation. Their description's
    /// directory with them; none where nothing is picked to talk about.
    fn conversation(&self) -> Option<(Reference, Vec<&Note>)> {
        let on = self.reference()?;
        let notes = self.notes.get(&on.description).map(Vec::as_slice).unwrap_or_default();
        let held = notes.iter().filter(|note| note.on.same(&on.on)).collect();
        Some((on, held))
    }

    /// The card of the conversation on what is picked: its note's id and
    /// where it stands, then what the person and the agent said, each in a
    /// bubble, in the order said, newest at the bottom; or, where nothing
    /// is said yet, how to say it in the box below. None where nothing is
    /// picked to talk about.
    fn conversation_card(&self) -> Option<Card<Do>> {
        let (_, notes) = self.conversation()?;
        let lang = self.lang;
        let Some(last) = notes.last() else {
            let blocks = vec![Block::Muted(say!(lang, talk_hint))];
            return Some(Card { ink: Ink::Note, ..Card::new(say!(lang, talk_none), blocks) });
        };
        let ids = notes.iter().map(|note| note.id.as_str()).collect::<Vec<_>>().join(", ");
        let title = say!(lang, talk_title, id = ids, status = note_status(last, lang));
        let mut blocks = Vec::new();
        for note in &notes {
            blocks.extend(note.messages.iter().map(|said| Block::Bubble(said.by, said.says.clone())));
        }
        let ink = if last.waits() { Ink::Note } else { Ink::Answered };
        Some(Card { ink, ..Card::new(title, blocks) })
    }

    /// The box at the sidebar's foot, while there is something picked to
    /// talk about and no ✗ is written in its place: what it holds, and
    /// Send, which sends it unless it is empty.
    fn foot(&self) -> Option<Foot<Do>> {
        if self.draft.is_some() {
            return None;
        }
        self.reference()?;
        let lang = self.lang;
        Some(Foot {
            field: self.talk.shown(),
            empty: say!(lang, talk_box),
            send: say!(lang, send),
            does: (!self.talk.is_blank()).then_some(Do::Send),
        })
    }

    /// The field of the ✗ being written, and what its keys do: Enter
    /// sends what it holds, unless it is empty.
    fn field_blocks(&self) -> Vec<Block<Do>> {
        let Some(draft) = &self.draft else {
            return Vec::new();
        };
        let lang = self.lang;
        let keys = if draft.text.is_blank() { say!(lang, not_sent_empty) } else { say!(lang, enter_sends) };
        vec![Block::Field(draft.text.shown(), say!(lang, ask_right)), Block::Muted(keys)]
    }

    /// The field keys and an input method type into: the ✗ being written,
    /// or else the box at the sidebar's foot while it is shown; none while
    /// neither is.
    fn field_mut(&mut self) -> Option<&mut Field> {
        let talking = self.typing();
        match &mut self.draft {
            Some(draft) => Some(&mut draft.text),
            None => talking.then_some(&mut self.talk),
        }
    }

    /// Whether there is a field keys type into: see [`Gallery::field_mut`].
    fn typing(&self) -> bool {
        self.draft.is_some() || (self.sidebar && self.reference().is_some())
    }

    /// What the box at the sidebar's foot holds said in the conversation on
    /// what is picked, added to its note in notes.json, or a note started
    /// on it, and shown at once, scrolled to; the box emptied. Nothing when
    /// it is empty; where it cannot be written, the panel says why and the
    /// box keeps it.
    fn send(&mut self) {
        let Some(on) = self.reference().filter(|_| !self.talk.is_blank()) else {
            return;
        };
        match notes::say(&on, &self.talk.text()) {
            Ok(_) => {
                self.notes = notes_of(&self.lines);
                self.talk = Field::new(&self.typesetter, sidebar::field_width(self.side));
                self.side_scroll = f64::MAX;
                self.clamp();
            }
            Err(e) => self.say(say!(self.lang, not_saved, e = format!("{e:#}"))),
        }
    }

    /// The ✗ being written saved to notes.json as a note on its node; or,
    /// where it cannot be, the panel says why and it stays to edit. Not
    /// sent empty: its field says so.
    fn reject(&mut self) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        if draft.text.is_blank() {
            self.draft = Some(draft);
            return;
        }
        let lang = self.lang;
        match notes::judge(&draft.on, draft.answers.clone(), &draft.text.text()) {
            Ok(id) => {
                self.notes = notes_of(&self.lines);
                self.say(say!(lang, note_sent, id, done = say!(lang, done_rejected)));
            }
            Err(e) => {
                self.say(say!(lang, not_saved, e = format!("{e:#}")));
                self.draft = Some(draft);
            }
        }
    }

    /// A key pressed, ⌘ not held, as `key` names it and with the `text` it
    /// types. Into the field written in ([`Gallery::field_mut`]): an edit,
    /// Enter sending it and Shift+Enter breaking a line, the keys an input
    /// method takes while it composes left to it, so that the Enter that
    /// commits what it composes sends nothing. ← and → step a journey
    /// played while the box is empty. Esc drops a ✗ being written, else
    /// what is picked. Whether that changed what is shown.
    fn key(&mut self, key: &Key, text: Option<&str>) -> bool {
        let stepping = self.playing.is_some() && self.draft.is_none() && self.talk.is_blank();
        match key {
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowLeft) if stepping => {
                let Some(playing) = &self.playing else { return false };
                let at = match key {
                    Key::Named(NamedKey::ArrowRight) => playing.at + 1,
                    _ => playing.at.wrapping_sub(1),
                };
                return at < playing.steps.len() && self.step(at);
            }
            Key::Named(NamedKey::Escape) => {
                if self.draft.take().is_some() {
                    return true;
                }
                return self.pick_thing(Picked::Nothing);
            }
            _ => {}
        }
        let shift = self.shift;
        let Some(field) = self.field_mut() else {
            return false;
        };
        if field.is_composing() {
            return false;
        }
        match key {
            Key::Named(NamedKey::Enter) if shift => {
                field.type_in("\n");
                true
            }
            Key::Named(NamedKey::Enter) => {
                if self.draft.is_some() {
                    self.reject();
                } else {
                    self.send();
                }
                true
            }
            Key::Named(
                named @ (NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::Home
                | NamedKey::End),
            ) => {
                let motion = match named {
                    NamedKey::ArrowLeft => Motion::Left,
                    NamedKey::ArrowRight => Motion::Right,
                    NamedKey::ArrowUp => Motion::Up,
                    NamedKey::ArrowDown => Motion::Down,
                    NamedKey::Home => Motion::Home,
                    _ => Motion::End,
                };
                field.edit(if shift { Edit::Select(motion) } else { Edit::Move(motion) })
            }
            Key::Named(NamedKey::Backspace) => field.edit(Edit::Back),
            Key::Named(NamedKey::Delete) => field.edit(Edit::Delete),
            _ => match text {
                Some(text) if !text.chars().any(char::is_control) => {
                    field.type_in(text);
                    true
                }
                _ => false,
            },
        }
    }

    /// `text` typed into the field written in at its caret: what an input
    /// method committed, or what was pasted. Whether one is written in.
    fn type_in(&mut self, text: &str) -> bool {
        let Some(field) = self.field_mut() else {
            return false;
        };
        field.type_in(text);
        true
    }

    /// What an input method says while a field is written in: what it is
    /// composing, shown at the caret, then what it commits, typed there.
    /// Whether one is written in.
    fn ime(&mut self, ime: Ime) -> bool {
        let Some(field) = self.field_mut() else {
            return false;
        };
        match ime {
            Ime::Preedit(composing, cursor) => field.compose(&composing, cursor),
            Ime::Commit(text) => field.type_in(&text),
            Ime::Enabled | Ime::Disabled => field.compose("", None),
        }
        true
    }

    /// ⌘A in the field written in: all of it selected. Whether one is.
    fn select_all(&mut self) -> bool {
        self.field_mut().is_some_and(|field| field.edit(Edit::All))
    }

    /// ⌘C, or ⌘X when `cut`, in the field written in: what is selected put
    /// on the clipboard, and, cut, taken out. Whether something was
    /// selected: ⌘C with nothing selected copies the reference instead.
    fn copy_selected(&mut self, cut: bool, clipboard: &mut dyn Clipboard) -> bool {
        let Some(field) = self.field_mut() else {
            return false;
        };
        let copied = if cut { field.cut() } else { field.selected().map(str::to_string) };
        let Some(copied) = copied else {
            return false;
        };
        let _ = clipboard.put(copied);
        true
    }

    /// Where the text of the field written in is, in window points, while
    /// one is and the sidebar is shown.
    fn field_rect(&mut self) -> Option<Rect> {
        if !self.sidebar {
            return None;
        }
        let (cards, foot) = (self.sidebar_cards(), self.foot());
        let (right, bottom) = (self.pane_right(), self.size.1);
        let top = -self.side_scroll;
        sidebar::lay_out(&cards, foot.as_ref(), (right, self.side), (top, bottom), &mut self.typesetter).field
    }

    /// The caret of the field written in put where the pointer is on its
    /// text (with Shift, what is selected drawn to there), and held there to
    /// drag a selection. Whether the pointer is on its text.
    fn press_field(&mut self) -> bool {
        let Some(field) = self.field_rect() else {
            return false;
        };
        let (x, y) = (f64::from(field.x), f64::from(field.y));
        let (at, shift) = (self.cursor, self.shift);
        let on = (x..=x + f64::from(field.width)).contains(&at.0)
            && (y..=y + f64::from(field.height)).contains(&at.1);
        let Some(text) = self.field_mut().filter(|_| on) else {
            return false;
        };
        text.point((at.0 - x, at.1 - y), shift);
        self.selecting = Some((x, y));
        true
    }

    /// What is selected in the field written in drawn to where the pointer
    /// is, while it drags from a press on its text. Whether it is.
    fn drag_field(&mut self) -> bool {
        let (Some((x, y)), cursor) = (self.selecting, self.cursor) else {
            return false;
        };
        let Some(field) = self.field_mut() else {
            return false;
        };
        field.point((cursor.0 - x, cursor.1 - y), true);
        true
    }

    /// The notes on what is shown, drawn on it; blue while waiting on the
    /// agent, green once answered. On a state's pictures, each on a node a
    /// numbered pin at the top-right corner of its node, in the picture of
    /// the state and width it was written in, and a faint dot there in
    /// every other that holds the node; each on the whole state a pin at
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
        for (reference, (_, (x, y))) in self.arrow_references().into_iter().zip(self.arrow_spots()) {
            let Some(notes) = self.notes.get(&reference.description) else { continue };
            let on = notes.iter().filter(|note| note.on == reference.on);
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
        let ink = if note.waits() { Ink::Note } else { Ink::Answered };
        Pin { dir: dir.to_path_buf(), id: note.id.clone(), rect, ink, faint }
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
            for note in notes {
                let (written, r, key, width) = match &note.on {
                    On::Node { state: written, node } => {
                        let Some(placed) = page.placed(&node.origin) else { continue };
                        (written, placed.rect, placed.id.clone(), node.width)
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
                let side = if faint { DOT } else { PIN };
                let x = corner.0 - side / 2.0 - *before as f64 * (side + 2.0);
                *before += 1;
                let at = rect(x, corner.1 - side / 2.0, side, side);
                pins.push(self.pin(&wanted.dir, note, at, faint));
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
            let on =
                notes.iter().filter(|note| matches!(&note.on, On::State { state: s } if Some(s) == state));
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

    /// The card of the state shown, with no node picked: its name, for the
    /// conversation under it on the whole state. None where no state is
    /// drawn.
    fn state_card(&self) -> Option<Card<Do>> {
        let Picture::Pages(_) = self.picture else {
            return None;
        };
        let state = self.state_shown()?;
        Some(Card::new(say!(self.lang, whole_state, state = state.name()), Vec::new()))
    }

    /// The card of the node picked: what it is (its component), the
    /// operation it calls, what was assumed there, and a button that folds
    /// out its details: its id, and its label and box at each width. None
    /// when none is.
    fn node_card(&self) -> Option<Card<Do>> {
        let (Some((id, _)), Picture::Pages(pages)) = (self.node(), &self.picture) else {
            return None;
        };
        let lang = self.lang;
        let (page, node) =
            pages.iter().find_map(|page| Some((page, page.nodes.iter().find(|n| &n.id == id)?)))?;
        let calls = match page.calls.get(id) {
            Some(operations) => operations.join(", "),
            None => say!(lang, calls_nothing),
        };
        let mut blocks = vec![Block::Fact(say!(lang, calls), calls)];
        let (fold, details) = (if self.details { "▾" } else { "▸" }, lang.words().details);
        blocks.push(Block::Fold(format!("{fold} {details}"), Do::Details));
        if self.details {
            blocks.push(Block::Fact(say!(lang, id), id.to_string()));
            for page in pages {
                let width = page.width;
                let at = say!(lang, at_width, width);
                blocks.push(Block::Fact(
                    at,
                    match page.nodes.iter().find(|n| &n.id == id) {
                        Some(node) => {
                            let r = node.rect;
                            let label = match node.label {
                                Some(n) => say!(lang, node_label, n),
                                None => say!(lang, node_no_label),
                            };
                            let (x, y, w, h) = (r.x, r.y, r.width, r.height);
                            say!(lang, node_box, label, x, y, w, h)
                        }
                        None => say!(lang, not_in_state),
                    },
                ));
            }
        }
        Some(Card::new(node.component.clone(), blocks))
    }

    /// How many notes wait on the agent on the tree's line of `row` and
    /// `state`: on the whole description, or on that state of it.
    fn waiting_notes(&self, row: &Row, state: Option<&State>) -> usize {
        let Some(notes) = row.description().and_then(|dir| self.notes.get(dir)) else {
            return 0;
        };
        let waiting = notes.iter().filter(|note| note.waits());
        match state {
            Some(state) => waiting.filter(|note| note.on.state() == Some(state)).count(),
            None => waiting.count(),
        }
    }

    /// The pins drawn: each numbered, those of the conversation in the
    /// sidebar ringed; each faint dot a small ring.
    fn pin_draws(&self) -> Vec<Draw> {
        let talked: Vec<(PathBuf, String)> = self
            .conversation()
            .map(|(on, notes)| notes.iter().map(|note| (on.description.clone(), note.id.clone())).collect())
            .unwrap_or_default();
        let mut list = Vec::new();
        for pin in self.pins() {
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
            if talked.iter().any(|(dir, id)| *dir == pin.dir && *id == pin.id) {
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
            .map(|reply| say!(self.lang, note_answered, id, reply));
        let Some(said) = reply else {
            return Vec::new();
        };
        let font = chrome::BODY;
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
                radius: chrome::PICKED_RADIUS,
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
        let path = self.line.and_then(|i| Some(self.lines.get(i)?.row.dir.clone())).unwrap_or_default();
        let within = path.strip_prefix(&self.root).unwrap_or(&path).display().to_string();
        let journey = self.line.and_then(|i| self.lines.get(i)).is_some_and(|line| line.journey);
        let lang = self.lang;
        let heading = match (journey, faults.len()) {
            (true, 1) => say!(lang, faults_not_played_one, within),
            (true, n) => say!(lang, faults_not_played_many, within, n),
            (false, 1) => say!(lang, faults_not_drawn_one, within),
            (false, n) => say!(lang, faults_not_drawn_many, within, n),
        };
        let mut list = vec![words(rect(0.0, MARGIN, wide, 18.0), heading, Ink::Text, chrome::HEADING)];
        let mut y = MARGIN + CAPTION + GAP;
        for fault in faults {
            list.push(words(rect(0.0, y, wide, 16.0), fault.at.clone(), Ink::Muted, chrome::HEADING));
            y += 18.0;
            let font = chrome::BODY;
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

    /// The tree's width, then the sidebar's, kept within their bounds: a
    /// window narrowed, or the sidebar shown, narrows the tree first. And
    /// the scrolls inside what there is to scroll.
    fn clamp(&mut self) {
        for edge in [Edge::Tree, Edge::Side] {
            let (least, most) = self.bounds(edge);
            let width = self.width_mut(edge);
            *width = width.clamp(least, most);
        }
        if let Some(draft) = &mut self.draft {
            draft.text.fit(sidebar::field_width(self.side));
        }
        self.talk.fit(sidebar::field_width(self.side));
        let height = self.size.1;
        let below = (self.pictures_height(self.pane()) - height).max(0.0);
        self.scroll = self.scroll.clamp(0.0, below);
        let beyond = (self.pictures_width(self.pane()) - self.pane()).max(0.0);
        self.across = self.across.clamp(0.0, beyond);
        let below = (self.sidebar_height() - self.sidebar_foot()).max(0.0);
        self.side_scroll = self.side_scroll.clamp(0.0, below);
        let below = (self.tree_height() - height).max(0.0);
        self.tree_scroll = self.tree_scroll.clamp(0.0, below);
    }

    /// How tall the tree's lines are, all of them, unscrolled.
    fn tree_height(&self) -> f64 {
        self.lines.len() as f64 * ROW + MARGIN
    }

    /// Each pane's scrollbar ([`scrollbar`](crate::scrollbar)): the
    /// pictures', down then across, and the tree's, then the sidebar's
    /// while it is shown. Where the pictures scroll both ways, the bar
    /// across stops short of the one down.
    fn bars(&mut self) -> Vec<(Scrolls, Bar)> {
        let (width, height) = self.size;
        let (tree, right) = (self.tree, self.pane_right());
        let pictures = Area::new(tree, 0.0, right, height);
        let content = self.pictures_height(self.pane());
        let down = Bar { pane: pictures, axis: Axis::Down, content, scrolled: self.scroll, corner: 0.0 };
        let corner =
            if down.thumb().is_some() { chrome::SCROLLBAR_WIDE + chrome::SCROLLBAR_INSET } else { 0.0 };
        let content = self.pictures_width(self.pane());
        let across = Bar { pane: pictures, axis: Axis::Across, content, scrolled: self.across, corner };
        let lines = Area::new(0.0, 0.0, tree, height);
        let content = self.tree_height();
        let lines = Bar { pane: lines, axis: Axis::Down, content, scrolled: self.tree_scroll, corner: 0.0 };
        let mut bars = vec![(Scrolls::Down, down), (Scrolls::Across, across), (Scrolls::Tree, lines)];
        if self.sidebar {
            let side = Area::new(right, 0.0, width, self.sidebar_foot());
            let content = self.sidebar_height();
            let side = Bar { pane: side, axis: Axis::Down, content, scrolled: self.side_scroll, corner: 0.0 };
            bars.push((Scrolls::Side, side));
        }
        bars
    }

    /// The thumb of each pane's scrollbar, in window points, where what
    /// it holds is larger than it, in the order of [`Gallery::bars`]: the
    /// pictures' drawn under their labels so that none covers an orange
    /// "?", the tree's and the sidebar's over the rest in the space right
    /// of the tree's names and of the sidebar's cards.
    fn thumbs(&mut self) -> Vec<(Scrolls, Area)> {
        self.bars().into_iter().filter_map(|(scrolls, bar)| Some((scrolls, bar.thumb()?))).collect()
    }

    /// The scrollbar under `at`, a point in the window, and the part of it
    /// there; none off every scrollbar.
    fn bar_at(&mut self, at: (f64, f64)) -> Option<(Scrolls, Bar, Part)> {
        self.bars().into_iter().find_map(|(scrolls, bar)| Some((scrolls, bar, bar.at(at)?)))
    }

    /// The whole window, in points.
    fn frame(&mut self) -> Scene {
        let (width, height) = self.size;
        let thumbs = self.thumbs();
        let hot = self.grabbed.map(|grab| grab.scrolls).or(self.hot);
        let (pictures_bars, side_bars): (Vec<_>, Vec<_>) =
            thumbs.into_iter().partition(|(scrolls, _)| matches!(scrolls, Scrolls::Down | Scrolls::Across));
        let tree = self.tree;
        let mut list = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let Line { row, state, .. } = line;
            let y = MARGIN / 2.0 + index as f64 * ROW - self.tree_scroll;
            if self.line == Some(index) {
                list.push(Draw::Box {
                    rect: rect(6.0, y + 1.0, tree - 12.0, ROW - 2.0),
                    radius: chrome::PICKED_RADIUS,
                    fill: Some(Ink::Fill),
                    stroke: None,
                    dashed: false,
                });
            }
            let x = 14.0 + row.depth as f64 * INDENT;
            let (name, ink, weight) = match (row.description(), state) {
                _ if line.journey => (format!("→ {}", row.name), Ink::Note, 500),
                (None, _) => (format!("{}/", row.name), Ink::Muted, chrome::HEADING),
                (Some(_), None) => (row.name.clone(), Ink::Text, chrome::HEADING),
                (Some(_), Some(State::Fixture(_))) => (row.name.clone(), Ink::Text, 400),
                (Some(_), Some(State::Phase(_))) => (row.name.clone(), Ink::Muted, 400),
            };
            let mut right = tree - 14.0;
            let open = self.waiting_notes(row, state.as_ref());
            if open > 0 {
                let said = match open {
                    1 => say!(self.lang, notes_one),
                    n => say!(self.lang, notes_many, n),
                };
                right = mark(&mut list, &mut self.typesetter, (right, y), said, Ink::Note);
            }
            if state.is_none() && !row.faults.is_empty() {
                let said = match row.faults.len() {
                    1 => say!(self.lang, faults_one),
                    n => say!(self.lang, faults_many, n),
                };
                right = mark(&mut list, &mut self.typesetter, (right, y), said, Ink::Strong);
            }
            let top = y + (ROW - f64::from(chrome::BODY.line_height)) / 2.0;
            list.push(words(rect(x, top, right - x, 16.0), name, ink, weight));
        }
        let left = tree + MARGIN;
        let (faults, _) = self.faults(self.pane());
        let mut pane: Vec<Draw> = faults.into_iter().map(|draw| moved(draw, left, -self.scroll)).collect();
        let zoom = self.zoom(self.pane());
        let mut pages = Vec::new();
        let labels = self.labels();
        let mut thumbs = Vec::new();
        // What the pictures are drawn on, each a sheet on the desk.
        let mut sheets = Vec::new();
        let (origins, outlines) = (self.origins(), self.outlines());
        let marks = self.marks();
        let (pins, reply) = (self.pin_draws(), self.reply_shown());
        let (cells, arrows) = (self.cells(), self.arrows());
        match &self.picture {
            Picture::Pages(shots) => {
                for (shot, at) in shots.iter().zip(origins) {
                    pane.push(caption(
                        rect(at.0, at.1 - CAPTION, shot.width * zoom, 16.0),
                        shot.caption.clone(),
                    ));
                    sheets.push(rect(at.0, at.1, shot.width * zoom, shot.height * zoom));
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
                            radius: chrome::PICKED_RADIUS - inset as f32,
                            fill: None,
                            stroke: Some(Ink::Note),
                            dashed: false,
                        });
                    }
                }
                for around in outlines {
                    pane.push(Draw::Box {
                        rect: around,
                        radius: chrome::PICKED_RADIUS / 2.0,
                        fill: None,
                        stroke: Some(Ink::Strong),
                        dashed: false,
                    });
                }
            }
            Picture::Overview(overview) => {
                for (thumb, (x, y, w, h)) in overview.thumbs.iter().zip(cells) {
                    let ink = if thumb.phase { Ink::Muted } else { Ink::Text };
                    pane.push(words(rect(x, y - CAPTION, w, 16.0), thumb.name.clone(), ink, chrome::HEADING));
                    sheets.push(rect(x, y, w, h));
                    thumbs.push(((x, y, w, h), w / thumb.width, &thumb.scene));
                }
                pane.extend(arrows);
                // Over the pictures, in the notes' ink, as the node a
                // journey acts on is marked when it plays.
                for (at, pass) in self.passes() {
                    pane.push(Draw::Box {
                        rect: at,
                        radius: chrome::MARK_RADIUS,
                        fill: Some(Ink::Note),
                        stroke: None,
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
                        font: MARK,
                        ink: Ink::Paper,
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
        let (side, foot, cut_at) = if self.sidebar {
            let (cards, foot) = (self.sidebar_cards(), self.foot());
            let mut side = vec![Draw::Line {
                from: [right as f32, 0.0],
                to: [right as f32, height as f32],
                ink: Ink::Line,
            }];
            let at = (-self.side_scroll, height);
            let laid = sidebar::lay_out(&cards, foot.as_ref(), (right, self.side), at, &mut self.typesetter);
            self.caret = laid.caret;
            side.extend(laid.draws);
            (side, laid.foot_draws, laid.foot)
        } else {
            self.caret = None;
            (Vec::new(), Vec::new(), height)
        };

        let mut scene = Scene::new();
        let at = Affine::IDENTITY;
        scene.fill(Fill::NonZero, at, chrome::SIDE, None, &Area::new(0.0, 0.0, tree, height));
        scene.fill(Fill::NonZero, at, chrome::DESK, None, &Area::new(tree, 0.0, right, height));
        scene.fill(Fill::NonZero, at, chrome::SIDE, None, &Area::new(right, 0.0, width, height));
        scene.push_clip_layer(Fill::NonZero, at, &Area::new(tree, 0.0, right, height));
        if let Picture::Flow(flow) = &self.picture {
            let at = Affine::translate(self.flow_origin()) * Affine::scale(zoom);
            for (place, tile) in flow.map.places.iter().zip(&flow.tiles) {
                if !matches!(tile, Tile::Undrawn) {
                    sheet(
                        &mut scene,
                        at,
                        rect(place.x, place.y, flow::WIDE, flow::TALL),
                        chrome::SHEET_RADIUS,
                    );
                }
            }
        }
        for around in sheets {
            sheet(&mut scene, at, around, 0.0);
        }
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
            paint::draws(
                &mut scene,
                at,
                &flow_draws(flow, self.arrow(), self.lang),
                chrome::colour,
                &mut self.typesetter,
            );
        }
        // The labels under what the window draws over the pictures: the
        // outline, the pins.
        for (scrolls, bar) in pictures_bars {
            thumb(&mut scene, bar, hot == Some(scrolls));
        }
        let mut over = labels;
        over.extend(pane);
        paint::draws(&mut scene, at, &over, chrome::colour, &mut self.typesetter);
        scene.pop_layer();
        // The cards cut where the box at the foot starts, drawn over them.
        scene.push_clip_layer(Fill::NonZero, at, &Area::new(right, 0.0, width, cut_at));
        paint::draws(&mut scene, at, &side, chrome::colour, &mut self.typesetter);
        scene.pop_layer();
        paint::draws(&mut scene, at, &foot, chrome::colour, &mut self.typesetter);
        // The tree's names cut at its edge, however narrow it is dragged.
        scene.push_clip_layer(Fill::NonZero, at, &Area::new(0.0, 0.0, tree, height));
        paint::draws(&mut scene, at, &list, chrome::colour, &mut self.typesetter);
        scene.pop_layer();
        let edge =
            [Draw::Line { from: [tree as f32, 0.0], to: [tree as f32, height as f32], ink: Ink::Line }];
        paint::draws(&mut scene, at, &edge, chrome::colour, &mut self.typesetter);
        for (scrolls, bar) in side_bars {
            thumb(&mut scene, bar, hot == Some(scrolls));
        }
        scene
    }

    /// The window as it now stands, in points.
    fn drawn(&mut self) -> Scene {
        self.clamp();
        self.frame()
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
            *font = Font { weight: 500, ..MARK };
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
    match answer {
        Answer::Right => lang.words().answer_right,
        Answer::Wrong => lang.words().answer_wrong,
    }
}

/// Where `note` stands, as the sidebar says it in `lang`.
fn note_status(note: &Note, lang: Lang) -> String {
    match note.stands() {
        Stands::Waiting => say!(lang, status_waiting),
        Stands::Answered => say!(lang, status_answered),
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

/// A mark on the tree's line at `y`, a pill of `ground` ending at `right`
/// with `said` in it in the paper's ink, added to `list`: where what is left
/// of the line ends.
fn mark(
    list: &mut Vec<Draw>,
    typesetter: &mut text::Typesetter,
    (right, y): (f64, f64),
    said: String,
    ground: Ink,
) -> f64 {
    let wide = f64::from(typesetter.text(&said, MARK, None).width) + 2.0 * MARK_INSET;
    let (left, top) = (right - wide, y + (ROW - MARK_TALL) / 2.0);
    list.push(Draw::Box {
        rect: rect(left, top, wide, MARK_TALL),
        radius: chrome::MARK_RADIUS,
        fill: Some(ground),
        stroke: None,
        dashed: false,
    });
    list.push(Draw::Text {
        rect: rect(
            left,
            top + (MARK_TALL - f64::from(MARK.line_height)) / 2.0,
            wide,
            f64::from(MARK.line_height),
        ),
        text: said,
        font: MARK,
        ink: Ink::Paper,
        max_lines: Some(1),
        align: Align::Center,
        underline: false,
        strike: false,
    });
    left - MARK_INSET
}

/// A sheet the size of `around`, in `at`'s points, laid on the desk: its
/// shadow, its paper, and the hairline round it.
fn sheet(scene: &mut Scene, at: Affine, around: Rect, radius: f32) {
    let (x, y) = (f64::from(around.x), f64::from(around.y));
    let (w, h) = (f64::from(around.width), f64::from(around.height));
    let r = f64::from(radius);
    let shadow = Area::new(x, y + chrome::SHADOW_DROP, x + w, y + h + chrome::SHADOW_DROP);
    scene.draw_blurred_rounded_rect(at, shadow, chrome::SHADOW, r, chrome::SHADOW_BLUR);
    let paper = vello::kurbo::RoundedRect::new(x, y, x + w, y + h, r);
    scene.fill(Fill::NonZero, at, chrome::colour(Ink::Paper), None, &paper);
    let edge = vello::kurbo::RoundedRect::new(x - 0.5, y - 0.5, x + w + 0.5, y + h + 0.5, r);
    scene.stroke(&vello::kurbo::Stroke::new(1.0), at, chrome::colour(Ink::Line), None, &edge);
}

/// A scrollbar's thumb at `bar`, in window points, in the chrome's look:
/// darker while `hot`, the pointer over it or dragging it.
fn thumb(scene: &mut Scene, bar: Area, hot: bool) {
    let round = vello::kurbo::RoundedRect::from_rect(bar, chrome::SCROLLBAR_WIDE / 2.0);
    let ink = if hot { chrome::SCROLLBAR_HOT } else { chrome::SCROLLBAR };
    scene.fill(Fill::NonZero, Affine::IDENTITY, ink, None, &round);
}

/// `text`, a picture's caption, in `rect`.
fn caption(rect: Rect, text: String) -> Draw {
    Draw::Text {
        rect,
        text,
        font: chrome::CAPTION_FONT,
        ink: Ink::Muted,
        max_lines: Some(1),
        align: Align::Start,
        underline: false,
        strike: false,
    }
}

/// `text` on one line in `rect`, in the interface's size.
fn words(rect: Rect, text: String, ink: Ink, weight: u16) -> Draw {
    Draw::Text {
        rect,
        text,
        font: Font { weight, ..chrome::BODY },
        ink,
        max_lines: Some(1),
        align: Align::Start,
        underline: false,
        strike: false,
    }
}

impl App {
    /// What the gallery says to do after an input, done to the window.
    fn apply(&mut self, effects: Effects, event_loop: &ActiveEventLoop) {
        if effects.exit {
            event_loop.exit();
        }
        let Some(shown) = &self.shown else {
            return;
        };
        if let Some(icon) = effects.cursor {
            shown.window.set_cursor(icon);
        }
        if effects.redraw {
            shown.window.request_redraw();
        }
        // What an input method composes goes into a note, and only there,
        // what it offers shown beside the caret.
        shown.window.set_ime_allowed(effects.ime);
        if let Some(caret) = effects.caret {
            let at = LogicalPosition::new(f64::from(caret.x), f64::from(caret.y));
            let size = LogicalSize::new(1.0, f64::from(caret.height));
            shown.window.set_ime_cursor_area(at, size);
        }
    }

    /// The window drawn as the gallery now stands.
    fn redraw(&mut self) {
        let frame = self.gallery.drawn();
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

impl ApplicationHandler<Told> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shown.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.gallery.title())
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
                self.gallery.handle(Input::Resized((size.width, size.height)), &mut self.clipboard);
                if let Some(proxy) = self.proxy.clone() {
                    let forward = proxy.clone();
                    let root = &self.gallery.root;
                    self.answering =
                        Answering::start(root, move || forward.send_event(Told::ComeForward).is_ok());
                    gallery::watch(root, move || proxy.send_event(Told::Changed).is_ok());
                }
                report(&json!({ "opened": true }));
                self.reported = true;
                // Drawn once the command has been told, so it answers at once.
                self.gallery.show_first();

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

    /// While a ✓ waits to be written: written once its time comes, the
    /// window woken each second until then to count down.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let written = self.gallery.confirm(now);
        let wake = self.gallery.confirming.as_ref().map(|confirming| {
            let left = confirming.at.saturating_duration_since(now);
            let tick = Duration::from_nanos((left.as_nanos() % 1_000_000_000) as u64);
            now + if tick.is_zero() { Duration::from_secs(1).min(left) } else { tick }
        });
        if let Some(shown) = &self.shown
            && (written || wake.is_some())
        {
            shown.window.request_redraw();
        }
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
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
                self.gallery.reload();
                if let Some(shown) = &self.shown {
                    shown.window.request_redraw();
                }
            }
        }
    }

    /// Each event turned into the [`Input`] it is, and nothing decided
    /// here: the left button alone, and a key only as it is pressed.
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(shown) = &mut self.shown else {
            return;
        };
        let scale = shown.window.scale_factor();
        let input = match event {
            WindowEvent::CloseRequested => Input::Close,
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    shown.gpu.resize(&mut shown.surface, size.width, size.height);
                }
                let points = size.to_logical::<f64>(scale);
                Input::Resized((points.width, points.height))
            }
            WindowEvent::CursorMoved { position, .. } => {
                let at = position.to_logical::<f64>(scale);
                Input::Moved((at.x, at.y))
            }
            WindowEvent::ModifiersChanged(modifiers) => Input::Modifiers(modifiers.state()),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => Input::Key {
                key: event.logical_key,
                code: event.physical_key,
                text: event.text.map(|text| text.to_string()),
                repeat: event.repeat,
            },
            WindowEvent::Ime(ime) => Input::Ime(ime),
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    Input::Wheel { x: x.into(), y: y.into(), unit: Unit::Lines }
                }
                MouseScrollDelta::PixelDelta(moved) => {
                    let moved = moved.to_logical::<f64>(scale);
                    Input::Wheel { x: moved.x, y: moved.y, unit: Unit::Points }
                }
            },
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed => Input::Pressed,
                ElementState::Released => Input::Released,
            },
            WindowEvent::RedrawRequested => {
                self.redraw();
                // Where the caret was drawn is where the input method shows.
                let (ime, caret) = self.gallery.composing();
                return self.apply(Effects { ime, caret, ..Effects::default() }, event_loop);
            }
            _ => return,
        };
        let effects = self.gallery.handle(input, &mut self.clipboard);
        self.apply(effects, event_loop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The drafts as git has them committed at `HEAD`, not as they stand in
    /// the working tree: a draft added, edited or noted on and not committed
    /// changes no test. They are written once for each commit to a directory
    /// all tests read and none writes, and a test that changes a draft
    /// copies it to a root of its own first.
    fn drafts() -> &'static Path {
        static DRAFTS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        DRAFTS.get_or_init(|| {
            let repo = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
            let git = |args: &[&str]| {
                let mut git = std::process::Command::new("git");
                git.arg("-C").arg(repo).args(args);
                git
            };
            let head = git(&["rev-parse", "HEAD"]).output().unwrap();
            assert!(head.status.success(), "the drafts are read from git's HEAD");
            let at = std::env::temp_dir()
                .join(format!("underdraw-drafts-{}", String::from_utf8(head.stdout).unwrap().trim()));
            if !at.is_dir() {
                // Written beside it and renamed into place: each test runs in
                // a process of its own, and another may be writing it too.
                let writing = at.with_extension(std::process::id().to_string());
                let _ = std::fs::remove_dir_all(&writing);
                std::fs::create_dir_all(&writing).unwrap();
                let mut archive =
                    git(&["archive", "HEAD", "drafts"]).stdout(std::process::Stdio::piped()).spawn().unwrap();
                let tar = std::process::Command::new("tar")
                    .arg("-x")
                    .arg("-C")
                    .arg(&writing)
                    .stdin(archive.stdout.take().unwrap())
                    .status()
                    .unwrap();
                assert!(
                    archive.wait().unwrap().success() && tar.success(),
                    "the drafts are read from git's HEAD"
                );
                if std::fs::rename(&writing, &at).is_err() {
                    std::fs::remove_dir_all(&writing).unwrap();
                }
            }
            at.join("drafts")
        })
    }

    /// The clipboard the tests lend the gallery: what was last put on it.
    #[derive(Default)]
    struct Board(Option<String>);

    impl Clipboard for Board {
        fn text(&mut self) -> Option<String> {
            self.0.clone()
        }

        fn put(&mut self, text: String) -> Result<(), Option<String>> {
            self.0 = Some(text);
            Ok(())
        }
    }

    thread_local! {
        /// Each test's clipboard: what it copies, it can paste.
        static BOARD: RefCell<Board> = RefCell::default();
    }

    /// A key pressed: `key` on the layout, typing `text`, on no key the
    /// keyboard names.
    fn key_of(key: Key, text: Option<&str>) -> Input {
        let code = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        Input::Key { key, code, text: text.map(str::to_string), repeat: false }
    }

    /// What the person does, as the window tells it: each helper the inputs
    /// it would be told, fed to [`Gallery::handle`].
    impl Gallery {
        /// `inputs` handled in turn, the tests' clipboard lent: what the
        /// window does after the last.
        fn feed(&mut self, inputs: impl IntoIterator<Item = Input>) -> Effects {
            BOARD.with_borrow_mut(|board| {
                inputs.into_iter().fold(Effects::default(), |_, input| self.handle(input, board))
            })
        }

        /// The pointer moved to `at`. Whether that changed what is shown.
        fn move_to(&mut self, at: (f64, f64)) -> bool {
            self.feed([Input::Moved(at)]).redraw
        }

        /// The pointer moved `by` from where it is.
        fn move_by(&mut self, by: (f64, f64)) -> bool {
            self.move_to((self.cursor.0 + by.0, self.cursor.1 + by.1))
        }

        /// The left button pressed where the pointer is. Whether that
        /// changed what is shown.
        fn press_here(&mut self) -> bool {
            self.feed([Input::Pressed]).redraw
        }

        /// The left button pressed at `at`, the pointer moved there first.
        /// Whether the press changed what is shown.
        fn press_at(&mut self, at: (f64, f64)) -> bool {
            self.move_to(at);
            self.press_here()
        }

        /// The left button let go. Whether that changed what is shown.
        fn let_go(&mut self) -> bool {
            self.feed([Input::Released]).redraw
        }

        /// The wheel turned to scroll `right` and `down` points, as a
        /// trackpad does, where the pointer is.
        fn wheel_by(&mut self, right: f64, down: f64) {
            self.feed([Input::Wheel { x: -right, y: -down, unit: Unit::Points }]);
        }

        /// The key of `letter` (one of A, C, V, X and \) pressed with ⌘
        /// held, on a US layout. Whether that changed what is shown.
        fn chord(&mut self, letter: &str) -> bool {
            let code = match letter {
                "a" => KeyCode::KeyA,
                "c" => KeyCode::KeyC,
                "v" => KeyCode::KeyV,
                "x" => KeyCode::KeyX,
                "\\" => KeyCode::Backslash,
                _ => panic!("no chord on {letter:?}"),
            };
            let key = Key::Character(letter.into());
            let pressed =
                Input::Key { key, code: PhysicalKey::Code(code), text: Some(letter.into()), repeat: false };
            self.feed([Input::Modifiers(ModifiersState::SUPER)]);
            let changed = self.feed([pressed]).redraw;
            self.feed([Input::Modifiers(ModifiersState::empty())]);
            changed
        }

        /// `text` put on the clipboard and pasted with ⌘V. Whether it was
        /// pasted.
        fn paste(&mut self, text: &str) -> bool {
            BOARD.with_borrow_mut(|board| board.0 = Some(text.to_string()));
            self.chord("v")
        }

        /// What an input method says. Whether that changed what is shown.
        fn compose(&mut self, ime: Ime) -> bool {
            self.feed([Input::Ime(ime)]).redraw
        }
    }

    /// The gallery over `root`, opened in a window `size` points wide and
    /// tall, drawn nowhere.
    fn opened(root: &Path, size: (f64, f64)) -> Gallery {
        let mut gallery = Gallery::new(root, text::Typesetter::new(None).unwrap());
        gallery.feed([Input::Resized(size)]);
        gallery
    }

    /// The gallery over the drafts, in a window of 1440 × 900 points.
    fn gallery() -> Gallery {
        opened(drafts(), (1440.0, 900.0))
    }

    /// The pointer moved over row `index` of the tree.
    fn over_row(gallery: &mut Gallery, index: usize) {
        gallery.move_to((40.0, MARGIN / 2.0 + (index as f64 + 0.5) * ROW));
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
        assert!(gallery.press_here());
        assert_eq!(gallery.line, Some(loading));
        assert_eq!(captions(&gallery), ["loading · 390", "loading · 1120"]);
    }

    #[test]
    fn picking_a_state_draws_it_at_both_widths() {
        let mut gallery = gallery();
        let docs = line_of(&gallery, "docs");
        over_row(&mut gallery, docs + 1);
        assert!(gallery.press_here(), "the state is picked");
        assert_eq!(gallery.line, Some(docs + 1));
        let Picture::Pages(pages) = &gallery.picture else { panic!("docs has no faults") };
        let widths: Vec<f64> = pages.iter().map(|page| page.width).collect();
        assert_eq!(widths, vec![390.0, 1120.0]);
        assert_eq!(captions(&gallery), ["default · 390", "default · 1120"]);
        assert!(!gallery.press_here(), "picking it again changes nothing");
        gallery.press_at((gallery.tree + 100.0, 100.0));
        assert_eq!(gallery.line, Some(docs + 1), "a press on the pictures leaves the state picked");
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
        assert!(gallery.press_here(), "the description is picked");
        assert_eq!(gallery.line, Some(orders), "the description itself, not a state of it");
        let listed: Vec<&str> = gallery.lines[orders + 1..]
            .iter()
            .take_while(|line| line.state.is_some())
            .map(|line| line.row.name.as_str())
            .collect();
        let (names, arrows) = overview(&gallery);
        assert_eq!(names, listed, "every state, in the order the tree lists them");
        assert!(names.contains(&"loading"), "a state no transition reaches or leaves is still shown");
        let screen: Value =
            serde_json::from_str(&std::fs::read_to_string(drafts().join("orders/screen.json")).unwrap())
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
            &drafts().join("orders"),
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
        assert!(
            gallery.press_at((error_cell.0 + 10.0, error_cell.1 + 10.0)),
            "clicking a state's picture picks it"
        );
        assert_eq!(gallery.line, Some(error));
        assert_eq!(captions(&gallery), ["error · 390", "error · 1120"]);
    }

    #[test]
    fn a_tall_page_scrolls_as_far_as_its_end() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        over_row(&mut gallery, listing + 1);
        gallery.press_here();
        let end = gallery.pictures_height(gallery.pane()) - 900.0;
        assert!(end > 0.0, "the listing is taller than the window");
        gallery.move_to((gallery.tree + 100.0, 400.0));
        gallery.wheel_by(0.0, 120.0);
        assert_eq!(gallery.scroll, 120.0);
        gallery.wheel_by(0.0, 1e6);
        assert_eq!(gallery.scroll, end, "no further than the end");
        gallery.wheel_by(0.0, -1e6);
        assert_eq!(gallery.scroll, 0.0, "no further than the top");
        over_row(&mut gallery, 0);
        gallery.wheel_by(0.0, 500.0);
        assert_eq!((gallery.scroll, gallery.tree_scroll), (0.0, 500.0), "over the tree, the tree scrolls");
        gallery.wheel_by(0.0, 1e6);
        let end = gallery.lines.len() as f64 * ROW + MARGIN - 900.0;
        assert_eq!(gallery.tree_scroll, end, "as far as its last line");
    }

    /// The thumbs of the pictures' scrollbars, down then across, and of
    /// the tree's then the sidebar's.
    fn scrollbars(gallery: &mut Gallery) -> (Vec<Area>, Vec<Area>) {
        let (pictures, others): (Vec<_>, Vec<_>) = gallery
            .thumbs()
            .into_iter()
            .partition(|(scrolls, _)| matches!(scrolls, Scrolls::Down | Scrolls::Across));
        (pictures.into_iter().map(|(_, bar)| bar).collect(), others.into_iter().map(|(_, bar)| bar).collect())
    }

    /// The scrollbar `scrolls` of the gallery as it is now.
    fn bar_of(gallery: &mut Gallery, scrolls: Scrolls) -> Bar {
        gallery.bars().into_iter().find(|(s, _)| *s == scrolls).unwrap().1
    }

    #[test]
    fn a_pane_shows_a_scrollbar_while_it_overflows_and_follows_its_scroll() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        over_row(&mut gallery, listing + 1);
        gallery.press_here();
        let (pictures, _) = scrollbars(&mut gallery);
        assert_eq!(pictures.len(), 2, "the listing overflows down, and at half size across: {pictures:?}");
        let (top, across) = (pictures[0], pictures[1]);
        assert!(across.x1 < top.x0, "the bar across stops short of the one down");
        assert_eq!(across.x0, gallery.tree + chrome::SCROLLBAR_INSET, "unscrolled across, at its start");
        assert_eq!(top.y0, chrome::SCROLLBAR_INSET, "unscrolled, at the very top");
        let share = 900.0 / gallery.pictures_height(gallery.pane());
        assert!((top.height() - (900.0 - 2.0 * chrome::SCROLLBAR_INSET) * share).abs() < 1e-9);
        gallery.move_to((gallery.tree + 100.0, 400.0));
        gallery.wheel_by(0.0, 1e6);
        let (pictures, _) = scrollbars(&mut gallery);
        assert_eq!(
            pictures[0].y1,
            900.0 - chrome::SCROLLBAR_INSET,
            "scrolled to the end, at the very bottom"
        );
        assert_eq!(pictures[0].height(), top.height());

        let (_, others) = scrollbars(&mut gallery);
        let tree = gallery.tree_height() > 900.0;
        assert_eq!(others.iter().any(|bar| bar.x1 <= gallery.tree), tree, "the tree's while it overflows");
        for bar in &others {
            assert!(bar.x0 >= gallery.tree - 14.0 || bar.x0 >= gallery.pane_right(), "clear of the names");
        }

        gallery.feed([Input::Resized((4000.0, 1e5))]);
        gallery.clamp();
        let (pictures, others) = scrollbars(&mut gallery);
        assert!(pictures.is_empty() && others.is_empty(), "a window large enough for all of it has none");
    }

    #[test]
    fn dragging_a_thumb_scrolls_its_pane_in_step_held_within_its_bounds() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        let (picked, node) = (gallery.line, gallery.picked.clone());
        let bar = bar_of(&mut gallery, Scrolls::Down);
        let thumb = bar.thumb().unwrap();

        assert!(gallery.move_to((thumb.center().x, thumb.center().y)), "the pointer over a thumb warms it");
        assert_eq!(gallery.hot, Some(Scrolls::Down));
        gallery.press_here();
        assert_eq!(gallery.grabbed.map(|grab| grab.scrolls), Some(Scrolls::Down), "the thumb taken hold of");
        assert_eq!(gallery.scroll, 0.0, "a press on the thumb scrolls nothing yet");
        assert!(gallery.move_by((0.0, 30.0)), "the drag moves the thumb");
        assert!((gallery.scroll - bar.dragged(0.0, 30.0)).abs() < 1e-9, "scrolled as far as the thumb moved");
        let moved = bar_of(&mut gallery, Scrolls::Down).thumb().unwrap();
        assert!((moved.y0 - (thumb.y0 + 30.0)).abs() < 1e-9, "the thumb under the pointer still: {moved:?}");
        gallery.move_by((0.0, 1e5));
        let below = gallery.pictures_height(gallery.pane()) - 900.0;
        assert_eq!(gallery.scroll, below, "no further than the end");
        gallery.move_to((gallery.cursor.0, -1e5));
        assert_eq!(gallery.scroll, 0.0, "no further than the start");
        assert!(gallery.let_go(), "letting go cools the thumb");
        assert!(gallery.grabbed.is_none());
        gallery.move_to((gallery.cursor.0, 500.0));
        assert_eq!(gallery.scroll, 0.0);
        assert_eq!((gallery.line, &gallery.picked), (picked, &node), "nothing picked by any of it");

        let across = bar_of(&mut gallery, Scrolls::Across).thumb().unwrap();
        gallery.press_at((across.center().x, across.center().y));
        gallery.move_by((40.0, 0.0));
        assert!(gallery.across > 0.0, "the thumb across scrolls across");
        gallery.let_go();
    }

    #[test]
    fn a_press_on_the_track_scrolls_a_page_toward_it() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        // The window short, so that a page is less than what is out of view.
        gallery.feed([Input::Resized((1440.0, 300.0))]);
        gallery.clamp();
        let thumb = bar_of(&mut gallery, Scrolls::Down).thumb().unwrap();
        assert!(gallery.press_at((thumb.center().x, thumb.y1 + 10.0)), "a press below the thumb scrolls");
        assert_eq!(gallery.scroll, 300.0, "one page down: as tall as the pane");
        assert!(gallery.grabbed.is_none(), "takes hold of nothing");
        gallery.let_go();
        let thumb = bar_of(&mut gallery, Scrolls::Down).thumb().unwrap();
        gallery.press_at((thumb.center().x, thumb.y0 - 1.0));
        assert_eq!(gallery.scroll, 0.0, "one page back up");
        gallery.let_go();
        gallery.move_to((thumb.center().x, 300.0 - chrome::SCROLLBAR_INSET - 1.0));
        for _ in 0..20 {
            gallery.press_here();
            gallery.let_go();
        }
        let below = gallery.pictures_height(gallery.pane()) - 300.0;
        assert_eq!(gallery.scroll, below, "never past the end");
    }

    #[test]
    fn a_press_on_a_scrollbar_picks_nothing_and_presses_nothing() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.feed([Input::Resized((1440.0, 300.0))]);
        gallery.clamp();
        let (picked, node) = (gallery.line, gallery.picked.clone());
        for scrolls in [Scrolls::Tree, Scrolls::Down, Scrolls::Across, Scrolls::Side] {
            let bar = bar_of(&mut gallery, scrolls);
            let thumb = bar.thumb().unwrap_or_else(|| panic!("{scrolls:?} overflows a short window"));
            for at in [(thumb.center().x, thumb.center().y), bar.lane().unwrap().center().into()] {
                gallery.move_to(at);
                assert_eq!(gallery.edge_at(), None, "{scrolls:?}: a scrollbar is never a line's");
                gallery.press_here();
                assert_eq!(gallery.line, picked, "{scrolls:?}: no line of the tree picked");
                assert_eq!(gallery.picked, node, "{scrolls:?}: no node picked");
                assert!(gallery.resizing.is_none(), "{scrolls:?}: no line");
                assert!(gallery.draft.is_none() && gallery.acted.is_none(), "{scrolls:?}: no button pressed");
                gallery.cursor.1 += 5.0;
                gallery.release();
                assert!(gallery.selecting.is_none(), "{scrolls:?}: no text selected");
            }
        }
    }

    #[test]
    fn a_line_between_panes_and_the_scrollbar_beside_it_never_share_a_point() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.feed([Input::Resized((1440.0, 300.0))]);
        gallery.clamp();
        let (tree, right) = (gallery.tree, gallery.pane_right());
        let lanes: Vec<_> =
            gallery.bars().into_iter().filter_map(|(s, bar)| Some((s, bar.lane()?))).collect();
        assert_eq!(lanes.len(), 4, "every pane overflows: {lanes:?}");
        for (scrolls, lane) in lanes {
            for line in [tree, right] {
                let clear = lane.x1 <= line - chrome::GRIP || lane.x0 >= line + chrome::GRIP;
                assert!(clear, "{scrolls:?}'s scrollbar {lane:?} clear of the line at {line}");
            }
        }
    }

    #[test]
    fn one_thing_is_picked_at_a_time_and_a_pin_picks_what_its_note_is_on() {
        let root = root_of("one-picked", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        let orders = line_of(&gallery, "orders");
        showing(&mut gallery, orders);
        let last = gallery.arrow_spots().len() - 1;
        let at = on_arrow(&gallery, last);
        say_at(&mut gallery, at, "should it say why?");
        assert_eq!(gallery.picked, Picked::Arrow(last), "the arrow talked about stays picked");

        assert!(gallery.press_at(on_arrow(&gallery, 0)));
        assert_eq!(gallery.picked, Picked::Arrow(0), "another arrow picked in its place");
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet", "and its own conversation");
        assert!(gallery.press_at(on_pin(&gallery, "N1")));
        assert_eq!(gallery.picked, Picked::Arrow(last), "its pin picks the arrow its note is on");

        // A point of the overview on no state, arrow, mark, pin or scrollbar.
        let nowhere = (0..40)
            .flat_map(|x| (0..40).map(move |y| (x, y)))
            .map(|(x, y)| (gallery.tree + 20.0 + f64::from(x) * 20.0, 10.0 + f64::from(y) * 20.0))
            .find(|&at| {
                gallery.state_at(at).is_none()
                    && gallery.arrow_at(at).is_none()
                    && gallery.pass_at(at).is_none()
                    && gallery.pin_at(at).is_none()
                    && at.0 < gallery.pane_right() - 20.0
            })
            .unwrap();
        assert!(gallery.press_at(nowhere), "a press on nothing unpicks the arrow");
        assert_eq!(gallery.picked, Picked::Nothing);

        show(&mut gallery, "orders", "default");
        say_on(&mut gallery, 0, "nav-export", "rename it");
        assert!(gallery.press_at(on_node(&gallery, 0, "order-0-cancel")));
        assert_eq!(gallery.picked, Picked::Node { id: "order-0-cancel".into(), pointed: 0 });
        assert!(gallery.press_at(on_pin(&gallery, "N2")));
        assert_eq!(
            gallery.picked,
            Picked::Node { id: "nav-export".into(), pointed: 0 },
            "the note's node, not the one picked before"
        );
        assert!(gallery.panel().iter().any(|line| line == "IconButton"), "whose card is shown");
        press_keys(&mut gallery, &["Esc"]);
        assert_eq!(gallery.picked, Picked::Nothing, "Esc lets it go");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The id of the node `gallery` holds picked, as [`Picked`] says it.
    fn picked_node(gallery: &Gallery) -> Option<&str> {
        gallery.node().map(|(id, _)| id)
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
        gallery.press_here();
    }

    #[test]
    fn a_node_picked_in_the_picture_is_outlined_and_says_what_it_is() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        assert!(gallery.press_at(on_node(&gallery, 0, "nav-save")), "a node is picked");
        assert_eq!(picked_node(&gallery), Some("nav-save"), "the innermost, not the bar it sits in");
        assert_eq!(
            gallery.panel(),
            [
                "IconButton",
                "calls  toggleSaved",
                "▸ details",
                "note · nothing said yet",
                "Say below what should change: Enter sends, Shift+Enter breaks the line."
            ],
            "what it is and what it calls, its id, label and boxes folded away; then its conversation"
        );
        assert!(!gallery.press_here(), "picking it again changes nothing");
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
        assert!(
            gallery.press_at(on_node(&gallery, 1, &id)),
            "the cancel button is picked, in the desktop picture"
        );
        let reference = gallery.reference().unwrap();
        assert_eq!(reference.description, drafts().join("orders"));
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
        gallery.chord("c");
        assert_eq!(gallery.panel()[0], "nothing to copy: pick a state, a node in it, or an arrow, first");
        let loading = state_of_in(&gallery, orders, "loading");
        showing(&mut gallery, loading);
        let Picture::Pages(_) = &gallery.picture else { panic!("no picture") };
        // A click on a picture's caption picks no node: the state itself.
        let (left, top) = gallery.origins()[0];
        gallery.press_at((left + 4.0, top - CAPTION / 2.0));
        let reference = gallery.reference().unwrap();
        assert_eq!(reference.on, On::State { state: State::Phase("loading".into()) });
        assert_eq!(reference.line(), format!("{} · phase loading", drafts().join("orders").display()));
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
        gallery.press_at(on_node(&gallery, 0, "card-number"));
        assert!(gallery.panel().iter().any(|line| line == "Card fields are drawn here"));
    }

    #[test]
    fn an_overlay_is_picked_before_what_it_floats_over() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.press_at(on_node(&gallery, 1, "photos-show-all"));
        assert_eq!(picked_node(&gallery), Some("photos-show-all"), "not the photos under it");
    }

    #[test]
    fn switching_state_keeps_the_node_picked_while_the_state_has_it() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.press_at(on_node(&gallery, 0, "nav-back"));
        showing(&mut gallery, listing + 2);
        assert_eq!(picked_node(&gallery), Some("nav-back"), "another fixture has it");
        showing(&mut gallery, listing + 5);
        assert_eq!(picked_node(&gallery), Some("nav-back"), "and so does the loading phase");
        gallery.press_at(on_node(&gallery, 0, "loading-skeleton"));
        showing(&mut gallery, listing + 1);
        assert_eq!(picked_node(&gallery), None, "the data has no skeleton");
        assert_eq!(gallery.panel()[0], "default · the whole state", "the state's card in its place");
    }

    #[test]
    fn a_press_where_no_node_is_clears_the_pick() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.press_at(on_node(&gallery, 0, "nav-save"));
        let (left, top) = gallery.origins()[1];
        assert!(gallery.press_at((left - MARGIN / 2.0, top + 10.0)), "between the pictures");
        assert_eq!(picked_node(&gallery), None);
        assert_eq!(gallery.panel()[0], "default · the whole state", "the node's card is gone");
    }

    #[test]
    fn the_sidebar_is_on_the_right_and_command_backslash_gives_its_width_to_the_pictures() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing);
        assert_eq!(gallery.pane_right(), 1440.0 - SIDEBAR, "the sidebar is shown by default");
        let shown = gallery.sidebar_cards();
        assert_eq!(
            shown.last().map(|card| card.title.as_str()),
            Some("Nothing picked"),
            "with nothing picked to talk about, it says what can be done, under the inbox"
        );
        showing(&mut gallery, listing + 1);
        gallery.cursor = on_node(&gallery, 0, "nav-save");
        gallery.press();
        let narrow = gallery.zoom(gallery.pane());
        let (left, _) = gallery.origins()[1];
        assert!(left + 1120.0 * narrow <= gallery.pane_right(), "the pictures end before the sidebar");
        assert!(
            !gallery.press_at((1440.0 - SIDEBAR / 2.0, 890.0)),
            "a press in the sidebar off its buttons does nothing"
        );
        assert_eq!(picked_node(&gallery), Some("nav-save"), "and leaves the node picked");

        gallery.chord("\\");
        assert_eq!(gallery.pane_right(), 1440.0, "hidden: the pictures take the width");
        assert!(gallery.zoom(gallery.pane()) > narrow, "and are drawn larger");
        assert!(
            gallery.press_at(on_node(&gallery, 1, "nav-save")),
            "a node near the window's right edge is picked, no sidebar there"
        );
        assert_eq!(gallery.node().map(|(_, pointed)| pointed), Some(1));
        gallery.chord("\\");
        assert_eq!(gallery.pane_right(), 1440.0 - SIDEBAR, "and shown again");
    }

    #[test]
    fn in_a_narrow_window_the_pictures_stop_at_half_size_and_scroll_across() {
        let mut gallery = gallery();
        gallery.feed([Input::Resized((1000.0, 900.0))]);
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        assert_eq!(gallery.zoom(gallery.pane()), SMALLEST, "no smaller than half");
        let beyond = gallery.pictures_width(gallery.pane()) - gallery.pane();
        assert!(beyond > 0.0, "wider than the pane");
        gallery.move_to((gallery.tree + 100.0, 400.0));
        gallery.wheel_by(1e6, 0.0);
        assert_eq!(gallery.across, beyond, "scrolled across as far as the last picture's end");
        gallery.move_to(on_node(&gallery, 1, "nav-save"));
        assert!(gallery.cursor.0 < gallery.pane_right(), "the desktop picture's save button in view");
        assert!(gallery.press_here());
        assert_eq!(gallery.node(), Some(("nav-save", 1)), "the right node, scaled and scrolled");

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
    fn dragging_the_line_between_the_tree_and_the_pictures_resizes_the_tree_within_its_bounds() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        assert_eq!(gallery.tree, TREE, "the window opens with the tree at 260pt");
        // The sidebar hidden, the pictures are above half size, to shrink.
        gallery.chord("\\");
        let (picked, wide) = (gallery.line, gallery.zoom(gallery.pane()));

        assert!(!gallery.press_at((TREE + 2.0, 300.0)), "a press on the line changes nothing shown");
        assert_eq!(gallery.line, picked, "and picks no line of the tree");
        gallery.cursor = (400.0, 320.0);
        assert!(gallery.resize(), "the drag moves the line");
        assert_eq!(gallery.tree, 400.0, "to where the pointer is");
        assert_eq!(gallery.origins()[0].0, 400.0 + MARGIN, "the pictures start after it");
        assert!(gallery.zoom(gallery.pane()) < wide, "and fit the width left");
        assert!(!gallery.let_go(), "letting go picks nothing");
        assert_eq!(gallery.line, picked);
        gallery.cursor = (500.0, 320.0);
        assert!(!gallery.resize(), "let go, the line stays");
        assert_eq!(gallery.tree, 400.0);
        gallery.chord("\\");

        gallery.press_at((402.0, 300.0));
        gallery.move_to((20.0, 300.0));
        assert_eq!(gallery.tree, TREE_LEAST, "no narrower than 160pt");
        gallery.move_to((1400.0, 300.0));
        let room = f64::from(WIDTHS[0]) * SMALLEST + 2.0 * MARGIN;
        assert_eq!(gallery.tree, gallery.pane_right() - room, "the pictures keep room for a half-size phone");
        gallery.let_go();

        gallery.feed([Input::Resized((900.0, 900.0))]);
        gallery.clamp();
        assert_eq!(gallery.tree, 900.0 - SIDEBAR - room, "a narrower window narrows the tree too");
        gallery.feed([Input::Resized((500.0, 900.0))]);
        gallery.clamp();
        assert_eq!(gallery.tree, TREE_LEAST, "never below its least");
    }

    #[test]
    fn dragging_the_line_between_the_pictures_and_the_sidebar_resizes_the_sidebar_within_its_bounds() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.press_at(on_node(&gallery, 0, "nav-save"));
        gallery.let_go();
        assert_eq!(gallery.side, SIDEBAR, "the window opens with the sidebar at 360pt");
        // A window wide enough for the pictures above half size, to shrink.
        let width = 2400.0;
        gallery.feed([Input::Resized((width, 900.0))]);
        gallery.clamp();
        let wide = gallery.zoom(gallery.pane());
        let line = width - SIDEBAR;

        gallery.move_to((line - 3.0, 300.0));
        assert_eq!(gallery.edge_at(), Some(Edge::Side), "the pointer takes hold of the line");
        assert!(!gallery.press_here(), "a press on the line changes nothing shown");
        assert_eq!(picked_node(&gallery), Some("nav-save"), "and leaves the node picked");
        assert!(!gallery.details, "nor presses a button");
        assert!(gallery.move_to((line - 400.0, 320.0)), "the drag moves the line");
        assert_eq!(gallery.side, SIDEBAR + 400.0, "to where the pointer is");
        assert_eq!(gallery.tree, TREE, "the tree keeps its width");
        assert!(gallery.zoom(gallery.pane()) < wide, "the pictures fit the width left");
        assert!(!gallery.let_go(), "letting go picks nothing");
        assert_eq!(picked_node(&gallery), Some("nav-save"));
        assert!(click_panel_line(&mut gallery, "▸ details"), "the details unfolded, wide");

        gallery.press_at((line - 400.0, 300.0));
        gallery.move_to((width - 40.0, 300.0));
        gallery.let_go();
        assert_eq!(gallery.side, SIDEBAR_LEAST, "no narrower than 280pt");
        let (cards, foot) = (gallery.sidebar_cards(), gallery.foot());
        let (at, bottom) = ((gallery.pane_right(), gallery.side), gallery.size.1);
        let laid = sidebar::lay_out(&cards, foot.as_ref(), at, (0.0, bottom), &mut gallery.typesetter);
        let inside = |r: &Rect| f64::from(r.x) > gallery.pane_right() && f64::from(r.x + r.width) < width;
        assert!(laid.buttons.iter().all(|(r, _, _)| inside(r)), "its buttons inside it");
        assert!(click_panel_line(&mut gallery, "▾ details"), "each still clicked by its own box");
        assert!(!gallery.details);

        gallery.chord("\\");
        gallery.move_to((width - SIDEBAR_LEAST, 300.0));
        assert_eq!(gallery.edge_at(), None, "hidden, there is no line");
        gallery.chord("\\");
        assert_eq!(gallery.side, SIDEBAR_LEAST, "shown again, at the width it was dragged to");
        assert_eq!(gallery.pane_right(), width - SIDEBAR_LEAST);

        gallery.press_at((gallery.pane_right(), 300.0));
        gallery.move_to((0.0, 300.0));
        gallery.let_go();
        let room = f64::from(WIDTHS[0]) * SMALLEST + 2.0 * MARGIN;
        assert_eq!(gallery.side, width - TREE - room, "the pictures keep room for a half-size phone");
        gallery.press_at((TREE, 300.0));
        gallery.move_to((600.0, 300.0));
        gallery.let_go();
        assert_eq!(gallery.tree, TREE, "the tree widens no further than leaves that room");
        gallery.feed([Input::Resized((1200.0, 900.0))]);
        gallery.clamp();
        assert_eq!(gallery.side + gallery.tree, 1200.0 - room, "a narrower window narrows them too");
        gallery.feed([Input::Resized((600.0, 900.0))]);
        gallery.clamp();
        assert_eq!((gallery.tree, gallery.side), (TREE_LEAST, SIDEBAR_LEAST), "never below their least");
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
        assert_eq!(
            gallery.panel()[..4],
            ["IconButton", "调用  toggleSaved", "▸ 详情", "便签 · 还没说过什么"]
        );
        assert_eq!(
            gallery.foot().map(|foot| (foot.empty, foot.send)),
            Some(("就这里说点什么…".into(), "发送".into()))
        );
        assert!(click_panel_line(&mut gallery, "▸ 详情"));
        assert_eq!(gallery.panel()[3..5], ["标识  nav-save", "宽 390  编号 3 · 框 334, 8 · 40 × 40"]);
        gallery.chord("c");
        assert!(gallery.panel()[0].starts_with("已复制  "), "{:?}", gallery.panel());
        // Nothing the window says itself is in English: what is left is the
        // description's own names.
        let english = |line: &String| {
            line.split(|c: char| !c.is_ascii_alphabetic())
                .any(|word| ["the", "to", "on", "in", "at", "a", "of", "it"].contains(&word))
        };
        gallery.chord("c");
        let said: Vec<String> =
            gallery.panel().into_iter().filter(|line| !line.starts_with("已复制")).collect();
        assert!(!said.iter().any(english), "{said:?}");
    }

    #[test]
    fn with_lang_ja_the_sidebar_and_the_title_say_it_in_japanese_set_in_japanese_fonts() {
        let mut gallery = gallery();
        gallery.speak(Lang::Ja).unwrap();
        assert!(gallery.title().starts_with("underdraw ギャラリー — "), "{}", gallery.title());
        assert_eq!(
            gallery.typesetter.families(),
            "system-ui, Hiragino Sans, Yu Gothic, Noto Sans CJK JP, sans-serif"
        );
        assert_eq!(gallery.sidebar_cards()[0].title, "何も選ばれていません");
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.press_at(on_node(&gallery, 0, "nav-save"));
        assert_eq!(
            gallery.panel()[..4],
            ["IconButton", "呼び出し  toggleSaved", "▸ 詳細", "メモ · まだ何も話していません"]
        );
        assert_eq!(gallery.foot().map(|foot| foot.send), Some("送信".into()));
        assert!(click_panel_line(&mut gallery, "▸ 詳細"));
        assert_eq!(gallery.panel()[3..5], ["ID  nav-save", "幅 390  番号 3 · 枠 334, 8 · 40 × 40"]);
        gallery.chord("c");
        assert!(gallery.panel()[0].starts_with("コピーしました  "), "{:?}", gallery.panel());
    }

    #[test]
    fn every_language_sets_its_words_in_fonts_of_its_own() {
        let mut gallery = gallery();
        for lang in Lang::all() {
            gallery.speak(lang).unwrap();
            assert_eq!(gallery.lang, lang);
        }
        gallery.speak(Lang::Zh).unwrap();
        assert!(gallery.typesetter.families().contains("Noto Sans CJK SC"), "Chinese stays Simplified");
    }

    #[test]
    fn a_description_with_faults_shows_them() {
        let root = std::env::temp_dir().join(format!("underdraw-window-faults-{}", std::process::id()));
        std::fs::create_dir_all(root.join("broken")).unwrap();
        std::fs::write(root.join("broken").join(crate::SCREEN), "{}").unwrap();
        let mut gallery = opened(&root, (1440.0, 900.0));
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
            std::fs::copy(drafts().join("todo").join(file), root.join("todo").join(file)).unwrap();
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
        let mut gallery = opened(&root, (1440.0, 120.0));
        let typing = state_of(&gallery, "typing");
        gallery.pick(typing);
        let before = heights(&gallery);
        gallery.move_to((gallery.tree + 100.0, 100.0));
        gallery.wheel_by(0.0, 40.0);
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
        assert_eq!(gallery.line, Some(typing), "the same state, though a state was added above it");
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
        assert_eq!(gallery.line, Some(todo), "a state gone: its description's overview");
        assert!(matches!(gallery.picture, Picture::Overview(_)));
        assert_eq!(gallery.scroll, 0.0, "from its top");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_description_added_or_removed_comes_into_the_tree_or_leaves_it() {
        let root = todo_root("added");
        let mut gallery = opened(&root, (1440.0, 900.0));
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
        assert_eq!(gallery.line, Some(typing), "still the state shown");
        assert_eq!(gallery.lines[gallery.line.unwrap()].row.description(), Some(&*root.join("todo")));

        std::fs::remove_dir_all(root.join("todo")).unwrap();
        gallery.reload();
        let names: Vec<&str> = gallery
            .lines
            .iter()
            .filter(|line| line.state.is_none())
            .map(|line| line.row.name.as_str())
            .collect();
        assert_eq!(names[1..], ["a-flow", "sign-in"], "the removed one has left");
        assert_eq!(gallery.line, Some(2), "its description gone: the first description's overview");
        assert_eq!(overview(&gallery).0[0], "default");

        std::fs::remove_dir_all(root.join("a-flow")).unwrap();
        gallery.reload();
        assert!(gallery.lines.is_empty(), "nothing left to list");
        assert_eq!(gallery.line, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_description_that_gains_faults_shows_them_and_its_picture_once_fixed() {
        let root = todo_root("gains");
        let mut gallery = opened(&root, (1440.0, 900.0));
        let typing = state_of(&gallery, "typing");
        gallery.pick(typing);
        let fine = todo_screen(&root);

        let mut broken = fine.clone();
        broken["data"]["stray"] = json!("nonsense");
        write_todo_screen(&root, &broken);
        gallery.reload();
        assert!(!gallery.lines[1].row.faults.is_empty(), "the tree marks it");
        assert_eq!(gallery.line, Some(typing), "the same state");
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
        let todo = drafts().join("todo");
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
        gallery.move_to(on_node(gallery, 0, id));
        gallery.feed([Input::Modifiers(ModifiersState::SUPER)]);
        let changed = gallery.press_here();
        gallery.feed([Input::Modifiers(ModifiersState::empty())]);
        changed
    }

    #[test]
    fn acting_on_a_node_follows_its_transition_and_a_plain_click_only_picks_it() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        assert!(gallery.press_at(on_node(&gallery, 0, "order-0-cancel")));
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
        assert_eq!(gallery.line, Some(export + 1), "the export screen's first state");
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
        gallery.press_here();
        assert!(
            gallery.history.is_empty() && gallery.acted.is_none(),
            "picking in the tree starts a new path"
        );
    }

    #[test]
    fn choosing_where_nothing_declared_should_go_writes_a_note_and_never_the_description() {
        let root = root_of("propose", &[("orders", "orders"), ("export", "export")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
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
            written[0].messages[0].says,
            "cd-keep.tap in cancel-unpaid should go to default: declare the transition"
        );
        let listed = notes::notes(&root, None, None, false).unwrap();
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
                let from = drafts().join(draft).join(file);
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
        let mut gallery = opened(&root, (1440.0, 900.0));
        let shop = gallery.lines.iter().position(|line| line.row.name == "shop").unwrap();
        over_row(&mut gallery, shop);
        assert!(gallery.press_here(), "a directory can be picked");
        assert_eq!(gallery.line, Some(shop));
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
        let mut gallery = opened(&root, (1440.0, 900.0));
        showing(&mut gallery, 0);
        let flow = flow_shown(&gallery);
        let place =
            flow.map.places.iter().find(|p| p.screen == Screen::Drawn(root.join("shop/listing"))).unwrap();
        let ((left, top), zoom) = (gallery.flow_origin(), gallery.zoom(gallery.pane()));
        let undrawn = flow.map.places.iter().find(|p| matches!(p.screen, Screen::Undrawn(_))).unwrap();
        let on = |p: &flow::Place| (left + (p.x + 20.0) * zoom, top + (p.y + 40.0) * zoom);
        let (on_listing, on_undrawn) = (on(place), on(undrawn));
        assert!(!gallery.press_at(on_undrawn), "a screen not drawn yet has no description to pick");
        assert!(gallery.press_at(on_listing), "its box picks the description");
        let listing = line_of(&gallery, "listing");
        assert_eq!(gallery.line, Some(listing), "the description itself");
        assert_eq!(overview(&gallery).0[0], "default", "showing its overview");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_description_with_faults_is_a_box_marked_faulty_without_a_picture() {
        let root = root_of("flow-faulty", &[("todo", "flow/todo")]);
        std::fs::create_dir_all(root.join("flow/broken")).unwrap();
        std::fs::write(root.join("flow/broken").join(crate::SCREEN), "{}").unwrap();
        let mut gallery = opened(&root, (1440.0, 900.0));
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
        let mut gallery = opened(&root, (1440.0, 900.0));
        showing(&mut gallery, 1);
        let undrawn = |gallery: &Gallery| {
            flow_shown(gallery).tiles.iter().filter(|c| matches!(c, Tile::Undrawn)).count()
        };
        assert_eq!(undrawn(&gallery), 2, "OrderDetail and ExportDocuments are not drawn yet");
        let export = root_of("flow-reload-export", &[("export", "export")]);
        std::fs::rename(export.join("export"), root.join("app/export")).unwrap();
        std::fs::remove_dir_all(export).unwrap();
        gallery.reload();
        assert_eq!(gallery.line, Some(1), "the directory is still picked");
        assert_eq!(undrawn(&gallery), 1, "ExportDocuments is drawn now");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Keys pressed in the window, ⌘ not held: each a letter, or one of
    /// Enter, Shift+Enter (written "⇧Enter"), Backspace and Esc.
    fn press_keys(gallery: &mut Gallery, keys: &[&str]) {
        for key in keys {
            let named = |named| Key::Named(named);
            let unshifted = key.strip_prefix('⇧').filter(|rest| !rest.is_empty());
            if unshifted.is_some() {
                gallery.feed([Input::Modifiers(ModifiersState::SHIFT)]);
            }
            let (pressed, text) = match unshifted.unwrap_or(key) {
                "Enter" => (named(NamedKey::Enter), Some("\r")),
                "Home" => (named(NamedKey::Home), None),
                "End" => (named(NamedKey::End), None),
                "↑" => (named(NamedKey::ArrowUp), None),
                "↓" => (named(NamedKey::ArrowDown), None),
                "Backspace" => (named(NamedKey::Backspace), None),
                "Delete" => (named(NamedKey::Delete), None),
                "Esc" => (named(NamedKey::Escape), None),
                " " => (named(NamedKey::Space), Some(" ")),
                "→" => (named(NamedKey::ArrowRight), None),
                "←" => (named(NamedKey::ArrowLeft), None),
                letter => (Key::Character(letter.into()), Some(letter)),
            };
            gallery.feed([key_of(pressed, text), Input::Modifiers(ModifiersState::empty())]);
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
        let (cards, foot) = (gallery.sidebar_cards(), gallery.foot());
        let (at, scrolled) = ((gallery.pane_right(), gallery.side), (-gallery.side_scroll, gallery.size.1));
        let laid = sidebar::lay_out(&cards, foot.as_ref(), at, scrolled, &mut gallery.typesetter);
        let Some(r) = laid.buttons.iter().find(|(_, says, _)| *says == text).map(|(r, _, _)| *r) else {
            panic!("no button {text:?}: {:?}", gallery.panel())
        };
        gallery.press_at((f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0)))
    }

    /// What the conversation card says: its title, then each message.
    fn talk_shown(gallery: &Gallery) -> Vec<String> {
        let card = gallery.conversation_card().expect("a conversation is shown");
        std::iter::once(card.title).chain(card.blocks.iter().flat_map(Block::says)).collect()
    }

    /// Node `id` in picture `page` picked, and `says` written about it in
    /// the box at the sidebar's foot, Enter sending it.
    fn say_on(gallery: &mut Gallery, page: usize, id: &str, says: &str) {
        gallery.cursor = on_node(gallery, page, id);
        gallery.press();
        gallery.release();
        type_text(gallery, says);
        press_keys(gallery, &["Enter"]);
    }

    #[test]
    fn talking_on_a_node_writes_its_note_and_shows_it_at_once() {
        let root = root_of("notes-write", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        let dir = root.join("orders");
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture") };
        let id = pages[1].nodes.iter().map(|n| &n.id).find(|id| id.ends_with("-cancel")).unwrap().clone();
        gallery.cursor = on_node(&gallery, 1, &id);
        gallery.press();
        assert_eq!(
            talk_shown(&gallery),
            [
                "note · nothing said yet",
                "Say below what should change: Enter sends, Shift+Enter breaks the line."
            ]
        );
        let foot = gallery.foot().expect("a box at the sidebar's foot");
        assert_eq!((foot.empty.as_str(), foot.send.as_str()), ("say something about this…", "Send"));
        assert_eq!(foot.does, None, "Send cannot send while the box is empty");
        press_keys(&mut gallery, &["Enter", " ", "Enter"]);
        assert!(!dir.join(notes::NOTES).exists(), "Enter sends nothing from an empty box");
        press_keys(&mut gallery, &["Backspace"]);
        type_text(&mut gallery, "价格太长");
        press_keys(&mut gallery, &["⇧Enter", "a", "x", "Backspace", " ", "b"]);
        assert_eq!(gallery.talk.text(), "价格太长\na b", "Shift+Enter breaks a line");
        assert_eq!(gallery.foot().unwrap().does, Some(Do::Send));
        assert!(!dir.join(notes::NOTES).exists(), "nothing is written while typing");
        press_keys(&mut gallery, &["Enter"]);
        assert!(gallery.talk.is_blank(), "Enter sends, and empties the box");
        assert_eq!(
            talk_shown(&gallery),
            ["note N1 · waiting on the agent", "价格太长\na b"],
            "shown at once"
        );

        let written = notes::read(&dir).unwrap();
        assert_eq!(written.len(), 1);
        let note = &written[0];
        assert_eq!((note.id.as_str(), note.stands()), ("N1", Stands::Waiting));
        let On::Node { state, node } = &note.on else { panic!("not on a node: {note:?}") };
        assert_eq!(*state, State::Fixture("default".into()));
        let reference = gallery.reference().unwrap();
        assert_eq!(note.reference(&dir), reference, "pinned as ⌘C references it");
        assert_eq!(node.origin.written, "order-{index}-cancel");
        assert_eq!(node.width, 1120);

        type_text(&mut gallery, "and the date");
        assert!(click_panel_line(&mut gallery, "Send"), "Send sends too");
        let written = notes::read(&dir).unwrap();
        assert_eq!(written.len(), 1, "the node has one note, said more in");
        let said: Vec<&str> = written[0].messages.iter().map(|said| said.says.as_str()).collect();
        assert_eq!(said, ["价格太长\na b", "and the date"]);
        assert_eq!(talk_shown(&gallery)[1..], ["价格太长\na b", "and the date"], "newest at the bottom");

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
            gallery.waiting_notes(row, state.as_ref())
        };
        assert_eq!((count(&gallery, orders), count(&gallery, default), count(&gallery, loading)), (1, 1, 0));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_key_needs_holding_to_speak_and_no_drag_picks_a_region() {
        let root = root_of("notes-no-mode", &[("orders", "orders")]);
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        show(&mut gallery, "orders", "default");
        press_keys(&mut gallery, &["n"]);
        assert_eq!(gallery.talk.text(), "n", "N is a letter typed in the box, no mode");
        assert!(!gallery.panel().iter().any(|line| line.contains("write a note")), "{:?}", gallery.panel());
        press_keys(&mut gallery, &["Backspace"]);
        gallery.cursor = on_node(&gallery, 0, "nav-export");
        gallery.press();
        let picked = gallery.reference();
        gallery.cursor.0 += 200.0;
        gallery.cursor.1 += 120.0;
        assert!(!gallery.release(), "a drag over the picture picks nothing more");
        assert_eq!(gallery.reference(), picked, "the node pressed stays picked");
        press_keys(&mut gallery, &["Esc"]);
        assert_eq!(picked_node(&gallery), None, "Esc lets it go");
        assert_eq!(
            gallery.reference().map(|on| on.on),
            Some(On::State { state: State::Fixture("default".into()) }),
            "and the conversation is the state's"
        );
        assert!(!root.join("orders").join(notes::NOTES).exists(), "nothing written");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_answered_note_turns_green_shows_its_reply_and_is_never_closed() {
        let root = root_of("notes-answered", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        for (id, says) in [("nav-export", "rename it"), ("order-0-cancel", "too red")] {
            say_on(&mut gallery, 0, id, says);
        }
        let dir = root.join("orders");
        notes::answer(&dir, "N1", "renamed it Export").unwrap();
        gallery.reload();
        let inks: Vec<(String, Ink)> =
            gallery.pins().into_iter().filter(|pin| !pin.faint).map(|pin| (pin.id, pin.ink)).collect();
        assert_eq!(inks, [("N1".to_string(), Ink::Answered), ("N2".to_string(), Ink::Note)], "drawn again");

        assert!(gallery.move_to(on_pin(&gallery, "N1")), "the pointer over an answered pin");
        let texts: Vec<String> = gallery
            .reply_shown()
            .into_iter()
            .filter_map(|d| if let Draw::Text { text, .. } = d { Some(text) } else { None })
            .collect();
        assert_eq!(texts, ["N1 answered: renamed it Export"]);
        assert!(!gallery.move_to(gallery.cursor), "still over it: nothing changed");
        assert!(
            gallery.move_to(on_pin(&gallery, "N2")) && gallery.reply_shown().is_empty(),
            "a note waiting on the agent has no reply to show"
        );

        press_keys(&mut gallery, &["Esc"]);
        gallery.cursor = on_pin(&gallery, "N1");
        assert!(gallery.press(), "clicking a pin picks what its note is on");
        assert_eq!(picked_node(&gallery), Some("nav-export"));
        assert_eq!(
            talk_shown(&gallery),
            ["note N1 · answered", "rename it", "renamed it Export"],
            "the person's words, then the agent's reply"
        );
        let panel = gallery.panel();
        for gone in ["close", "not yet…", "reopen…", "reopen on the whole state…"] {
            assert!(
                !panel.iter().any(|line| line == gone),
                "a note is never closed: no {gone:?} in {panel:?}"
            );
        }
        gallery.cursor = on_pin(&gallery, "N2");
        gallery.press();
        assert_eq!(talk_shown(&gallery), ["note N2 · waiting on the agent", "too red"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_note_whose_node_is_renamed_joins_its_states_note_and_is_shown_there() {
        let root = root_of("notes-gone", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        say_on(&mut gallery, 0, "nav-export", "say export what");
        let dir = root.join("orders");
        notes::answer(&dir, "N1", "it says Export").unwrap();
        let before = std::fs::read_to_string(dir.join(notes::NOTES)).unwrap();

        let file = dir.join(crate::SCREEN);
        let screen =
            std::fs::read_to_string(&file).unwrap().replace("\"id\": \"export\"", "\"id\": \"download\"");
        std::fs::write(&file, screen).unwrap();
        gallery.reload();
        assert_ne!(std::fs::read_to_string(dir.join(notes::NOTES)).unwrap(), before, "it joined");
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.on, On::State { state: State::Fixture("default".into()) });
        assert_eq!((note.stands(), note.messages.len()), (Stands::Answered, 3), "never closed: {note:?}");
        assert_eq!(note.messages[0].says, "on `nav-export`, now gone");
        gallery.press_at(on_pin(&gallery, "N1"));
        assert_eq!(picked_node(&gallery), None, "its pin picks the whole state");
        assert_eq!(
            talk_shown(&gallery),
            ["note N1 · answered", "on `nav-export`, now gone", "say export what", "it says Export"]
        );
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
        let mut gallery = opened(&root, (1440.0, 900.0));
        let checkout = line_of(&gallery, "checkout");
        let states = gallery.lines[checkout + 1..].iter().take_while(|line| line.state.is_some()).count();
        let shown = (1..=states).find(|offset| {
            showing(&mut gallery, checkout + offset);
            let Picture::Pages(pages) = &gallery.picture else { return false };
            pages[0].has("card-number")
        });
        assert!(shown.is_some(), "a state of checkout shows the card form");
        gallery.press_at(on_guess(&gallery, 0, "card-number"));
        assert_eq!(picked_node(&gallery), Some("card-number"), "a click on its \"?\" picks it");
        let panel = gallery.panel();
        for part in [
            "Card fields are drawn here",
            "or else  the real build may use the payment provider's hosted fields",
            "✓ right, keep it",
            "✗ wrong…",
        ] {
            assert!(panel.iter().any(|line| line == part), "{part:?} in {panel:?}");
        }

        assert!(click_panel_line(&mut gallery, "✗ wrong…"));
        let panel = gallery.panel();
        assert!(panel.iter().any(|line| line == "what is right?"), "a field in place: {panel:?}");
        assert!(!panel.iter().any(|line| line == "✓ right, keep it"), "in place of the buttons");
        press_keys(&mut gallery, &["Enter", " ", "Enter"]);
        assert!(gallery.draft.is_some(), "the ✗ is not sent empty");
        assert!(gallery.panel().iter().any(|line| line.starts_with("not sent empty")));
        let dir = root.join("checkout");
        assert!(!dir.join(notes::NOTES).exists(), "nothing written");
        press_keys(&mut gallery, &["Backspace"]);
        type_text(&mut gallery, "use the hosted fields");
        press_keys(&mut gallery, &["Enter"]);
        let wrong = notes::read(&dir).unwrap().remove(0);
        assert_eq!(wrong.stands(), Stands::Waiting);
        assert_eq!(wrong.messages[0].says, "use the hosted fields");
        let answers = wrong.answers.clone().unwrap();
        assert_eq!((answers.answer, answers.guess), (Answer::Wrong, Guess::Author));
        assert!(answers.assumed.starts_with("Card fields are drawn here;"));

        assert!(click_panel_line(&mut gallery, "✓ right, keep it"));
        assert!(gallery.panel()[0].starts_with("✓ right on card-number in "), "{:?}", gallery.panel());
        assert!(!gallery.panel().iter().any(|line| line == "✓ right, keep it"), "one ✓ at a time");
        assert!(click_panel_line(&mut gallery, "undo"));
        assert!(!gallery.confirm(Instant::now() + UNDO * 2), "undone: nothing waits");
        assert_eq!(notes::read(&dir).unwrap().len(), 1, "an undone ✓ leaves notes.json as it was");

        assert!(click_panel_line(&mut gallery, "✓ right, keep it"));
        assert!(!gallery.confirm(Instant::now() + UNDO - Duration::from_secs(1)), "it can still be undone");
        assert_eq!(notes::read(&dir).unwrap().len(), 1, "not written while it can be undone");
        assert!(gallery.confirm(Instant::now() + UNDO));
        assert!(!gallery.confirm(Instant::now() + UNDO * 2), "written once");
        let right = notes::read(&dir).unwrap().remove(1);
        assert_eq!(right.answers.as_ref().map(|answers| answers.answer), Some(Answer::Right));
        assert_eq!(right.messages[0].says, notes::RIGHT, "✓ needs no line");
        assert!(gallery.panel().iter().any(|line| line == "N1 answers it ✗ wrong · waiting on the agent"));

        notes::answer(&dir, "N1", "kept them drawn: hosted fields are for the build").unwrap();
        let file = dir.join(crate::SCREEN);
        let screen = std::fs::read_to_string(&file).unwrap().replace(
            ",\n                        \"assumed\": \"Card fields are drawn here; the real build may use the payment provider's hosted fields\"",
            "",
        );
        assert!(!screen.contains("Card fields are drawn here"), "the guess taken out");
        std::fs::write(&file, screen).unwrap();
        gallery.reload();
        let notes = notes::read(&dir).unwrap();
        assert_eq!(
            notes[1].stands(),
            Stands::Waiting,
            "the ✓ is never closed: it waits on the agent's answer"
        );
        assert_eq!(notes[0].stands(), Stands::Answered, "the agent spoke last in the ✗");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_note_is_edited_anywhere_selected_by_keys_or_a_drag_and_pasted_into() {
        let root = root_of("notes-edit", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        let written = |gallery: &Gallery| gallery.talk.text();
        type_text(&mut gallery, "ac");
        press_keys(&mut gallery, &["←", "b", "Home", ">", "End", "!"]);
        assert_eq!(written(&gallery), ">abc!", "typed where the caret is");
        type_text(&mut gallery, " too red");
        press_keys(&mut gallery, &["⇧←", "⇧←", "⇧←"]);
        let selected = gallery.talk.shown().selected;
        assert_eq!(selected.len(), 3, "the selection is drawn");
        type_text(&mut gallery, "blue");
        assert_eq!(written(&gallery), ">abc! too blue", "typing replaces what is selected");
        press_keys(&mut gallery, &["Home", "⇧→", "⇧→", "⇧→", "⇧→", "⇧→"]);
        assert!(gallery.paste("abc"), "⌘V pastes in place of what is selected");
        assert_eq!(written(&gallery), "abc too blue");

        let field = gallery.field_rect().expect("the box is at the sidebar's foot");
        let middle = f64::from(field.y) + f64::from(field.height) / 2.0;
        gallery.cursor = (f64::from(field.x) + 1.0, middle);
        assert!(gallery.press(), "a press on the field's text puts the caret there");
        gallery.cursor = (f64::from(field.x + field.width) - 1.0, middle);
        assert!(gallery.drag_field(), "a drag selects");
        gallery.release();
        assert!(!gallery.drag_field(), "released, the pointer selects no more");
        assert_eq!(gallery.talk.selected(), Some("abc too blue"));
        type_text(&mut gallery, "x");
        press_keys(&mut gallery, &["Enter"]);
        let note = notes::read(&root.join("orders")).unwrap().remove(0);
        assert_eq!(note.messages[0].says, "x");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_note_typed_with_an_input_method_shows_what_it_composes_and_saves_what_it_committed() {
        let root = root_of("notes-ime", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        gallery.cursor = on_node(&gallery, 0, "nav-export");
        gallery.press();
        type_text(&mut gallery, "ab");
        press_keys(&mut gallery, &["←"]);
        assert!(gallery.ime(Ime::Preedit("ni hao".to_string(), Some((6, 6)))));
        let shown = gallery.talk.shown();
        assert_eq!(shown.text, "ani haob", "composed at the caret");
        assert_eq!(&shown.text[shown.composing.clone()], "ni hao", "drawn underlined");
        let foot = gallery.foot();
        let (at, bottom) = ((gallery.pane_right(), gallery.side), gallery.size.1);
        let laid = sidebar::lay_out(&[], foot.as_ref(), at, (0.0, bottom), &mut gallery.typesetter);
        let underlined =
            laid.foot_draws.iter().filter(|draw| matches!(draw, Draw::Line { ink: Ink::Text, .. })).count();
        assert_eq!(underlined, 1, "one line under what is composed");
        assert!(laid.caret.is_some(), "the input method is shown where the caret is");
        press_keys(&mut gallery, &["Enter", "Backspace"]);
        assert_eq!(gallery.talk.shown().text, "ani haob", "while it composes, its keys are its own");
        assert!(!root.join("orders").join(notes::NOTES).exists(), "the Enter that confirms it sends nothing");
        assert!(gallery.ime(Ime::Commit("你好".to_string())));
        press_keys(&mut gallery, &["→", "Delete", "Backspace", "←"]);
        type_text(&mut gallery, "!");
        assert_eq!(gallery.talk.text(), "a你!好", "the caret moved, deleting where it is");
        assert!(gallery.select_all());
        type_text(&mut gallery, "价格");
        assert!(gallery.compose(Ime::Preedit("tai".to_string(), None)));
        assert!(gallery.compose(Ime::Commit("太".to_string())));
        press_keys(&mut gallery, &["Enter"]);
        let note = notes::read(&root.join("orders")).unwrap().remove(0);
        assert_eq!(note.messages[0].says, "价格太", "what was committed, never what was composing");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_confirmation_waiting_is_written_when_the_window_closes() {
        let root = root_of("assume-close", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        gallery.press_at(on_guess(&gallery, 0, "nav-export"));
        assert!(click_panel_line(&mut gallery, "✓ right, keep it"));
        let dir = root.join("orders");
        assert!(!dir.join(notes::NOTES).exists(), "it can still be undone");
        gallery.leave();
        let notes = notes::read(&dir).unwrap();
        assert_eq!(notes.len(), 1, "closing the window writes it");
        assert_eq!(notes[0].answers.as_ref().map(|answers| answers.answer), Some(Answer::Right));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_confirmed_screen_not_drawn_yet_is_noted_with_its_guess_and_never_closed() {
        let root = root_of("assume-undrawn", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        gallery.press_at(on_guess(&gallery, 0, "nav-export"));
        let panel = gallery.panel();
        assert!(panel.iter().any(|line| line.starts_with("This navigates to ExportDocuments")), "{panel:?}");
        assert!(
            panel.iter().any(|line| line.starts_with("or else  it could instead be a misspelt name")),
            "{panel:?}"
        );
        assert!(click_panel_line(&mut gallery, "✓ right, keep it"));
        assert!(gallery.confirm(Instant::now() + UNDO));
        let dir = root.join("orders");
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.answers.unwrap().guess, Guess::Undrawn(vec!["ExportDocuments".into()]));

        let drawn = root_of("assume-undrawn-export", &[("export", "export")]);
        std::fs::rename(drawn.join("export"), root.join("export")).unwrap();
        gallery.reload();
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.stands(), Stands::Waiting, "drawn now, but the agent has not answered: it waits");
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(drawn).unwrap();
    }

    #[test]
    fn a_note_shows_as_a_faint_dot_in_another_state_that_holds_its_node() {
        let root = root_of("notes-dot", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        say_on(&mut gallery, 0, "order-0-cancel", "too red");
        show(&mut gallery, "orders", "cancel-dialog");
        let shown = pins_shown(&gallery);
        assert!(!shown.is_empty() && shown.iter().all(|(_, _, _, faint)| *faint), "{shown:?}");
        let orders = line_of(&gallery, "orders");
        let dialog = state_of_in(&gallery, orders, "cancel-dialog");
        let Line { row, state, .. } = &gallery.lines[dialog];
        assert_eq!(gallery.waiting_notes(row, state.as_ref()), 0, "a state counts the notes written in it");
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

    /// What is at `at` picked with a click, and `text` written about it in
    /// the box at the sidebar's foot, Enter sending it.
    fn say_at(gallery: &mut Gallery, at: (f64, f64), text: &str) {
        gallery.cursor = at;
        gallery.press();
        type_text(gallery, text);
        press_keys(gallery, &["Enter"]);
    }

    #[test]
    fn with_nothing_picked_the_conversation_is_the_whole_states() {
        let root = root_of("notes-state", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        let panel = gallery.panel();
        assert!(panel.iter().any(|line| line == "default · the whole state"), "the card names it: {panel:?}");
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet");
        let (left, top) = gallery.origins()[1];
        say_at(&mut gallery, (left + 4.0, top - CAPTION / 2.0), "too busy");
        let dir = root.join("orders");
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.on, On::State { state: State::Fixture("default".into()) });
        assert_eq!(talk_shown(&gallery), ["note N1 · waiting on the agent", "too busy"], "shown at once");
        let pins = pins_shown(&gallery);
        assert_eq!(pins, [("N1".to_string(), 0, Ink::Note, false)], "one pin, by the first caption");
        gallery.cursor = on_node(&gallery, 0, "nav-export");
        gallery.press();
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet", "the node's own conversation");
        gallery.cursor = on_pin(&gallery, "N1");
        assert!(gallery.press(), "clicking its pin picks the whole state");
        assert_eq!(picked_node(&gallery), None);
        assert_eq!(talk_shown(&gallery)[0], "note N1 · waiting on the agent");
        let orders = line_of(&gallery, "orders");
        let default = state_of_in(&gallery, orders, "default");
        let Line { row, state, .. } = &gallery.lines[default];
        assert_eq!(gallery.waiting_notes(row, state.as_ref()), 1, "counted on its state");

        showing(&mut gallery, orders);
        let pins: Vec<String> = gallery.pins().into_iter().map(|pin| pin.id).collect();
        assert_eq!(pins, ["N1"], "pinned on its state in the overview too");
        assert!(gallery.foot().is_none() && gallery.conversation_card().is_none(), "nothing to talk about");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_transition_arrow_is_picked_copied_and_talked_about_in_the_overview() {
        let root = root_of("notes-transition", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        let orders = line_of(&gallery, "orders");
        showing(&mut gallery, orders);
        let dir = root.join("orders");
        let screen: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(crate::SCREEN)).unwrap()).unwrap();
        let declared = screen["transitions"].as_array().unwrap().clone();

        assert!(gallery.press_at(on_arrow(&gallery, 0)), "a click on its label picks the arrow");
        assert_eq!(gallery.arrow(), Some(0));
        let reference = gallery.reference().unwrap();
        let On::Transition { transition } = &reference.on else { panic!("not a transition: {reference:?}") };
        assert_eq!(serde_json::to_value(transition).unwrap(), declared[0], "as screen.json writes it");
        let line = reference.line();
        assert!(line.starts_with(&format!("{} · transition from ", dir.display())), "{line}");
        assert!(gallery.foot().is_some(), "a box to talk about it");
        gallery.chord("c");
        assert!(gallery.panel()[0].ends_with(&line), "⌘C copies it: {:?}", gallery.panel());

        let last = declared.len() - 1;
        let at = on_arrow(&gallery, last);
        say_at(&mut gallery, at, "should it say why?");
        let note = notes::read(&dir).unwrap().remove(0);
        let On::Transition { transition } = &note.on else { panic!("not a transition: {note:?}") };
        assert_eq!(serde_json::to_value(transition).unwrap(), declared[last]);
        assert_eq!(talk_shown(&gallery), ["note N1 · waiting on the agent", "should it say why?"]);
        assert_eq!(gallery.pins().len(), 1, "a pin beside its arrow");
        press_keys(&mut gallery, &["Esc"]);
        assert_eq!(gallery.arrow(), None);
        gallery.cursor = on_pin(&gallery, "N1");
        assert!(gallery.press());
        assert_eq!(gallery.arrow(), Some(last), "its pin picks its arrow");
        assert_eq!(talk_shown(&gallery)[0], "note N1 · waiting on the agent");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_navigate_arrow_is_picked_copied_and_talked_about_in_the_flow() {
        let root = root_of("notes-navigate", &[("orders", "app/orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        showing(&mut gallery, 0);
        assert!(gallery.foot().is_none(), "nothing picked in a flow: nothing to talk about");
        let dir = root.join("app/orders");
        let link = flow_shown(&gallery).map.arrows[0].link.clone();

        assert!(gallery.press_at(on_arrow(&gallery, 0)), "a click on its name picks the arrow");
        let line = gallery.reference().unwrap().line();
        assert_eq!(
            line,
            format!("{} · navigate from {} on {} to {}", dir.display(), link.from, link.node, link.to)
        );
        gallery.chord("c");
        assert!(gallery.panel()[0].ends_with(&line), "⌘C copies it: {:?}", gallery.panel());

        let at = on_arrow(&gallery, 0);
        say_at(&mut gallery, at, "is this the right screen?");
        let file: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(notes::NOTES)).unwrap()).unwrap();
        assert_eq!(
            file["notes"][0]["navigate"],
            json!({ "from": link.from, "node": link.node, "to": link.to }),
            "kept by the description it is written in"
        );
        assert_eq!(talk_shown(&gallery), ["note N1 · waiting on the agent", "is this the right screen?"]);
        assert_eq!(gallery.pins().len(), 1, "a pin under its arrow");
        gallery.cursor = on_pin(&gallery, "N1");
        assert!(!gallery.press() || gallery.arrow() == Some(0), "its pin keeps its arrow picked");
        let Line { row, state, .. } = &gallery.lines[line_of(&gallery, "orders")];
        assert_eq!(gallery.waiting_notes(row, state.as_ref()), 1, "counted on its description");
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
        assert!(gallery.press_here(), "the journey is picked");
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
        assert!(
            !gallery.feed([key_of(Key::Named(NamedKey::ArrowRight), None)]).redraw,
            "nothing past the last"
        );

        press_keys(&mut gallery, &["←", "←"]);
        assert_eq!(played(&gallery).0, 1, "back to the first step");
        press_keys(&mut gallery, &["a", "→", "←", "b"]);
        assert_eq!(
            played(&gallery).0,
            1,
            "while the box holds something, → and ← move its caret, not the journey"
        );
        assert_eq!(gallery.talk.text(), "ba");
        assert!(gallery.select_all());
        press_keys(&mut gallery, &["Backspace"]);
        assert!(click_panel_line(&mut gallery, "→ next step"));
        assert_eq!(played(&gallery).0, 2);
        assert!(click_panel_line(&mut gallery, "← previous step"));
        assert_eq!(played(&gallery).0, 1);
        assert_eq!(gallery.line, Some(journey), "the journey stays picked as it plays");

        let orders = line_of(&gallery, "orders");
        over_row(&mut gallery, orders);
        gallery.press_here();
        assert!(gallery.playing.is_none(), "picking another line stops playing");
    }

    /// The line of the description in `dir`, a directory of the drafts.
    fn description_in(gallery: &Gallery, dir: &str) -> usize {
        let dir = drafts().join(dir);
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
        assert!(
            gallery.press_at((f64::from(open.x) + 4.0, f64::from(open.y) + 4.0)),
            "picking the mark plays the journey"
        );
        assert_eq!(gallery.line, Some(journey_of(&gallery, "cancel-an-order")));
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
        let shop = drafts().join("shop");
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
        let mut gallery = opened(&root, (1440.0, 900.0));
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
        say_on(&mut gallery, 0, "order-row-cancel", "say which order");
        let dir = root.join("components/OrderRow");
        let note = notes::read(&dir).unwrap().remove(0);
        let On::Node { node, .. } = &note.on else { panic!("not on a node: {note:?}") };
        assert_eq!(node.origin.written, "order-row-cancel");
        let listed = notes::notes(&root, None, None, false).unwrap();
        assert_eq!(listed["notes"][0]["description"], "components/OrderRow", "{listed}");

        let file = root.join("components/OrderRow.json");
        let renamed = std::fs::read_to_string(&file).unwrap().replace(r#""id": "cancel""#, r#""id": "drop""#);
        std::fs::write(&file, renamed).unwrap();
        gallery.reload();
        let joined = notes::read(&dir).unwrap();
        assert_eq!(joined.len(), 1, "its node renamed, the note is never closed: {joined:?}");
        assert_eq!(joined[0].id, note.id, "it keeps its id");
        assert_eq!(
            joined[0].on,
            On::State { state: State::Fixture("pending".into()) },
            "the state's note now"
        );
        let said: Vec<(notes::By, &str)> =
            joined[0].messages.iter().map(|said| (said.by, said.says.as_str())).collect();
        assert_eq!(
            said,
            [
                (notes::By::Underdraw, "on `order-row-cancel`, now gone"),
                (notes::By::Person, "say which order")
            ]
        );
        assert_eq!(gallery.notes[&dir], joined, "the gallery shows it joined");
        gallery.reload();
        assert_eq!(notes::read(&dir).unwrap(), joined, "read again, nothing moves twice");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_broken_journey_shows_its_fault_and_is_played_once_fixed() {
        let root = shop("broken-journey");
        let order = root.join("order").join(crate::SCREEN);
        let walks = std::fs::read_to_string(&order).unwrap();
        std::fs::write(&order, walks.replacen(r#""to": "cancelled""#, r#""to": "shipped""#, 1)).unwrap();
        let mut gallery = opened(&root, (1440.0, 900.0));
        let journey = journey_of(&gallery, "cancel-an-order");
        assert_eq!(gallery.lines[journey].row.depth, 1, "listed under the root, one in from its line");
        over_row(&mut gallery, journey);
        assert!(gallery.press_here());
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
        assert_eq!(gallery.line, Some(journey), "the journey stays picked");
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
                let blocks = card.blocks.iter().flat_map(Block::says).collect::<Vec<_>>();
                std::iter::once(card.title).chain(blocks)
            })
            .collect()
    }

    #[test]
    fn the_inbox_lists_what_waits_on_the_person_and_shows_each_item_picked() {
        let root = root_of("inbox", &[("orders", "orders"), ("checkout", "pay/checkout")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        let orders = line_of(&gallery, "orders");
        gallery.pick(orders);
        // Each assumption an item: its "?" and what it is on, then what was guessed.
        let guesses = [
            "? nav-export in default",
            "This navigates to ExportDocuments, which no description under the root declares, so it is guessed that ExportDocuments is a screen not drawn yet",
            "? order-{index} in default",
            "This navigates to OrderDetail, which no description under the root declares, so it is guessed that OrderDetail is a screen not drawn yet",
        ];
        assert_eq!(inbox(&gallery)[..], [&["waits on you · 2"][..], &guesses].concat());

        assert!(click_panel_line(&mut gallery, "nav-export in default"), "an item is clicked by its box");
        assert_eq!(shown(&gallery), "orders · default", "clicking an item shows its state");
        assert_eq!(picked_node(&gallery), Some("nav-export"), "with its node picked");
        assert_eq!(gallery.panel()[0], "assumed, for you to answer", "its assumption, to answer");
        assert!(gallery.panel().iter().any(|line| line == "IconButton"), "and its card");
        assert!(inbox(&gallery).is_empty(), "a node picked: the inbox gives way to its card");

        let dir = root.join("orders");
        let on_export = gallery.reference().unwrap();
        notes::say(&on_export, "say Export").unwrap();
        notes::answer(&dir, "N1", "it says Export now").unwrap();
        let whole =
            Reference { description: dir.clone(), on: On::State { state: State::Fixture("default".into()) } };
        notes::say(&whole, "too dense").unwrap();
        gallery.reload();
        gallery.pick(orders);
        let answered = ["N1 on nav-export in default at 390", "agent: it says Export now"];
        assert_eq!(
            inbox(&gallery)[..],
            [&["waits on you · 3"][..], &guesses, &answered, &["▸ waiting on the agent · 1"]].concat(),
            "two assumptions unanswered and one note answered; the open note folded to its count"
        );
        assert!(click_panel_line(&mut gallery, "▸ waiting on the agent · 1"));
        assert_eq!(inbox(&gallery)[7..], ["▾ waiting on the agent · 1", "N2 on the whole of default"]);

        assert!(click_panel_line(&mut gallery, "on nav-export in default at 390"));
        assert_eq!(shown(&gallery), "orders · default");
        assert_eq!(picked_node(&gallery), Some("nav-export"), "the note's node picked");
        assert_eq!(talk_shown(&gallery)[0], "note N1 · answered", "with its conversation");

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
            everything[at_orders + 1..at_orders + 7],
            [&guesses[..], &answered].concat(),
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

        let assumed = guesses[1].to_string();
        let guess = Guess::Undrawn(vec!["ExportDocuments".into()]);
        notes::judge(&on_export, Answers { assumed, guess, answer: Answer::Right }, "").unwrap();
        gallery.reload();
        gallery.pick(orders);
        assert_eq!(
            inbox(&gallery),
            [
                "waits on you · 2",
                guesses[2],
                guesses[3],
                answered[0],
                answered[1],
                "▾ waiting on the agent · 2",
                "N2 on the whole of default",
                "N3 on nav-export in default at 390",
            ],
            "answered ✓, the assumption leaves as the files change; the ✓ waits on the agent"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// What the tests' clipboard holds.
    fn board() -> Option<String> {
        BOARD.with_borrow(|board| board.0.clone())
    }

    /// A key pressed: `key` on the layout, typing `text`, on `code`.
    fn key_on(key: &str, code: KeyCode, repeat: bool) -> Input {
        Input::Key {
            key: Key::Character(key.into()),
            code: PhysicalKey::Code(code),
            text: Some(key.into()),
            repeat,
        }
    }

    #[test]
    fn a_key_with_command_or_ctrl_held_is_a_shortcut_and_without_it_is_typed() {
        let mut gallery = gallery();
        show(&mut gallery, "airbnb-listing", "default");
        assert!(gallery.chord("\\"), "⌘\\ hides the sidebar");
        assert!(!gallery.sidebar);
        let backslash = key_on("\\", KeyCode::Backslash, false);
        assert!(!gallery.feed([backslash.clone()]).redraw, "\\ alone is no shortcut");
        assert!(!gallery.sidebar);
        gallery.feed([Input::Modifiers(ModifiersState::CONTROL), backslash]);
        assert!(gallery.sidebar, "Ctrl+\\ shows it again, as ⌘\\ does");
        gallery.feed([Input::Modifiers(ModifiersState::empty())]);

        assert!(gallery.chord("c"), "⌘C with nothing written copies the reference");
        assert_eq!(board(), gallery.reference().map(|reference| reference.line()));
        BOARD.with_borrow_mut(|board| board.0 = None);
        let held = Input::Modifiers(ModifiersState::SUPER);
        gallery.feed([held.clone(), key_on("c", KeyCode::KeyC, true)]);
        assert_eq!(board(), None, "a C held down, repeating, copies nothing");
        gallery.feed([key_on("с", KeyCode::KeyC, false)]);
        assert!(board().is_some(), "a layout with no C copies on the key where C is");
        BOARD.with_borrow_mut(|board| board.0 = None);
        gallery.feed([key_on("c", KeyCode::KeyJ, false)]);
        assert!(board().is_some(), "a layout with its C elsewhere copies on its C");
        gallery.feed([key_on("n", KeyCode::KeyN, false), Input::Modifiers(ModifiersState::empty())]);
        assert_eq!(gallery.talk.text(), "", "⌘N types nothing");

        gallery.press_at(on_node(&gallery, 0, "nav-save"));
        gallery.let_go();
        assert_eq!(picked_node(&gallery), Some("nav-save"));
        type_text(&mut gallery, "too small");
        assert!(gallery.chord("a"), "⌘A selects all of the box");
        assert!(gallery.chord("c"), "⌘C copies what is selected in it");
        assert_eq!(board().as_deref(), Some("too small"), "not the reference");
        assert!(gallery.chord("x"), "⌘X cuts it");
        assert_eq!(gallery.talk.shown().text, "");
        assert!(!gallery.chord("x"), "with nothing selected, nothing to cut");
        assert!(gallery.chord("v"), "⌘V pastes what was cut");
        assert!(gallery.chord("v"));
        assert_eq!(gallery.talk.shown().text, "too smalltoo small");
        press_keys(&mut gallery, &["Esc"]);
        assert!(gallery.draft.is_none());
    }

    #[test]
    fn the_wheel_scrolls_the_pane_under_the_pointer_by_notches_or_points_and_across_with_shift() {
        let mut gallery = opened(drafts(), (1000.0, 900.0));
        show(&mut gallery, "airbnb-listing", "default");
        let notch = |y: f64| Input::Wheel { x: 0.0, y, unit: Unit::Lines };
        gallery.move_to((gallery.tree + MARGIN, MARGIN));
        assert!(gallery.feed([notch(-2.0)]).redraw);
        assert_eq!(gallery.scroll, 2.0 * NOTCH, "two notches toward the person scroll down");
        gallery.feed([Input::Wheel { x: 0.0, y: 15.0, unit: Unit::Points }]);
        assert_eq!(gallery.scroll, 2.0 * NOTCH - 15.0, "a trackpad scrolls by points");
        gallery.feed([Input::Wheel { x: -10.0, y: 0.0, unit: Unit::Points }]);
        assert_eq!(gallery.across, 10.0, "and across");

        gallery.feed([Input::Modifiers(ModifiersState::SHIFT), notch(-1.0)]);
        assert_eq!(
            (gallery.scroll, gallery.across),
            (2.0 * NOTCH - 15.0, 10.0 + NOTCH),
            "Shift turns it across"
        );
        gallery.feed([Input::Wheel { x: -5.0, y: -5.0, unit: Unit::Points }]);
        assert_eq!(
            (gallery.scroll, gallery.across),
            (2.0 * NOTCH - 10.0, 15.0 + NOTCH),
            "a wheel already turning across scrolls as it turns, Shift held or not"
        );
        gallery.feed([Input::Modifiers(ModifiersState::empty())]);

        let at = gallery.scroll;
        gallery.move_to((40.0, MARGIN));
        gallery.feed([notch(-1.0)]);
        assert_eq!(gallery.scroll, at, "over the tree, the tree scrolls, not the pictures");
    }

    #[test]
    fn the_pointer_over_a_line_between_panes_grips_and_an_input_method_is_on_only_while_there_is_a_box() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        let moved = |gallery: &mut Gallery, at| gallery.feed([Input::Moved(at)]).cursor;
        let (tree, side) = (gallery.tree, gallery.pane_right());
        assert_eq!(moved(&mut gallery, (tree + 1.0, 300.0)), Some(CursorIcon::EwResize));
        assert_eq!(moved(&mut gallery, (side - 1.0, 300.0)), Some(CursorIcon::EwResize));
        assert_eq!(moved(&mut gallery, (tree + 40.0, 300.0)), Some(CursorIcon::Default));
        gallery.press_at((gallery.tree, 300.0));
        assert_eq!(
            moved(&mut gallery, (600.0, 300.0)),
            Some(CursorIcon::EwResize),
            "held, off the line still"
        );
        gallery.let_go();
        assert_eq!(gallery.feed([Input::Pressed]).cursor, None, "only a move says how the pointer looks");
        gallery.let_go();

        assert!(
            gallery.feed([Input::Moved((0.0, 0.0))]).ime,
            "a state shown: its box takes the input method"
        );
        gallery.press_at(on_node(&gallery, 0, "nav-export"));
        assert!(gallery.feed([Input::Released]).ime, "a node picked: its box too");
        gallery.drawn();
        let caret = gallery.feed([Input::Moved((0.0, 0.0))]).caret.expect("drawn, its caret is known");
        assert!(caret.x as f64 >= gallery.pane_right(), "in the sidebar, in the box at its foot");
        assert!(gallery.compose(Ime::Preedit("ni".into(), None)));
        assert!(gallery.talk.is_composing());
        assert!(gallery.compose(Ime::Disabled), "the input method turned off");
        assert!(!gallery.talk.is_composing(), "drops what it was composing");
        gallery.chord("\\");
        let effects = gallery.feed([Input::Moved((0.0, 0.0))]);
        assert!(!effects.ime && effects.caret.is_none(), "the sidebar hidden: the input method off");
        assert!(!gallery.compose(Ime::Commit("你".into())), "and nothing it commits goes anywhere");
        assert_eq!(gallery.talk.text(), "");

        assert!(gallery.feed([Input::Close]).exit, "closing the window exits");
    }

    /// Each surface of the window drawn to a PNG and held to a golden one.
    mod frames;
}
