//! Where a description's `navigate` actions go. Each names a screen, which
//! resolves to the description under the same root that declares it as its
//! "screen"; one description alone cannot see the others, so this says only
//! what it navigates to, and marks a navigate to a screen not drawn yet as
//! an assumption.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::Lang;

/// The parts of a description that hold nodes, and so actions.
const TREES: [&str; 3] = ["root", "phases", "components"];

/// One `navigate`: the node it is on, by its id as written and, inside a
/// component, joined to its instance's (empty for a node with none), and
/// the screen it names.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Navigate {
    pub node: String,
    pub to: String,
}

/// Every `navigate` in `screen`, a handler's `cases` included: node by node
/// in the order written, each node's screens by name, each once.
pub fn navigates(screen: &Value) -> Vec<Navigate> {
    let mut found = Vec::new();
    for (node, written) in crate::components::written(screen) {
        if let Some(on) = written.get("on") {
            let mut screens = BTreeSet::new();
            targets(on, &mut screens);
            for to in screens {
                // A component used in more than one tree is one node.
                let navigate = Navigate { node: node.clone(), to };
                if !found.contains(&navigate) {
                    found.push(navigate);
                }
            }
        }
    }
    found
}

/// The screens the handlers in `on` navigate to, by name.
fn targets(on: &Value, to: &mut BTreeSet<String>) {
    match on {
        Value::Object(object) => {
            for (key, inner) in object {
                match (key.as_str(), inner) {
                    ("navigate", Value::String(screen)) => {
                        to.insert(screen.clone());
                    }
                    _ => targets(inner, to),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                targets(item, to);
            }
        }
        _ => {}
    }
}

/// `screen` with each node that navigates to a screen not in `drawn` marked
/// `"assumed"`, saying which: drawn with the orange "?" for the person to
/// confirm, in `lang`. A node's own `"assumed"` is kept.
pub fn assume_undrawn(screen: &Value, drawn: &BTreeSet<String>, lang: Lang) -> Value {
    let Value::Object(object) = screen else {
        return screen.clone();
    };
    let mut marked = Map::new();
    for (key, value) in object {
        let value = if TREES.contains(&key.as_str()) { mark(value, drawn, lang) } else { value.clone() };
        marked.insert(key.clone(), value);
    }
    Value::Object(marked)
}

fn mark(value: &Value, drawn: &BTreeSet<String>, lang: Lang) -> Value {
    match value {
        Value::Object(object) => {
            let mut marked = Map::new();
            for (key, inner) in object {
                let inner = if key == "on" { inner.clone() } else { mark(inner, drawn, lang) };
                marked.insert(key.clone(), inner);
            }
            if let Some(on) = object.get("on")
                && !object.contains_key("assumed")
            {
                let mut to = BTreeSet::new();
                targets(on, &mut to);
                let mut undrawn = Vec::new();
                for screen in to {
                    if !drawn.contains(&screen) {
                        undrawn.push(screen);
                    }
                }
                if !undrawn.is_empty() {
                    let why = crate::assumption::undrawn(&undrawn, lang);
                    marked.insert("assumed".to_string(), Value::String(why));
                }
            }
            Value::Object(marked)
        }
        Value::Array(items) => {
            let mut marked = Vec::new();
            for item in items {
                marked.push(mark(item, drawn, lang));
            }
            Value::Array(marked)
        }
        _ => value.clone(),
    }
}
