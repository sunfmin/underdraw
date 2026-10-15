//! A reference to what the person picked in the gallery, naming it as the
//! description writes it so that an agent can act on it without asking
//! where: a node or a region in a state, a whole state, a transition's
//! arrow in a
//! description's overview, or a navigate's arrow in a directory's flow.
//! What ⌘C copies, as one line, and what a note is pinned to.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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

impl Region {
    /// The region in a few words, as the gallery's panel names it: its box
    /// and how many nodes it covers.
    pub fn named(&self) -> String {
        let Frame { x, y, width, height } = self.area;
        let covering = match self.nodes.len() {
            0 => "no node".to_string(),
            1 => "1 node".to_string(),
            n => format!("{n} nodes"),
        };
        format!("region {x}, {y} · {width} × {height} covering {covering}")
    }
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

    /// What it is, as the panel says a note is on it: `on <id> in <state> at
    /// <width>`, `on the whole of <state>`, `on transition …`, `on navigate …`.
    pub fn said(&self) -> String {
        match self {
            On::Node { state, node } => {
                format!("on {} in {} at {}", node.origin.written, state.name(), node.width)
            }
            On::Region { state, region } => {
                format!("on {} in {} at {}", region.named(), state.name(), region.width)
            }
            On::State { state } => format!("on the whole of {}", state.name()),
            On::Transition { transition } => format!("on transition {}", transition.said()),
            On::Navigate { navigate } => format!("on navigate {}", navigate.said()),
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
                    written_in(&origin.at),
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
/// its components/<Name>.json.
fn written_in(at: &str) -> String {
    let shared = at
        .strip_prefix('/')
        .and_then(|at| at.strip_prefix(underdraw_format::SHARED_COMPONENTS))
        .and_then(|at| at.strip_prefix('/'));
    match shared.and_then(|at| at.split_once('/')) {
        Some((name, inside)) => format!("/{inside} in {}/{name}.json", crate::project::COMPONENTS),
        None => format!("{at} in screen.json"),
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
            written_in("/$components/OrderRow/nodes/children/1"),
            "/nodes/children/1 in components/OrderRow.json",
            "a node of a component the project shares is written in its file"
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
}
