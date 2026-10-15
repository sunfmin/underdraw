//! Handoff states what each expression comes to in each state (ADR 0024),
//! the values the tree was built from: what render draws and what a
//! platform's test asserts cannot disagree.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Evaluated, Screen, Shared};
use underdraw_handoff::{Expected, Handoff};

/// A draft and its notes: save is disabled until there is a draft, a hint
/// shows meanwhile, and each note's label is chosen by whether it is done.
fn screen() -> Value {
    json!({
        "screen": "Notes",
        "types": { "Note": { "title": "string 1..", "done": "bool" } },
        "data": { "draft": "string", "notes": "Note[]" },
        "conditions": { "canSave": { "when": "draft != \"\"", "label": "There is a draft" } },
        "fixtures": {
            "empty": { "draft": "", "notes": [] },
            "writing": { "draft": "Milk", "notes": [ { "title": "Eggs", "done": true }, { "title": "Tea", "done": false } ] }
        },
        "root": { "id": "screen", "component": "Screen", "children": [
            { "id": "hint", "component": "Text", "text": "Write something", "if": "!canSave" },
            { "id": "save", "component": "Button", "text": "Save", "disabled": "!canSave" },
            { "id": "notes", "component": "List", "each": "notes", "item":
                { "id": "note-{index}", "component": "Text", "text": { "cases": [
                    { "if": "item.done", "then": "{item.title} ✓" }, { "else": "{item.title}" } ] } } }
        ] }
    })
}

fn mapping() -> Value {
    json!({
        "platform": "web", "language": "typescript", "testId": "{id}",
        "tokenNaming": { "pattern": "{group}.{name}" },
        "components": { "Screen": "s", "Text": "t", "Button": "b", "List": "l" }, "actions": {}
    })
}

fn handoff() -> Handoff {
    underdraw_handoff::handoff(&Screen::read(&screen(), Shared::default()), &mapping(), &json!({}))
        .unwrap_or_else(|checked| panic!("{:?}", checked.findings))
}

/// The state of fixture `name` at compact.
fn state<'h>(handoff: &'h Handoff, name: &str) -> &'h Expected {
    handoff
        .states
        .iter()
        .find(|state| state.state == json!({ "fixture": name }) && state.breakpoint == Breakpoint::Compact)
        .unwrap()
}

/// What the expression at `at` on `node` comes to in `state`.
fn value(state: &Expected, node: &str, at: &str) -> Value {
    let found = state.expressions.iter().find(|e| e.node == node && e.at == at);
    found.unwrap_or_else(|| panic!("nothing at {node}.{at}: {:#?}", state.expressions)).value.clone()
}

#[test]
fn each_fixture_states_its_conditions_use_sites_and_chosen_branches() {
    let handoff = handoff();
    let empty = state(&handoff, "empty");
    assert_eq!(value(empty, "", "conditions.canSave"), json!(false));
    assert_eq!(value(empty, "hint", "if"), json!(true));
    assert_eq!(value(empty, "save", "disabled"), json!(true));

    let writing = state(&handoff, "writing");
    assert_eq!(value(writing, "", "conditions.canSave"), json!(true));
    assert_eq!(value(writing, "hint", "if"), json!(false), "a node hidden by its if still says why");
    assert_eq!(value(writing, "save", "disabled"), json!(false));
    assert_eq!(value(writing, "note-0", "text.cases[0].if"), json!(true));
    let chosen: Vec<(Option<usize>, &str, &Value)> = writing
        .expressions
        .iter()
        .filter(|e| e.at == "text.cases")
        .map(|e| (e.branch, e.expression.as_str(), &e.value))
        .collect();
    assert_eq!(
        chosen,
        [(Some(0), "item.done", &json!("Eggs ✓")), (Some(1), "else", &json!("Tea"))],
        "each row's chosen branch, its value filled"
    );
}

#[test]
fn the_values_are_what_the_built_state_shows() {
    for state in &handoff().states {
        for Evaluated { node, at, value, .. } in &state.expressions {
            let shown = state.shows.get(node.as_str());
            match at.as_str() {
                "if" => assert_eq!(
                    state.present.contains(node),
                    value == &json!(true),
                    "{node} present exactly where its if holds"
                ),
                "disabled" => assert_eq!(shown.and_then(|s| s.get("disabled")), Some(value)),
                "text.cases" => assert_eq!(shown.and_then(|s| s.get("text")), Some(value)),
                _ => {}
            }
        }
    }
}
