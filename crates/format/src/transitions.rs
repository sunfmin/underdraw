//! The transitions a screen declares (docs/adr/0018): which state a person
//! reaches when they act on a node in another state. They serve the
//! gallery's state overview and its click-through prototype; nothing is
//! handed off from them.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::check::Findings;
use crate::instance::Node;
use crate::operations::Operations;
use crate::subtrees::Subtrees;

/// One transition: in state `from`, the person raises `event` on the node
/// `node`, and the screen is then in state `to`. `outcome` names how the
/// operation the event calls answered: `ok`, or one of its `fails`.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub from: String,
    /// The node's id as the description writes it: a template inside an
    /// item (`order-{index}-cancel`).
    pub node: String,
    pub event: String,
    pub outcome: Option<String>,
    pub to: String,
}

/// The keys a transition holds.
const KEYS: [&str; 4] = ["from", "on", "outcome", "to"];

/// The outcome of an operation that answered as asked.
const OK: &str = "ok";

/// Every transition `screen` declares, in the order written; an entry that
/// is not one is left out, for `check` to name.
pub fn read(screen: &Value) -> Vec<Transition> {
    #[cfg(test)]
    crate::screen::tests::count("transitions");
    let mut read = Vec::new();
    for entry in entries(screen) {
        let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
        let on = text("on");
        let Some((node, event)) = on.as_deref().and_then(|on| on.rsplit_once('.')) else {
            continue;
        };
        if let (Some(from), Some(to)) = (text("from"), text("to")) {
            read.push(Transition {
                from,
                node: node.to_string(),
                event: event.to_string(),
                outcome: text("outcome"),
                to,
            });
        }
    }
    read
}

fn entries(screen: &Value) -> Vec<&Value> {
    match screen.get("transitions") {
        Some(Value::Array(entries)) => entries.iter().collect(),
        _ => Vec::new(),
    }
}

/// Every fault in `screen`'s transitions that the description alone shows:
/// a state that is neither a fixture nor a phase (`states`), a node no tree
/// writes, an event the node has no handler for or one that leaves the
/// screen, and an outcome missing where the event calls an operation, given
/// where it calls none, or not one the operation answers with.
pub(crate) fn check(
    screen: &Value,
    subtrees: &Subtrees,
    states: &[String],
    operations: &Operations,
    findings: &mut Findings,
) {
    match screen.get("transitions") {
        None | Some(Value::Array(_)) => {}
        Some(_) => {
            return findings.add(
                "transitions",
                "transitions is a list: [{\"from\": <state>, \"on\": \"<node id>.<event>\", \"to\": <state>}, …]",
            );
        }
    }
    let nodes = written(screen, subtrees);
    for (index, entry) in entries(screen).into_iter().enumerate() {
        let at = format!("transitions.{index}");
        let Some(entry) = entry.as_object() else {
            findings.add(&at, "a transition is {\"from\", \"on\", \"outcome\"?, \"to\"}");
            continue;
        };
        for key in entry.keys() {
            if !KEYS.contains(&key.as_str()) && !key.starts_with('$') {
                findings.add(&format!("{at}.{key}"), "a transition holds only from, on, outcome and to");
            }
        }
        for key in ["from", "to"] {
            let at = format!("{at}.{key}");
            match entry.get(key) {
                Some(Value::String(state)) if states.contains(state) => {}
                Some(Value::String(state)) => findings.add(
                    &at,
                    &format!("{state} is not a state: name a fixture or a phase ({})", states.join(", ")),
                ),
                _ => findings.add(&at, &format!("a transition names its {key} state: a fixture or a phase")),
            }
        }
        let at_on = format!("{at}.on");
        let Some((node, event)) = entry.get("on").and_then(Value::as_str).and_then(|on| on.rsplit_once('.'))
        else {
            findings.add(&at_on, "on is \"<node id>.<event>\": \"cancel-button.tap\"");
            continue;
        };
        let Some(written) = nodes.get(node) else {
            findings.add(&at_on, &format!("no node has the id {node}: write it as the node's \"id\" says"));
            continue;
        };
        let Some(handler) = written.get("on").and_then(|on| on.get(event)) else {
            let has: Vec<&String> = match written.get("on") {
                Some(Value::Object(events)) => events.keys().collect(),
                _ => Vec::new(),
            };
            let has = if has.is_empty() {
                "none".to_string()
            } else {
                has.iter().map(|event| event.as_str()).collect::<Vec<_>>().join(", ")
            };
            findings.add(&at_on, &format!("{node} has no {event} event; its events: {has}"));
            continue;
        };
        let mut effect = Effect::default();
        effect.read(handler);
        if !effect.stays {
            findings.add(
                &at_on,
                &format!("{node}.{event} goes to another screen (navigate or back): it leaves this screen, so it is not a transition"),
            );
            continue;
        }
        outcome(entry.get("outcome"), &effect.calls, operations, &format!("{at}.outcome"), findings);
    }
}

/// The outcome at `at` of an event that calls `calls`: given exactly when
/// it calls one, and then `ok` or one of the operation's `fails`.
fn outcome(
    given: Option<&Value>,
    calls: &[String],
    operations: &Operations,
    at: &str,
    findings: &mut Findings,
) {
    let mut outcomes = vec![OK.to_string()];
    for name in calls {
        if let Some(operation) = operations.get(name) {
            for (failure, _) in &operation.fails {
                if !outcomes.contains(failure) {
                    outcomes.push(failure.clone());
                }
            }
        }
    }
    let called = calls.join(", ");
    let Some(given) = given else {
        if !calls.is_empty() {
            findings.add(
                at,
                &format!(
                    "the event calls {called}: say which outcome this is, one of {}",
                    outcomes.join(", ")
                ),
            );
        }
        return;
    };
    if calls.is_empty() {
        return findings.add(at, "the event calls no operation, so it has no outcome: take outcome out");
    }
    let known = given.as_str().is_some_and(|said| outcomes.iter().any(|outcome| outcome == said));
    if !known {
        findings.add(at, &format!("{given} is not an outcome of {called}: one of {}", outcomes.join(", ")));
    }
}

/// What an event's handler does, over all its cases: the operations it
/// calls, and whether any branch keeps the person on this screen.
#[derive(Default)]
struct Effect {
    calls: Vec<String>,
    stays: bool,
}

impl Effect {
    fn read(&mut self, handler: &Value) {
        if let Some(Value::Array(branches)) = handler.get("cases") {
            for branch in branches {
                if let Some(chosen) = branch.get("then").or_else(|| branch.get("else")) {
                    self.read(chosen);
                }
            }
            return;
        }
        if let Some(Value::String(called)) = handler.get("call")
            && !self.calls.contains(called)
        {
            self.calls.push(called.clone());
        }
        if handler.get("navigate").is_none() && handler.get("back").is_none() {
            self.stays = true;
        }
    }
}

/// Every node the description `screen`, whose components with nodes are
/// `subtrees`, writes, in its trees and phases and, through each instance,
/// its components, by its id as written and joined.
pub(crate) fn written<'a>(
    screen: &'a Value,
    subtrees: &'a Subtrees,
) -> BTreeMap<String, &'a Map<String, Value>> {
    let mut nodes = BTreeMap::new();
    for (id, node) in crate::subtrees::written(screen, subtrees) {
        nodes.entry(id).or_insert(node);
    }
    nodes
}

/// Whether the tree `node` holds a node written with the id `written`.
pub(crate) fn holds(node: &Node, written: &str) -> bool {
    node.written_id == written || node.children.iter().any(|child| holds(child, written))
}
