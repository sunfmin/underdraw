//! Journeys (docs/adr/0022): one named, linear path a person takes across
//! screens, told as steps, each a state of one screen; each step but the
//! last acts on a node, and that act leads to the next step, by a
//! transition on the same screen or a navigate to the next step's screen.
//! Across screens, data carries: the values a navigate computes in its
//! step's state are exactly the params the next step's fixture holds.
//! For the gallery and check only; nothing is handed off from them.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::breakpoint::Breakpoint;
use crate::check::{Finding, Findings};
use crate::instance::{Node, State};
use crate::screen::Screen;

/// The keys a step holds.
const KEYS: [&str; 4] = ["screen", "state", "on", "outcome"];

/// Every fault in walking `journey`, whose screens `screens` holds by their
/// path from the project's root, each read with what its project shares: a step
/// naming a screen not in `screens` or a state it does not have, an act
/// whose node its step's state does not show or that has no such event,
/// an act that does not lead to the next step, and a navigate that opens
/// the next step's screen with values other than its fixture's params.
pub fn walk(journey: &Value, screens: &BTreeMap<String, Screen>) -> Vec<Finding> {
    let mut findings = Findings::default();
    let Some(object) = journey.as_object() else {
        findings.add("", "a journey is {\"steps\": [{\"screen\", \"state\", \"on\"?, \"outcome\"?}, …]}");
        return findings.0;
    };
    for key in object.keys() {
        if key != "steps" && !key.starts_with('$') {
            findings.add(key, "a journey holds only steps");
        }
    }
    let steps: Vec<&Value> = match object.get("steps") {
        Some(Value::Array(steps)) => steps.iter().collect(),
        _ => Vec::new(),
    };
    if steps.len() < 2 {
        findings.add(
            "steps",
            "a journey is two steps or more: [{\"screen\": <path>, \"state\": <state>, \"on\": \"<node id>.<event>\"}, …, {\"screen\", \"state\"}]",
        );
        return findings.0;
    }
    let read: Vec<Option<Step>> = steps
        .iter()
        .enumerate()
        .map(|(index, step)| Step::read(step, index, screens, &mut findings))
        .collect();
    let last = steps.len().saturating_sub(1);
    for (index, (step, next)) in read.iter().zip(read.iter().skip(1)).enumerate() {
        let (Some(step), Some(next)) = (step, next) else { continue };
        step.leads(next, index, &mut findings);
    }
    if let Some(Some(step)) = read.last()
        && step.on.is_some()
    {
        findings.add(&format!("steps.{last}.on"), "the last step acts on nothing: take on out");
    }
    findings.0
}

/// One step as read: its screen and state, and the act on it, if any.
struct Step<'a> {
    path: &'a str,
    screen: &'a Screen,
    state: &'a str,
    /// The state built at each breakpoint it builds at.
    trees: Vec<Node>,
    /// Each param the screen declares, with the value this step's fixture
    /// gives it; none in a phase, or when the screen declares no params.
    params: Vec<(String, Value)>,
    on: Option<(&'a str, &'a str)>,
    outcome: Option<&'a str>,
}

impl<'a> Step<'a> {
    /// The step `step`, the `index`th, its screen found in `screens`; none
    /// when it names no screen or state there, each fault added to
    /// `findings`.
    fn read(
        step: &'a Value,
        index: usize,
        screens: &'a BTreeMap<String, Screen>,
        findings: &mut Findings,
    ) -> Option<Step<'a>> {
        let at = format!("steps.{index}");
        let Some(fields) = step.as_object() else {
            findings.add(&at, "a step is {\"screen\", \"state\", \"on\"?, \"outcome\"?}");
            return None;
        };
        for key in fields.keys() {
            if !KEYS.contains(&key.as_str()) && !key.starts_with('$') {
                findings.add(&format!("{at}.{key}"), "a step holds only screen, state, on and outcome");
            }
        }
        let text = |key: &str| fields.get(key).and_then(Value::as_str);
        let on = match fields.get("on") {
            None => None,
            Some(on) => match on.as_str().and_then(|on| on.rsplit_once('.')) {
                Some(on) => Some(on),
                None => {
                    findings.add(&format!("{at}.on"), "on is \"<node id>.<event>\": \"cancel-button.tap\"");
                    None
                }
            },
        };
        let outcome = text("outcome");
        let Some(path) = text("screen") else {
            findings
                .add(&format!("{at}.screen"), "a step names its screen by its path from the project's root");
            return None;
        };
        let Some(screen) = screens.get(path) else {
            findings.add(
                &format!("{at}.screen"),
                &format!("{path} is no screen description: name a screen's directory by its path from the project's root"),
            );
            return None;
        };
        let mut states = screen.fixture_names();
        states.extend(screen.phases());
        let Some(state) = text("state") else {
            findings
                .add(&format!("{at}.state"), "a step names its state: a fixture or a phase of its screen");
            return None;
        };
        if !states.iter().any(|known| known == state) {
            findings.add(
                &format!("{at}.state"),
                &format!(
                    "{state} is not a state of {path}: name a fixture or a phase ({})",
                    states.join(", ")
                ),
            );
            return None;
        }
        let built = screen.state(state);
        let mut trees = Vec::new();
        for breakpoint in Breakpoint::ALL {
            if let Ok(tree) = screen.instantiate(built, breakpoint, None) {
                trees.push(tree);
            }
        }
        let mut held = Vec::new();
        if let State::Fixture(name) = built
            && let Some((_, data)) = screen.fixtures.iter().find(|(fixture, _)| fixture == name)
        {
            for (param, _) in screen.params.iter().flatten() {
                let value = data.get(param).cloned().unwrap_or(Value::Null);
                held.push((param.clone(), value));
            }
        }
        Some(Step { path, screen, state, trees, params: held, on, outcome })
    }

    /// Whether this step, the `index`th, acts on a node its state shows,
    /// and that act leads to `next`; each fault added to `findings`.
    fn leads(&self, next: &Step, index: usize, findings: &mut Findings) {
        let at = format!("steps.{index}.on");
        let Some((node, event)) = self.on else {
            return findings.add(&at, "a step before the last acts on a node: \"on\": \"<node id>.<event>\"");
        };
        let (path, state) = (self.path, self.state);
        let Some(shown) = self.trees.iter().find_map(|tree| find(tree, node)) else {
            return findings.add(
                &at,
                &format!("{node} is not shown in {path} {state}, so a person cannot act on it there: name a node it shows"),
            );
        };
        let Some(handler) = shown.props.get("on").and_then(|on| on.get(event)) else {
            return findings.add(&at, &format!("{node} has no {event} event in {path} {state}"));
        };
        if next.path == self.path {
            let reaches = self.screen.transitions.iter().any(|t| {
                t.from == state
                    && t.node == node
                    && t.event == event
                    && t.to == next.state
                    && (self.outcome.is_none() || t.outcome.as_deref() == self.outcome)
            });
            if reaches {
                return;
            }
        }
        let names = Some(next.screen.name()).filter(|name| !name.is_empty());
        let navigates = handler.get("navigate").and_then(Value::as_str);
        if next.path != self.path && navigates.is_some() && navigates == names {
            let with = handler.get("with");
            for (param, held) in &next.params {
                let given = with.and_then(|with| with.get(param)).unwrap_or(&Value::Null);
                if given != held {
                    findings.add(
                        &at,
                        &format!(
                            "{node}.{event} in {path} {state} opens {} with {param} {given}, but the next step, {} {}, holds {param} {held}: give the same value on both sides",
                            next.path, next.path, next.state
                        ),
                    );
                }
            }
            return;
        }
        let outcome = self.outcome.map(|outcome| format!(" ({outcome})")).unwrap_or_default();
        let fix = if next.path == self.path {
            format!("declare a transition from {state} on it to {}", next.state)
        } else {
            format!("navigate it to {}", names.unwrap_or(next.path))
        };
        findings.add(
            &at,
            &format!(
                "{node}.{event}{outcome} in {path} {state} does not reach the next step, {} {}: {fix}",
                next.path, next.state
            ),
        );
    }
}

/// The first node in `tree` written with the id `written`.
fn find<'a>(tree: &'a Node, written: &str) -> Option<&'a Node> {
    if tree.written_id == written {
        return Some(tree);
    }
    tree.children.iter().find_map(|child| find(child, written))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn screens() -> BTreeMap<String, Screen> {
        let list = json!({
            "screen": "List",
            "data": { "items": "string[]" },
            "fixtures": { "some": { "items": ["a"] }, "none": { "items": [] } },
            "root": { "id": "list", "component": "Screen", "children": [
                { "id": "rows", "component": "List", "each": "items", "item":
                    { "id": "row-{index}", "component": "Button", "text": "{item}", "on": { "tap": { "navigate": "Detail", "with": { "id": "{item}" } } } } },
                { "id": "more", "component": "Button", "text": "More", "on": { "tap": { "set": { "open": true } } } }
            ] },
            "transitions": []
        });
        let detail = json!({
            "screen": "Detail",
            "params": ["id"],
            "data": { "id": "string", "open": { "type": "bool = false", "view": true } },
            "fixtures": { "shut": { "id": "a" }, "opened": { "$patch": "shut", "open": true }, "other": { "id": "b" } },
            "root": { "id": "detail", "component": "Screen", "children": [
                { "id": "toggle", "component": "Button", "text": "Open", "on": { "tap": { "toggle": "open" } } }
            ] },
            "transitions": [ { "from": "shut", "on": "toggle.tap", "to": "opened" } ]
        });
        let read = |description: &Value| Screen::read(description, Default::default());
        BTreeMap::from([("app/list".to_string(), read(&list)), ("app/detail".to_string(), read(&detail))])
    }

    fn walked(steps: Value) -> Vec<String> {
        walk(&json!({ "steps": steps }), &screens())
            .into_iter()
            .map(|f| format!("{}: {}", f.at, f.problem))
            .collect()
    }

    #[test]
    fn a_journey_by_a_navigate_then_a_transition_walks() {
        let found = walked(json!([
            { "screen": "app/list", "state": "some", "on": "row-{index}.tap" },
            { "screen": "app/detail", "state": "shut", "on": "toggle.tap" },
            { "screen": "app/detail", "state": "opened" }
        ]));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_navigate_that_opens_the_next_step_with_other_params_is_named() {
        let found = walked(json!([
            { "screen": "app/list", "state": "some", "on": "row-{index}.tap" },
            { "screen": "app/detail", "state": "other" }
        ]));
        assert_eq!(
            found,
            vec![
                "steps.0.on: row-{index}.tap in app/list some opens app/detail with id \"a\", but the next step, app/detail other, holds id \"b\": give the same value on both sides"
            ]
        );
    }

    #[test]
    fn an_unknown_screen_or_state_is_named_at_its_step() {
        let found = walked(json!([
            { "screen": "app/nowhere", "state": "some", "on": "row-{index}.tap" },
            { "screen": "app/detail", "state": "gone" }
        ]));
        assert!(found[0].starts_with("steps.0.screen: app/nowhere is no screen description"), "{found:?}");
        assert!(found[1].starts_with("steps.1.state: gone is not a state of app/detail"), "{found:?}");
    }

    #[test]
    fn an_act_its_state_does_not_show_is_named() {
        let found = walked(json!([
            { "screen": "app/list", "state": "none", "on": "row-{index}.tap" },
            { "screen": "app/detail", "state": "shut" }
        ]));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("steps.0.on: row-{index} is not shown in app/list none"), "{found:?}");
    }

    #[test]
    fn a_step_nothing_reaches_is_named() {
        let found = walked(json!([
            { "screen": "app/list", "state": "some", "on": "more.tap" },
            { "screen": "app/detail", "state": "shut", "on": "toggle.tap" },
            { "screen": "app/detail", "state": "shut" }
        ]));
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].starts_with(
                "steps.0.on: more.tap in app/list some does not reach the next step, app/detail shut"
            ),
            "{found:?}"
        );
        assert!(
            found[1].starts_with(
                "steps.1.on: toggle.tap in app/detail shut does not reach the next step, app/detail shut"
            ),
            "{found:?}"
        );
    }

    #[test]
    fn a_step_before_the_last_acts_and_the_last_does_not() {
        let found = walked(json!([
            { "screen": "app/detail", "state": "shut" },
            { "screen": "app/detail", "state": "opened", "on": "toggle.tap" }
        ]));
        assert!(found[0].starts_with("steps.0.on: a step before the last acts"), "{found:?}");
        assert!(found[1].starts_with("steps.1.on: the last step acts on nothing"), "{found:?}");
    }
}
