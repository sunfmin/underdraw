//! The toolbar along the window's top, over the three panes, drawn from
//! the gallery's own description of it: the Toolbar component the project
//! under `drafts/gallery` shares, built into the binary, laid out through
//! underdraw's layout as `underdraw render` lays out that component shown
//! on its own, with the window as it now stands as its `bar` (ADR 0024).
//! So the window and the render agree on every box, and changing the
//! component changes the toolbar with no edit here. What is pressed is
//! found by the id the component gives each control. It lies above the
//! gallery's points, which start at its foot: its own are from minus its
//! height to 0.

use serde_json::{Map, Value, json};
use underdraw_format::{Breakpoint, Lang, Screen, Shared, State};
use underdraw_layout::{Draw, Measure, NoPictures, Page, Rect};

use crate::area::over;

mod project {
    include!(concat!(env!("OUT_DIR"), "/gallery.rs"));
}

/// The component the toolbar is drawn from.
const COMPONENT: &str = "Toolbar";
/// The one state the window shows it in: the fixture its live `bar` is.
const LIVE: &str = "window";

/// What a control of the toolbar does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Tool {
    Back,
    Forward,
    /// A plain click on a picture inspects it.
    Inspect,
    /// A plain click on a picture acts on it, as a ⌘-click does.
    Play,
    /// The inbox of everything under the root shown in the sidebar, or no
    /// longer.
    Inbox,
    Sidebar,
    /// The marks over the pictures hidden or shown: ⌘. alone does it.
    Marks,
}

impl Tool {
    /// The control of the component whose id, as it writes it, is
    /// `written`: none for one that does nothing, such as the crumbs.
    fn written(written: &str) -> Option<Tool> {
        Some(match written {
            "back" => Tool::Back,
            "forward" => Tool::Forward,
            "inspect" => Tool::Inspect,
            "play" => Tool::Play,
            "inbox" => Tool::Inbox,
            "sidebar" => Tool::Sidebar,
            _ => return None,
        })
    }
}

/// The window as the toolbar shows it: the component's `bar`, a value of
/// the gallery domain's `Bar`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Bar {
    pub back: bool,
    pub forward: bool,
    /// Where the window is, from the root.
    pub crumbs: String,
    /// Whether a plain click on a picture acts on it.
    pub playing: bool,
    /// Whether an agent waits on the root with `notes --wait`.
    pub listening: bool,
    pub waiting_on_you: usize,
    pub inbox_open: bool,
    pub sidebar: bool,
}

impl Bar {
    /// As the component's params take it: `{"bar": …}`.
    fn value(&self) -> Value {
        json!({
            "bar": {
                "back": self.back,
                "forward": self.forward,
                "crumbs": self.crumbs,
                "mode": if self.playing { "play" } else { "inspect" },
                "agent": if self.listening { "listening" } else { "away" },
                "waitingOnYou": self.waiting_on_you,
                "inboxOpen": self.inbox_open,
                "sidebar": self.sidebar,
            }
        })
    }
}

/// The toolbar laid out for one `bar` at one width.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Toolbar {
    /// The component shown on its own, laid out as `render` lays it out.
    page: Page,
    /// Where in `page` the toolbar is: the window's top left corner.
    at: Rect,
}

impl Toolbar {
    /// The toolbar `bar` shows, `width` wide, its words measured by
    /// `measure`; or why the component cannot be laid out.
    pub fn lay_out(measure: &mut dyn Measure, width: f64, bar: &Bar, lang: Lang) -> Result<Toolbar, String> {
        Toolbar::given(measure, width, bar.value(), lang)
    }

    /// The toolbar the component's params `params` show, as
    /// [`Toolbar::lay_out`] lays it out.
    fn given(measure: &mut dyn Measure, width: f64, params: Value, lang: Lang) -> Result<Toolbar, String> {
        let (screen, pad) = showcase(params)?;
        let tokens = tokens()?;
        let page_width = width + 2.0 * pad;
        let breakpoint = Breakpoint::for_width(&tokens, page_width as u32);
        let node = screen.instantiate(State::Fixture(LIVE), breakpoint, None)?;
        let page =
            underdraw_layout::layout(&node, &tokens, &screen, page_width as f32, lang, measure, &NoPictures)?;
        let instance = kebab();
        let at = page
            .nodes
            .iter()
            .find(|node| node.id == instance)
            .map(|node| node.rect)
            .ok_or_else(|| format!("the {COMPONENT} component laid out no node {instance}"))?;
        Ok(Toolbar { page, at })
    }

    /// How tall it is.
    pub fn tall(&self) -> f64 {
        f64::from(self.at.height)
    }

    /// Where its node `id` is, as the showcase names it (`toolbar-back`),
    /// in the gallery's points.
    #[cfg(test)]
    pub fn area(&self, id: &str) -> Option<Rect> {
        self.page.nodes.iter().find(|node| node.id == id).map(|node| self.placed(node.rect))
    }

    /// `rect`, a box in the page, in the gallery's points.
    fn placed(&self, rect: Rect) -> Rect {
        Rect { x: rect.x - self.at.x, y: rect.y - self.at.y - self.at.height, ..rect }
    }

    /// Where the page is moved to draw it in the window's points, whose
    /// top is the toolbar's.
    pub fn offset(&self) -> (f64, f64) {
        (-f64::from(self.at.x), -f64::from(self.at.y))
    }

    /// What it draws, in the page's points: the wireframe, without its
    /// labels and guesses' marks.
    pub fn draws(&self) -> &[Draw] {
        self.page.wireframe()
    }

    /// What the control at `at`, a point in the gallery's points, does:
    /// the innermost under it that does anything.
    pub fn tool_at(&self, at: (f64, f64)) -> Option<Tool> {
        let instance = format!("{}-", kebab());
        self.page.nodes.iter().rev().find_map(|node| {
            let tool = Tool::written(node.id.strip_prefix(&instance)?)?;
            over(self.placed(node.rect), at).then_some(tool)
        })
    }
}

/// The id the showcase gives the component's instance (`toolbar`).
fn kebab() -> String {
    underdraw_format::showcase(COMPONENT, &Value::Null)["root"]["children"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// The gallery's own tokens.
fn tokens() -> Result<Value, String> {
    serde_json::from_str(project::TOKENS).map_err(|e| format!("drafts/gallery/tokens.json: {e}"))
}

/// The component shown on its own, as `render` reads it, its one fixture
/// `params`; and the padding around it.
fn showcase(params: Value) -> Result<(Screen, f64), String> {
    let mut components = Map::new();
    for (name, text) in project::COMPONENTS {
        let component: Value =
            serde_json::from_str(text).map_err(|e| format!("drafts/gallery/components/{name}.json: {e}"))?;
        components.insert((*name).to_string(), component);
    }
    let domain: Value =
        serde_json::from_str(project::DOMAIN).map_err(|e| format!("drafts/gallery/domain.json: {e}"))?;
    let component =
        components.get(COMPONENT).ok_or_else(|| format!("drafts/gallery has no {COMPONENT} component"))?;
    let mut shown = underdraw_format::showcase(COMPONENT, component);
    shown["fixtures"] = json!({ LIVE: params });
    let pad = shown["root"]["padding"].as_f64().unwrap_or_default();
    Ok((Screen::read(&shown, Shared { domain: Some(&domain), components: Some(&components) }), pad))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::draw::{Description, Shown};
    use crate::project::Project;
    use crate::text::Typesetter;

    const GALLERY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/gallery");

    #[test]
    fn the_toolbar_is_the_gallery_projects_components_as_they_stand_on_disk() {
        let on_disk = Project::of(Path::new(GALLERY)).unwrap();
        let built: Map<String, Value> = project::COMPONENTS
            .iter()
            .map(|(name, text)| ((*name).to_string(), serde_json::from_str(text).unwrap()))
            .collect();
        assert_eq!(built, on_disk.components, "every component, as written");
        assert_eq!(Some(serde_json::from_str::<Value>(project::DOMAIN).unwrap()), on_disk.domain);
        assert_eq!(tokens().unwrap(), on_disk.tokens);
    }

    #[test]
    fn the_window_and_render_agree_on_every_box_of_the_toolbar_in_each_of_its_fixtures() {
        let path = Path::new(GALLERY).join("components").join(COMPONENT);
        let description = Description::read(&path).unwrap();
        assert!(description.findings().is_empty(), "{:?}", description.findings());
        let screen = Project::of(&path).unwrap().screen(&underdraw_format::showcase(
            COMPONENT,
            &serde_json::from_str(project::COMPONENTS.iter().find(|(name, _)| *name == COMPONENT).unwrap().1)
                .unwrap(),
        ));
        let fixtures: Vec<String> =
            description.states().iter().map(|state| state.name().to_string()).collect();
        assert_eq!(fixtures, ["inspecting", "playing", "inbox-open"]);
        let mut typesetter = Typesetter::new(None).unwrap();
        for width in [1120.0, 600.0] {
            for (name, params) in &screen.fixtures {
                let rendered = description
                    .lay_out(&Shown::Fixture(name.clone()).wanted(width as u32, 1), None, Lang::En)
                    .unwrap();
                let window = Toolbar::given(&mut typesetter, width - 32.0, params.clone(), Lang::En).unwrap();
                let boxes = |page: &Page| {
                    page.nodes.iter().map(|node| (node.id.clone(), node.rect)).collect::<Vec<_>>()
                };
                assert_eq!(boxes(&window.page), boxes(&rendered.page), "{name} at {width}");
                let bar = rendered.page.nodes.iter().find(|node| node.id == "toolbar").unwrap().rect;
                let back = window.area("toolbar-back").unwrap();
                let drawn = rendered.page.nodes.iter().find(|node| node.id == "toolbar-back").unwrap().rect;
                assert_eq!(
                    (back.x, back.y),
                    (drawn.x - bar.x, drawn.y - bar.y - bar.height),
                    "in the gallery's points, the toolbar's top left at (0, -its height)"
                );
            }
        }
    }

    #[test]
    fn each_control_is_found_by_its_id_and_the_bar_says_the_window() {
        let mut typesetter = Typesetter::new(None).unwrap();
        let bar = Bar {
            back: true,
            crumbs: "drafts › shop".to_string(),
            waiting_on_you: 2,
            sidebar: true,
            ..Bar::default()
        };
        let toolbar = Toolbar::lay_out(&mut typesetter, 1200.0, &bar, Lang::En).unwrap();
        let middle = |id: &str| {
            let Rect { x, y, width, height } = toolbar.area(&format!("toolbar-{id}")).unwrap();
            (f64::from(x + width / 2.0), f64::from(y + height / 2.0))
        };
        for (id, tool) in [
            ("back", Tool::Back),
            ("forward", Tool::Forward),
            ("inspect", Tool::Inspect),
            ("play", Tool::Play),
            ("inbox-count", Tool::Inbox),
            ("sidebar", Tool::Sidebar),
        ] {
            assert_eq!(toolbar.tool_at(middle(id)), Some(tool), "{id}");
        }
        assert_eq!(toolbar.tool_at(middle("crumbs")), None, "the crumbs do nothing");
        assert!(toolbar.tall() > 0.0);

        let none = Bar { waiting_on_you: 0, ..bar.clone() };
        let toolbar = Toolbar::lay_out(&mut typesetter, 1200.0, &none, Lang::En).unwrap();
        assert!(toolbar.area("toolbar-inbox").is_none(), "nothing waits: no inbox");
        let narrow = Toolbar::lay_out(&mut typesetter, 500.0, &bar, Lang::En).unwrap();
        assert!(narrow.area("toolbar-sidebar").is_none() && narrow.area("toolbar-back").is_some(), "compact");
    }
}
