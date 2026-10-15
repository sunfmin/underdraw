//! A reference to what the person picked in the gallery, naming it as the
//! description writes it so that an agent can act on it without asking
//! where: a node or a region in a state, a whole state, a transition's
//! arrow in a
//! description's overview, or a navigate's arrow in a directory's flow.
//! What ⌘C copies, as one line, and what a note is pinned to.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use underdraw_format::{Lang, say};

use crate::draw::{Origin, Shown};

/// What is picked in one description: the directory it is in, and what in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    /// The directory the description is in; for a navigate, the one it is
    /// written in.
    pub description: PathBuf,
    pub on: On,
}

/// What in a description a reference names. Written, in notes.json, as the
/// fields of the one it is: `{"state", "node"}`, `{"state", "region"}`,
/// `{"state"}`, `{"transition"}` or `{"navigate"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum On {
    /// A node of one state, and the instance of it pointed at.
    Node { state: Shown, node: Pointed },
    /// A region of one state: a box dragged over its picture.
    Region { state: Shown, region: Region },
    /// A whole state.
    State { state: Shown },
    /// A transition the description declares, its arrow in the overview.
    Transition { transition: Transition },
    /// A navigate written in the description, its arrow in a flow.
    Navigate { navigate: Navigate },
}

/// A box the person dragged over a picture, and the nodes wholly inside
/// it, each as the description writes it: where it is drawn follows them
/// as the layout moves, and stays at the box when it covered none.
/// Written as one object: `{"box", "width", "nodes"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// The box as dragged, in the picture's points.
    #[serde(rename = "box")]
    pub area: Frame,
    /// The width of the picture it was dragged on.
    pub width: u32,
    /// The outermost nodes inside the box, in the order laid out: one
    /// inside another is in it already.
    pub nodes: Vec<Origin>,
}

/// A box on a picture, in whole points from its top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A node as the description writes it, and the one instance of it the
/// person pointed at: which data showed it, its label, and the width.
/// Written as one object: `{"id", "at", "instance", "label", "width"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pointed {
    #[serde(flatten)]
    pub origin: Origin,
    pub label: Option<u32>,
    pub width: u32,
}

/// A transition as `transitions` in screen.json writes it: from a state, on
/// `<node id as written>.<event>`, for a call the outcome, to a state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub to: String,
}

/// A navigate as `underdraw check <root>` reports it among its links: the
/// screen it is in, the node it is on by its id as written, and the screen
/// it names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Navigate {
    pub from: String,
    pub node: String,
    pub to: String,
}

impl On {
    /// The state it is in, for a node, a region or a whole state.
    pub fn state(&self) -> Option<&Shown> {
        match self {
            On::Node { state, .. } | On::Region { state, .. } | On::State { state } => Some(state),
            On::Transition { .. } | On::Navigate { .. } => None,
        }
    }

    /// What it is, as the gallery's sidebar says a note is on it, in
    /// `lang`: `on <id> in <state> at <width>`, `on the whole of <state>`,
    /// `on transition …`, `on navigate …`.
    pub fn said(&self, lang: Lang) -> String {
        match self {
            On::Node { state, node } => {
                say!(lang, on_node, node = node.origin.written, state = state.name(), width = node.width)
            }
            On::Region { state, region } => {
                let Frame { x, y, width: w, height: h } = region.area;
                let covering = match region.nodes.len() {
                    0 => say!(lang, covering_none),
                    1 => say!(lang, covering_one),
                    n => say!(lang, covering_many, n),
                };
                say!(lang, on_region, x, y, w, h, covering, state = state.name(), width = region.width)
            }
            On::State { state } => say!(lang, on_state, state = state.name()),
            On::Transition { transition: t } => {
                let outcome = t
                    .outcome
                    .as_ref()
                    .map(|outcome| say!(lang, transition_outcome, outcome))
                    .unwrap_or_default();
                say!(lang, on_transition, from = t.from, on = t.on, outcome, to = t.to)
            }
            On::Navigate { navigate: n } => say!(lang, on_navigate, from = n.from, node = n.node, to = n.to),
        }
    }
}

impl Transition {
    /// `from <state> on <node>.<event> [outcome <outcome>] to <state>`.
    pub fn said(&self) -> String {
        let outcome = self.outcome.as_ref().map(|outcome| format!(" outcome {outcome}")).unwrap_or_default();
        format!("from {} on {}{outcome} to {}", self.from, self.on, self.to)
    }
}

impl Navigate {
    /// `from <screen> on <node> to <screen>`.
    pub fn said(&self) -> String {
        format!("from {} on {} to {}", self.from, self.node, self.to)
    }
}

impl Reference {
    /// The reference as one readable line: the description's directory,
    /// then what in it. For a state, the state; for a node, the state, the
    /// node's id as written and where in screen.json, then the instance
    /// pointed at, as context; for an arrow, the transition or navigate
    /// with the keys screen.json and `check` name it by.
    pub fn line(&self) -> String {
        let state = |state: &Shown| match state {
            Shown::Fixture(name) => format!("fixture {name}"),
            Shown::Phase(name) => format!("phase {name}"),
        };
        let description = self.description.display();
        match &self.on {
            On::State { state: shown } => format!("{description} · {}", state(shown)),
            On::Node { state: shown, node: Pointed { origin, label, width } } => {
                let mut context = Vec::new();
                if !origin.instance.is_empty() {
                    context.push(instance(&origin.instance).trim_start().to_string());
                }
                context.push(label.map_or_else(|| "no label".to_string(), |label| format!("label {label}")));
                context.push(format!("width {width}"));
                format!(
                    "{description} · {} · {} at {} · pointed at {}",
                    state(shown),
                    origin.written,
                    written_in(&origin.at, &self.description),
                    context.join(", ")
                )
            }
            On::Region { state: shown, region: Region { area, width, nodes } } => {
                let Frame { x, y, width: wide, height } = area;
                let covered: Vec<String> = nodes
                    .iter()
                    .map(|origin| {
                        format!("{} at {}{}", origin.written, origin.at, instance(&origin.instance))
                    })
                    .collect();
                let covering = if covered.is_empty() { "no node".to_string() } else { covered.join(", ") };
                format!(
                    "{description} · {} · region {x}, {y} · {wide} × {height} at width {width} · covering {covering}",
                    state(shown)
                )
            }
            On::Transition { transition } => format!("{description} · transition {}", transition.said()),
            On::Navigate { navigate } => format!("{description} · navigate {}", navigate.said()),
        }
    }
}

/// Where the node at `at` (a JSON pointer into the description as read) is
/// written: in screen.json, or, inside a component the project shares, in
/// its components/<Name>.json. In the description in `description` that
/// shows a shared component on its own, a node outside the component's
/// nodes is the instance the gallery draws it as, written nowhere.
fn written_in(at: &str, description: &std::path::Path) -> String {
    let shared = at
        .strip_prefix('/')
        .and_then(|at| at.strip_prefix(underdraw_format::SHARED_COMPONENTS))
        .and_then(|at| at.strip_prefix('/'));
    match shared.and_then(|at| at.split_once('/')) {
        Some((name, inside)) => format!("/{inside} in {}/{name}.json", crate::project::COMPONENTS),
        None => match crate::project::showcased(description) {
            Some((name, _)) => format!("the instance shown of {}/{name}.json", crate::project::COMPONENTS),
            None => format!("{at} in screen.json"),
        },
    }
}

/// The element of each repeat around a node, outermost first, after a
/// space: ` [2][0]`; nothing outside any repeat.
pub fn instance(of: &[usize]) -> String {
    match of {
        [] => String::new(),
        _ => format!(" {}", of.iter().map(|index| format!("[{index}]")).collect::<String>()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn a_state_names_its_description_and_itself() {
        let state = Reference {
            description: "/d/todo".into(),
            on: On::State { state: Shown::Phase("loading".into()) },
        };
        assert_eq!(state.line(), "/d/todo · phase loading");
    }

    #[test]
    fn a_node_names_its_template_where_it_is_written_and_the_instance_pointed_at() {
        let origin =
            Origin { written: "order-{index}".into(), at: "/root/children/1/item".into(), instance: vec![2] };
        let node = Reference {
            description: "/d/orders".into(),
            on: On::Node {
                state: Shown::Fixture("default".into()),
                node: Pointed { origin, label: Some(4), width: 390 },
            },
        };
        assert_eq!(
            node.line(),
            "/d/orders · fixture default · order-{index} at /root/children/1/item in screen.json · pointed at [2], label 4, width 390"
        );
        assert_eq!(
            written_in("/$components/OrderRow/nodes/children/1", Path::new("/d/orders")),
            "/nodes/children/1 in components/OrderRow.json",
            "a node of a component the project shares is written in its file"
        );
        let shop =
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/shop/components/OrderStatus"));
        assert_eq!(
            written_in("/root/children/0", shop),
            "the instance shown of components/OrderStatus.json",
            "the instance the gallery shows a shared component as is written nowhere"
        );
    }

    #[test]
    fn a_region_names_its_box_and_each_node_it_covers_where_it_is_written() {
        let origin = |written: &str, at: &str, instance: Vec<usize>| Origin {
            written: written.into(),
            at: at.into(),
            instance,
        };
        let nodes = vec![
            origin("order-{index}", "/root/children/1/item", vec![2]),
            origin("nav-export", "/root/children/0", vec![]),
        ];
        let area = Frame { x: 16, y: 120, width: 358, height: 200 };
        let mut region = Reference {
            description: "/d/orders".into(),
            on: On::Region {
                state: Shown::Fixture("default".into()),
                region: Region { area, width: 390, nodes },
            },
        };
        assert_eq!(
            region.line(),
            "/d/orders · fixture default · region 16, 120 · 358 × 200 at width 390 · covering order-{index} at /root/children/1/item [2], nav-export at /root/children/0"
        );
        if let On::Region { region: Region { nodes, .. }, .. } = &mut region.on {
            nodes.clear();
        }
        assert!(region.line().ends_with("at width 390 · covering no node"), "{}", region.line());
    }

    #[test]
    fn an_arrow_names_its_transition_or_navigate_by_the_keys_it_is_written_with() {
        let transition = Transition {
            from: "default".into(),
            on: "order-{index}-cancel.tap".into(),
            outcome: Some("ok".into()),
            to: "cancel-dialog".into(),
        };
        let reference = Reference { description: "/d/orders".into(), on: On::Transition { transition } };
        assert_eq!(
            reference.line(),
            "/d/orders · transition from default on order-{index}-cancel.tap outcome ok to cancel-dialog"
        );
        let navigate =
            Navigate { from: "Orders".into(), node: "nav-export".into(), to: "ExportDocuments".into() };
        let reference = Reference { description: "/d/orders".into(), on: On::Navigate { navigate } };
        assert_eq!(reference.line(), "/d/orders · navigate from Orders on nav-export to ExportDocuments");
    }

    #[test]
    fn what_a_note_is_on_is_said_in_each_language() {
        let origin = |written: &str| Origin { written: written.into(), at: "/root".into(), instance: vec![] };
        let default = || Shown::Fixture("default".into());
        let area = Frame { x: 16, y: 120, width: 358, height: 200 };
        let region =
            |nodes: Vec<Origin>| On::Region { state: default(), region: Region { area, width: 390, nodes } };
        let transition = |outcome: Option<&str>| On::Transition {
            transition: Transition {
                from: "default".into(),
                on: "order-{index}-cancel.tap".into(),
                outcome: outcome.map(Into::into),
                to: "cancel-dialog".into(),
            },
        };
        let cases = [
            (
                On::Node {
                    state: default(),
                    node: Pointed { origin: origin("order-{index}"), label: None, width: 390 },
                },
                "on order-{index} in default at 390",
                "在 order-{index}，default，宽 390",
            ),
            (
                region(vec![]),
                "on region 16, 120 · 358 × 200 covering no node in default at 390",
                "在区域 16, 120 · 358 × 200（覆盖 0 个节点），default，宽 390",
            ),
            (
                region(vec![origin("a")]),
                "on region 16, 120 · 358 × 200 covering 1 node in default at 390",
                "在区域 16, 120 · 358 × 200（覆盖 1 个节点），default，宽 390",
            ),
            (
                region(vec![origin("a"), origin("b")]),
                "on region 16, 120 · 358 × 200 covering 2 nodes in default at 390",
                "在区域 16, 120 · 358 × 200（覆盖 2 个节点），default，宽 390",
            ),
            (
                On::State { state: Shown::Phase("loading".into()) },
                "on the whole of loading",
                "在整个 loading",
            ),
            (
                transition(Some("ok")),
                "on transition from default on order-{index}-cancel.tap outcome ok to cancel-dialog",
                "在过渡上：从 default 经 order-{index}-cancel.tap，结果 ok 到 cancel-dialog",
            ),
            (
                transition(None),
                "on transition from default on order-{index}-cancel.tap to cancel-dialog",
                "在过渡上：从 default 经 order-{index}-cancel.tap 到 cancel-dialog",
            ),
            (
                On::Navigate {
                    navigate: Navigate {
                        from: "Orders".into(),
                        node: "nav-export".into(),
                        to: "ExportDocuments".into(),
                    },
                },
                "on navigate from Orders on nav-export to ExportDocuments",
                "在跳转上：从 Orders 经 nav-export 到 ExportDocuments",
            ),
        ];
        for (on, en, zh) in cases {
            assert_eq!(on.said(Lang::En), en);
            assert_eq!(on.said(Lang::Zh), zh);
        }
    }
}
