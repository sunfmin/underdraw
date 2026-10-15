//! The gallery's window, run in a process of its own: the tree of
//! descriptions and their states on the left and, for the state picked, its
//! picture drawn at a phone and a desktop width side by side, where a node
//! can be picked to read what it is. The tree and panels are drawn by hand,
//! in the inks a wireframe is drawn in.

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

use crate::draw::{self, Shown as State, Wanted};
use crate::gallery::{self, Answering, Row};
use crate::{gpu, paint, text};

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

/// What a later run of the command over the same root asks of the window.
struct ComeForward;

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
    /// The line of the state shown, or of the description when it has none.
    picked: Option<usize>,
    picture: Picture,
    /// The id of the node picked in the picture, kept from state to state
    /// while the state shown has it.
    node: Option<String>,
    /// How far the pictures, and the tree, are scrolled down, in points.
    scroll: f64,
    tree_scroll: f64,
    /// Where the pointer is, in points.
    cursor: (f64, f64),
    /// The window's width and height, in points.
    size: (f64, f64),
    /// Sets the tree's and the captions' text.
    typesetter: text::Typesetter,
    /// The window, once open.
    shown: Option<Shown>,
    /// Whether the starting command has been told the window opened.
    reported: bool,
    /// What brings the window forward, from another thread.
    proxy: Option<EventLoopProxy<ComeForward>>,
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
        let states = row.description.as_deref().map(draw::states).and_then(Result::ok).unwrap_or_default();
        let (depth, description, faults) = (row.depth + 1, row.description.clone(), row.faults.clone());
        lines.push(Line { row, state: None });
        for state in states {
            let name = state.name().to_string();
            let row = Row { depth, name, description: description.clone(), faults: faults.clone() };
            lines.push(Line { row, state: Some(state) });
        }
    }
    lines
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
    /// Why it cannot be drawn: what `check` finds wrong with it.
    Faults(Vec<Finding>),
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
}

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
            picture: Picture::Faults(Vec::new()),
            node: None,
            scroll: 0.0,
            tree_scroll: 0.0,
            cursor: (0.0, 0.0),
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

    /// A press where the pointer is: on the tree, picks the state, or the
    /// description, on the line under it; on the pictures, picks the node
    /// under it, or none. Whether that changed what is shown.
    fn press(&mut self) -> bool {
        if self.cursor.0 >= TREE {
            let node = self.node_at(self.cursor);
            let changed = node != self.node;
            self.node = node;
            return changed;
        }
        let line = ((self.cursor.1 + self.tree_scroll - MARGIN / 2.0) / ROW).floor();
        if line < 0.0 {
            return false;
        }
        match self.shows(line as usize) {
            Some(index) if Some(index) != self.picked => {
                self.pick(index);
                true
            }
            _ => false,
        }
    }

    /// The line whose picture picking line `index` shows: a state's own; a
    /// description's first state, the line under it, or the description
    /// itself when it has none; nothing for a directory.
    fn shows(&self, index: usize) -> Option<usize> {
        let line = self.lines.get(index)?;
        line.row.description.as_ref()?;
        let first = self.lines.get(index + 1).filter(|next| line.state.is_none() && next.state.is_some());
        Some(if first.is_some() { index + 1 } else { index })
    }

    /// What picking line `index` shows drawn at each of `WIDTHS`.
    fn pick(&mut self, index: usize) {
        let Some(index) = self.shows(index) else {
            return;
        };
        let line = &self.lines[index];
        let Some(dir) = line.row.description.clone() else {
            return;
        };
        let (state, faults) = (line.state.clone(), line.row.faults.clone());
        self.picked = Some(index);
        self.scroll = 0.0;
        if !faults.is_empty() {
            self.picture = Picture::Faults(faults);
            return;
        }
        let mut pages = Vec::new();
        for width in WIDTHS {
            let wanted = match &state {
                Some(state) => state.wanted(width, 1),
                // A description with no state: lay_out says why.
                None => Wanted { fixture: None, phase: None, width, scale: 1 },
            };
            match draw::lay_out(&dir, &wanted) {
                Ok(laid) => {
                    let drawing = laid.draw(1, u32::MAX);
                    pages.push(Page {
                        caption: format!("{} · {width}", drawing.shown.name()),
                        width: f64::from(drawing.page.width),
                        height: f64::from(drawing.page.height),
                        scene: drawing.scene,
                        nodes: drawing.page.nodes,
                        calls: drawing.calls,
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
                    .then(|| page.node_at((x, y)).map(|node| node.id.clone()))
            })
            .flatten()
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

    /// What the panel says of the node picked, one line each: its id, kind,
    /// the operation it calls, what was assumed there, then its label and
    /// box at each width. Nothing when no node is picked.
    fn panel(&self) -> Vec<String> {
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
            Picture::Faults(_) => 1.0,
        }
    }

    /// How tall the pictures' pane is, all of it, in a pane `pane` wide.
    fn pictures_height(&mut self, pane: f64) -> f64 {
        match &self.picture {
            Picture::Pages(pages) => {
                let tallest = pages.iter().map(|p| p.height).fold(0.0, f64::max);
                MARGIN + CAPTION + tallest * self.zoom(pane) + MARGIN + self.panel_height()
            }
            Picture::Faults(_) => self.faults(pane).1 + MARGIN,
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
        let path = self.picked.and_then(|i| self.lines.get(i)?.row.description.clone()).unwrap_or_default();
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
            let (name, ink, weight) = match (&row.description, state) {
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
        let (origins, outlines, panel) = (self.origins(), self.outlines(), self.panel());
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
            Picture::Faults(_) => {}
        }

        let mut scene = Scene::new();
        let at = Affine::IDENTITY;
        let paper = Area::new(0.0, 0.0, width, height);
        scene.fill(Fill::NonZero, at, vello::peniko::Color::WHITE, None, &paper);
        scene.push_clip_layer(Fill::NonZero, at, &Area::new(TREE, 0.0, width, height));
        for ((x, y), page) in pages {
            scene.append(page, Some(Affine::translate((x, y)) * Affine::scale(zoom)));
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

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x: x as f32, y: y as f32, width: width.max(0.0) as f32, height: height.max(0.0) as f32 }
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

impl ApplicationHandler<ComeForward> for Gallery {
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
                    self.answering =
                        Answering::start(&self.root, move || proxy.send_event(ComeForward).is_ok());
                }
                report(&json!({ "opened": true }));
                self.reported = true;
                // Drawn once the command has been told, so it answers at once.
                if let Some(first) = self.lines.iter().position(|line| line.row.description.is_some()) {
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

    fn user_event(&mut self, _: &ActiveEventLoop, _: ComeForward) {
        if let Some(shown) = &self.shown {
            shown.window.set_minimized(false);
            shown.window.set_visible(true);
            shown.window.focus_window();
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

    /// The names of the pictures drawn, each with its width.
    fn captions(gallery: &Gallery) -> Vec<&str> {
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture: it has faults") };
        pages.iter().map(|page| page.caption.as_str()).collect()
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
    fn picking_a_description_picks_its_first_state_at_both_widths() {
        let mut gallery = gallery();
        let docs = line_of(&gallery, "docs");
        over_row(&mut gallery, docs + 2);
        assert!(gallery.press(), "a later state is picked");
        over_row(&mut gallery, docs);
        assert!(gallery.press(), "the description's line is picked");
        assert_eq!(gallery.picked, Some(docs + 1), "its first state, the line under it");
        let Picture::Pages(pages) = &gallery.picture else { panic!("docs has no faults") };
        let widths: Vec<f64> = pages.iter().map(|page| page.width).collect();
        assert_eq!(widths, vec![390.0, 1120.0]);
        assert_eq!(captions(&gallery), ["default · 390", "default · 1120"]);
        assert!(!gallery.press(), "picking it again changes nothing");
        over_row(&mut gallery, docs + 1);
        assert!(!gallery.press(), "nor does picking the state it shows");
        gallery.cursor = (TREE + 100.0, 100.0);
        gallery.press();
        assert_eq!(gallery.picked, Some(docs + 1), "a press on the pictures leaves the state picked");
    }

    #[test]
    fn a_tall_page_scrolls_as_far_as_its_end() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        over_row(&mut gallery, listing);
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
        showing(&mut gallery, listing);
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
        showing(&mut gallery, listing);
        gallery.cursor = on_node(&gallery, 1, "photos-show-all");
        gallery.press();
        assert_eq!(gallery.node.as_deref(), Some("photos-show-all"), "not the photos under it");
    }

    #[test]
    fn switching_state_keeps_the_node_picked_while_the_state_has_it() {
        let mut gallery = gallery();
        let listing = line_of(&gallery, "airbnb-listing");
        showing(&mut gallery, listing);
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
        showing(&mut gallery, listing);
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
}
