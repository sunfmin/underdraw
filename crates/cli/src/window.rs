//! The gallery's window, run in a process of its own: the tree of
//! descriptions on the left and, for the one picked, its first fixture
//! drawn at a phone and a desktop width side by side. The tree and panels
//! are drawn by hand, in the inks a wireframe is drawn in.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use underdraw_format::Finding;
use underdraw_layout::{Align, Draw, Font, Ink, Measure, Rect};
use vello::Scene;
use vello::kurbo::{Affine, Rect as Area};
use vello::peniko::Fill;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::draw::{self, Wanted};
use crate::gallery::{self, Row};
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
/// The space between one finding and the next.
const GAP: f64 = 12.0;

/// Opens the window over `root` and runs it until the person closes it.
/// Says on stdout, in one JSON line, whether the window opened, and nothing
/// after: the command that started this process reads that line and exits.
pub fn run(root: &Path) {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => return report(&json!({ "error": e.to_string() })),
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    let typesetter = match text::Typesetter::new(None) {
        Ok(typesetter) => typesetter,
        Err(e) => return report(&json!({ "error": e })),
    };
    let mut gallery = Gallery::new(root, typesetter);
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
    rows: Vec<Row>,
    /// The row of the description shown.
    picked: Option<usize>,
    picture: Picture,
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
}

impl Gallery {
    /// The tree of `root`, nothing picked and no window open yet.
    fn new(root: &Path, typesetter: text::Typesetter) -> Gallery {
        Gallery {
            root: root.to_path_buf(),
            rows: gallery::tree(root),
            picked: None,
            picture: Picture::Faults(Vec::new()),
            scroll: 0.0,
            tree_scroll: 0.0,
            cursor: (0.0, 0.0),
            size: (0.0, 0.0),
            typesetter,
            shown: None,
            reported: false,
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

    /// A press where the pointer is: picks the description on the row of the
    /// tree under it. Whether that changed what is shown.
    fn press(&mut self) -> bool {
        if self.cursor.0 >= TREE {
            return false;
        }
        let row = ((self.cursor.1 + self.tree_scroll - MARGIN / 2.0) / ROW).floor();
        if row < 0.0 || Some(row as usize) == self.picked {
            return false;
        }
        let before = self.picked;
        self.pick(row as usize);
        self.picked != before
    }

    /// The description on row `index` drawn, in its first fixture; its
    /// faults instead, when it has any.
    fn pick(&mut self, index: usize) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let Some(dir) = row.description.clone() else {
            return;
        };
        self.picked = Some(index);
        self.scroll = 0.0;
        if !row.faults.is_empty() {
            self.picture = Picture::Faults(row.faults.clone());
            return;
        }
        let mut pages = Vec::new();
        for width in WIDTHS {
            let wanted = Wanted { fixture: None, phase: None, width, scale: 1 };
            match draw::lay_out(&dir, &wanted) {
                Ok(laid) => {
                    let drawing = laid.draw(1, u32::MAX);
                    pages.push(Page {
                        caption: format!("{} · {width}", drawing.shown.name()),
                        width: f64::from(drawing.page.width),
                        height: f64::from(drawing.page.height),
                        scene: drawing.scene,
                    });
                }
                Err(e) => {
                    let at = crate::SCREEN.to_string();
                    self.picture = Picture::Faults(vec![Finding { at, problem: format!("{e:#}") }]);
                    return;
                }
            }
        }
        self.picture = Picture::Pages(pages);
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
                MARGIN + CAPTION + tallest * self.zoom(pane) + MARGIN
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
        let path = self.picked.and_then(|i| self.rows.get(i)?.description.clone()).unwrap_or_default();
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
        let below = (self.rows.len() as f64 * ROW + MARGIN - height).max(0.0);
        self.tree_scroll = self.tree_scroll.clamp(0.0, below);
    }

    /// The whole window, in points.
    fn frame(&mut self) -> Scene {
        let (width, height) = self.size;
        let mut list =
            vec![Draw::Line { from: [TREE as f32, 0.0], to: [TREE as f32, height as f32], ink: Ink::Line }];
        for (index, row) in self.rows.iter().enumerate() {
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
            let (name, ink, weight) = match row.description {
                Some(_) => (row.name.clone(), Ink::Text, 400),
                None => (format!("{}/", row.name), Ink::Muted, 600),
            };
            let mut right = TREE - 8.0;
            if !row.faults.is_empty() {
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
        match &self.picture {
            Picture::Pages(shots) => {
                let mut x = left;
                for shot in shots {
                    let top = MARGIN - self.scroll;
                    pane.push(words(
                        rect(x, top, shot.width * zoom, 16.0),
                        shot.caption.clone(),
                        Ink::Muted,
                        400,
                    ));
                    let at = (x, top + CAPTION);
                    pane.push(Draw::Box {
                        rect: rect(at.0 - 0.5, at.1 - 0.5, shot.width * zoom + 1.0, shot.height * zoom + 1.0),
                        radius: 0.0,
                        fill: None,
                        stroke: Some(Ink::Line),
                        dashed: false,
                    });
                    pages.push((at, &shot.scene));
                    x += shot.width * zoom + MARGIN;
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

impl ApplicationHandler for Gallery {
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
                report(&json!({ "opened": true }));
                self.reported = true;
                // Drawn once the command has been told, so it answers at once.
                if let Some(first) = self.rows.iter().position(|row| row.description.is_some()) {
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

    fn row_of(gallery: &Gallery, name: &str) -> usize {
        gallery.rows.iter().position(|row| row.name == name).unwrap()
    }

    #[test]
    fn picking_a_row_draws_its_first_fixture_at_both_widths() {
        let mut gallery = gallery();
        let docs = row_of(&gallery, "docs");
        over_row(&mut gallery, docs);
        assert!(gallery.press(), "a description's row is picked");
        assert_eq!(gallery.picked, Some(docs));
        let Picture::Pages(pages) = &gallery.picture else { panic!("docs has no faults") };
        let widths: Vec<f64> = pages.iter().map(|page| page.width).collect();
        assert_eq!(widths, vec![390.0, 1120.0]);
        assert!(pages[0].caption.starts_with("default"), "the first fixture: {}", pages[0].caption);
        assert!(!gallery.press(), "picking it again changes nothing");
        gallery.cursor = (TREE + 100.0, 100.0);
        assert!(!gallery.press(), "a press on the pictures picks nothing");
    }

    #[test]
    fn a_tall_page_scrolls_as_far_as_its_end() {
        let mut gallery = gallery();
        let listing = row_of(&gallery, "airbnb-listing");
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
        assert_eq!(
            (gallery.scroll, gallery.tree_scroll),
            (0.0, 0.0),
            "over the tree, the tree scrolls, and it fits"
        );
    }

    #[test]
    fn a_description_with_faults_shows_them() {
        let root = std::env::temp_dir().join(format!("underdraw-window-faults-{}", std::process::id()));
        std::fs::create_dir_all(root.join("broken")).unwrap();
        std::fs::write(root.join("broken").join(crate::SCREEN), "{}").unwrap();
        let mut gallery = Gallery::new(&root, text::Typesetter::new(None).unwrap());
        gallery.size = (1440.0, 900.0);
        let broken = &gallery.rows[0];
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
}
