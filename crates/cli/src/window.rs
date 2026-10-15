//! The gallery's window, run in a process of its own: the tree of
//! descriptions and their states on the left and, for the state picked, its
//! picture drawn at a phone and a desktop width side by side, where a node
//! can be picked to read what it is, or acted on to follow what the
//! description says happens, as a prototype; for a directory picked, the
//! flow between the screens under it. The tree and panels are drawn by hand,
//! in the inks a wireframe is drawn in. The window is drawn from the files
//! as they now stand: as they change under its root, the tree and the state
//! shown follow.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use underdraw_format::Finding;
use underdraw_layout::{Align, Draw, Font, Ink, Measure, Placed, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Rect as Area};
use vello::peniko::Fill;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::act::{self, Act};
use crate::draw::{self, Handlers, Shown as State, Wanted};
use crate::flow::{self, Map, Screen};
use crate::gallery::{self, Answering, Row};
use crate::{gpu, links, paint, text};

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
/// One line of the panel of the node picked.
const PANEL_LINE: f64 = 20.0;
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
pub fn run(root: &Path) {
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
    /// How far the pictures, and the tree, are scrolled down, in points.
    scroll: f64,
    tree_scroll: f64,
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
}

/// One line of the tree: a directory, a description, or one of a
/// description's states, listed under it.
struct Line {
    /// The directory or description; for a state, the description it is of,
    /// one level further in and named as the state is.
    row: Row,
    state: Option<State>,
}

/// The lines of the tree over `root`: each description followed by its
/// states, as `check` finds them. A description that cannot be read has none.
fn lines(root: &Path) -> Vec<Line> {
    let mut lines = Vec::new();
    for row in gallery::tree(root) {
        let states = row.description().map(draw::states).and_then(Result::ok).unwrap_or_default();
        let (depth, dir, faults) = (row.depth + 1, row.dir.clone(), row.faults.clone());
        lines.push(Line { row, state: None });
        for state in states {
            let name = state.name().to_string();
            let row = Row { depth, name, dir: dir.clone(), described: true, faults: faults.clone() };
            lines.push(Line { row, state: Some(state) });
        }
    }
    lines
}

/// What an act on a node came to, as the panel says it at its top.
struct Acted {
    says: String,
    /// The outcomes the person chooses from, each with the state it goes
    /// to; none once the act has gone somewhere, or where it goes nowhere.
    choices: Vec<(String, String)>,
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
    cards: Vec<Card>,
}

/// What one box of the flow shows under its name.
enum Card {
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
    /// The operations each node calls when acted on, by id.
    calls: BTreeMap<String, Vec<String>>,
    /// What each node does when acted on, by id.
    handlers: BTreeMap<String, Handlers>,
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
}

/// The font the overview's arrows are labelled in.
const ARROW_FONT: Font = Font { size: 12.0, weight: 600, line_height: 16.0 };

impl Page {
    /// The node that a press at `at`, in the page's points, is on: the
    /// innermost there, an overlay before what it floats over.
    fn node_at(&self, at: (f64, f64)) -> Option<&Placed> {
        self.nodes.iter().rev().find(|node| {
            let r = node.rect;
            let (x, y) = (f64::from(r.x), f64::from(r.y));
            (x..=x + f64::from(r.width)).contains(&at.0) && (y..=y + f64::from(r.height)).contains(&at.1)
        })
    }

    /// Whether a node of id `id` is in it.
    fn has(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }
}

impl Gallery {
    /// The tree of `root`, nothing picked and no window open yet.
    fn new(root: &Path, typesetter: text::Typesetter) -> Gallery {
        Gallery {
            root: root.to_path_buf(),
            lines: lines(root),
            picked: None,
            wanted: None,
            picture: Picture::Faults(Vec::new()),
            node: None,
            scroll: 0.0,
            tree_scroll: 0.0,
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
        }
    }

    /// Scrolled `down` points, the pane under the pointer: the tree or the
    /// pictures.
    fn wheel(&mut self, down: f64) {
        if self.cursor.0 < TREE {
            self.tree_scroll += down;
        } else {
            self.scroll += down;
        }
        self.clamp();
    }

    /// A press where the pointer is: on the tree, picks the line under it;
    /// on the pictures, picks the node under it, or none, or acts on it
    /// while [`Gallery::acting`]; on an outcome listed in the panel, chooses
    /// it; on the flow, picks the description whose box is under it.
    /// Whether that changed what is shown.
    fn press(&mut self) -> bool {
        if self.cursor.0 >= TREE
            && let Picture::Flow(flow) = &self.picture
        {
            let at = self.in_flow(self.cursor);
            let Some(Screen::Drawn(dir)) = flow.map.place_at(at).map(|place| &place.screen) else {
                return false;
            };
            let Some(index) = self.description_line(dir) else {
                return false;
            };
            self.pick(index);
            return true;
        }
        if self.cursor.0 >= TREE {
            if let Some(line) = self.panel_line(self.cursor.1) {
                return self.choose(line);
            }
            if let Picture::Overview(_) = self.picture {
                let Some(line) = self.state_at(self.cursor) else {
                    return false;
                };
                self.history.clear();
                self.acted = None;
                self.pick(line);
                return true;
            }
            if self.acting {
                return self.act();
            }
            let node = self.node_at(self.cursor);
            let changed = node != self.node || self.acted.is_some();
            self.node = node;
            self.acted = None;
            return changed;
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

    /// The line picking line `index` shows: its own, a directory's (its
    /// flow), a description's (its overview) or a state's.
    fn shows(&self, index: usize) -> Option<usize> {
        self.lines.get(index)?;
        Some(index)
    }

    /// The tree read again from the files as they now stand, and the state
    /// or overview shown drawn again from them, scrolled as it was. Where
    /// that state went, its description's overview is shown; where its
    /// description went too, the first description's.
    fn reload(&mut self) {
        let (wanted, scroll) = (self.wanted.take(), self.scroll);
        self.lines = lines(&self.root);
        self.picked = None;
        self.acted = None;
        self.picture = Picture::Faults(Vec::new());
        let again = wanted.as_ref().and_then(|wanted| self.line_named(wanted));
        let description = wanted.as_ref().and_then(|wanted| {
            self.lines.iter().position(|line| line.state.is_none() && line.row.dir == wanted.dir)
        });
        let first = self.lines.iter().position(|line| line.row.description().is_some());
        if let Some(index) = again.or(description).or(first) {
            self.pick(index);
        }
        if again.is_some() {
            self.scroll = scroll;
        } else if description.is_some() {
            self.wanted = wanted;
        }
        self.clamp();
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
        let Some((id, handlers, calls)) = self.hit(self.cursor).map(|(page, node)| {
            let handlers = page.handlers.get(&node.id).cloned().unwrap_or_default();
            (node.id.clone(), handlers, page.calls.get(&node.id).cloned().unwrap_or_default())
        }) else {
            let changed = self.node.is_some() || self.acted.is_some();
            self.node = None;
            self.acted = None;
            return changed;
        };
        self.node = Some(id);
        let Some(here) = self.wanted.clone() else {
            return true;
        };
        let state = here.state.clone().unwrap_or_default();
        let on = format!("{}.{}", handlers.written, act::event(&handlers.events));
        let screen = crate::read(&here.dir).map(|(screen, _)| screen).unwrap_or(Value::Null);
        match act::act(&screen, &state, &handlers.written, &handlers.events) {
            Act::Go { to } => {
                let says = format!("{on} in {state} → {to}");
                self.go(Picked { dir: here.dir, state: Some(to) }, says);
            }
            Act::Choose(choices) => {
                let says = format!("{on} in {state} calls {}: click how it answered", calls.join(", "));
                self.acted = Some(Acted { says, choices });
            }
            Act::Navigate(screen) => match links::declared(&self.root).remove(&screen) {
                Some(description) => {
                    let says = format!("{on} navigates to {screen}");
                    self.go(Picked { dir: description, state: None }, says);
                }
                None => self.say(format!(
                    "{on} navigates to {screen}, which no description under the root declares: a screen not drawn yet"
                )),
            },
            Act::Back => match self.history.pop() {
                Some(back) => {
                    let to = self.named(&back);
                    match self.line_named(&back) {
                        Some(index) => {
                            self.pick(index);
                            self.say(format!("{on} goes back to {to}"));
                        }
                        None => self.say(format!("{on} goes back to {to}, which is gone")),
                    }
                }
                None => self.say(format!(
                    "{on} goes back, but no act led here: back returns along the path taken by acting"
                )),
            },
            Act::Nothing => self.say(format!("no transition from {state} on {on}")),
        }
        true
    }

    /// The outcome on line `line` of the panel chosen: the state it goes
    /// to shown. Whether there was one there.
    fn choose(&mut self, line: usize) -> bool {
        let chosen = self.acted.as_ref().and_then(|acted| acted.choices.get(line.checked_sub(1)?)).cloned();
        let (Some((outcome, to)), Some(here)) = (chosen, self.wanted.clone()) else {
            return false;
        };
        let state = here.state.clone().unwrap_or_default();
        self.go(Picked { dir: here.dir, state: Some(to.clone()) }, format!("{outcome} in {state} → {to}"));
        true
    }

    /// The state `to` names shown, or its description's first state when
    /// it names none, as an act on a node led to it: where
    /// the person was is kept for `back`, and the panel says `says`.
    fn go(&mut self, to: Picked, says: String) {
        let Some(mut index) = self.line_named(&to) else {
            return self.say(format!("{says}, which the tree does not list"));
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
    /// a description's overview, or a directory's flow.
    fn pick(&mut self, index: usize) {
        let Some(index) = self.shows(index) else {
            return;
        };
        let line = &self.lines[index];
        let (dir, described) = (line.row.dir.clone(), line.row.description().is_some());
        let (state, faults) = (line.state.clone(), line.row.faults.clone());
        self.picked = Some(index);
        let name = state.as_ref().map(|state| state.name().to_string());
        self.wanted = Some(Picked { dir: dir.clone(), state: name });
        self.scroll = 0.0;
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
        let mut pages = Vec::new();
        for width in WIDTHS {
            let wanted = match &state {
                Some(state) => state.wanted(width, 1),
                // A description with no state: lay_out says why.
                None => Wanted { fixture: None, phase: None, width, scale: 1 },
            };
            match draw::lay_out(&dir, &wanted, Some(&drawn)) {
                Ok(laid) => {
                    let drawing = laid.draw(1, u32::MAX);
                    pages.push(Page {
                        caption: format!("{} · {width}", drawing.shown.name()),
                        width: f64::from(drawing.page.width),
                        height: f64::from(drawing.page.height),
                        scene: drawing.scene,
                        nodes: drawing.page.nodes,
                        calls: drawing.calls,
                        handlers: drawing.handlers,
                    });
                }
                Err(e) => {
                    let at = crate::SCREEN.to_string();
                    self.picture = Picture::Faults(vec![Finding { at, problem: format!("{e:#}") }]);
                    return;
                }
            }
        }
        let kept = self.node.as_ref().is_some_and(|id| pages.iter().any(|page| page.has(id)));
        if !kept {
            self.node = None;
        }
        self.picture = Picture::Pages(pages);
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
        // Each state laid out on a thread of its own, all at once.
        let drawings: Vec<anyhow::Result<draw::Drawing>> = std::thread::scope(|scope| {
            let laying: Vec<_> = states
                .iter()
                .map(|(_, state)| {
                    scope.spawn(move || {
                        Ok(draw::lay_out(dir, &state.wanted(OVERVIEW, 1), Some(drawn))?.draw(1, u32::MAX))
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
            });
        }
        let at = |name: &str| thumbs.iter().position(|thumb| thumb.name == name);
        let mut arrows = Vec::new();
        for transition in draw::transitions(dir)? {
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
            arrows.push(Arrow { from, to, label, wide });
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
        let columns = Self::columns(self.size.0 - TREE);
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

    /// The overview's arrows and their labels, in window points. An arrow
    /// runs from the edge of one picture to the edge of the other, each to
    /// the right of its way, so that two between the same states, either
    /// way, run apart, and is labelled nearer where it starts, so that their
    /// labels do too; one that stays in its state loops out of its right.
    fn arrows(&self) -> Vec<Draw> {
        let Picture::Overview(overview) = &self.picture else {
            return Vec::new();
        };
        let cells = self.cells();
        let mut list = Vec::new();
        for (index, arrow) in overview.arrows.iter().enumerate() {
            let before =
                overview.arrows[..index].iter().filter(|a| (a.from, a.to) == (arrow.from, arrow.to)).count();
            let (x, y, w, h) = cells[arrow.from];
            let (start, end, middle) = if arrow.from == arrow.to {
                let top = y + 16.0 + before as f64 * 32.0;
                let out = x + w + 20.0;
                list.push(line((x + w, top), (out, top), Ink::Text));
                list.push(line((out, top), (out, top + 18.0), Ink::Text));
                ((out, top + 18.0), (x + w + 2.0, top + 18.0), (out + 4.0 + arrow.wide / 2.0, top + 9.0))
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
                (start, end, (start.0 + (end.0 - start.0) * along, start.1 + (end.1 - start.1) * along))
            };
            list.push(line(start, end, Ink::Text));
            list.extend(head(start, end, Ink::Text));
            let at = rect(middle.0 - arrow.wide / 2.0, middle.1 - 9.0, arrow.wide, 18.0);
            list.push(Draw::Box {
                rect: at,
                radius: 9.0,
                fill: Some(Ink::Paper),
                stroke: Some(Ink::Text),
                dashed: false,
            });
            let label = Draw::Text {
                rect: rect(middle.0 - arrow.wide / 2.0, middle.1 - 8.0, arrow.wide, 16.0),
                text: arrow.label.clone(),
                font: ARROW_FONT,
                ink: Ink::Text,
                max_lines: Some(1),
                align: Align::Center,
                underline: false,
                strike: false,
            };
            list.push(label);
        }
        list
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
        let cards = map
            .places
            .iter()
            .map(|place| match &place.screen {
                Screen::Drawn(dir) => self.card(dir, &drawn),
                Screen::Undrawn(_) => Card::Undrawn,
            })
            .collect();
        Flow { map, cards }
    }

    /// The box of the description in `dir`: its first state at the phone's
    /// width, or how many faults keep it from being drawn.
    fn card(&self, dir: &Path, drawn: &std::collections::BTreeSet<String>) -> Card {
        let line = self.description_line(dir);
        let faults = line.map_or(0, |index| self.lines[index].row.faults.len());
        if faults > 0 {
            return Card::Faulty(faults);
        }
        let first = line.and_then(|index| self.lines.get(index + 1)).and_then(|next| next.state.clone());
        let wanted = match first {
            Some(state) => state.wanted(WIDTHS[0], 1),
            None => Wanted { fixture: None, phase: None, width: WIDTHS[0], scale: 1 },
        };
        match draw::lay_out(dir, &wanted, Some(drawn)) {
            Ok(laid) => {
                let drawing = laid.draw(1, u32::MAX);
                Card::Picture { scene: Box::new(drawing.scene), width: f64::from(drawing.page.width) }
            }
            Err(_) => Card::Faulty(1),
        }
    }

    /// Where the flow's top-left corner is in the window, in points.
    fn flow_origin(&self) -> (f64, f64) {
        (TREE + MARGIN, MARGIN - self.scroll)
    }

    /// `at`, a point in the window, in the flow's points.
    fn in_flow(&self, at: (f64, f64)) -> (f64, f64) {
        let ((left, top), zoom) = (self.flow_origin(), self.zoom(self.size.0 - TREE));
        ((at.0 - left) / zoom, (at.1 - top) / zoom)
    }

    /// Where each picture's top-left corner is in the window, in points.
    fn origins(&self) -> Vec<(f64, f64)> {
        let Picture::Pages(pages) = &self.picture else {
            return Vec::new();
        };
        let zoom = self.zoom(self.size.0 - TREE);
        let mut x = TREE + MARGIN;
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

    /// The id of the node under `at`, a point in the window, in whichever
    /// picture it falls on; none off the pictures or where no node is.
    fn node_at(&self, at: (f64, f64)) -> Option<String> {
        self.hit(at).map(|(_, node)| node.id.clone())
    }

    /// The node under `at`, a point in the window, and the picture it is
    /// in; none off the pictures or where no node is.
    fn hit(&self, at: (f64, f64)) -> Option<(&Page, &Placed)> {
        let Picture::Pages(pages) = &self.picture else {
            return None;
        };
        let zoom = self.zoom(self.size.0 - TREE);
        pages
            .iter()
            .zip(self.origins())
            .find_map(|(page, (left, top))| {
                let (x, y) = ((at.0 - left) / zoom, (at.1 - top) / zoom);
                ((0.0..=page.width).contains(&x) && (0.0..=page.height).contains(&y))
                    .then(|| page.node_at((x, y)).map(|node| (page, node)))
            })
            .flatten()
    }

    /// The line of the panel at `y`, a height in the window; none off the
    /// panel, or where there is no panel.
    fn panel_line(&self, y: f64) -> Option<usize> {
        let tall = self.panel_height();
        let top = self.size.1 - tall;
        (tall > 0.0 && y >= top).then(|| ((y - top - MARGIN / 2.0) / PANEL_LINE).floor().max(0.0) as usize)
    }

    /// The outline around the node picked, in window points, in each
    /// picture that has it.
    fn outlines(&self) -> Vec<Rect> {
        let (Some(id), Picture::Pages(pages)) = (&self.node, &self.picture) else {
            return Vec::new();
        };
        let zoom = self.zoom(self.size.0 - TREE);
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

    /// What the panel says, one line each: what the last act on a node
    /// came to, and any outcomes to choose from, each on a line of its own;
    /// then, of the node picked, its id, kind, the operation it calls, what
    /// was assumed there, then its label and box at each width. Nothing
    /// when no node is picked and nothing was acted on.
    fn panel(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(Acted { says, choices }) = &self.acted {
            lines.push(says.clone());
            for (outcome, to) in choices {
                lines.push(format!("    {outcome} → {to}"));
            }
        }
        lines.extend(self.node_panel());
        lines
    }

    /// What the panel says of the node picked: nothing when none is.
    fn node_panel(&self) -> Vec<String> {
        let (Some(id), Picture::Pages(pages)) = (&self.node, &self.picture) else {
            return Vec::new();
        };
        let found: Vec<(&Page, &Placed)> =
            pages.iter().filter_map(|page| Some((page, page.nodes.iter().find(|n| &n.id == id)?))).collect();
        let Some(&(page, node)) = found.first() else {
            return Vec::new();
        };
        let calls = match page.calls.get(id) {
            Some(operations) => operations.join(", "),
            None => "nothing".to_string(),
        };
        let mut lines = vec![
            format!("id  {id}"),
            format!("kind  {}", node.kind),
            format!("calls  {calls}"),
            format!("assumed  {}", node.assumed.as_deref().unwrap_or("nothing")),
        ];
        for page in pages {
            let width = page.width;
            lines.push(match page.nodes.iter().find(|n| &n.id == id) {
                Some(node) => {
                    let r = node.rect;
                    let label = node.label.map_or_else(|| "no label".to_string(), |n| format!("label {n}"));
                    format!("at {width}  {label} · box {}, {} · {} × {}", r.x, r.y, r.width, r.height)
                }
                None => format!("at {width}  not in this state"),
            });
        }
        lines
    }

    /// How tall the panel is, at the foot of the pictures' pane.
    fn panel_height(&self) -> f64 {
        match self.panel().len() {
            0 => 0.0,
            lines => lines as f64 * PANEL_LINE + MARGIN,
        }
    }

    /// How much the pictures are scaled down to fit side by side in a pane
    /// `pane` points wide; never up.
    fn zoom(&self, pane: f64) -> f64 {
        match &self.picture {
            Picture::Pages(pages) => {
                let wide: f64 = pages.iter().map(|p| p.width).sum();
                let gaps = MARGIN * (pages.len() + 1) as f64;
                ((pane - gaps) / wide).clamp(0.1, 1.0)
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
                MARGIN + CAPTION + tallest * self.zoom(pane) + MARGIN + self.panel_height()
            }
            Picture::Overview(overview) => {
                let rows = overview.thumbs.len().div_ceil(Self::columns(pane)) as f64;
                MARGIN + rows * (CAPTION + THUMB_TALL + SPACE) - SPACE + MARGIN
            }
            Picture::Faults(_) => self.faults(pane).1 + MARGIN,
            Picture::Flow(flow) => MARGIN + flow.map.height * self.zoom(pane) + MARGIN,
        }
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
        let heading = match faults.len() {
            1 => format!("{within} has a fault, so it is not drawn: `underdraw check` it"),
            n => format!("{within} has {n} faults, so it is not drawn: `underdraw check` it"),
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
        let (width, height) = self.size;
        let below = (self.pictures_height(width - TREE) - height).max(0.0);
        self.scroll = self.scroll.clamp(0.0, below);
        let below = (self.lines.len() as f64 * ROW + MARGIN - height).max(0.0);
        self.tree_scroll = self.tree_scroll.clamp(0.0, below);
    }

    /// The whole window, in points.
    fn frame(&mut self) -> Scene {
        let (width, height) = self.size;
        let mut list =
            vec![Draw::Line { from: [TREE as f32, 0.0], to: [TREE as f32, height as f32], ink: Ink::Line }];
        for (index, Line { row, state }) in self.lines.iter().enumerate() {
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
                (None, _) => (format!("{}/", row.name), Ink::Muted, 600),
                (Some(_), None) => (row.name.clone(), Ink::Text, 600),
                (Some(_), Some(State::Fixture(_))) => (row.name.clone(), Ink::Text, 400),
                (Some(_), Some(State::Phase(_))) => (row.name.clone(), Ink::Muted, 400),
            };
            let mut right = TREE - 8.0;
            if state.is_none() && !row.faults.is_empty() {
                let mark = match row.faults.len() {
                    1 => "1 fault".to_string(),
                    n => format!("{n} faults"),
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
        let (faults, _) = self.faults(width - TREE);
        let mut pane: Vec<Draw> = faults.into_iter().map(|draw| moved(draw, left, -self.scroll)).collect();
        let zoom = self.zoom(width - TREE);
        let mut pages = Vec::new();
        let mut thumbs = Vec::new();
        let (origins, outlines, panel) = (self.origins(), self.outlines(), self.panel());
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
                for around in outlines {
                    pane.push(Draw::Box {
                        rect: around,
                        radius: 3.0,
                        fill: None,
                        stroke: Some(Ink::Strong),
                        dashed: false,
                    });
                }
                if !panel.is_empty() {
                    let tall = self.panel_height();
                    let top = height - tall;
                    pane.push(Draw::Box {
                        rect: rect(TREE, top, width - TREE, tall),
                        radius: 0.0,
                        fill: Some(Ink::Paper),
                        stroke: Some(Ink::Line),
                        dashed: false,
                    });
                    for (index, line) in panel.into_iter().enumerate() {
                        let y = top + MARGIN / 2.0 + index as f64 * PANEL_LINE;
                        let weight = if index == 0 { 600 } else { 400 };
                        pane.push(words(
                            rect(left, y + 2.0, width - left - MARGIN, 16.0),
                            line,
                            Ink::Text,
                            weight,
                        ));
                    }
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
            }
            Picture::Faults(_) | Picture::Flow(_) => {}
        }

        let mut scene = Scene::new();
        let at = Affine::IDENTITY;
        let paper = Area::new(0.0, 0.0, width, height);
        scene.fill(Fill::NonZero, at, vello::peniko::Color::WHITE, None, &paper);
        scene.push_clip_layer(Fill::NonZero, at, &Area::new(TREE, 0.0, width, height));
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
            for (place, card) in flow.map.places.iter().zip(&flow.cards) {
                let Card::Picture { scene: picture, width } = card else { continue };
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
            paint::draws(&mut scene, at, &flow_draws(flow), &mut self.typesetter);
        }
        paint::draws(&mut scene, at, &pane, &mut self.typesetter);
        scene.pop_layer();
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

/// The flow drawn, in its own points, but for the pictures in its boxes:
/// each box with its name over it, and each navigate an arrow labelled with
/// its node, in orange to a screen not drawn yet.
fn flow_draws(flow: &Flow) -> Vec<Draw> {
    let mut list = Vec::new();
    let (places, arrows) = (&flow.map.places, &flow.map.arrows);
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
        let ink = if matches!(to.screen, Screen::Undrawn(_)) { Ink::Assumed } else { Ink::Muted };
        for pair in path.windows(2) {
            list.push(line(pair[0], pair[1], ink));
        }
        list.extend(head(path[path.len() - 2], path[path.len() - 1], ink));
        // Named where it leaves its box, so that the arrows from one box,
        // each leaving at its own height, are named apart.
        let start = path[0];
        let mut label = words(rect(start.0 + 6.0, start.1 - 17.0, 128.0, 16.0), arrow.node.clone(), ink, 400);
        if let Draw::Text { align, rect, font, .. } = &mut label {
            font.size = 11.0;
            if path[1].0 < start.0 {
                rect.x = (start.0 - 134.0) as f32;
                *align = Align::End;
            }
        }
        list.push(label);
    }
    for (place, card) in places.iter().zip(&flow.cards) {
        let (x, y) = (place.x, place.y);
        let (name, ink) = match &place.screen {
            Screen::Drawn(dir) => {
                let name =
                    dir.file_name().map_or_else(|| ".".to_string(), |n| n.to_string_lossy().to_string());
                (name, Ink::Text)
            }
            Screen::Undrawn(screen) => (screen.clone(), Ink::Assumed),
        };
        let undrawn = matches!(card, Card::Undrawn);
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
            Card::Picture { .. } => {}
            Card::Faulty(faults) => {
                let mark = match faults {
                    1 => "faulty: 1 fault, not drawn".to_string(),
                    n => format!("faulty: {n} faults, not drawn"),
                };
                let mut said =
                    words(rect(x + 8.0, middle.1 - 8.0, flow::WIDE - 16.0, 16.0), mark, Ink::Strong, 600);
                if let Draw::Text { align, .. } = &mut said {
                    *align = Align::Center;
                }
                list.push(said);
            }
            Card::Undrawn => list.push(Draw::Tag {
                rect: rect(middle.0 - 14.0, middle.1 - 14.0, 28.0, 28.0),
                text: "?".to_string(),
                ink: Ink::Assumed,
            }),
        }
    }
    list
}

/// A line from `from` to `to`.
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
            .with_title(format!("underdraw gallery — {}", self.root.display()))
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
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let held = modifiers.state();
                self.acting = held.super_key() || held.control_key();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let down = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -f64::from(y) * NOTCH,
                    MouseScrollDelta::PixelDelta(moved) => -moved.to_logical::<f64>(scale).y,
                };
                shown.window.request_redraw();
                self.wheel(down);
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let window = shown.window.clone();
                if self.press() {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
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
        let end = gallery.pictures_height(1440.0 - TREE) - 900.0;
        assert!(end > 0.0, "the listing is taller than the window");
        gallery.cursor = (TREE + 100.0, 400.0);
        gallery.wheel(120.0);
        assert_eq!(gallery.scroll, 120.0);
        gallery.wheel(1e6);
        assert_eq!(gallery.scroll, end, "no further than the end");
        gallery.wheel(-1e6);
        assert_eq!(gallery.scroll, 0.0, "no further than the top");
        over_row(&mut gallery, 0);
        gallery.wheel(500.0);
        assert_eq!((gallery.scroll, gallery.tree_scroll), (0.0, 500.0), "over the tree, the tree scrolls");
        gallery.wheel(1e6);
        let end = gallery.lines.len() as f64 * ROW + MARGIN - 900.0;
        assert_eq!(gallery.tree_scroll, end, "as far as its last line");
    }

    /// The point in the window at the middle of node `id` in picture `page`.
    fn on_node(gallery: &Gallery, page: usize, id: &str) -> (f64, f64) {
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture: it has faults") };
        let node = pages[page].nodes.iter().find(|n| n.id == id).unwrap();
        let (left, top) = gallery.origins()[page];
        let zoom = gallery.zoom(gallery.size.0 - TREE);
        let r = node.rect;
        let middle = (f64::from(r.x + r.width / 2.0), f64::from(r.y + r.height / 2.0));
        (left + middle.0 * zoom, top + middle.1 * zoom)
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
        let panel = gallery.panel();
        assert_eq!(
            panel[..5],
            [
                "id  nav-save",
                "kind  IconButton",
                "calls  toggleSaved",
                "assumed  nothing",
                "at 390  label 3 · box 334, 8 · 40 × 40",
            ]
        );
        assert!(panel[5].starts_with("at 1120  label "), "and where it is at the other width: {}", panel[5]);
        assert!(!gallery.press(), "picking it again changes nothing");
        assert_eq!(gallery.outlines().len(), 2, "it is outlined at both widths");
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
    fn a_description_with_faults_shows_them() {
        let root = std::env::temp_dir().join(format!("underdraw-window-faults-{}", std::process::id()));
        std::fs::create_dir_all(root.join("broken")).unwrap();
        std::fs::write(root.join("broken").join(crate::SCREEN), "{}").unwrap();
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let broken = &gallery.lines[0].row;
        assert!(!broken.faults.is_empty(), "the tree marks it");
        let found = broken.faults.clone();
        gallery.pick(0);
        let Picture::Faults(shown) = &gallery.picture else {
            panic!("a description with faults is not drawn")
        };
        assert_eq!(shown, &found, "each finding as `check` names it");
        let (list, _) = gallery.faults(1440.0 - TREE);
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
        for file in [crate::SCREEN, crate::TOKENS, crate::KINDS] {
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
        gallery.wheel(40.0);
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
        for file in [crate::SCREEN, crate::TOKENS, crate::KINDS] {
            std::fs::copy(root.join("todo").join(file), added.join(file)).unwrap();
        }
        gallery.reload();
        let names: Vec<&str> = gallery
            .lines
            .iter()
            .filter(|line| line.state.is_none())
            .map(|line| line.row.name.as_str())
            .collect();
        assert_eq!(names, ["a-flow", "sign-in", "todo"], "the new one, under its directory");
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
        assert_eq!(names, ["a-flow", "sign-in"], "the removed one has left");
        assert_eq!(gallery.picked, Some(1), "its description gone: the first description's overview");
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
        assert!(!gallery.lines[0].row.faults.is_empty(), "the tree marks it");
        assert_eq!(gallery.picked, Some(typing), "the same state");
        assert!(matches!(&gallery.picture, Picture::Faults(shown) if shown == &gallery.lines[0].row.faults));

        std::fs::write(root.join("todo").join(crate::KINDS), "{").unwrap();
        gallery.reload();
        let Picture::Faults(shown) = &gallery.picture else { panic!("kinds that are not JSON are a fault") };
        assert!(shown.iter().any(|f| f.problem.contains("kinds.json")), "{shown:?}");

        std::fs::copy(
            Path::new(DRAFTS).join("todo").join(crate::KINDS),
            root.join("todo").join(crate::KINDS),
        )
        .unwrap();
        write_todo_screen(&root, &fine);
        gallery.reload();
        assert!(gallery.lines[0].row.faults.is_empty(), "fixed");
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
        assert!(!gallery.lines[0].row.faults.is_empty(), "the stray field is a fault");
        assert!(gallery.lines[1].state.is_some(), "its states are still listed");
        gallery.pick(1);
        assert!(matches!(&gallery.picture, Picture::Faults(shown) if shown == &gallery.lines[0].row.faults));
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
        assert_eq!(panel[1..3], ["    ok → default", "    shipped → cancel-ready"]);
        let top = gallery.size.1 - gallery.panel_height();
        gallery.cursor = (TREE + 100.0, top + MARGIN / 2.0 + 1.5 * PANEL_LINE);
        assert!(gallery.press(), "the first outcome clicked");
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
            "order-{index}.tap navigates to OrderDetail, which no description under the root declares: a screen not drawn yet"
        );
    }

    #[test]
    fn a_node_with_nothing_declared_says_so() {
        let mut gallery = gallery();
        show(&mut gallery, "orders", "cancel-unpaid");
        act_on(&mut gallery, "cd-keep");
        assert_eq!(shown(&gallery), "orders · cancel-unpaid", "nothing declared: it stays");
        assert_eq!(gallery.panel()[0], "no transition from cancel-unpaid on cd-keep.tap");
        assert_eq!(gallery.panel()[1], "id  cd-keep", "and the node is picked");
        act_on(&mut gallery, "nav-export");
        let todo = line_of(&gallery, "todo");
        over_row(&mut gallery, todo);
        gallery.press();
        assert!(
            gallery.history.is_empty() && gallery.acted.is_none(),
            "picking in the tree starts a new path"
        );
    }

    /// A root of its own named `name`, holding copies of the drafts `copied`
    /// at the paths beside each.
    fn root_of(name: &str, copied: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("underdraw-window-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (draft, at) in copied {
            std::fs::create_dir_all(root.join(at)).unwrap();
            for file in [crate::SCREEN, crate::TOKENS, crate::KINDS] {
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
        flow_draws(flow)
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
                (from.clone(), arrow.node.clone(), flow.map.places[arrow.to].screen.clone())
            })
            .collect();
        assert_eq!(drawn, reported);
        assert!(drawn.contains(&(
            root.join("shop/listing"),
            "reserve-button".to_string(),
            Screen::Drawn(root.join("shop/checkout"))
        )));
        assert!(
            matches!(flow.cards[..2], [Card::Picture { .. }, Card::Picture { .. }]),
            "each with its first state"
        );
        assert!(flow.cards[2..].iter().all(|card| matches!(card, Card::Undrawn)));
        let texts = flow_texts(flow);
        for name in ["checkout", "listing", "PhotoGallery", "reserve-button", "photos-show-all"] {
            assert!(texts.iter().any(|text| text == name), "{name} in {texts:?}");
        }
        let draws = flow_draws(flow);
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
        let ((left, top), zoom) = (gallery.flow_origin(), gallery.zoom(1440.0 - TREE));
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
        assert!(matches!(flow.cards[..], [Card::Faulty(n), Card::Picture { .. }] if n == faults));
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
        showing(&mut gallery, 0);
        let undrawn = |gallery: &Gallery| {
            flow_shown(gallery).cards.iter().filter(|c| matches!(c, Card::Undrawn)).count()
        };
        assert_eq!(undrawn(&gallery), 2, "OrderDetail and ExportDocuments are not drawn yet");
        let export = root_of("flow-reload-export", &[("export", "export")]);
        std::fs::rename(export.join("export"), root.join("app/export")).unwrap();
        std::fs::remove_dir_all(export).unwrap();
        gallery.reload();
        assert_eq!(gallery.picked, Some(0), "the directory is still picked");
        assert_eq!(undrawn(&gallery), 1, "ExportDocuments is drawn now");
        std::fs::remove_dir_all(root).unwrap();
    }
}
