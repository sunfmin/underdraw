//! Where a description's `navigate` actions go. Each names a screen, which
//! resolves to the description under the same root that declares it as its
//! "screen"; one description alone cannot see the others, so this says only
//! what it navigates to, and marks a navigate to a screen not drawn yet as
//! an assumption.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;

use crate::Lang;
use crate::subtrees::Subtrees;

/// One `navigate`: the node it is on, by its id as written and, inside a
/// component, joined to its instance's (empty for a node with none), and
/// the screen it names.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Navigate {
    pub node: String,
    pub to: String,
}

/// Every `navigate` in the description `screen`, whose components with
/// nodes are `subtrees`, a handler's `cases` included: node by node in the
/// order written, each node's screens by name, each once.
pub(crate) fn navigates(screen: &Value, subtrees: &Subtrees) -> Vec<Navigate> {
    let mut found = Vec::new();
    for (node, written) in crate::subtrees::written(screen, subtrees) {
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

/// Which screens are drawn: each `navigate` to one that is not is marked
/// assumed when the screen is instantiated, for the person to confirm,
/// drawn with the orange "?" and said in `lang`. A node's own `"assumed"`
/// is kept.
pub struct Undrawn<'a> {
    pub drawn: &'a BTreeSet<String>,
    pub lang: Lang,
}

impl Undrawn<'_> {
    /// What a node whose handlers are `on` assumes: that each screen it
    /// navigates to and is not drawn will be; none when all are.
    pub(crate) fn of(&self, on: &Value) -> Option<String> {
        let mut to = BTreeSet::new();
        targets(on, &mut to);
        let mut undrawn = Vec::new();
        for screen in to {
            if !self.drawn.contains(&screen) {
                undrawn.push(screen);
            }
        }
        match undrawn.is_empty() {
            true => None,
            false => Some(crate::assumption::undrawn(&undrawn, self.lang)),
        }
    }
}
