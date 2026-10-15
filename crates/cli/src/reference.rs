//! A reference to what the person picked in the gallery, a node, a region
//! or a whole state, naming it as the description writes it so that an
//! agent can act on it without asking where: what ⌘C copies, as one line.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::draw::{Origin, Shown};

/// A state of one description and, when one is picked, what in it: a
/// node, or a region.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    /// The directory the description is in.
    pub description: PathBuf,
    pub state: Shown,
    pub on: Option<On>,
}

/// What in a state is picked, and a note is pinned to: a node, or a region.
/// Written as one key, `{"node": …}` or `{"region": …}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum On {
    Node(Pointed),
    Region(Region),
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

impl On {
    /// The width of the picture it was picked in.
    pub fn width(&self) -> u32 {
        match self {
            On::Node(pointed) => pointed.width,
            On::Region(region) => region.width,
        }
    }

    /// What it is, in a few words, as the gallery's panel names it: the
    /// node's id as written, or the region's box and how many nodes it
    /// covers.
    pub fn named(&self) -> String {
        match self {
            On::Node(pointed) => pointed.origin.written.clone(),
            On::Region(Region { area: Frame { x, y, width, height }, nodes, .. }) => {
                let covering = match nodes.len() {
                    0 => "no node".to_string(),
                    1 => "1 node".to_string(),
                    n => format!("{n} nodes"),
                };
                format!("region {x}, {y} · {width} × {height} covering {covering}")
            }
        }
    }
}

impl Reference {
    /// The reference as one readable line: the description's directory and
    /// the state, then the node's id as written and where in screen.json,
    /// then the instance pointed at, as context; or the region's box and
    /// width, then each node it covers, as written, where, and which
    /// instance.
    pub fn line(&self) -> String {
        let state = match &self.state {
            Shown::Fixture(name) => format!("fixture {name}"),
            Shown::Phase(name) => format!("phase {name}"),
        };
        let mut line = format!("{} · {state}", self.description.display());
        if let Some(On::Region(Region { area, width, nodes })) = &self.on {
            let Frame { x, y, width: wide, height } = area;
            line.push_str(&format!(" · region {x}, {y} · {wide} × {height} at width {width} · covering "));
            if nodes.is_empty() {
                line.push_str("no node");
            }
            let covered: Vec<String> = nodes
                .iter()
                .map(|origin| format!("{} at {}{}", origin.written, origin.at, instance(&origin.instance)))
                .collect();
            line.push_str(&covered.join(", "));
        }
        if let Some(On::Node(Pointed { origin, label, width })) = &self.on {
            let mut context = Vec::new();
            if !origin.instance.is_empty() {
                context.push(instance(&origin.instance).trim_start().to_string());
            }
            context.push(label.map_or_else(|| "no label".to_string(), |label| format!("label {label}")));
            context.push(format!("width {width}"));
            line.push_str(&format!(
                " · {} at {} in screen.json · pointed at {}",
                origin.written,
                origin.at,
                context.join(", ")
            ));
        }
        line
    }
}

/// The element of each repeat around a node, outermost first, after a
/// space: ` [2][0]`; nothing outside any repeat.
fn instance(of: &[usize]) -> String {
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
        let state =
            Reference { description: "/d/todo".into(), state: Shown::Phase("loading".into()), on: None };
        assert_eq!(state.line(), "/d/todo · phase loading");
    }

    #[test]
    fn a_node_names_its_template_where_it_is_written_and_the_instance_pointed_at() {
        let origin =
            Origin { written: "order-{index}".into(), at: "/root/children/1/item".into(), instance: vec![2] };
        let node = Reference {
            description: "/d/orders".into(),
            state: Shown::Fixture("default".into()),
            on: Some(On::Node(Pointed { origin, label: Some(4), width: 390 })),
        };
        assert_eq!(
            node.line(),
            "/d/orders · fixture default · order-{index} at /root/children/1/item in screen.json · pointed at [2], label 4, width 390"
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
            state: Shown::Fixture("default".into()),
            on: Some(On::Region(Region { area, width: 390, nodes })),
        };
        assert_eq!(
            region.line(),
            "/d/orders · fixture default · region 16, 120 · 358 × 200 at width 390 · covering order-{index} at /root/children/1/item [2], nav-export at /root/children/0"
        );
        if let Some(On::Region(Region { nodes, .. })) = &mut region.on {
            nodes.clear();
        }
        assert!(region.line().ends_with("at width 390 · covering no node"), "{}", region.line());
    }
}
