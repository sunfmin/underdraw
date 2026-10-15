//! The gallery's window, run in a process of its own: the tree of
//! descriptions and their states on the left; for the state picked, its
//! picture drawn at a phone and a desktop width side by side in the middle,
//! where a node can be picked to read what it is, or acted on to follow what
//! the description says happens, as a prototype; and on the right the
//! sidebar ([`crate::sidebar`]), the cards of what is picked and the buttons
//! of what can be done with it; for a directory picked, the
//! flow between the screens under it; for a journey picked (ADR 0022), its
//! steps played one by one. Along the top, over all three, the toolbar
//! ([`toolbar`]): back and forward through what the window has shown
//! ([`history`]), and the button that hides and shows the sidebar. The person talks to the agent about what they
//! picked (ADR 0023): a node, a whole state, an arrow of an overview or
//! a flow, or a whole journey played has one note, a conversation shown in the sidebar under its
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
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::act::{self, Way};
use crate::area::{over, rect};
use crate::chrome::{ARROW_FONT, INDENT, MARK, MARK_INSET, MARK_TALL, PIN, ROW, TOOLBAR};
use crate::draw::{self, Shown as State, Wanted};
use crate::field::{self, Edit, Field, Motion};
use crate::flow::{self, Screen};
use crate::gallery::{self, Answering, Row};
use crate::inbox::{Guessed, Inbox, Item};
use crate::journeys::{self, Step};
use crate::notes::{self, Answer, Answers, Goes, Guess, Mark, Note, Proposal, Stands};
use crate::reference::{self, On, Reference, Whole};
use crate::sidebar::Item as Listed;
use crate::sidebar::{self, Block, Card, Foot, Sidebar};
use crate::{chrome, gpu, links, paint, text};
use history::History;
use panes::{Content, Pane, Panes, Scrolls, Took};
use picture::{
    Arrow, Desk, Faults, Flow, Offer, Overview, Page, Pages, Pass, Picture, Pin, Seen, Target, Thumb, Tile,
    offer_widths,
};
use toolbar::Tool;

mod history;
mod panes;
mod picture;
mod toolbar;

/// How far one notch of a mouse wheel scrolls.
const NOTCH: f64 = 40.0;
/// How small the pictures of a state get, fitting the width left, before
/// they scroll sideways instead.
const SMALLEST: f64 = 0.5;
/// How long a ✓ on an assumption can be undone before it is written.
const UNDO: Duration = Duration::from_secs(5);
/// How long every node that acting on goes somewhere is lit up after a
/// ⌘-click that goes nowhere: long enough to see where to click instead.
const FLASH: Duration = Duration::from_millis(700);

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
    /// The window became, or stopped being, the one keys go to.
    Focused(bool),
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
    /// The panes' widths and scrolls, and the line or thumb held.
    panes: Panes,
    /// Whether the card of the node picked is unfolded to its details: its
    /// id, and its label and box at each width.
    details: bool,
    /// Where the pointer is, in points.
    cursor: (f64, f64),
    /// Whether a click acts on the node under it, rather than picks it: ⌘,
    /// or Ctrl, is held.
    acting: bool,
    /// Whether the marks over the pictures are shown ([`Seen::marks`]), and
    /// the pins of the notes: so when the window opens, until the toolbar
    /// or ⌘. hides them.
    marks: bool,
    /// Until when every node that acting on goes somewhere is lit up, after
    /// a ⌘-click that went nowhere.
    flashed: Option<Instant>,
    /// The states acting has come from, the last one last: where `back`
    /// returns to. Picking a state in the tree starts a new path.
    path: Vec<Named>,
    /// What the window has shown, which the toolbar's back and forward,
    /// and ⌘[ and ⌘], step through: the window's, not a design's `back`.
    history: History<Place>,
    /// What the last act on a node came to, while nothing else is done.
    acted: Option<Acted>,
    /// Sets the tree's and the captions' text.
    typesetter: text::Typesetter,
    /// Every description's notes, and every directory's on the screens
    /// under it, by its directory, as notes.json now holds them.
    notes: BTreeMap<PathBuf, Vec<Note>>,
    /// The sidebar: the box at its foot, where the person writes to the
    /// conversation on what is picked, and its cards as last laid out.
    sidebar: Sidebar<Do>,
    /// A ✗ on an assumption clicked, while what is right is written in the
    /// box at the sidebar's foot: Send sends it as the ✗.
    rejecting: Option<Rejecting>,
    /// A ✓ on an assumption waiting to be written, while it can be undone.
    confirming: Option<Confirming>,
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
    /// The time the window counts how long ago a message was taken from,
    /// in seconds since 1970: the system's, unless a test holds it still.
    time: Option<u64>,
    /// Whether the window is the one keys go to: the caret of the field
    /// written in blinks only then, and holds still, shown, otherwise.
    focused: bool,
    /// When the field written in was last edited, its caret moved, or it
    /// opened: its caret blinks from then ([`field::caret_shows`]).
    edited: Instant,
}

/// What the person picked in what is shown, to read what it is, copy it
/// or talk about it: one thing, or nothing, at a time.
#[derive(Clone, Debug, PartialEq)]
enum Picked {
    /// Nothing: the state, description or directory shown, as a whole.
    Nothing,
    /// A node of the state shown, by its id, kept from state to state while
    /// the state shown has it; and the picture, of those at each of the
    /// widths its tokens name, it was picked in: the instance the person pointed at.
    Node { id: String, pointed: usize },
    /// An arrow of the overview or the flow shown, by its place among their
    /// arrows.
    Arrow(usize),
}

/// A ✗ on the assumption of the node `on` references, clicked: what it
/// answers, sent with what the box at the sidebar's foot holds.
struct Rejecting {
    on: Reference,
    answers: Answers,
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
    /// Goes where acting on the node picked goes this way, as a ⌘-click on
    /// it does: one outcome its operation answers with, chosen.
    Go(Way),
    /// Says in the note on the node acted on where acting on it should go:
    /// a message marked as that proposal.
    Propose(Proposal),
    /// Sends what the box at the sidebar's foot holds to the conversation
    /// on what is picked.
    Send,
    /// Answers the assumption on the node picked: ✓ is written once it can
    /// no longer be undone; ✗ puts the caret in the box, for what is right.
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

/// The notes of every description and directory on the lines of the
/// tree, by its directory, each on what is gone joined its state's note
/// first; one whose notes.json cannot be read has none (`check` says why).
fn notes_of(lines: &[Line]) -> BTreeMap<PathBuf, Vec<Note>> {
    lines
        .iter()
        .filter(|line| line.state.is_none() && !line.journey)
        .map(|line| line.row.dir.as_path())
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
#[derive(Clone, Debug, PartialEq)]
struct Named {
    dir: PathBuf,
    state: Option<String>,
}

/// What the window shows, as its history keeps it: the line of the tree
/// picked, by name, and for a journey the step of it shown.
#[derive(Clone, Debug, PartialEq)]
struct Place {
    line: Named,
    step: Option<usize>,
}

/// The window open, and the GPU drawing in it.
struct Shown {
    window: Arc<Window>,
    gpu: gpu::Gpu,
    surface: gpu::Window<'static>,
}

impl Gallery {
    /// Says its own words in `lang`, set in the fonts of its writing: the
    /// same Han character is drawn one way in Japanese, another in Chinese.
    fn speak(&mut self, lang: Lang) -> Result<(), String> {
        self.typesetter = text::Typesetter::new(lang.locale())?;
        self.sidebar.speak();
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
        let sidebar = Sidebar::new(&typesetter);
        Gallery {
            root: root.to_path_buf(),
            lang: Lang::default(),
            notes: notes_of(&lines),
            lines,
            line: None,
            wanted: None,
            picture: Picture::Faults(Faults { findings: Vec::new(), within: String::new(), journey: false }),
            picked: Picked::Nothing,
            panes: Panes::new(),
            details: false,
            cursor: (0.0, 0.0),
            acting: false,
            marks: true,
            flashed: None,
            path: Vec::new(),
            history: History::new(),
            acted: None,
            typesetter,
            sidebar,
            rejecting: None,
            confirming: None,
            selecting: None,
            hovered: None,
            shift: false,
            playing: None,
            descriptions: RefCell::new(HashMap::new()),
            guessed: RefCell::new(HashMap::new()),
            waiting: false,
            time: None,
            focused: false,
            edited: Instant::now(),
        }
    }

    /// What the person did, `input`, done: the one way in for all the
    /// window is told, the clipboard lent for ⌘C, ⌘X and ⌘V. What the
    /// window does after.
    fn handle(&mut self, input: Input, clipboard: &mut dyn Clipboard) -> Effects {
        let mut effects = Effects::default();
        // The panes and the pointer below the toolbar, in points from its
        // foot: the toolbar itself is above them.
        if let Input::Resized((width, height)) = input {
            self.panes.resize((width, (height - TOOLBAR).max(0.0)));
        }
        // The panes fitted to the window and to what they hold as it now
        // stands, for the input to find their scrollbars where they are
        // drawn.
        self.clamp();
        let was = self.typing().then(|| self.sidebar.talk().shown());
        match input {
            Input::Resized(_) => effects.redraw = true,
            Input::Moved((x, y)) => {
                let at = (x, y - TOOLBAR);
                self.cursor = at;
                let grip = self.panes.grip(at);
                effects.cursor = Some(if grip {
                    CursorIcon::EwResize
                } else if self.leads_at(at) {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                });
                let dragged = self.panes.drag(at);
                if dragged {
                    self.clamp();
                }
                effects.redraw = dragged || self.hover() || self.drag_field();
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
                let acting = held.super_key() || held.control_key();
                // Every node that acting on goes somewhere lit up, or no
                // longer.
                effects.redraw = acting != self.acting && matches!(self.picture, Picture::Pages(_));
                self.acting = acting;
                self.shift = held.shift_key();
            }
            Input::Key { key, code, text, repeat } => {
                effects.redraw = self.key_pressed(&key, code, text.as_deref(), repeat, clipboard);
            }
            Input::Ime(ime) => effects.redraw = self.ime(ime),
            Input::Focused(focused) => {
                self.focused = focused;
                self.edited = Instant::now();
                effects.redraw = self.typing();
            }
            Input::Close => {
                self.leave();
                effects.exit = true;
            }
        }
        effects.redraw |= self.see();
        // Edited, its caret moved, or opened: the caret shown at once and
        // blinking again from now.
        if self.typing().then(|| self.sidebar.talk().shown()) != was {
            self.edited = Instant::now();
        }
        (effects.ime, effects.caret) = self.composing();
        effects
    }

    /// Whether the caret of the field written in shows at `now`: always,
    /// held still, while the window is not the one keys go to.
    fn caret_shows(&self, now: Instant) -> bool {
        !self.focused || field::caret_shows(now.saturating_duration_since(self.edited))
    }

    /// When the caret of the field written in next shows or hides, from
    /// `now`, for the window to wake then and draw it: none while there is
    /// no such field, or the window is not the one keys go to.
    fn blinks(&self, now: Instant) -> Option<Instant> {
        (self.focused && self.typing())
            .then(|| now + field::caret_turns(now.saturating_duration_since(self.edited)))
    }

    /// The conversation shown in the sidebar seen by the person: each
    /// answer in it not read yet marked read in notes.json, and the notes
    /// read again. Whether any was.
    fn see(&mut self) -> bool {
        if self.panes.side().is_none() {
            return false;
        }
        let Some((on, notes)) = self.conversation() else {
            return false;
        };
        let unread: Vec<String> =
            notes.iter().filter(|note| note.stands() == Stands::Unread).map(|note| note.id.clone()).collect();
        if unread.is_empty() || !notes::seen(&on.description, &unread).unwrap_or(false) {
            return false;
        }
        self.notes = notes_of(&self.lines);
        true
    }

    /// The time now, in seconds since 1970, as the window counts from it.
    fn now(&self) -> u64 {
        self.time.unwrap_or_else(notes::now)
    }

    /// Whether the conversation shown counts how long ago the agent took
    /// what the person said, so the window is drawn again each second.
    fn counting(&self) -> bool {
        self.panes.side().is_some()
            && self
                .conversation()
                .is_some_and(|(_, notes)| notes.last().is_some_and(|note| note.taken().is_some()))
    }

    /// Whether an input method may compose, only into a field keys type
    /// into ([`Gallery::field_mut`]), and where the caret it shows what it
    /// offers beside is.
    fn composing(&mut self) -> (bool, Option<Rect>) {
        let typing = self.typing();
        let caret = self.placed().and_then(|placed| placed.caret()).filter(|_| typing);
        // In the window's points, below the toolbar.
        (typing, caret.map(|caret| Rect { y: caret.y + TOOLBAR as f32, ..caret }))
    }

    /// A key pressed: with ⌘ (or Ctrl) held, ⌘V pastes into the note being
    /// written, ⌘A selects all of it, ⌘X cuts what is selected in it, ⌘C
    /// copies that, or with nothing selected the reference to what is
    /// picked, ⌘\ hides or shows the sidebar, ⌘. the marks over the
    /// pictures, and ⌘[ and ⌘] go back and
    /// forward through what the window has shown; without, what
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
            self.use_tool(Tool::Sidebar)
        } else if chord(".", KeyCode::Period) {
            self.use_tool(Tool::Marks)
        } else if chord("[", KeyCode::BracketLeft) {
            self.use_tool(Tool::Back)
        } else if chord("]", KeyCode::BracketRight) {
            self.use_tool(Tool::Forward)
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
        self.panes.wheel(self.cursor, right, down);
        self.clamp();
    }

    /// What a button of the toolbar does, done: back or forward through
    /// what the window has shown, the marks over the pictures hidden or
    /// shown, or the sidebar hidden or shown, its width given to the
    /// pictures. Whether that changed what is shown.
    fn use_tool(&mut self, tool: Tool) -> bool {
        match tool {
            Tool::Back => self.go_through(History::back, History::forward),
            Tool::Forward => self.go_through(History::forward, History::back),
            Tool::Marks => {
                self.marks = !self.marks;
                true
            }
            Tool::Sidebar => {
                self.panes.toggle_side();
                self.clamp();
                true
            }
        }
    }

    /// The place `way` steps the history to shown, as picking it in the
    /// tree shows it: a new path for acting, and nothing said of the last
    /// act. Where that place has left the tree, the history is stepped
    /// back with `undo` and nothing changes. Whether there was a place.
    fn go_through(
        &mut self,
        way: fn(&mut History<Place>) -> Option<Place>,
        undo: fn(&mut History<Place>) -> Option<Place>,
    ) -> bool {
        let Some(place) = way(&mut self.history) else {
            return false;
        };
        let Some(index) = self.line_named(&place.line) else {
            undo(&mut self.history);
            return false;
        };
        self.path.clear();
        self.acted = None;
        match place.step {
            Some(at) => self.play(index, at),
            None => self.pick(index),
        }
        true
    }

    /// What the window shows now, as its history keeps it.
    fn place(&self) -> Option<Place> {
        let line = self.lines.get(self.line?)?;
        let state = line.state.as_ref().map(|state| state.name().to_string());
        let step = self.playing.as_ref().map(|playing| playing.at);
        Some(Place { line: Named { dir: line.row.dir.clone(), state }, step })
    }

    /// What the window shows now kept in its history: a step, unless it
    /// shows it already.
    fn arrive(&mut self) {
        if let Some(place) = self.place() {
            self.history.arrive(place);
        }
    }

    /// The toolbar's buttons, laid out across the window as it now stands.
    fn tools(&mut self) -> Vec<toolbar::Button> {
        let lang = self.lang;
        let left = vec![
            (Tool::Back, say!(lang, go_back), self.history.can_back()),
            (Tool::Forward, say!(lang, go_forward), self.history.can_forward()),
        ];
        let sidebar =
            if self.panes.side().is_some() { say!(lang, hide_sidebar) } else { say!(lang, show_sidebar) };
        let keys = if cfg!(target_os = "macos") { "⌘." } else { "Ctrl+." };
        let marks = if self.marks { say!(lang, hide_marks, keys) } else { say!(lang, show_marks, keys) };
        let right = vec![(Tool::Marks, marks, true), (Tool::Sidebar, sidebar, true)];
        toolbar::lay_out(&mut self.typesetter, self.panes.window().0, left, right)
    }

    /// The desk the pictures lie on: where it is, and how far it is
    /// scrolled.
    fn desk(&self) -> Desk {
        let pictures = self.panes.pictures();
        let (scroll, across) = (self.panes.scrolled(Scrolls::Down), self.panes.scrolled(Scrolls::Across));
        Desk { left: pictures.x0, width: pictures.width(), scroll, across }
    }

    /// What is at `at`, a point in the window, on the picture shown: with
    /// the marks hidden, the state under a journey's mark, not the mark.
    fn target_at(&self, at: (f64, f64)) -> Option<Target> {
        match self.picture.shows().target_at(at, &self.desk()) {
            Some(Target::Pass { state, .. }) if !self.marks => Some(Target::State(state)),
            target => target,
        }
    }

    /// `findings` shown as the faults of the line shown.
    fn faulty(&self, findings: Vec<Finding>) -> Picture {
        let path = self.line.and_then(|i| Some(self.lines.get(i)?.row.dir.clone())).unwrap_or_default();
        let within = path.strip_prefix(&self.root).unwrap_or(&path).display().to_string();
        let journey = self.line.and_then(|i| self.lines.get(i)).is_some_and(|line| line.journey);
        Picture::Faults(Faults { findings, within, journey })
    }

    /// A press where the pointer is: on the toolbar, does what its button
    /// there does; on the tree, picks the line under it;
    /// on a button of the sidebar, does what it does; on a pin, picks what
    /// its note is on; on the pictures, picks the
    /// node under it, or none, or acts on it while [`Gallery::acting`]; on
    /// the overview, picks the arrow under it, or plays the journey whose
    /// mark is, from that step, or picks the state; on the flow,
    /// picks the arrow, or the description whose box is, under it. Whether
    /// that changed what is shown.
    fn press(&mut self) -> bool {
        // On the toolbar, above the panes: its button there pressed.
        if self.cursor.1 < 0.0 {
            let tools = self.tools();
            return toolbar::tool_at(&tools, self.cursor).is_some_and(|tool| self.use_tool(tool));
        }
        // On a line between two panes, only taking hold of it; on a
        // scrollbar, only scrolling its pane: no line of the tree, nor
        // region, is picked, nor button pressed.
        if let Some(took) = self.panes.press(self.cursor) {
            self.clamp();
            return took == Took::Bar;
        }
        let pane = self.panes.pane_at(self.cursor);
        if pane == Pane::Side {
            if self.press_field() {
                return true;
            }
            return match self.button_at(self.cursor) {
                Some(does) => self.click(does),
                None => false,
            };
        }
        if pane == Pane::Pictures {
            // As they are drawn: the pins over the buttons floating beside
            // the node picked, and both over the nodes.
            if !self.acting
                && let Some((dir, id)) = self.pin_at(self.cursor)
            {
                return self.visit(Visit::Note(dir, id));
            }
            if let Some(way) = self.offer_at(self.cursor) {
                return self.click(Do::Go(way));
            }
            if self.acting && matches!(self.picture, Picture::Pages(_)) {
                return self.act();
            }
            return match self.target_at(self.cursor) {
                Some(Target::Node { id, pointed }) => self.pick_thing(Picked::Node { id, pointed }),
                Some(Target::Arrow(arrow)) => self.pick_thing(Picked::Arrow(arrow)),
                Some(Target::Box(dir)) => {
                    let Some(index) = self.description_line(&dir) else {
                        return false;
                    };
                    self.pick(index);
                    true
                }
                Some(Target::Pass { journey, step, .. }) => {
                    self.path.clear();
                    self.acted = None;
                    self.play(journey, step);
                    true
                }
                Some(Target::State(line)) => {
                    self.path.clear();
                    self.acted = None;
                    self.pick(line);
                    true
                }
                Some(Target::Caption) | None => self.pick_thing(Picked::Nothing),
            };
        }
        let Some(line) = self.panes.row_at(self.cursor) else {
            return false;
        };
        match self.shows(line) {
            Some(index) if Some(index) != self.line => {
                self.path.clear();
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
                Picture::Pages(pages) => pages.pages.iter().any(|page| page.has(id)),
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
        self.panes.release()
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
        let (wanted, acted) = (self.wanted.take(), self.acted.take());
        let scroll = self.panes.scrolled(Scrolls::Down);
        let arrow = self.arrow().and_then(|_| self.reference());
        let journey = self.line.and_then(|index| self.lines.get(index)).filter(|line| line.journey);
        let journey = journey.map(|line| (line.row.dir.clone(), self.playing.as_ref().map_or(0, |p| p.at)));
        self.lines = lines(&self.root);
        self.descriptions.borrow_mut().clear();
        self.guessed.borrow_mut().clear();
        self.notes = notes_of(&self.lines);
        self.line = None;
        self.picture = self.faulty(Vec::new());
        let playing = journey.and_then(|(file, at)| {
            Some((self.lines.iter().position(|line| line.journey && line.row.dir == file)?, at))
        });
        if let Some((index, at)) = playing {
            self.play(index, at);
            self.clamp();
            self.panes.scroll_to(Scrolls::Down, scroll);
            self.see();
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
            self.clamp();
            self.panes.scroll_to(Scrolls::Down, scroll);
            self.acted = acted;
            if let Some(at) =
                arrow.and_then(|arrow| self.picture.arrow_references().iter().position(|r| *r == arrow))
            {
                self.picked = Picked::Arrow(at);
            }
        } else if description.is_some() {
            self.wanted = wanted;
        }
        self.clamp();
        // An answer come in to the conversation shown is seen as it comes.
        self.see();
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
        let (Some(Target::Node { id, pointed }), Picture::Pages(pages)) =
            (self.target_at(self.cursor), &self.picture)
        else {
            self.flash();
            self.pick_thing(Picked::Nothing);
            return true;
        };
        let calls = pages.pages[pointed].calls.get(&id).cloned().unwrap_or_default();
        self.pick_thing(Picked::Node { id, pointed });
        let (Some(here), Some(on)) = (self.wanted.clone(), self.acted_on()) else {
            return true;
        };
        let state = here.state.clone().unwrap_or_default();
        match self.ways().to_vec().as_slice() {
            [] => {
                self.flash();
                let says = say!(self.lang, no_transition, state, on);
                let choices = self.proposals(&here.dir, &on);
                self.acted = Some(Acted { says, choices });
            }
            [way] => self.follow(&on, way.clone()),
            // One for each outcome, offered on the node's card under it.
            _ => {
                let calls = calls.join(", ");
                self.say(say!(self.lang, acted_calls, on, state, calls));
            }
        }
        true
    }

    /// The node picked as acting on it names it: its id as written and the
    /// event a click raises on it. None while no node is picked.
    fn acted_on(&self) -> Option<String> {
        let ((id, pointed), Picture::Pages(pages)) = (self.node()?, &self.picture) else {
            return None;
        };
        let handled = |page: &&Page| page.handlers.contains_key(id);
        let page = pages.pages.get(pointed).filter(handled).or_else(|| pages.pages.iter().find(handled));
        let handlers = page.and_then(|page| page.handlers.get(id)).cloned().unwrap_or_default();
        Some(format!("{}.{}", handlers.written, act::event(&handlers.events)))
    }

    /// Where acting on the node picked goes from the state shown, in the
    /// order written: none while no node is picked, or it goes nowhere.
    fn ways(&self) -> &[Way] {
        match (self.node(), &self.picture) {
            (Some((id, _)), Picture::Pages(pages)) => pages.ways.get(id).map_or(&[], Vec::as_slice),
            _ => &[],
        }
    }

    /// `way` as it is offered beside the node picked and on its card: where
    /// it goes, in --lang, grayed for a screen not drawn yet.
    fn offer(&self, way: &Way) -> Offer {
        let lang = self.lang;
        let (says, live) = match way {
            Way::To { outcome: None, to } => (say!(lang, way_state, to), true),
            Way::To { outcome: Some(outcome), to } => (say!(lang, way_outcome, to, outcome), true),
            Way::Navigate { screen, drawn: true } => (say!(lang, way_navigate, screen), true),
            Way::Navigate { screen, drawn: false } => (say!(lang, way_undrawn, screen), false),
            Way::Back => (say!(lang, way_back), true),
        };
        Offer { says, live }
    }

    /// The button floating beside the node picked under `at`, a point in
    /// the window: where it goes. A grayed one only says where it would,
    /// and a press goes through it to what is under it.
    fn offer_at(&mut self, at: (f64, f64)) -> Option<Way> {
        let (id, pointed) = self.node().map(|(id, pointed)| (id.to_string(), pointed))?;
        let ways = self.ways().to_vec();
        let offers: Vec<Offer> = ways.iter().map(|way| self.offer(way)).collect();
        let wide = offer_widths(&offers, &mut self.typesetter);
        let Picture::Pages(pages) = &self.picture else {
            return None;
        };
        let offered = pages.offered(&id, pointed, &wide, &self.desk());
        let at = vello::kurbo::Point::new(at.0, at.1);
        let live = |index: &usize| offers[*index].live && offered[*index].contains(at);
        (0..offered.len()).find(live).map(|index| ways[index].clone())
    }

    /// Goes where `way` says acting on the node picked, `on` (its id as
    /// written and event), goes from the state shown: to a state of the
    /// screen; to the first state of the screen a navigate names, or, for
    /// one not drawn yet, says so; or back along the path taken. The panel
    /// says what came of it.
    fn follow(&mut self, on: &str, way: Way) {
        let Some(here) = self.wanted.clone() else {
            return;
        };
        let state = here.state.clone().unwrap_or_default();
        let lang = self.lang;
        match way {
            Way::To { outcome: None, to } => {
                let says = say!(lang, acted_to_state, on, state, to);
                self.go(Named { dir: here.dir, state: Some(to) }, says);
            }
            Way::To { outcome: Some(outcome), to } => {
                let says = say!(lang, acted_outcome, outcome, state, to);
                self.go(Named { dir: here.dir, state: Some(to) }, says);
            }
            Way::Navigate { screen, .. } => match links::declared(&self.root).remove(&screen) {
                Some(description) => {
                    let says = say!(lang, acted_navigates, on, screen);
                    self.go(Named { dir: description, state: None }, says);
                }
                None => self.say(format!("{on}: {}", underdraw_format::assumption::undrawn(&[screen], lang))),
            },
            Way::Back => match self.path.pop() {
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
        }
    }

    /// Every node that acting on goes somewhere lit up for a moment, to show
    /// where a ⌘-click would.
    fn flash(&mut self) {
        self.flashed = Some(Instant::now() + FLASH);
    }

    /// When the flash after a ⌘-click that went nowhere ends, from `now`,
    /// for the window to wake then and draw the nodes unlit: none while
    /// there is none.
    fn flash_ends(&self, now: Instant) -> Option<Instant> {
        self.flashed.filter(|&end| end > now)
    }

    /// Whether `at`, a point under the toolbar, is on a node of the state
    /// shown that acting on goes somewhere.
    fn leads_at(&self, at: (f64, f64)) -> bool {
        let Picture::Pages(pages) = &self.picture else {
            return false;
        };
        if self.panes.pane_at(at) != Pane::Pictures {
            return false;
        }
        matches!(
            self.picture.shows().target_at(at, &self.desk()),
            Some(Target::Node { id, .. }) if pages.ways.contains_key(&id)
        )
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
            Do::Go(way) => {
                let on = self.acted_on().unwrap_or_default();
                self.follow(&on, way);
                true
            }
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

    /// `proposal` said in the note on the node acted on, at the instance
    /// pointed at, as a message marked as that proposal, for the agent to
    /// declare; the panel says so, or why it was not. The window writes the
    /// note only, never the description.
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
            self.path.push(here);
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

    /// What picking line `index` shows: a state drawn at each of the widths
    /// its tokens name,
    /// a description's overview, a directory's flow, or a journey's first
    /// step, kept in the window's history as a step when it is new.
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
        self.panes.scroll_to(Scrolls::Down, 0.0);
        self.panes.scroll_to(Scrolls::Across, 0.0);
        self.picture = if !described {
            Picture::Flow(self.flow(&dir))
        } else if !faults.is_empty() {
            self.faulty(faults)
        } else if state.is_none() && self.lines.get(index + 1).is_some_and(|next| next.state.is_some()) {
            // The screens drawn under the root as they now stand: a
            // navigate to any other is a screen not drawn yet.
            match self.overview(index, &dir, &links::drawn(&self.root)) {
                Ok(overview) => Picture::Overview(overview),
                Err(e) => {
                    self.faulty(vec![Finding { at: crate::SCREEN.to_string(), problem: format!("{e:#}") }])
                }
            }
        } else {
            self.lay_out_pages(&dir, state.as_ref(), &links::drawn(&self.root))
        };
        self.picked = self.still_picked(described);
        self.arrive();
    }

    /// The state `state` of the description in `dir` drawn at each of the
    /// widths its tokens name, given the screens `drawn` under the root; or why it
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
                let laying: Vec<_> = description
                    .widths()
                    .into_iter()
                    .map(|width| {
                        let description = &description;
                        scope.spawn(move || {
                            let wanted = match state {
                                Some(state) => state.wanted(width, 1),
                                // A description with no state: lay_out says why.
                                None => Wanted { fixture: None, phase: None, width, scale: 1 },
                            };
                            let mut laid = description.lay_out(&wanted, Some(drawn), lang)?;
                            // The marks, drawn by the window, the labels at
                            // their own size, over a picture scaled down.
                            let tags = laid.page.take_marks();
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
                    .collect();
                laying.into_iter().map(|laid| laid.join().expect("laying out a state panicked")).collect()
            }),
            Err(e) => vec![Err(e)],
        };
        match laid.into_iter().collect::<anyhow::Result<Vec<Page>>>() {
            Ok(pages) => {
                let ways = self.ways_of(dir, state, &pages, drawn);
                Picture::Pages(Pages { dir: dir.to_path_buf(), state: state.cloned(), pages, ways })
            }
            Err(e) => self.faulty(vec![Finding { at: crate::SCREEN.to_string(), problem: format!("{e:#}") }]),
        }
    }

    /// Where acting on each node of `pages` goes from `state` of the
    /// description in `dir`, given the screens `drawn` under the root: by
    /// id, a node that goes nowhere left out.
    fn ways_of(
        &self,
        dir: &Path,
        state: Option<&State>,
        pages: &[Page],
        drawn: &std::collections::BTreeSet<String>,
    ) -> BTreeMap<String, Vec<Way>> {
        let Some(state) = state else {
            return BTreeMap::new();
        };
        let transitions = self.description(dir).and_then(|d| d.transitions()).unwrap_or_default();
        let mut ways = BTreeMap::new();
        for (id, handlers) in pages.iter().flat_map(|page| &page.handlers) {
            if ways.contains_key(id) {
                continue;
            }
            let goes = act::act(&transitions, state.name(), &handlers.written, &handlers.events).ways(drawn);
            if !goes.is_empty() {
                ways.insert(id.clone(), goes);
            }
        }
        ways
    }

    /// The journey on line `index` played from its step `at` (its last, past
    /// the end): that step's state drawn at each of the widths its tokens
    /// name, the node it
    /// acts on marked. One `check` finds broken shows its faults instead,
    /// and is not played. Kept in the window's history as a step when
    /// it is new.
    fn play(&mut self, index: usize, at: usize) {
        self.lay_out_step(index, at);
        self.arrive();
    }

    /// The journey on line `index` shown at its step `at`, as [`Gallery::play`]
    /// plays it, and kept in the history by it.
    fn lay_out_step(&mut self, index: usize, at: usize) {
        let line = &self.lines[index];
        let (file, faults) = (line.row.dir.clone(), line.row.faults.clone());
        self.line = Some(index);
        self.playing = None;
        self.picked = Picked::Nothing;
        self.panes.scroll_to(Scrolls::Down, 0.0);
        self.panes.scroll_to(Scrolls::Across, 0.0);
        if !faults.is_empty() {
            self.wanted = None;
            self.picture = self.faulty(faults);
            return;
        }
        let steps = match journeys::steps(&file) {
            Ok(steps) => steps,
            Err(e) => {
                self.picture = self.faulty(vec![Finding { at: String::new(), problem: format!("{e:#}") }]);
                return;
            }
        };
        let at = at.min(steps.len().saturating_sub(1));
        let Some(step) = steps.get(at) else {
            self.picture = self.faulty(Vec::new());
            return;
        };
        let state = draw::states(&step.dir)
            .ok()
            .and_then(|states| states.into_iter().find(|state| state.name() == step.state));
        let Some(state) = state else {
            let (within, state) = (gallery::within(&self.root, &step.dir), &step.state);
            let problem = say!(self.lang, no_such_state, within, state);
            self.picture = self.faulty(vec![Finding { at: format!("steps.{at}"), problem }]);
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
        let (Some(written), Picture::Pages(pages)) = (self.written_marked(), &self.picture) else {
            return Vec::new();
        };
        pages.marked(written)
    }

    /// The id, as written, of the node the journey played acts on in the
    /// step shown; none in the last step.
    fn written_marked(&self) -> Option<&str> {
        let playing = self.playing.as_ref()?;
        playing.steps[playing.at].on.as_ref().map(|(written, _)| written.as_str())
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
    /// on the lines under it, each drawn at the first of the widths its
    /// tokens name, and its transitions.
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
        let (description, width) = (&description, description.widths()[0]);
        // Each state laid out on a thread of its own, all at once.
        let drawings: Vec<anyhow::Result<draw::Drawing>> = std::thread::scope(|scope| {
            let laying: Vec<_> = states
                .iter()
                .map(|(_, state)| {
                    scope.spawn(move || {
                        Ok(description.lay_out(&state.wanted(width, 1), Some(drawn), lang)?.draw(1, u32::MAX))
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
                state: state.clone(),
                line,
                width: f64::from(drawing.page.width),
                height: f64::from(drawing.page.height),
                scene: drawing.scene,
                marks: drawing.marks,
                journeys: Vec::new(),
            });
        }
        // Each step of a journey listed in the tree that is in one of these
        // states, read as written: one that does not walk is marked too,
        // and picking it shows its faults.
        for (line, journey) in self.lines.iter().enumerate().filter(|(_, line)| line.journey) {
            for (step, on) in journeys::steps(&journey.row.dir).unwrap_or_default().iter().enumerate() {
                let thumb = thumbs.iter().position(|thumb| thumb.state.name() == on.state);
                let Some(thumb) = thumb.filter(|_| on.dir == dir) else { continue };
                let (name, number) = (&journey.row.name, step + 1);
                let label = say!(lang, journey_mark, name, number);
                let wide = f64::from(self.typesetter.text(&label, MARK, None).width) + 10.0;
                thumbs[thumb].journeys.push(Pass { line, step, label, wide });
            }
        }
        let at = |name: &str| thumbs.iter().position(|thumb| thumb.state.name() == name);
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
        Ok(Overview { dir: dir.to_path_buf(), thumbs, arrows })
    }

    /// The line of the description in `dir`.
    fn description_line(&self, dir: &Path) -> Option<usize> {
        self.lines.iter().position(|line| line.state.is_none() && line.row.description() == Some(dir))
    }

    /// The line of the directory `dir`, holding no description, picked to
    /// show its flow.
    fn directory_line(&self, dir: &Path) -> Option<usize> {
        self.lines.iter().position(|line| {
            line.state.is_none() && !line.journey && line.row.description().is_none() && line.row.dir == dir
        })
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
        Flow { dir: dir.to_path_buf(), map, tiles }
    }

    /// The box of the description in `dir`: its first state at the first of
    /// the widths its tokens name, or how many faults keep it from being
    /// drawn.
    fn tile(&self, dir: &Path, drawn: &std::collections::BTreeSet<String>) -> Tile {
        let line = self.description_line(dir);
        let faults = line.map_or(0, |index| self.lines[index].row.faults.len());
        if faults > 0 {
            return Tile::Faulty(faults);
        }
        let first = line.and_then(|index| self.lines.get(index + 1)).and_then(|next| next.state.clone());
        let laid = self.description(dir).and_then(|description| {
            let width = description.widths()[0];
            let wanted = match first {
                Some(state) => state.wanted(width, 1),
                None => Wanted { fixture: None, phase: None, width, scale: 1 },
            };
            description.lay_out(&wanted, Some(drawn), self.lang)
        });
        match laid {
            Ok(laid) => {
                let drawing = laid.draw(1, u32::MAX);
                let width = f64::from(drawing.page.width);
                Tile::Picture { scene: Box::new(drawing.scene), marks: Box::new(drawing.marks), width }
            }
            Err(_) => Tile::Faulty(1),
        }
    }

    /// What ⌘C copies, and what the conversation in the sidebar is on: the
    /// node picked, as the description writes it, and the instance of it
    /// pointed at (in the picture it was picked in, or the first that has
    /// it); or, with none picked, the state shown, the whole screen whose
    /// overview is shown, or the screens under the directory whose flow is,
    /// or the whole journey picked, through every step it plays; or the
    /// arrow picked in an overview or a flow. None where nothing is.
    fn reference(&self) -> Option<Reference> {
        let picked = match &self.picked {
            Picked::Nothing if self.journey_picked().is_some() => return self.journey_picked(),
            Picked::Nothing => Target::Caption,
            Picked::Node { id, pointed } => Target::Node { id: id.clone(), pointed: *pointed },
            Picked::Arrow(arrow) => Target::Arrow(*arrow),
        };
        self.picture.shows().reference(&picked)
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
        let warmed = self.panes.hover(self.cursor);
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
        self.placed()?.at(at).cloned()
    }

    /// The sidebar as it now stands, where the panes show it and scrolled
    /// as they scroll it: laid out again only when what it shows, or its
    /// size, changed since it last was. None while it is hidden.
    fn placed(&mut self) -> Option<sidebar::Placed<'_, Do>> {
        let side = self.panes.side()?;
        let (cards, foot) = (self.sidebar_cards(), self.foot());
        let top = -self.panes.scrolled(Scrolls::Side);
        Some(self.sidebar.lay(cards, foot, (side.x0, side.width()), (top, side.y1), &mut self.typesetter))
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
    /// click does: the journey picked, as a whole; the journey played, its
    /// step and the steps beside it;
    /// what the last act on a node came to, and any outcomes to choose
    /// from; a ✓ waiting to be written, to undo; the assumption on the node
    /// picked, to answer; then what is picked: the node (what it is, what
    /// it calls, and, folded into "details", its id and its label and box
    /// at each width), the arrow, or, with neither, the state shown; then
    /// its note, as a conversation, newest at the bottom. None when none of
    /// these is there.
    fn cards(&self) -> Vec<Card<Do>> {
        let mut cards: Vec<Card<Do>> = self.journey_card().into_iter().chain(self.playing_card()).collect();
        if let Some(Acted { says, choices }) = &self.acted {
            let buttons =
                choices.iter().map(|(said, does)| Block::Button(said.clone(), does.clone())).collect();
            cards.push(Card::new(says.clone(), buttons));
        }
        cards.extend(self.confirming_card(Instant::now()));
        cards.extend(self.assumption_card());
        cards.extend(
            self.node_card()
                .or_else(|| self.arrow_card())
                .or_else(|| self.state_card())
                .or_else(|| self.screen_card())
                .or_else(|| self.screens_card()),
        );
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
            Block::Muted(say!(lang, hint_marks)),
            Block::Muted(say!(lang, hint_keys)),
        ];
        cards.push(Card { ink: Ink::Muted, ..Card::new(say!(lang, nothing_picked), blocks) });
        cards
    }

    /// The descriptions and directories whose inbox the sidebar shows: the
    /// description picked, or whose state is, or the directory picked and
    /// each description and directory under it, in the order the tree
    /// lists them. None while a node or an arrow is picked, or a
    /// journey played.
    fn inboxed(&self) -> Vec<PathBuf> {
        // The node picked is kept through an overview, for the next state:
        // it is picked only while a state is drawn.
        let drawn = matches!(self.picture, Picture::Pages(_));
        let busy = drawn && self.node().is_some() || self.arrow().is_some();
        if busy || self.playing.is_some() {
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
            .map(|line| line.row.dir.as_path())
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
                let wanted = state.wanted(description.widths()[0], 1);
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
                    Item::Unread(note) => {
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
    /// description's overview with its transition picked, or with nothing
    /// picked for the whole screen, or the flow its navigate is drawn in
    /// with that picked, or a directory's flow, for the screens under it,
    /// with nothing picked; and so its conversation in
    /// the sidebar. The state shown already is not drawn again. Whether
    /// there was anything to show.
    fn visit(&mut self, visit: Visit) -> bool {
        self.path.clear();
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
            On::Whole { whole } => {
                let line = match whole {
                    Whole::Screen => self.description_line(&dir),
                    Whole::Screens => self.directory_line(&dir),
                };
                let Some(index) = line else {
                    return false;
                };
                self.pick(index);
                self.picked = Picked::Nothing;
            }
            On::Journey { .. } => {
                let wanted = Reference { description: dir.clone(), on: on.clone() };
                let Some(index) = self.lines.iter().position(|line| {
                    line.journey && journeys::noted(&line.row.dir).as_ref() == Some(&wanted)
                }) else {
                    return false;
                };
                self.pick(index);
                self.picked = Picked::Nothing;
            }
            On::Transition { .. } | On::Navigate { .. } => {
                // A transition's arrow is in its description's overview; a
                // navigate's, in the flow of a directory above it.
                let shown_in: Vec<usize> = match &on {
                    On::Transition { .. } => self.description_line(&dir).into_iter().collect(),
                    _ => dir
                        .ancestors()
                        .skip(1)
                        .filter_map(|above| self.directory_line(above))
                        .chain(self.description_line(&dir))
                        .collect(),
                };
                let wanted = Reference { description: dir.clone(), on: on.clone() };
                for index in shown_in {
                    self.pick(index);
                    if let Some(at) =
                        self.picture.arrow_references().iter().position(|arrow| *arrow == wanted)
                    {
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
        let Picture::Pages(Pages { pages, .. }) = &self.picture else {
            return;
        };
        let mut order: Vec<usize> = (0..pages.len()).collect();
        order.sort_by_key(|&at| Some(pages[at].width as u32) != width);
        if let Some((pointed, id)) = order.into_iter().find_map(|at| Some((at, found(&pages[at])?))) {
            self.picked = Picked::Node { id, pointed };
        }
    }

    /// The card of the arrow picked: what it is.
    fn arrow_card(&self) -> Option<Card<Do>> {
        self.arrow()?;
        let said = self.reference()?.on.said(self.lang);
        Some(Card::new(said.strip_prefix("on ").unwrap_or(&said).to_string(), Vec::new()))
    }

    /// The card of the assumption on the node picked, for the person to
    /// answer, above the conversation on it: what was guessed, in large
    /// type, what else it could be, then ✓ and ✗ and what each means; or,
    /// once ✗ is clicked, how to say what is right in the box below, in
    /// their place. The answers already given are messages of the
    /// conversation. None when the node picked has none.
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
        if self.rejecting.as_ref().is_some_and(|rejecting| rejecting.on == on) {
            blocks.push(Block::Muted(say!(lang, rejecting_hint)));
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
            pages.pages.iter().find_map(|page| page.nodes.iter().find(|n| n.id == id)?.assumed.clone())?;
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
        let screen = crate::read(&on.description).ok().map(|(screen, _)| screen);
        let id = self.node().map(|(id, _)| id.to_string()).unwrap_or_default();
        let component = pages
            .pages
            .iter()
            .find_map(|page| page.nodes.iter().find(|n| n.id == id))
            .map(|n| n.component.clone())
            .unwrap_or_default();
        let guess = match &screen {
            Some(screen) => Guess::of(screen, &node.origin.written, &component, &links::drawn(&self.root)),
            None => Guess::Component(component),
        };
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
            Answer::Wrong => self.rejecting = Some(Rejecting { on, answers }),
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
    /// ([`Gallery::reference`]), in the order written: its note, and any
    /// other on the same thing, written before a note held every answer
    /// and proposal on it, each a part of the one conversation. Their
    /// description's directory with them; none where nothing is picked to
    /// talk about.
    fn conversation(&self) -> Option<(Reference, Vec<&Note>)> {
        let on = self.reference()?;
        let notes = self.notes.get(&on.description).map(Vec::as_slice).unwrap_or_default();
        let held = notes.iter().filter(|note| note.on.same(&on.on)).collect();
        Some((on, held))
    }

    /// The card of the conversation on what is picked: its note's id and
    /// where it stands, then what the person and the agent said, each in a
    /// bubble, in the order said, newest at the bottom, a message that
    /// answers an assumption or proposes where an act goes under a line
    /// saying so, and under the person's last message, while it waits,
    /// whether the agent has taken it and how long ago; or, where nothing
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
        for said in notes.iter().flat_map(|note| &note.messages) {
            match &said.mark {
                Some(Mark::Answers(answers)) => {
                    let mark = answered(answers.answer, lang);
                    blocks.push(Block::Muted(say!(lang, marked_answers, mark)));
                }
                Some(Mark::Proposes(_)) => blocks.push(Block::Muted(say!(lang, marked_proposes))),
                None => {}
            }
            blocks.push(Block::Bubble(said.by, said.says.clone()));
        }
        match last.taken() {
            Some(taken) => {
                let since = since(self.now().saturating_sub(taken));
                blocks.push(Block::Muted(say!(lang, agent_changing, since)));
            }
            None if last.waits() => blocks.push(Block::Muted(say!(lang, waiting_for_agent))),
            None => {}
        }
        let ink = if last.waits() { Ink::Note } else { Ink::Answered };
        Some(Card { ink, ..Card::new(title, blocks) })
    }

    /// The box at the sidebar's foot, while there is something picked to
    /// talk about: what it holds, and Send, which sends it unless it is
    /// empty; asking what is right while a ✗ on it is clicked.
    fn foot(&self) -> Option<Foot<Do>> {
        let on = self.reference()?;
        let lang = self.lang;
        let rejecting = self.rejecting.as_ref().is_some_and(|rejecting| rejecting.on == on);
        Some(Foot {
            field: self.sidebar.talk().shown(),
            empty: if rejecting { say!(lang, ask_right) } else { say!(lang, talk_box) },
            send: say!(lang, send),
            does: (!self.sidebar.talk().is_blank()).then_some(Do::Send),
        })
    }

    /// The field keys and an input method type into: the box at the
    /// sidebar's foot while it is shown; none while it is not.
    fn field_mut(&mut self) -> Option<&mut Field> {
        self.typing().then_some(self.sidebar.talk_mut())
    }

    /// Whether there is a field keys type into: see [`Gallery::field_mut`].
    fn typing(&self) -> bool {
        self.panes.side().is_some() && self.reference().is_some()
    }

    /// The ✗ clicked on what is picked, while it is: Send sends the box as
    /// it.
    fn rejecting(&self) -> Option<&Rejecting> {
        let on = self.reference()?;
        self.rejecting.as_ref().filter(|rejecting| rejecting.on == on)
    }

    /// What the box at the sidebar's foot holds said in the conversation on
    /// what is picked, added to its note in notes.json, or a note started
    /// on it, and shown at once, scrolled to; the box emptied. While a ✗
    /// on it is clicked, sent as a message marked as rejecting the
    /// assumption, and refused when empty, as ✗ says what is right;
    /// otherwise nothing when it is empty. Where it cannot be written, the
    /// panel says why and the box keeps it.
    fn send(&mut self) {
        let Some(on) = self.reference() else {
            return;
        };
        let lang = self.lang;
        let sent = match self.rejecting() {
            Some(rejecting) => notes::judge(&on, rejecting.answers.clone(), &self.sidebar.talk().text())
                .map(|id| Some(say!(lang, note_sent, id, done = say!(lang, done_rejected)))),
            None if self.sidebar.talk().is_blank() => return,
            None => notes::say(&on, &self.sidebar.talk().text()).map(|_| None),
        };
        match sent {
            Ok(rejected) => {
                if let Some(says) = rejected {
                    self.rejecting = None;
                    self.say(says);
                }
                self.notes = notes_of(&self.lines);
                self.sidebar.empty(&self.typesetter);
                self.clamp();
                self.panes.scroll_to(Scrolls::Side, f64::MAX);
            }
            Err(e) => self.say(say!(lang, not_saved, e = format!("{e:#}"))),
        }
    }

    /// A key pressed, ⌘ not held, as `key` names it and with the `text` it
    /// types. Into the field written in ([`Gallery::field_mut`]): an edit,
    /// Enter sending it and Shift+Enter breaking a line, the keys an input
    /// method takes while it composes left to it, so that the Enter that
    /// commits what it composes sends nothing. ← and → step a journey
    /// played while the box is empty. Esc takes back a ✗ clicked, else
    /// what is picked. Whether that changed what is shown.
    fn key(&mut self, key: &Key, text: Option<&str>) -> bool {
        let stepping = self.playing.is_some() && self.rejecting().is_none() && self.sidebar.talk().is_blank();
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
                if self.rejecting().is_some() {
                    self.rejecting = None;
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
                self.send();
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
        self.placed()?.field()
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
        let on = over(field, at);
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

    /// The notes on what is shown, drawn on it where its picture says
    /// ([`picture::Shows::pins`]); blue while waiting on the agent, green
    /// while its answer is unread, faint gray once read. None while the
    /// marks are hidden.
    fn pins(&self) -> Vec<Pin> {
        if !self.marks {
            return Vec::new();
        }
        self.picture.shows().pins(&self.notes, &self.desk())
    }

    /// The note whose pin, not a faint dot, is under `at`, a point in the
    /// window: its description's directory, and its id.
    fn pin_at(&self, at: (f64, f64)) -> Option<(PathBuf, String)> {
        self.pins()
            .into_iter()
            .rev()
            .find_map(|pin| (over(pin.rect, at) && !pin.faint).then_some((pin.dir, pin.id)))
    }

    /// The card of the state shown, with no node picked: its name, for the
    /// conversation under it on the whole state. None where no state is
    /// drawn, or the state is a step of the journey picked, whose
    /// conversation is the journey's.
    fn state_card(&self) -> Option<Card<Do>> {
        let Picture::Pages(_) = self.picture else {
            return None;
        };
        if self.journey_picked().is_some() {
            return None;
        }
        let state = self.state_shown()?;
        Some(Card::new(say!(self.lang, whole_state, state = state.name()), Vec::new()))
    }

    /// The card of the description picked, its overview shown with no
    /// arrow picked: the screen's name, for the conversation under it on
    /// the whole screen. None where no overview is.
    fn screen_card(&self) -> Option<Card<Do>> {
        let Picture::Overview(overview) = &self.picture else {
            return None;
        };
        let named = self.description(&overview.dir).ok();
        let screen = named.as_ref().map_or("", |description| description.name());
        let screen =
            if screen.is_empty() { gallery::within(&self.root, &overview.dir) } else { screen.to_string() };
        Some(Card::new(say!(self.lang, whole_screen, screen), Vec::new()))
    }

    /// The card of the journey picked, played or broken: its name, for the
    /// conversation on the whole journey, kept above the step it plays as
    /// it steps. None while no journey is picked.
    fn journey_card(&self) -> Option<Card<Do>> {
        let Some(On::Journey { journey }) = self.journey_picked().map(|picked| picked.on) else {
            return None;
        };
        Some(Card { ink: Ink::Note, ..Card::new(say!(self.lang, whole_journey, journey), Vec::new()) })
    }

    /// The whole journey on the line picked, as a note is pinned to it;
    /// none while the line picked is no journey.
    fn journey_picked(&self) -> Option<Reference> {
        let line = self.line.and_then(|index| self.lines.get(index)).filter(|line| line.journey)?;
        journeys::noted(&line.row.dir)
    }

    /// The card of the directory picked, its flow shown with no arrow
    /// picked: the directory's name, as the tree lists it, for the
    /// conversation under it on the screens under it, as a group. None
    /// where no flow is.
    fn screens_card(&self) -> Option<Card<Do>> {
        let Picture::Flow(flow) = &self.picture else {
            return None;
        };
        let line = self.directory_line(&flow.dir).and_then(|index| self.lines.get(index));
        let directory =
            line.map_or_else(|| gallery::within(&self.root, &flow.dir), |line| line.row.name.clone());
        Some(Card::new(say!(self.lang, these_screens, directory), Vec::new()))
    }

    /// The card of the node picked: what it is (its component), the
    /// operation it calls, what was assumed there, and a button that folds
    /// out its details: its id, and its label and box at each width. None
    /// when none is.
    fn node_card(&self) -> Option<Card<Do>> {
        let (Some((id, _)), Picture::Pages(Pages { pages, .. })) = (self.node(), &self.picture) else {
            return None;
        };
        let lang = self.lang;
        let (page, node) =
            pages.iter().find_map(|page| Some((page, page.nodes.iter().find(|n| n.id == id)?)))?;
        let calls = match page.calls.get(id) {
            Some(operations) => operations.join(", "),
            None => say!(lang, calls_nothing),
        };
        let mut blocks = vec![Block::Fact(say!(lang, calls), calls)];
        let ways = self.ways();
        for way in ways {
            let Offer { says, live } = self.offer(way);
            blocks.push(if live { Block::Button(says, Do::Go(way.clone())) } else { Block::Muted(says) });
        }
        if !ways.is_empty() {
            blocks.push(Block::Muted(say!(lang, way_hint)));
        }
        let (fold, details) = (if self.details { "▾" } else { "▸" }, lang.words().details);
        blocks.push(Block::Fold(format!("{fold} {details}"), Do::Details));
        if self.details {
            blocks.push(Block::Fact(say!(lang, id), id.to_string()));
            for page in pages {
                let width = page.width;
                let at = say!(lang, at_width, width);
                blocks.push(Block::Fact(
                    at,
                    match page.nodes.iter().find(|n| n.id == id) {
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
    /// `state`: on the whole description, or on that state of it; or on
    /// the screens under a directory, as a group; or on a whole journey.
    fn waiting_notes(&self, row: &Row, state: Option<&State>) -> usize {
        if let Some(journey) = journeys::noted(&row.dir) {
            let notes = self.notes.get(&journey.description).map(Vec::as_slice).unwrap_or_default();
            return notes.iter().filter(|note| note.waits() && note.on == journey.on).count();
        }
        let Some(notes) = self.notes.get(&row.dir) else {
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

    /// How wide and tall what the pictures' pane shows is, all of it, the
    /// space around it included: wider than the pane only for a state's
    /// pages at [`SMALLEST`].
    fn pictures_size(&mut self) -> (f64, f64) {
        let desk = self.desk();
        self.picture.shows().size(&desk, &mut self.typesetter)
    }

    /// The panes fitted to how large what each holds is at their widths:
    /// each width, and each scroll, held within its bounds: the sidebar,
    /// while it is shown, laid out for how tall its cards are and how tall
    /// the box at its foot is.
    fn clamp(&mut self) {
        let bottom = self.panes.window().1;
        let (side, foot) =
            self.placed().map_or((0.0, 0.0), |placed| (placed.height(), bottom - placed.foot()));
        let pictures = self.pictures_size();
        let content = Content { rows: self.lines.len(), pictures, side, foot };
        self.panes.fit(content);
    }

    /// What the window marks on the picture shown.
    fn seen(&self) -> Seen {
        let picked = match &self.picked {
            Picked::Nothing => None,
            Picked::Node { id, pointed } => Some(Target::Node { id: id.clone(), pointed: *pointed }),
            Picked::Arrow(arrow) => Some(Target::Arrow(*arrow)),
        };
        let lit = self.acting || self.flash_ends(Instant::now()).is_some();
        let offers = self.ways().iter().map(|way| self.offer(way)).collect();
        let marked = self.written_marked().map(str::to_string);
        Seen { picked, marked, offers, lit, marks: self.marks, lang: self.lang }
    }

    /// The tree's lines, in window points, top to bottom: for each, the
    /// fill behind it when it is picked, the marks at its right end (its
    /// notes waiting, then its faults), then its name.
    fn tree_draws(&mut self) -> Vec<Draw> {
        let tree = self.panes.pictures().x0;
        let mut list = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let Line { row, state, .. } = line;
            let y = self.panes.row_top(index);
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
        list
    }

    /// The whole window, in points.
    fn frame(&mut self) -> Scene {
        let (width, height) = self.panes.window();
        // The pictures' thumbs drawn under their labels so that none covers
        // an orange "?", the tree's and the sidebar's over the rest in the
        // space right of the tree's names and of the sidebar's cards.
        let thumbs = self.panes.bars().into_iter().filter_map(|(scrolls, bar)| Some((scrolls, bar.thumb()?)));
        let hot = self.panes.hot();
        let (pictures_bars, side_bars): (Vec<_>, Vec<_>) =
            thumbs.partition(|(scrolls, _)| matches!(scrolls, Scrolls::Down | Scrolls::Across));
        let tree = self.panes.pictures().x0;
        let list = self.tree_draws();
        let (pins, reply) = (self.pin_draws(), self.reply_shown());
        let seen = self.seen();
        let right = self.panes.pictures().x1;
        let caret = self.caret_shows(Instant::now());
        let (side, foot, cut_at) = match self.placed() {
            Some(placed) => {
                let edge = Draw::Line {
                    from: [right as f32, 0.0],
                    to: [right as f32, height as f32],
                    ink: Ink::Line,
                };
                let ((cards, foot), cut_at) = (placed.draws(caret), placed.foot());
                (std::iter::once(edge).chain(cards).collect(), foot, cut_at)
            }
            None => (Vec::new(), Vec::new(), height),
        };

        let mut scene = Scene::new();
        let at = Affine::IDENTITY;
        scene.fill(Fill::NonZero, at, chrome::SIDE, None, &Area::new(0.0, 0.0, tree, height));
        scene.fill(Fill::NonZero, at, chrome::DESK, None, &Area::new(tree, 0.0, right, height));
        scene.fill(Fill::NonZero, at, chrome::SIDE, None, &Area::new(right, 0.0, width, height));
        scene.push_clip_layer(Fill::NonZero, at, &Area::new(tree, 0.0, right, height));
        let desk = self.desk();
        let mut over = self.picture.shows().draw(&mut scene, &desk, &seen, &mut self.typesetter);
        over.extend(pins);
        over.extend(reply);
        // The labels under what the window draws over the pictures: the
        // outline, the pins.
        for (scrolls, bar) in pictures_bars {
            thumb(&mut scene, bar, hot == Some(scrolls));
        }
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

    /// The window as it now stands, in points: the toolbar along its top,
    /// on the tree's surface, and the panes under it.
    fn drawn(&mut self) -> Scene {
        self.clamp();
        let width = self.panes.window().0;
        let tools = self.tools();
        let mut scene = Scene::new();
        let at = Affine::IDENTITY;
        scene.fill(Fill::NonZero, at, chrome::SIDE, None, &Area::new(0.0, 0.0, width, TOOLBAR));
        let below = Affine::translate((0.0, TOOLBAR));
        scene.append(&self.frame(), Some(below));
        paint::draws(&mut scene, below, &toolbar::draws(width, &tools), chrome::colour, &mut self.typesetter);
        scene
    }
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
    if note.waits() { say!(lang, status_waiting) } else { say!(lang, status_answered) }
}

/// How long ago, `seconds`, as the sidebar counts it: `23s`, `4m 05s`,
/// `2h 03m`.
fn since(seconds: u64) -> String {
    match seconds {
        ..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, seconds % 3600 / 60),
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

/// The node picked, at `node` in window points: washed and outlined in the
/// interaction ink, the outline's paper hairline against the node's edge.
fn picked(scene: &mut Scene, node: Area, radius: f32) {
    let at = Affine::IDENTITY;
    let r = f64::from(radius);
    scene.fill(Fill::NonZero, at, chrome::INTERACTION_FILL, None, &node.to_rounded_rect(r));
    let paper = node.inflate(0.5, 0.5).to_rounded_rect(r + 0.5);
    scene.stroke(&vello::kurbo::Stroke::new(1.0), at, chrome::colour(Ink::Paper), None, &paper);
    let out = 1.0 + chrome::PICKED_STROKE / 2.0;
    let outline = node.inflate(out, out).to_rounded_rect(r + out);
    scene.stroke(&vello::kurbo::Stroke::new(chrome::PICKED_STROKE), at, chrome::INTERACTION, None, &outline);
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

    /// Woken at the time [`App::about_to_wait`] set: drawn again, for the
    /// caret to blink.
    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if let (StartCause::ResumeTimeReached { .. }, Some(shown)) = (cause, &self.shown) {
            shown.window.request_redraw();
        }
    }

    /// While a ✓ waits to be written: written once its time comes, the
    /// window woken each second until then to count down. While the
    /// conversation shown counts how long ago the agent took it, woken
    /// each second too. While nodes flash, woken as the flash ends. While the field written in is the one keys go to,
    /// woken as its caret shows or hides, and only then.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let written = self.gallery.confirm(now);
        let confirming = self.gallery.confirming.as_ref().map(|confirming| {
            let left = confirming.at.saturating_duration_since(now);
            let tick = Duration::from_nanos((left.as_nanos() % 1_000_000_000) as u64);
            now + if tick.is_zero() { Duration::from_secs(1).min(left) } else { tick }
        });
        let counting = self.gallery.counting().then(|| now + Duration::from_secs(1));
        let flash = self.gallery.flash_ends(now);
        let wake = confirming.into_iter().chain(counting).chain(flash).min();
        if let Some(shown) = &self.shown
            && (written || wake.is_some())
        {
            shown.window.request_redraw();
        }
        let wake = wake.into_iter().chain(self.gallery.blinks(now)).min();
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
            WindowEvent::Focused(focused) => Input::Focused(focused),
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
    use crate::act::Leads;
    use crate::chrome::{CAPTION, MARGIN};
    use crate::chrome::{SIDEBAR, SIDEBAR_LEAST, TREE, TREE_LEAST};

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

        /// The pointer moved to `at`, in the gallery's points, from the
        /// toolbar's foot. Whether that changed what is shown.
        fn move_to(&mut self, at: (f64, f64)) -> bool {
            self.feed([Input::Moved((at.0, at.1 + TOOLBAR))]).redraw
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
                "." => KeyCode::Period,
                "[" => KeyCode::BracketLeft,
                "]" => KeyCode::BracketRight,
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

    /// The pages of the state shown.
    fn pages_of(gallery: &Gallery) -> &Pages {
        let Picture::Pages(pages) = &gallery.picture else { panic!("no state's pages are shown") };
        pages
    }

    /// Where each page of the state shown is, in the window.
    fn origins(gallery: &Gallery) -> Vec<(f64, f64)> {
        pages_of(gallery).origins(&gallery.desk())
    }

    /// How much the pages or the flow shown are scaled down.
    fn zoom(gallery: &Gallery) -> f64 {
        let pane = gallery.panes.pictures().width();
        match &gallery.picture {
            Picture::Pages(pages) => pages.zoom(pane),
            Picture::Flow(flow) => flow.zoom(pane),
            Picture::Overview(_) | Picture::Faults(_) => 1.0,
        }
    }

    /// What the picture shown draws over itself, in window points.
    fn overlays(gallery: &mut Gallery) -> Vec<Draw> {
        let (desk, seen) = (gallery.desk(), gallery.seen());
        gallery.picture.shows().draw(&mut Scene::new(), &desk, &seen, &mut gallery.typesetter)
    }

    /// Where each arrow of the overview or flow shown is labelled, in the
    /// window.
    fn arrow_labels(gallery: &Gallery) -> Vec<Rect> {
        let desk = gallery.desk();
        match &gallery.picture {
            Picture::Overview(overview) => {
                overview.paths(&desk).into_iter().map(|(_, label)| label).collect()
            }
            Picture::Flow(flow) => flow.spots(&desk).into_iter().map(|(label, _)| label).collect(),
            Picture::Pages(_) | Picture::Faults(_) => Vec::new(),
        }
    }

    /// Where each state of the overview shown is, in the window.
    fn cells(gallery: &Gallery) -> Vec<(f64, f64, f64, f64)> {
        let Picture::Overview(overview) = &gallery.picture else { panic!("no overview is shown") };
        overview.cells(&gallery.desk())
    }

    /// The faults shown, listed from the pane's top.
    fn faults_listed(gallery: &mut Gallery) -> Vec<Draw> {
        let Picture::Faults(faults) = &gallery.picture else { panic!("no faults are shown") };
        faults.list(gallery.panes.pictures().width(), gallery.lang, &mut gallery.typesetter).0
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
        gallery.move_to((40.0, gallery.panes.row_top(index) + ROW / 2.0));
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
            Picture::Pages(Pages { pages, .. }) => pages.iter().map(|page| page.caption.as_str()).collect(),
            Picture::Faults(Faults { findings: faults, .. }) => {
                panic!("no picture: it has faults: {faults:?}")
            }
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
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("docs has no faults") };
        let widths: Vec<f64> = pages.iter().map(|page| page.width).collect();
        assert_eq!(widths, vec![390.0, 1120.0]);
        assert_eq!(captions(&gallery), ["default · 390", "default · 1120"]);
        assert!(!gallery.press_here(), "picking it again changes nothing");
        gallery.press_at((gallery.panes.pictures().x0 + 100.0, 100.0));
        assert_eq!(gallery.line, Some(docs + 1), "a press on the pictures leaves the state picked");
    }

    #[test]
    fn a_state_is_drawn_at_the_widths_its_tokens_name() {
        let root = todo_root("widths");
        let tokens_file = root.join("todo").join(crate::TOKENS);
        let mut tokens: Value =
            serde_json::from_str(&std::fs::read_to_string(&tokens_file).unwrap()).unwrap();
        tokens["widths"] = json!([360, 1440, 800]);
        std::fs::write(&tokens_file, tokens.to_string()).unwrap();
        let mut gallery = opened(&root, (1440.0, 900.0));
        let todo = line_of(&gallery, "todo");
        over_row(&mut gallery, todo + 1);
        assert!(gallery.press_here());
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("todo has no faults") };
        let widths: Vec<f64> = pages.iter().map(|page| page.width).collect();
        assert_eq!(widths, vec![360.0, 1440.0, 800.0], "in the order named");
        over_row(&mut gallery, todo);
        assert!(gallery.press_here());
        let Picture::Overview(overview) = &gallery.picture else { panic!("not an overview") };
        assert!(overview.thumbs.iter().all(|thumb| thumb.width == 360.0), "the overview at the first");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The overview shown: each state's name, and each arrow as (from, to,
    /// label).
    fn overview(gallery: &Gallery) -> (Vec<&str>, Vec<(&str, &str, &str)>) {
        let Picture::Overview(overview) = &gallery.picture else { panic!("not an overview") };
        let names = overview.thumbs.iter().map(|thumb| thumb.state.name()).collect();
        let arrows = overview
            .arrows
            .iter()
            .map(|arrow| {
                let name = |index: usize| overview.thumbs[index].state.name();
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
        let retry =
            shown.arrows.iter().find(|arrow| shown.thumbs[arrow.from].state.name() == "error").unwrap();
        let error = orders + 1 + names.iter().position(|name| *name == "error").unwrap();
        let drawing = draw::Description::read(&drafts().join("orders"))
            .unwrap()
            .lay_out(
                &gallery.lines[error].state.as_ref().unwrap().wanted(underdraw_layout::DEFAULT_WIDTHS[0], 1),
                None,
                Lang::En,
            )
            .unwrap();
        let label = drawing.page.nodes.iter().find(|node| node.id == "retry").unwrap().label.unwrap();
        assert_eq!(retry.label, format!("{label} · ok"));

        let cells = cells(&gallery);
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
        let tall = gallery.panes.window().1;
        assert_eq!(tall, 900.0 - TOOLBAR, "the panes are below the toolbar");
        let end = gallery.pictures_size().1 - tall;
        assert!(end > 0.0, "the listing is taller than the window");
        gallery.move_to((gallery.panes.pictures().x0 + 100.0, 400.0));
        gallery.wheel_by(0.0, 1e6);
        assert_eq!(gallery.panes.scrolled(Scrolls::Down), end, "as far as its end");
        let bar = gallery.panes.bar(Scrolls::Down).unwrap();
        assert_eq!(bar.content, end + tall, "its scrollbar over all of it");
        gallery.wheel_by(0.0, -1e6);
        assert_eq!(gallery.panes.scrolled(Scrolls::Down), 0.0, "no further than the top");
    }

    #[test]
    fn a_press_on_a_scrollbar_or_a_line_between_panes_picks_nothing_and_presses_nothing() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        gallery.feed([Input::Resized((1440.0, 300.0))]);
        let (line, picked) = (gallery.line, gallery.picked.clone());
        let mut presses = Vec::new();
        for scrolls in [Scrolls::Tree, Scrolls::Down, Scrolls::Across, Scrolls::Side] {
            let bar = gallery.panes.bar(scrolls).unwrap();
            let thumb = bar.thumb().unwrap_or_else(|| panic!("{scrolls:?} overflows a short window"));
            presses.push((format!("{scrolls:?}"), (thumb.center().x, thumb.center().y)));
            presses.push((format!("{scrolls:?}'s track"), bar.lane().unwrap().center().into()));
        }
        let pictures = gallery.panes.pictures();
        presses.push(("the tree's line".into(), (pictures.x0, 150.0)));
        presses.push(("the sidebar's line".into(), (pictures.x1, 150.0)));
        for (what, at) in presses {
            gallery.move_to(at);
            gallery.press_here();
            assert_eq!(gallery.line, line, "{what}: no line of the tree picked");
            assert_eq!(gallery.picked, picked, "{what}: no node picked");
            assert!(gallery.rejecting.is_none() && gallery.acted.is_none(), "{what}: no button pressed");
            gallery.move_by((0.0, 5.0));
            gallery.let_go();
            assert!(gallery.selecting.is_none(), "{what}: no text selected");
        }
    }

    /// The id of the node `gallery` holds picked, as [`Picked`] says it.
    fn picked_node(gallery: &Gallery) -> Option<&str> {
        gallery.node().map(|(id, _)| id)
    }

    /// The point in the window at the middle of node `id` in picture `page`.
    fn on_node(gallery: &Gallery, page: usize, id: &str) -> (f64, f64) {
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else {
            panic!("no picture: it has faults")
        };
        let node = pages[page].nodes.iter().find(|n| n.id == id).unwrap();
        let (left, top) = origins(gallery)[page];
        let zoom = zoom(gallery);
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
    fn a_node_picked_in_the_picture_is_marked_and_says_what_it_is() {
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
        let Picture::Pages(pages) = &gallery.picture else { panic!("a state is shown") };
        assert_eq!(pages.picked("nav-save", &gallery.desk()).len(), 2, "it is marked at both widths");
        assert!(
            !overlays(&mut gallery).iter().any(|d| matches!(d, Draw::Box { stroke: Some(Ink::Strong), .. })),
            "in the interaction ink, not the wireframe's strong one"
        );
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
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("no picture") };
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
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("no picture") };
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
        assert!(gallery.reference().is_none(), "a flow is no state");
        gallery.chord("c");
        assert_eq!(gallery.panel()[0], "nothing to copy: pick a state, a node in it, or an arrow, first");
        let loading = state_of_in(&gallery, orders, "loading");
        showing(&mut gallery, loading);
        let Picture::Pages(_) = &gallery.picture else { panic!("no picture") };
        // A click on a picture's caption picks no node: the state itself.
        let (left, top) = origins(&gallery)[0];
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
            let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { return false };
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
        let (left, top) = origins(&gallery)[1];
        assert!(gallery.press_at((left - MARGIN / 2.0, top + 10.0)), "between the pictures");
        assert_eq!(picked_node(&gallery), None);
        assert_eq!(gallery.panel()[0], "default · the whole state", "the node's card is gone");
    }

    #[test]
    fn the_sidebar_is_on_the_right_and_command_backslash_gives_its_width_to_the_pictures() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, 0);
        assert_eq!(gallery.panes.pictures().x1, 1440.0 - SIDEBAR, "the sidebar is shown by default");
        let shown = gallery.sidebar_cards();
        assert!(shown[0].title.ends_with(" · these screens"), "a flow is talked about as a group");
        assert!(shown.iter().all(|card| card.title != "Nothing picked"));
        showing(&mut gallery, listing);
        let shown = gallery.sidebar_cards();
        assert!(shown[0].title.ends_with(" · the whole screen"), "a description is talked about as a whole");
        assert!(shown.iter().all(|card| card.title != "Nothing picked"));
        showing(&mut gallery, listing + 1);
        gallery.cursor = on_node(&gallery, 0, "nav-save");
        gallery.press();
        let narrow = zoom(&gallery);
        let (left, _) = origins(&gallery)[1];
        assert!(left + 1120.0 * narrow <= gallery.panes.pictures().x1, "the pictures end before the sidebar");
        assert!(
            !gallery.press_at((1440.0 - SIDEBAR / 2.0, 890.0)),
            "a press in the sidebar off its buttons does nothing"
        );
        assert_eq!(picked_node(&gallery), Some("nav-save"), "and leaves the node picked");

        gallery.chord("\\");
        assert_eq!(gallery.panes.pictures().x1, 1440.0, "hidden: the pictures take the width");
        assert!(zoom(&gallery) > narrow, "and are drawn larger");
        assert!(
            gallery.press_at(on_node(&gallery, 1, "nav-save")),
            "a node near the window's right edge is picked, no sidebar there"
        );
        assert_eq!(gallery.node().map(|(_, pointed)| pointed), Some(1));
        gallery.chord("\\");
        assert_eq!(gallery.panes.pictures().x1, 1440.0 - SIDEBAR, "and shown again");
    }

    #[test]
    fn in_a_narrow_window_the_pictures_stop_at_half_size_and_scroll_across() {
        let mut gallery = gallery();
        gallery.feed([Input::Resized((1000.0, 900.0))]);
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        assert_eq!(zoom(&gallery), SMALLEST, "no smaller than half");
        let beyond = gallery.pictures_size().0 - gallery.panes.pictures().width();
        assert!(beyond > 0.0, "wider than the pane");
        gallery.move_to((gallery.panes.pictures().x0 + 100.0, 400.0));
        gallery.wheel_by(1e6, 0.0);
        assert_eq!(
            gallery.panes.scrolled(Scrolls::Across),
            beyond,
            "scrolled across as far as the last picture's end"
        );
        gallery.move_to(on_node(&gallery, 1, "nav-save"));
        assert!(gallery.cursor.0 < gallery.panes.pictures().x1, "the desktop picture's save button in view");
        assert!(gallery.press_here());
        assert_eq!(gallery.node(), Some(("nav-save", 1)), "the right node, scaled and scrolled");

        let tag = |draw: &Draw| match draw {
            Draw::Tag { rect, text, .. } => Some((rect.width, text.clone())),
            _ => None,
        };
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("no picture") };
        let tagged: Vec<(f32, String)> = pages.iter().flat_map(|page| &page.tags).filter_map(tag).collect();
        assert!(!tagged.is_empty(), "the labels are kept apart from the scene");
        let drawn: Vec<(f32, String)> = overlays(&mut gallery).iter().filter_map(tag).collect();
        assert_eq!(drawn, tagged, "each drawn at its own size, at half size too");
    }

    #[test]
    fn dragging_a_line_between_panes_fits_the_pictures_to_the_width_left() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing + 1);
        // The sidebar hidden, the pictures are above half size, to shrink.
        gallery.chord("\\");
        let wide = zoom(&gallery);
        assert!(!gallery.press_at((TREE + 2.0, 300.0)), "a press on the line changes nothing shown");
        assert!(gallery.move_to((400.0, 320.0)), "the drag moves the line");
        assert_eq!(origins(&gallery)[0].0, 400.0 + MARGIN, "the pictures start after it");
        assert!(zoom(&gallery) < wide, "and fit the width left");
        assert!(!gallery.let_go(), "letting go picks nothing");
        gallery.chord("\\");

        gallery.press_at(on_node(&gallery, 0, "nav-save"));
        gallery.let_go();
        let line = gallery.panes.pictures().x1;
        gallery.press_at((line, 300.0));
        gallery.move_to((line + 40.0, 300.0));
        gallery.let_go();
        assert_eq!(picked_node(&gallery), Some("nav-save"), "the node still picked");
        assert!(!gallery.details, "no button pressed");
        let right = gallery.panes.pictures().x1;
        let laid = gallery.placed().expect("the sidebar is shown");
        let inside = |r: &Rect| f64::from(r.x) > right && f64::from(r.x + r.width) < 1440.0;
        assert!(laid.buttons().iter().all(|(r, _, _)| inside(r)), "the sidebar's buttons inside it");
        assert!(click_panel_line(&mut gallery, "▸ details"), "each still clicked by its own box");
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
        let Picture::Faults(Faults { findings: shown, .. }) = &gallery.picture else {
            panic!("a description with faults is not drawn")
        };
        assert_eq!(shown, &found, "each finding as `check` names it");
        let list = faults_listed(&mut gallery);
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
            Picture::Pages(Pages { pages, .. }) => pages.iter().map(|page| page.height).collect(),
            Picture::Faults(Faults { findings: faults, .. }) => {
                panic!("no picture: it has faults: {faults:?}")
            }
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
        gallery.move_to((gallery.panes.pictures().x0 + 100.0, 100.0));
        gallery.wheel_by(0.0, 40.0);
        assert_eq!(gallery.panes.scrolled(Scrolls::Down), 40.0);

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
        assert_eq!(gallery.panes.scrolled(Scrolls::Down), 40.0, "scrolled as it was");

        let mut screen = todo_screen(&root);
        let fixtures = screen["fixtures"].as_object_mut().unwrap();
        let typing = fixtures.shift_remove("typing").unwrap();
        fixtures.insert("typed".to_string(), typing);
        write_todo_screen(&root, &screen);
        gallery.reload();
        let todo = line_of(&gallery, "todo");
        assert_eq!(gallery.line, Some(todo), "a state gone: its description's overview");
        assert!(matches!(gallery.picture, Picture::Overview(_)));
        assert_eq!(gallery.panes.scrolled(Scrolls::Down), 0.0, "from its top");
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
        assert!(
            matches!(&gallery.picture, Picture::Faults(Faults { findings: shown, .. }) if shown == &gallery.lines[1].row.faults)
        );

        std::fs::write(root.join("todo").join(crate::KINDS), "{}").unwrap();
        gallery.reload();
        let Picture::Faults(Faults { findings: shown, .. }) = &gallery.picture else {
            panic!("a kinds.json left behind is a fault")
        };
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
        assert!(
            matches!(&gallery.picture, Picture::Faults(Faults { findings: shown, .. }) if shown == &gallery.lines[2].row.faults)
        );
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
    fn the_nodes_that_go_somewhere_are_marked_pointed_at_with_a_hand_and_lit_while_command_is_held() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        let Picture::Pages(pages) = &gallery.picture else { panic!("orders has faults") };
        let leads = |id: &str| pages.ways.get(id).and_then(|ways| ways.first()).map(Way::leads);
        let phone = &pages.pages[0];
        let rows: Vec<&str> =
            phone.nodes.iter().map(|n| n.id.as_str()).filter(|id| id.ends_with("-cancel")).collect();
        assert!(rows.len() > 1, "{rows:?}");
        for row in &rows {
            assert_eq!(leads(row), Some(Leads::State), "{row}: every row's button, by its transition");
        }
        assert_eq!(leads("nav-export"), Some(Leads::Screen), "a navigate");
        assert_eq!(leads("nav-back"), Some(Leads::Screen), "back");
        assert!(phone.nodes.iter().any(|n| leads(&n.id).is_none()), "not every node goes somewhere");

        let button = on_node(&gallery, 0, rows[0]);
        let moved = |gallery: &mut Gallery, at: (f64, f64)| {
            gallery.feed([Input::Moved((at.0, at.1 + TOOLBAR))]).cursor
        };
        assert_eq!(moved(&mut gallery, button), Some(CursorIcon::Pointer), "a hand over it");
        let margin = (gallery.panes.pictures().x0 + 10.0, 300.0);
        assert_eq!(moved(&mut gallery, margin), Some(CursorIcon::Default), "off every node");

        assert!(!gallery.seen().lit, "outlined only");
        assert!(gallery.feed([Input::Modifiers(ModifiersState::SUPER)]).redraw);
        assert!(gallery.seen().lit, "lit while ⌘ is held");
        assert!(gallery.feed([Input::Modifiers(ModifiersState::empty())]).redraw);
        assert!(!gallery.seen().lit);

        // A ⌘-click that goes nowhere: each lit for a moment, then not.
        gallery.feed([Input::Modifiers(ModifiersState::CONTROL)]);
        assert!(gallery.press_here(), "drawn again, to flash");
        gallery.feed([Input::Modifiers(ModifiersState::empty())]);
        assert!(gallery.seen().lit, "flashed");
        let now = Instant::now();
        assert!(gallery.flash_ends(now).is_some_and(|end| end <= now + FLASH), "the window wakes as it ends");
        assert_eq!(gallery.flash_ends(now + FLASH), None, "and then it is over");
        assert_eq!(shown(&gallery), "orders · default", "going nowhere");
    }

    #[test]
    fn several_outcomes_are_chosen_in_the_panel() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "cancel-ready");
        act_on(&mut gallery, "cd-confirm");
        assert_eq!(shown(&gallery), "orders · cancel-ready", "nothing is shown until an outcome is chosen");
        let panel = gallery.panel();
        assert_eq!(panel[0], "cd-confirm.tap in cancel-ready calls cancelOrder: click how it answered");
        assert_eq!(panel[3..5], ["→ default · ok", "→ cancel-ready · shipped"], "on the node's card");
        assert!(click_panel_line(&mut gallery, "→ default · ok"), "the first outcome clicked");
        assert_eq!(shown(&gallery), "orders · default");
        assert_eq!(gallery.panel()[0], "ok in cancel-ready → default");
    }

    /// The buttons floating beside the node picked: what each says, and
    /// whether it goes there.
    fn offers(gallery: &Gallery) -> Vec<(String, bool)> {
        gallery.seen().offers.into_iter().map(|offer| (offer.says, offer.live)).collect()
    }

    /// Whether a line of the sidebar reads `line`.
    fn reads(gallery: &Gallery, line: &str) -> bool {
        gallery.panel().iter().any(|l| l.trim() == line)
    }

    /// The middle of the button floating beside the node picked that says
    /// `says`.
    fn on_offer(gallery: &mut Gallery, says: &str) -> (f64, f64) {
        let (id, pointed) = gallery.node().map(|(id, pointed)| (id.to_string(), pointed)).unwrap();
        let offers = gallery.seen().offers;
        let wide = offer_widths(&offers, &mut gallery.typesetter);
        let Picture::Pages(pages) = &gallery.picture else { panic!("no pages") };
        let offered = pages.offered(&id, pointed, &wide, &gallery.desk());
        let index = offers.iter().position(|offer| offer.says == says).unwrap();
        let area = offered[index];
        (area.center().x, area.center().y)
    }

    #[test]
    fn a_node_picked_offers_where_acting_on_it_goes_beside_it_and_on_its_card() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        gallery.press_at(on_node(&gallery, 0, "nav-export"));
        assert_eq!(offers(&gallery), [("↗ Go to ExportDocuments".to_string(), true)]);
        let hint = "Holding ⌘ (Ctrl) and clicking the node does the same.";
        assert!(reads(&gallery, "↗ Go to ExportDocuments") && reads(&gallery, hint));
        let at = on_offer(&mut gallery, "↗ Go to ExportDocuments");
        assert!(gallery.press_at(at), "the button floating beside it clicked");
        let export = line_of(&gallery, "export");
        assert_eq!(gallery.line, Some(export + 1), "as a ⌘-click does: the screen's first state");
        assert_eq!(gallery.panel()[0], "nav-export.tap navigates to ExportDocuments");

        gallery.press_at(on_node(&gallery, 0, "nav-back"));
        assert_eq!(offers(&gallery), [("← Back".to_string(), true)]);
        assert!(click_panel_line(&mut gallery, "← Back"), "on the card, as beside it");
        assert_eq!(shown(&gallery), "orders · default", "back where it came from");

        gallery.press_at(on_node(&gallery, 0, "order-0"));
        assert_eq!(
            offers(&gallery),
            [("↗ OrderDetail · not drawn yet".to_string(), false)],
            "a screen not drawn yet, grayed"
        );
        assert!(reads(&gallery, "↗ OrderDetail · not drawn yet"), "said on the card");
        assert!(
            !gallery.placed().unwrap().buttons().iter().any(|(_, says, _)| says.contains("OrderDetail")),
            "and not a button there"
        );

        show(&mut gallery, "orders", "cancel-ready");
        gallery.press_at(on_node(&gallery, 0, "cd-confirm"));
        assert_eq!(
            offers(&gallery),
            [("→ default · ok".to_string(), true), ("→ cancel-ready · shipped".to_string(), true)],
            "one for each outcome"
        );
        let at = on_offer(&mut gallery, "→ default · ok");
        assert!(gallery.press_at(at));
        assert_eq!(shown(&gallery), "orders · default");
        assert_eq!(gallery.panel()[0], "ok in cancel-ready → default");

        show(&mut gallery, "orders", "cancel-unpaid");
        gallery.press_at(on_node(&gallery, 0, "cd-keep"));
        assert!(offers(&gallery).is_empty(), "a node that goes nowhere offers nothing");
        assert!(!reads(&gallery, hint));
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

    /// Where the toolbar's button that does `tool` is, in the gallery's
    /// points, and whether it can be pressed: not grayed.
    fn tool_button(gallery: &mut Gallery, tool: Tool) -> ((f64, f64), bool) {
        let button = gallery.tools().into_iter().find(|button| button.tool == tool).unwrap();
        let Rect { x, y, width, height } = button.area;
        ((f64::from(x + width / 2.0), f64::from(y + height / 2.0)), button.enabled)
    }

    #[test]
    fn back_and_forward_step_through_what_the_window_showed_and_something_new_drops_the_steps_forward() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        let (back, can_back) = tool_button(&mut gallery, Tool::Back);
        let (forward, can_forward) = tool_button(&mut gallery, Tool::Forward);
        assert!(
            back.1 < 0.0 && forward.0 > back.0,
            "back, then forward, along the top: {back:?} {forward:?}"
        );
        assert!(!can_back && !can_forward, "the first thing shown: both grayed");
        assert!(!gallery.press_at(back) && !gallery.chord("["), "grayed: nowhere to go");

        act_on(&mut gallery, "order-0-cancel");
        act_on(&mut gallery, "nav-export");
        let export = shown(&gallery);
        act_on(&mut gallery, "nav-back");
        assert_eq!(shown(&gallery), "orders · cancel-dialog", "back on a node returns along the acts");
        assert!(gallery.press_at(back), "and is a step of the window's");
        assert_eq!(shown(&gallery), export);
        assert!(gallery.chord("["));
        assert_eq!(shown(&gallery), "orders · cancel-dialog", "acts are steps");
        assert!(gallery.chord("["));
        assert_eq!(shown(&gallery), "orders · default", "a tree pick is a step");
        assert!(!tool_button(&mut gallery, Tool::Back).1 && tool_button(&mut gallery, Tool::Forward).1);
        assert!(gallery.press_at(forward));
        assert_eq!(shown(&gallery), "orders · cancel-dialog");
        assert!(gallery.chord("]") && gallery.chord("]"));
        assert_eq!(shown(&gallery), "orders · cancel-dialog", "back on the node, gone forward to");
        assert!(!tool_button(&mut gallery, Tool::Forward).1 && !gallery.chord("]"), "the last: grayed");

        assert!(gallery.chord("["));
        let journey = journey_of(&gallery, "cancel-an-order");
        showing(&mut gallery, journey);
        assert!(gallery.click(Do::Step(1)));
        assert!(!tool_button(&mut gallery, Tool::Forward).1, "playing a journey drops the steps forward");
        assert!(gallery.chord("["));
        assert_eq!(gallery.playing.as_ref().map(|playing| playing.at), Some(0), "each step is one");
        assert!(gallery.chord("["));
        assert_eq!(shown(&gallery), export);
        assert!(gallery.chord("]") && gallery.chord("]"));
        assert_eq!(gallery.playing.as_ref().map(|playing| playing.at), Some(1), "played again at that step");
    }

    #[test]
    fn the_toolbars_sidebar_button_hides_and_shows_the_sidebar_and_its_words_are_said_in_lang() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        let said = |gallery: &mut Gallery| gallery.tools().into_iter().map(|b| b.said).collect::<Vec<_>>();
        let keys = if cfg!(target_os = "macos") { "⌘." } else { "Ctrl+." };
        assert_eq!(
            said(&mut gallery),
            ["← back", "→ forward", "hide sidebar", &format!("hide marks {keys}")]
        );
        let (sidebar, _) = tool_button(&mut gallery, Tool::Sidebar);
        assert!(sidebar.0 > gallery.panes.pictures().x1, "at the right end, over the sidebar");
        assert!(gallery.press_at(sidebar));
        assert!(gallery.panes.side().is_none(), "hidden, as ⌘\\ hides it");
        assert_eq!(said(&mut gallery)[2], "show sidebar");
        let (sidebar, _) = tool_button(&mut gallery, Tool::Sidebar);
        assert!(gallery.press_at(sidebar));
        assert!(gallery.panes.side().is_some(), "shown again");
        assert!(gallery.chord("\\") && gallery.panes.side().is_none(), "⌘\\ the same");
        assert_eq!(said(&mut gallery)[2], "show sidebar");

        gallery.speak(Lang::Zh).unwrap();
        assert_eq!(said(&mut gallery), ["← 后退", "→ 前进", "显示侧栏", &format!("隐藏标记 {keys}")]);
        gallery.speak(Lang::Ja).unwrap();
        assert_eq!(
            said(&mut gallery),
            ["← 戻る", "→ 進む", "サイドバーを表示", &format!("マークを隠す {keys}")]
        );
    }

    /// The labels and "?"s drawn over the pictures shown.
    fn tags_drawn(gallery: &mut Gallery) -> usize {
        overlays(gallery).iter().filter(|draw| matches!(draw, Draw::Tag { .. })).count()
    }

    /// The nodes marked as going somewhere on the pages shown.
    fn leads_drawn(gallery: &Gallery) -> usize {
        let Picture::Pages(pages) = &gallery.picture else { return 0 };
        pages.leads_shown(&gallery.desk(), &gallery.seen()).len()
    }

    /// Whether a journey's mark is drawn over the overview shown.
    fn passes_drawn(gallery: &mut Gallery) -> bool {
        overlays(gallery).iter().any(|draw| matches!(draw, Draw::Box { fill: Some(Ink::Note), .. }))
    }

    #[test]
    fn the_toolbars_marks_button_hides_and_shows_every_mark_over_the_pictures() {
        let keys = if cfg!(target_os = "macos") { "⌘." } else { "Ctrl+." };
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        assert!(gallery.seen().marks, "shown when the window opens");
        assert!(tags_drawn(&mut gallery) > 0 && leads_drawn(&gallery) > 0, "labels and purple marks");
        let (marks, _) = tool_button(&mut gallery, Tool::Marks);
        assert!(gallery.press_at(marks));
        assert!(!gallery.seen().marks);
        assert_eq!(gallery.tools()[3].said, format!("show marks {keys}"));
        assert_eq!(tags_drawn(&mut gallery), 0, "no label, no \"?\"");
        assert_eq!(leads_drawn(&gallery), 0, "no purple mark");

        // Hidden: a click still picks, drawn as picked with its offers.
        assert!(gallery.press_at(on_node(&gallery, 0, "order-0-cancel")));
        assert_eq!(picked_node(&gallery), Some("order-0-cancel"));
        assert!(!gallery.seen().offers.is_empty(), "its go-to button still offered");
        assert!(!overlays(&mut gallery).is_empty(), "and drawn");
        // ⌘ held: the purple marks, for as long as it is.
        gallery.feed([Input::Modifiers(ModifiersState::SUPER)]);
        assert!(leads_drawn(&gallery) > 0, "lit while ⌘ is held");
        assert_eq!(tags_drawn(&mut gallery), 0, "but not the labels");
        gallery.feed([Input::Modifiers(ModifiersState::empty())]);
        assert_eq!(leads_drawn(&gallery), 0, "hidden again once it is let go");
        // ⌘-click still acts.
        assert!(act_on(&mut gallery, "order-0-cancel"));
        assert_ne!(shown(&gallery), "orders · default", "acted on");
        assert!(!gallery.seen().marks, "the choice holds as states change");

        // An overview: no journey's mark, and a press there picks the state.
        let order = description_in(&gallery, "shop/order");
        showing(&mut gallery, order);
        assert!(!gallery.seen().marks, "and screens");
        assert!(!passes_drawn(&mut gallery));
        let Picture::Overview(overview) = &gallery.picture else { panic!("no overview is shown") };
        let open = overview.passes(&gallery.desk())[0].0;
        assert!(gallery.press_at((f64::from(open.x) + 4.0, f64::from(open.y) + 4.0)));
        assert!(gallery.playing.is_none(), "no journey played");
        assert_eq!(gallery.lines[gallery.line.unwrap()].row.name, "open", "its state picked");

        assert!(gallery.chord("."), "⌘. shows them again");
        assert!(gallery.seen().marks);
        showing(&mut gallery, order);
        assert!(passes_drawn(&mut gallery), "the journeys' marks");
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
        assert!(gallery.path.is_empty() && gallery.acted.is_none(), "picking in the tree starts a new path");
    }

    #[test]
    fn choosing_where_nothing_declared_should_go_says_it_in_the_nodes_note_and_never_the_description() {
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

        assert!(gallery.panel()[0].starts_with("N1 saved"), "the same note: {:?}", gallery.panel());

        let written = notes::read(&dir).unwrap();
        assert_eq!(written.len(), 1, "one note on the node: {written:#?}");
        let On::Node { state, node } = &written[0].on else { panic!("not on a node: {written:?}") };
        assert_eq!((node.origin.written.as_str(), state.name()), ("cd-keep", "cancel-unpaid"));
        let on = "cd-keep.tap".to_string();
        let marks: Vec<Option<&Mark>> = written[0].messages.iter().map(|said| said.mark.as_ref()).collect();
        assert_eq!(
            marks,
            [
                Some(&Mark::Proposes(Proposal { on: on.clone(), goes: Goes::To("default".into()) })),
                Some(&Mark::Proposes(Proposal { on, goes: Goes::Navigate("ExportDocuments".into()) })),
            ]
        );
        assert_eq!(
            written[0].messages[0].says,
            "cd-keep.tap in cancel-unpaid should go to default: declare the transition"
        );
        let listed = notes::notes(&root, None, None, false).unwrap();
        assert_eq!(
            listed["notes"][0]["messages"][1]["proposes"],
            json!({ "on": "cd-keep.tap", "navigate": "ExportDocuments" }),
            "`notes` lists the mark"
        );
        gallery.press_at(on_node(&gallery, 0, "cd-keep"));
        gallery.let_go();
        let panel = gallery.panel();
        assert!(
            panel.iter().filter(|line| *line == "proposes where acting on it goes").count() == 2,
            "the conversation marks each: {panel:?}"
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
        flow.draws(None, Lang::En)
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
        let draws = flow.draws(None, Lang::En);
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
        let ((left, top), zoom) = (Flow::origin(&gallery.desk()), zoom(&gallery));
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
        let origins = origins(gallery);
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
        let laid = gallery.placed().expect("the sidebar is shown");
        let Some(r) = laid.buttons().iter().find(|(_, says, _)| *says == text).map(|(r, _, _)| *r) else {
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
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("no picture") };
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
        assert_eq!(gallery.sidebar.talk().text(), "价格太长\na b", "Shift+Enter breaks a line");
        assert_eq!(gallery.foot().unwrap().does, Some(Do::Send));
        assert!(!dir.join(notes::NOTES).exists(), "nothing is written while typing");
        press_keys(&mut gallery, &["Enter"]);
        assert!(gallery.sidebar.talk().is_blank(), "Enter sends, and empties the box");
        assert_eq!(
            talk_shown(&gallery),
            ["note N1 · waiting on the agent", "价格太长\na b", "waiting for the agent"],
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
        assert_eq!(
            talk_shown(&gallery)[1..],
            ["价格太长\na b", "and the date", "waiting for the agent"],
            "newest at the bottom"
        );

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
        gallery.panes.resize((1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        press_keys(&mut gallery, &["n"]);
        assert_eq!(gallery.sidebar.talk().text(), "n", "N is a letter typed in the box, no mode");
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
        assert_eq!(
            talk_shown(&gallery),
            ["note N2 · waiting on the agent", "too red", "waiting for the agent"]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_conversation_is_waiting_then_taken_then_answered_unread_then_read() {
        let root = root_of("notes-stands", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        gallery.time = Some(1_000_023);
        show(&mut gallery, "orders", "default");
        say_on(&mut gallery, 0, "nav-export", "rename it");
        let dir = root.join("orders");
        let ink = |gallery: &Gallery| {
            gallery.pins().into_iter().find(|pin| pin.id == "N1" && !pin.faint).map(|pin| pin.ink)
        };
        assert_eq!(talk_shown(&gallery)[2], "waiting for the agent", "under the person's last message");
        assert_eq!(ink(&gallery), Some(Ink::Note), "waiting on the agent: its mark");
        assert!(!gallery.counting(), "nothing to count yet");

        assert!(notes::hand_over(&dir, 1_000_000).unwrap(), "`--wait` hands it to the agent");
        gallery.reload();
        assert_eq!(talk_shown(&gallery)[2], "the agent is changing it (23s)");
        assert!(gallery.counting(), "the window is drawn again each second");
        gallery.time = Some(1_000_125);
        assert_eq!(talk_shown(&gallery)[2], "the agent is changing it (2m 05s)", "counting up");
        assert_eq!(ink(&gallery), Some(Ink::Note), "taken, it still waits on the agent: the same mark");

        // Its conversation not shown, the answer comes in unread.
        press_keys(&mut gallery, &["Esc"]);
        notes::answer(&dir, "N1", "renamed it Export").unwrap();
        gallery.reload();
        assert_eq!(notes::read(&dir).unwrap()[0].stands(), Stands::Unread);
        assert_eq!(ink(&gallery), Some(Ink::Answered), "unread: a mark of its own");
        assert!(!gallery.counting(), "answered: nothing counts");

        assert!(gallery.press_at(on_pin(&gallery, "N1")), "its conversation shown");
        assert_eq!(talk_shown(&gallery), ["note N1 · answered", "rename it", "renamed it Export"]);
        assert_eq!(notes::read(&dir).unwrap()[0].stands(), Stands::Read, "seen, it is read");
        assert_eq!(ink(&gallery), Some(Ink::Line), "answered and read: quiet");

        let mut again = opened(&root, (1440.0, 900.0));
        show(&mut again, "orders", "default");
        assert_eq!(ink(&again), Some(Ink::Line), "still read in a window opened again");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn how_long_ago_is_counted_in_seconds_then_minutes_then_hours() {
        assert_eq!(
            [0, 23, 59, 60, 125, 3599, 3600, 7380].map(since),
            ["0s", "23s", "59s", "1m 00s", "2m 05s", "59m 59s", "1h 00m", "2h 03m"]
        );
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
        assert_eq!(
            (note.stands(), note.messages.len()),
            (Stands::Read, 3),
            "never closed, and read, its state shown: {note:?}"
        );
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
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else {
            panic!("no picture: it has faults")
        };
        let node = pages[page].nodes.iter().find(|n| n.id == id).unwrap();
        let r = node.guess_tag.expect("it is guessed");
        let (left, top) = origins(gallery)[page];
        let zoom = zoom(gallery);
        let middle = (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0));
        (left + middle.0 * zoom, top + middle.1 * zoom)
    }

    #[test]
    fn an_assumption_is_answered_with_marked_messages_in_its_nodes_note() {
        let root = root_of("assume", &[("checkout", "checkout")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        let checkout = line_of(&gallery, "checkout");
        let states = gallery.lines[checkout + 1..].iter().take_while(|line| line.state.is_some()).count();
        let shown = (1..=states).find(|offset| {
            showing(&mut gallery, checkout + offset);
            let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { return false };
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
            "✓: the agent takes the guess out · ✗: say what is right",
        ] {
            assert!(panel.iter().any(|line| line == part), "{part:?} in {panel:?}");
        }
        let at = |panel: &[String], line: &str| panel.iter().position(|said| said == line);
        assert!(
            at(&panel, "✗ wrong…") < at(&panel, "note · nothing said yet"),
            "above the conversation: {panel:?}"
        );

        type_text(&mut gallery, "the card form");
        press_keys(&mut gallery, &["Enter"]);
        assert!(click_panel_line(&mut gallery, "✗ wrong…"));
        assert!(gallery.typing(), "the caret is in the box");
        let panel = gallery.panel();
        assert!(panel.iter().any(|line| line.starts_with("✗: write what is right")), "{panel:?}");
        assert!(!panel.iter().any(|line| line == "✓ right, keep it"), "in place of the buttons");
        assert_eq!(gallery.foot().unwrap().empty, "what is right?");
        press_keys(&mut gallery, &["Enter", " ", "Enter"]);
        assert!(gallery.rejecting.is_some(), "the ✗ is not sent empty");
        assert!(gallery.panel()[0].contains("✗ says what is right"), "{:?}", gallery.panel());
        let dir = root.join("checkout");
        assert_eq!(notes::read(&dir).unwrap()[0].messages.len(), 1, "nothing more written");
        press_keys(&mut gallery, &["Backspace"]);
        type_text(&mut gallery, "use the hosted fields");
        press_keys(&mut gallery, &["Enter"]);
        assert!(gallery.rejecting.is_none() && gallery.sidebar.talk().is_blank(), "sent");
        let written = notes::read(&dir).unwrap();
        assert_eq!(written.len(), 1, "one note on the node: {written:#?}");
        let wrong = &written[0];
        assert_eq!(wrong.stands(), Stands::Waiting);
        assert_eq!(wrong.messages[0].mark, None, "plain talk is unmarked");
        assert_eq!(wrong.messages[1].says, "use the hosted fields");
        let Some(Mark::Answers(answers)) = &wrong.messages[1].mark else { panic!("unmarked: {wrong:?}") };
        assert_eq!((answers.answer, &answers.guess), (Answer::Wrong, &Guess::Author));
        assert!(answers.assumed.starts_with("Card fields are drawn here;"));
        assert!(gallery.panel().iter().any(|line| line == "✗ wrong, answering the assumption"));

        assert!(click_panel_line(&mut gallery, "✗ wrong…"));
        press_keys(&mut gallery, &["Esc"]);
        assert!(gallery.rejecting.is_none(), "Esc takes the ✗ back");
        type_text(&mut gallery, "and the expiry");
        press_keys(&mut gallery, &["Enter"]);
        assert_eq!(notes::read(&dir).unwrap()[0].messages[2].mark, None, "sent as plain talk");

        assert!(click_panel_line(&mut gallery, "✓ right, keep it"));
        assert!(gallery.panel()[0].starts_with("✓ right on card-number in "), "{:?}", gallery.panel());
        assert!(!gallery.panel().iter().any(|line| line == "✓ right, keep it"), "one ✓ at a time");
        assert!(click_panel_line(&mut gallery, "undo"));
        assert!(!gallery.confirm(Instant::now() + UNDO * 2), "undone: nothing waits");
        assert_eq!(
            notes::read(&dir).unwrap()[0].messages.len(),
            3,
            "an undone ✓ leaves notes.json as it was"
        );

        assert!(click_panel_line(&mut gallery, "✓ right, keep it"));
        assert!(!gallery.confirm(Instant::now() + UNDO - Duration::from_secs(1)), "it can still be undone");
        assert_eq!(notes::read(&dir).unwrap()[0].messages.len(), 3, "not written while it can be undone");
        assert!(gallery.confirm(Instant::now() + UNDO));
        assert!(!gallery.confirm(Instant::now() + UNDO * 2), "written once");
        let note = notes::read(&dir).unwrap().remove(0);
        let right = &note.messages[3];
        assert_eq!(right.says, notes::RIGHT, "✓ needs no line");
        assert!(matches!(&right.mark, Some(Mark::Answers(answers)) if answers.answer == Answer::Right));
        assert!(gallery.panel().iter().any(|line| line == "✓ right, answering the assumption"));

        notes::answer(&dir, "N1", "kept them drawn: hosted fields are for the build").unwrap();
        let file = dir.join(crate::SCREEN);
        let screen = std::fs::read_to_string(&file).unwrap().replace(
            ",\n                        \"assumed\": \"Card fields are drawn here; the real build may use the payment provider's hosted fields\"",
            "",
        );
        assert!(!screen.contains("Card fields are drawn here"), "the guess taken out");
        std::fs::write(&file, screen).unwrap();
        gallery.reload();
        assert_eq!(notes::read(&dir).unwrap()[0].stands(), Stands::Read, "the agent spoke last, shown: read");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_sidebar_is_laid_out_at_most_once_a_redraw_however_the_pointer_moves() {
        let root = root_of("sidebar-once", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        gallery.drawn();
        let laid = gallery.sidebar.layouts();
        for x in [100.0, 600.0, 1150.0, 1300.0, 1400.0] {
            gallery.move_to((x, 300.0));
            gallery.drawn();
        }
        assert_eq!(gallery.sidebar.layouts(), laid, "nothing it shows changed: not laid out again");
        let field = gallery.field_rect().expect("the box is at the sidebar's foot");
        let (_, caret) = gallery.composing();
        let caret = caret.expect("its caret is known");
        let at = (f64::from(caret.x), f64::from(caret.y) - TOOLBAR);
        assert!(over(field, at), "the caret in the field's text, in the window's points below the toolbar");
        assert!(gallery.button_at((f64::from(field.x) + 1.0, f64::from(field.y) - 1.0)).is_none());
        assert_eq!(gallery.sidebar.layouts(), laid, "hit-tested and its caret found on the one layout");

        gallery.press_at(on_node(&gallery, 0, "nav-export"));
        gallery.let_go();
        gallery.move_to((1300.0, 300.0));
        gallery.drawn();
        assert_eq!(gallery.sidebar.layouts(), laid + 1, "a node picked: laid out once, for what it shows");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_note_is_edited_anywhere_selected_by_keys_or_a_drag_and_pasted_into() {
        let root = root_of("notes-edit", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        let written = |gallery: &Gallery| gallery.sidebar.talk().text();
        type_text(&mut gallery, "ac");
        press_keys(&mut gallery, &["←", "b", "Home", ">", "End", "!"]);
        assert_eq!(written(&gallery), ">abc!", "typed where the caret is");
        type_text(&mut gallery, " too red");
        press_keys(&mut gallery, &["⇧←", "⇧←", "⇧←"]);
        let selected = gallery.sidebar.talk().shown().selected;
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
        assert_eq!(gallery.sidebar.talk().selected(), Some("abc too blue"));
        type_text(&mut gallery, "x");
        press_keys(&mut gallery, &["Enter"]);
        let note = notes::read(&root.join("orders")).unwrap().remove(0);
        assert_eq!(note.messages[0].says, "x");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_caret_blinks_from_each_edit_and_only_in_the_window_keys_go_to() {
        let root = root_of("notes-blink", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        show(&mut gallery, "orders", "default");
        let half = field::BLINK;
        gallery.focused = true;
        gallery.chord("\\");
        assert_eq!(gallery.blinks(Instant::now()), None, "the sidebar hidden: nothing wakes the window");
        gallery.chord("\\");
        assert!(gallery.blinks(Instant::now()).is_some(), "shown: its box blinks");
        gallery.cursor = on_node(&gallery, 0, "nav-export");
        gallery.press();
        type_text(&mut gallery, "ab");
        let typed = gallery.edited;
        assert!(gallery.caret_shows(typed), "shown just after an edit");
        assert!(!gallery.caret_shows(typed + half), "hidden half a second on");
        assert_eq!(gallery.blinks(typed + half / 2), Some(typed + half), "woken as it hides");
        press_keys(&mut gallery, &["←"]);
        let moved = gallery.edited;
        assert!(moved > typed, "the caret moved: the blink starts again");
        assert!(gallery.caret_shows(moved), "shown at once");
        assert!(!gallery.caret_shows(moved + half));
        gallery.feed([Input::Focused(false)]);
        assert!(gallery.caret_shows(moved + half), "another window has the keys: held still, shown");
        assert_eq!(gallery.blinks(moved + half), None, "and nothing wakes the window");
        let frame = gallery.feed([Input::Focused(true)]);
        assert!(frame.redraw && gallery.caret_shows(gallery.edited), "back: shown at once");
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
        let shown = gallery.sidebar.talk().shown();
        assert_eq!(shown.text, "ani haob", "composed at the caret");
        assert_eq!(&shown.text[shown.composing.clone()], "ni hao", "drawn underlined");
        let laid = gallery.placed().expect("the sidebar is shown");
        let underlined = laid
            .draws(true)
            .1
            .iter()
            .filter(|draw| matches!(draw, Draw::Line { ink: Ink::Text, .. }))
            .count();
        assert_eq!(underlined, 1, "one line under what is composed");
        assert!(laid.caret().is_some(), "the input method is shown where the caret is");
        press_keys(&mut gallery, &["Enter", "Backspace"]);
        assert_eq!(
            gallery.sidebar.talk().shown().text,
            "ani haob",
            "while it composes, its keys are its own"
        );
        assert!(!root.join("orders").join(notes::NOTES).exists(), "the Enter that confirms it sends nothing");
        assert!(gallery.ime(Ime::Commit("你好".to_string())));
        press_keys(&mut gallery, &["→", "Delete", "Backspace", "←"]);
        type_text(&mut gallery, "!");
        assert_eq!(gallery.sidebar.talk().text(), "a你!好", "the caret moved, deleting where it is");
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
        assert_eq!(notes[0].answers().next().map(|answers| answers.answer), Some(Answer::Right));
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
        assert_eq!(note.answers().next().unwrap().guess, Guess::Undrawn(vec!["ExportDocuments".into()]));

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
        let r = arrow_labels(gallery)[index];
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
        let (left, top) = origins(&gallery)[1];
        say_at(&mut gallery, (left + 4.0, top - CAPTION / 2.0), "too busy");
        let dir = root.join("orders");
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!(note.on, On::State { state: State::Fixture("default".into()) });
        assert_eq!(
            talk_shown(&gallery),
            ["note N1 · waiting on the agent", "too busy", "waiting for the agent"],
            "shown at once"
        );
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
        assert!(gallery.chord(".") && gallery.pins().is_empty(), "no pin while the marks are hidden");
        assert!(gallery.chord(".") && gallery.pins().len() == 1, "and back once shown");
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet", "the whole screen's, not the state's");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_description_picked_talks_about_the_whole_screen() {
        let root = root_of("notes-screen", &[("orders", "orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        let orders = line_of(&gallery, "orders");
        showing(&mut gallery, orders);
        let Picture::Overview(_) = &gallery.picture else { panic!("not an overview") };
        let panel = gallery.panel();
        assert_eq!(panel[0], "OrderList · the whole screen", "the card names the screen: {panel:?}");
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet");
        let dir = root.join("orders");
        let line = format!("{} · the whole screen", dir.display());
        assert_eq!(gallery.reference().unwrap().line(), line);
        gallery.chord("c");
        assert!(gallery.panel()[0].ends_with(&line), "⌘C copies it: {:?}", gallery.panel());

        gallery.acted = None;
        type_text(&mut gallery, "this screen tries to do too much");
        press_keys(&mut gallery, &["Enter"]);
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!((note.id.as_str(), &note.on), ("N1", &On::Whole { whole: Whole::Screen }));
        let file: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(notes::NOTES)).unwrap()).unwrap();
        assert_eq!(file["notes"][0]["whole"], "screen", "kept in its notes.json");
        assert_eq!(
            talk_shown(&gallery),
            ["note N1 · waiting on the agent", "this screen tries to do too much", "waiting for the agent"]
        );
        let Line { row, state, .. } = &gallery.lines[orders];
        assert_eq!(gallery.waiting_notes(row, state.as_ref()), 1, "counted on its description");

        assert!(gallery.press_at(on_arrow(&gallery, 0)), "an arrow picked");
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet", "the arrow's own conversation");
        let at = on_arrow(&gallery, 0);
        say_at(&mut gallery, at, "why here?");
        assert_eq!(notes::read(&dir).unwrap()[1].id, "N2", "numbered with its other notes");
        press_keys(&mut gallery, &["Esc"]);
        assert_eq!(gallery.arrow(), None);
        assert_eq!(talk_shown(&gallery)[1], "this screen tries to do too much", "back to the screen's");

        let default = state_of_in(&gallery, orders, "default");
        showing(&mut gallery, default);
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet", "a state's note is its own");
        assert!(gallery.click(Do::Visit(Visit::Note(dir.clone(), "N1".into()))));
        assert_eq!(gallery.line, Some(orders), "its inbox item shows the overview");
        assert_eq!(talk_shown(&gallery)[1], "this screen tries to do too much");
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
        assert_eq!(
            talk_shown(&gallery),
            ["note N1 · waiting on the agent", "should it say why?", "waiting for the agent"]
        );
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
        let screens = Reference { description: root.clone(), on: On::Whole { whole: Whole::Screens } };
        assert_eq!(gallery.reference(), Some(screens), "nothing picked in a flow: the screens under it");
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
        assert_eq!(
            talk_shown(&gallery),
            ["note N1 · waiting on the agent", "is this the right screen?", "waiting for the agent"]
        );
        assert_eq!(gallery.pins().len(), 1, "a pin under its arrow");
        gallery.cursor = on_pin(&gallery, "N1");
        assert!(!gallery.press() || gallery.arrow() == Some(0), "its pin keeps its arrow picked");
        let Line { row, state, .. } = &gallery.lines[line_of(&gallery, "orders")];
        assert_eq!(gallery.waiting_notes(row, state.as_ref()), 1, "counted on its description");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_directory_picked_talks_about_the_screens_under_it() {
        let root = root_of("notes-screens", &[("orders", "app/orders")]);
        let mut gallery = opened(&root, (1440.0, 900.0));
        let app = line_of(&gallery, "app");
        showing(&mut gallery, app);
        let Picture::Flow(_) = &gallery.picture else { panic!("not a flow") };
        let panel = gallery.panel();
        assert_eq!(panel[0], "app · these screens", "the card names the directory: {panel:?}");
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet");
        let dir = root.join("app");
        let line = format!("{} · these screens", dir.display());
        assert_eq!(gallery.reference().unwrap().line(), line);
        gallery.chord("c");
        assert!(gallery.panel()[0].ends_with(&line), "⌘C copies it: {:?}", gallery.panel());

        gallery.acted = None;
        type_text(&mut gallery, "these screens repeat the same header");
        press_keys(&mut gallery, &["Enter"]);
        let note = notes::read(&dir).unwrap().remove(0);
        assert_eq!((note.id.as_str(), &note.on), ("N1", &On::Whole { whole: Whole::Screens }));
        assert!(!dir.join(crate::SCREEN).exists(), "the directory holds no description");
        let file: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(notes::NOTES)).unwrap()).unwrap();
        assert_eq!(file["notes"][0]["whole"], "screens", "kept in the directory's own notes.json");
        assert_eq!(
            talk_shown(&gallery),
            [
                "note N1 · waiting on the agent",
                "these screens repeat the same header",
                "waiting for the agent"
            ]
        );
        let Line { row, state, .. } = &gallery.lines[app];
        assert_eq!(gallery.waiting_notes(row, state.as_ref()), 1, "counted on its directory");

        assert!(gallery.press_at(on_arrow(&gallery, 0)), "an arrow picked");
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet", "the navigate's own conversation");
        press_keys(&mut gallery, &["Esc"]);
        assert_eq!(gallery.arrow(), None);
        assert_eq!(talk_shown(&gallery)[1], "these screens repeat the same header", "back to the group's");

        showing(&mut gallery, 0);
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet", "the root's note is its own");
        type_text(&mut gallery, "the whole app needs a tab bar");
        press_keys(&mut gallery, &["Enter"]);
        let kept = notes::read(&root).unwrap();
        assert_eq!((kept[0].id.as_str(), &kept[0].on), ("N1", &On::Whole { whole: Whole::Screens }));
        assert!(
            gallery.inboxed().contains(&dir),
            "the root's inbox holds the directories under it: {:?}",
            gallery.inboxed()
        );

        let orders = line_of(&gallery, "orders");
        showing(&mut gallery, orders);
        assert!(gallery.click(Do::Visit(Visit::Note(dir.clone(), "N1".into()))));
        assert_eq!(gallery.line, Some(app), "its inbox item shows the directory's flow");
        assert_eq!(talk_shown(&gallery)[1], "these screens repeat the same header");
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
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("no pictures") };
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
        assert_eq!(panel[0], "→ cancel-an-order · the whole journey", "above the step it plays");
        assert_eq!(panel[1], "journey cancel-an-order · step 1 of 3 · shop/orders · default");
        assert!(panel[2].trim().starts_with("acts on order-{index} (label "), "{panel:?}");
        assert!(panel[2].ends_with("· tap (marked)"), "{panel:?}");
        assert!(!panel.iter().any(|line| line.trim() == "← previous step"), "no step before the first");

        press_keys(&mut gallery, &["→"]);
        assert_eq!(captions(&gallery), ["open · 390", "open · 1120"]);
        let marked = Some("summary-cancel".to_string());
        assert_eq!(played(&gallery), (2, vec![marked.clone(), marked]));
        assert!(gallery.panel()[2].contains("· tap, answered ok (marked)"), "{:?}", gallery.panel());

        assert!(click_panel_line(&mut gallery, "→ next step"));
        assert_eq!(captions(&gallery), ["cancelled · 390", "cancelled · 1120"]);
        assert_eq!(played(&gallery), (3, Vec::new()), "the last step acts on nothing");
        let panel = gallery.panel();
        assert_eq!(panel[2].trim(), "the journey ends here");
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
        assert_eq!(gallery.sidebar.talk().text(), "ba");
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

    #[test]
    fn a_journey_picked_talks_about_the_whole_path_as_it_steps() {
        let root = shop("notes-journey");
        let mut gallery = opened(&root, (1440.0, 900.0));
        let journey = journey_of(&gallery, "cancel-an-order");
        showing(&mut gallery, journey);
        assert_eq!(gallery.panel()[0], "→ cancel-an-order · the whole journey");
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet");
        assert!(gallery.state_card().is_none(), "the step's state is not what is talked about");
        let line = format!("{} · journey cancel-an-order", root.display());
        assert_eq!(gallery.reference().unwrap().line(), line);
        gallery.chord("c");
        assert!(
            gallery.panel().iter().any(|said| said.ends_with(&line)),
            "⌘C copies it: {:?}",
            gallery.panel()
        );

        gallery.acted = None;
        type_text(&mut gallery, "cancelling takes four steps; too many");
        press_keys(&mut gallery, &["Enter"]);
        let note = notes::read(&root).unwrap().remove(0);
        assert_eq!((note.id.as_str(), &note.on), ("N1", &On::Journey { journey: "cancel-an-order".into() }));
        let file: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join(notes::NOTES)).unwrap()).unwrap();
        assert_eq!(file["notes"][0]["journey"], "cancel-an-order", "kept in its directory's notes.json");
        let Line { row, state, .. } = &gallery.lines[journey];
        assert_eq!(gallery.waiting_notes(row, state.as_ref()), 1, "counted on its journey");

        press_keys(&mut gallery, &["→"]);
        assert_eq!(played(&gallery).0, 2);
        assert_eq!(gallery.panel()[0], "→ cancel-an-order · the whole journey", "kept as it steps");
        assert_eq!(talk_shown(&gallery)[1], "cancelling takes four steps; too many", "the note stays shown");

        assert!(gallery.press_at(on_node(&gallery, 0, "summary-cancel")), "a node in the step picked");
        assert_eq!(picked_node(&gallery), Some("summary-cancel"));
        assert_eq!(talk_shown(&gallery)[0], "note · nothing said yet", "the node's own conversation");
        assert!(matches!(gallery.reference().unwrap().on, On::Node { .. }));
        press_keys(&mut gallery, &["Esc"]);
        assert_eq!(talk_shown(&gallery)[1], "cancelling takes four steps; too many", "back to the journey's");

        let orders = line_of(&gallery, "orders");
        showing(&mut gallery, orders);
        assert!(gallery.click(Do::Visit(Visit::Note(root.clone(), "N1".into()))));
        assert_eq!(gallery.line, Some(journey), "its inbox item plays the journey");
        assert_eq!(talk_shown(&gallery)[1], "cancelling takes four steps; too many");
        std::fs::remove_dir_all(root).unwrap();
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
            .map(|thumb| (thumb.state.name(), thumb.journeys.iter().map(|p| p.label.as_str()).collect()))
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
        let Picture::Overview(overview) = &gallery.picture else { panic!("no overview is shown") };
        let passes = overview.passes(&gallery.desk());
        let (open, cell) = (
            passes[0].0,
            cells(&gallery)[gallery.lines[order + 1..].iter().position(|l| l.row.name == "open").unwrap()],
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
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("no picture") };
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
        let Picture::Faults(Faults { findings: faults, .. }) = &gallery.picture else {
            panic!("its faults are shown")
        };
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert_eq!(faults[0].at, "steps.1.on");
        assert!(faults[0].problem.contains("does not reach the next step"), "{faults:?}");
        let list = faults_listed(&mut gallery);
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
        // Its conversation not shown while the agent answers, the answer
        // comes in unread.
        gallery.pick(orders);
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
        assert_eq!(notes::read(&dir).unwrap()[0].stands(), Stands::Read, "shown, it is read");
        gallery.pick(orders);
        assert_eq!(
            inbox(&gallery)[..],
            [
                &["waits on you · 2"][..],
                &guesses,
                &["▾ waiting on the agent · 1", "N2 on the whole of default"]
            ]
            .concat(),
            "read, it leaves the inbox"
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
        assert_eq!(everything[at_orders + 1..at_orders + 5], guesses, "{everything:?}");
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
                "waits on you · 1",
                guesses[2],
                guesses[3],
                "▾ waiting on the agent · 2",
                "N1 on nav-export in default at 390",
                "N2 on the whole of default",
            ],
            "answered ✓, the assumption leaves as the files change; the ✓ goes on in nav-export's note, which waits on the agent"
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
        assert!(gallery.panes.side().is_none());
        let backslash = key_on("\\", KeyCode::Backslash, false);
        assert!(!gallery.feed([backslash.clone()]).redraw, "\\ alone is no shortcut");
        assert!(gallery.panes.side().is_none());
        gallery.feed([Input::Modifiers(ModifiersState::CONTROL), backslash]);
        assert!(gallery.panes.side().is_some(), "Ctrl+\\ shows it again, as ⌘\\ does");
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
        assert_eq!(gallery.sidebar.talk().text(), "", "⌘N types nothing");

        gallery.press_at(on_node(&gallery, 0, "nav-save"));
        gallery.let_go();
        assert_eq!(picked_node(&gallery), Some("nav-save"));
        type_text(&mut gallery, "too small");
        assert!(gallery.chord("a"), "⌘A selects all of the box");
        assert!(gallery.chord("c"), "⌘C copies what is selected in it");
        assert_eq!(board().as_deref(), Some("too small"), "not the reference");
        assert!(gallery.chord("x"), "⌘X cuts it");
        assert_eq!(gallery.sidebar.talk().shown().text, "");
        assert!(!gallery.chord("x"), "with nothing selected, nothing to cut");
        assert!(gallery.chord("v"), "⌘V pastes what was cut");
        assert!(gallery.chord("v"));
        assert_eq!(gallery.sidebar.talk().shown().text, "too smalltoo small");
        press_keys(&mut gallery, &["Esc"]);
        assert!(gallery.rejecting.is_none());
    }

    #[test]
    fn the_wheel_scrolls_the_pane_under_the_pointer_by_notches_or_points_and_across_with_shift() {
        let mut gallery = opened(drafts(), (1000.0, 900.0));
        show(&mut gallery, "airbnb-listing", "default");
        let notch = |y: f64| Input::Wheel { x: 0.0, y, unit: Unit::Lines };
        gallery.move_to((gallery.panes.pictures().x0 + MARGIN, MARGIN));
        assert!(gallery.feed([notch(-2.0)]).redraw);
        assert_eq!(
            gallery.panes.scrolled(Scrolls::Down),
            2.0 * NOTCH,
            "two notches toward the person scroll down"
        );
        gallery.feed([Input::Wheel { x: 0.0, y: 15.0, unit: Unit::Points }]);
        assert_eq!(gallery.panes.scrolled(Scrolls::Down), 2.0 * NOTCH - 15.0, "a trackpad scrolls by points");
        gallery.feed([Input::Wheel { x: -10.0, y: 0.0, unit: Unit::Points }]);
        assert_eq!(gallery.panes.scrolled(Scrolls::Across), 10.0, "and across");

        gallery.feed([Input::Modifiers(ModifiersState::SHIFT), notch(-1.0)]);
        assert_eq!(
            (gallery.panes.scrolled(Scrolls::Down), gallery.panes.scrolled(Scrolls::Across)),
            (2.0 * NOTCH - 15.0, 10.0 + NOTCH),
            "Shift turns it across"
        );
        gallery.feed([Input::Wheel { x: -5.0, y: -5.0, unit: Unit::Points }]);
        assert_eq!(
            (gallery.panes.scrolled(Scrolls::Down), gallery.panes.scrolled(Scrolls::Across)),
            (2.0 * NOTCH - 10.0, 15.0 + NOTCH),
            "a wheel already turning across scrolls as it turns, Shift held or not"
        );
        gallery.feed([Input::Modifiers(ModifiersState::empty())]);

        let at = gallery.panes.scrolled(Scrolls::Down);
        gallery.move_to((40.0, MARGIN));
        gallery.feed([notch(-1.0)]);
        assert_eq!(
            gallery.panes.scrolled(Scrolls::Down),
            at,
            "over the tree, the tree scrolls, not the pictures"
        );
    }

    #[test]
    fn the_pointer_over_a_line_between_panes_grips_and_an_input_method_is_on_only_while_there_is_a_box() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "default");
        let moved = |gallery: &mut Gallery, at| gallery.feed([Input::Moved(at)]).cursor;
        let (tree, side) = (gallery.panes.pictures().x0, gallery.panes.pictures().x1);
        assert_eq!(moved(&mut gallery, (tree + 1.0, 300.0)), Some(CursorIcon::EwResize));
        assert_eq!(moved(&mut gallery, (side - 1.0, 300.0)), Some(CursorIcon::EwResize));
        assert_eq!(moved(&mut gallery, (tree + 10.0, 300.0)), Some(CursorIcon::Default));
        gallery.press_at((gallery.panes.pictures().x0, 300.0));
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
        assert!(caret.x as f64 >= gallery.panes.pictures().x1, "in the sidebar, in the box at its foot");
        assert!(gallery.compose(Ime::Preedit("ni".into(), None)));
        assert!(gallery.sidebar.talk().is_composing());
        assert!(gallery.compose(Ime::Disabled), "the input method turned off");
        assert!(!gallery.sidebar.talk().is_composing(), "drops what it was composing");
        gallery.chord("\\");
        let effects = gallery.feed([Input::Moved((0.0, 0.0))]);
        assert!(!effects.ime && effects.caret.is_none(), "the sidebar hidden: the input method off");
        assert!(!gallery.compose(Ime::Commit("你".into())), "and nothing it commits goes anywhere");
        assert_eq!(gallery.sidebar.talk().text(), "");

        assert!(gallery.feed([Input::Close]).exit, "closing the window exits");
    }

    /// Each surface of the window drawn to a PNG and held to a golden one.
    mod frames;
}
