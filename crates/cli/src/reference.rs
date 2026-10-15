//! A reference to what the person picked in the gallery, a node or a whole
//! state, naming it as the description writes it so that an agent can act
//! on it without asking where: what ⌘C copies, as one line.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::draw::{Origin, Shown};

/// A state of one description and, when one is picked, a node in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    /// The directory the description is in.
    pub description: PathBuf,
    pub state: Shown,
    pub node: Option<Pointed>,
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

impl Reference {
    /// The reference as one readable line: the description's directory and
    /// the state, then the node's id as written and where in screen.json,
    /// then the instance pointed at, as context.
    pub fn line(&self) -> String {
        let state = match &self.state {
            Shown::Fixture(name) => format!("fixture {name}"),
            Shown::Phase(name) => format!("phase {name}"),
        };
        let mut line = format!("{} · {state}", self.description.display());
        if let Some(Pointed { origin, label, width }) = &self.node {
            let mut context = Vec::new();
            if !origin.instance.is_empty() {
                context.push(origin.instance.iter().map(|index| format!("[{index}]")).collect::<String>());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_names_its_description_and_itself() {
        let state =
            Reference { description: "/d/todo".into(), state: Shown::Phase("loading".into()), node: None };
        assert_eq!(state.line(), "/d/todo · phase loading");
    }

    #[test]
    fn a_node_names_its_template_where_it_is_written_and_the_instance_pointed_at() {
        let origin =
            Origin { written: "order-{index}".into(), at: "/root/children/1/item".into(), instance: vec![2] };
        let node = Reference {
            description: "/d/orders".into(),
            state: Shown::Fixture("default".into()),
            node: Some(Pointed { origin, label: Some(4), width: 390 }),
        };
        assert_eq!(
            node.line(),
            "/d/orders · fixture default · order-{index} at /root/children/1/item in screen.json · pointed at [2], label 4, width 390"
        );
    }
}
