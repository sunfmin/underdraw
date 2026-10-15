//! Handoff names each node inside an instance of a component as the
//! gallery's instantiation does, and sits it where it does: both walk into
//! instances through one expander.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Checked, Node, Screen, Shared, State};
use underdraw_handoff::Handoff;

/// `ItemRow` holds a `Tag` per tag, each given a trailing node, and places
/// what its instance gives in `children` inside its row.
fn screen() -> Value {
    json!({
        "screen": "Items",
        "types": { "Item": { "id": "string", "name": "string 1..", "tags": "string[]" } },
        "data": { "items": "Item[]", "title": "string" },
        "fixtures": { "some": { "title": "Today", "items": [ { "id": "a", "name": "Milk", "tags": ["x"] } ] } },
        "components": {
            "Tag": { "params": { "label": "string" }, "nodes": { "component": "Row", "children": [
                { "id": "text", "component": "Text", "text": "{label}" }, { "slot": "trailing" } ] } },
            "ItemRow": {
                "params": { "item": "Item" },
                "nodes": { "component": "Row", "children": [
                    { "id": "name", "component": "Text", "text": "{item.name}" },
                    { "id": "tags", "component": "List", "each": "item.tags", "item":
                        { "id": "tag-{index}", "component": "Tag", "with": { "label": "{item}" },
                          "trailing": { "id": "x-{index}", "component": "Text", "text": "{item}" } } },
                    { "slot": "children" }
                ] }
            }
        },
        "root": { "id": "screen", "component": "Screen", "children": [
            { "id": "list", "component": "List", "each": "items", "item":
                { "id": "row-{index}", "component": "ItemRow", "with": { "item": "{item}" },
                  "children": { "id": "row-{index}-title", "component": "Text", "text": "{title}" } } }
        ] }
    })
}

fn mapping() -> Value {
    json!({
        "platform": "web", "language": "typescript", "testId": "{id}",
        "tokenNaming": { "pattern": "{group}.{name}" },
        "components": { "Screen": "s", "Text": "t", "Row": "r", "List": "l" }, "actions": {}
    })
}

/// Each node of `node`'s tree by its id as written, with its parent's.
fn drawn(node: &Node, parent: &str, out: &mut BTreeSet<(String, String)>) {
    out.insert((node.written_id.clone(), parent.to_string()));
    for child in &node.children {
        drawn(child, &node.written_id, out);
    }
}

#[test]
fn handoff_names_and_places_each_node_as_instantiation_does() {
    let screen = screen();
    let handoff = handoff(&screen, &mapping(), &json!({})).unwrap_or_else(|c| panic!("{:?}", c.findings));
    let handed: BTreeSet<(String, String)> =
        handoff.nodes.iter().map(|n| (n.id.clone(), n.parent.clone().unwrap_or_default())).collect();
    let mut built = BTreeSet::new();
    drawn(
        &read_screen(&screen).instantiate(State::Fixture("some"), Breakpoint::Compact, None).unwrap(),
        "",
        &mut built,
    );
    assert_eq!(handed, built);
    assert!(
        handed.contains(&("row-{index}-x-{index}".to_string(), "row-{index}-tag-{index}".to_string())),
        "a node given in a slot is joined to the instance the giving is written in, and sits in the row of the instance it is given to: {handed:?}"
    );
}

#[test]
fn handing_off_a_component_that_uses_itself_is_a_fault() {
    let mut screen = screen();
    screen["components"]["Tag"]["nodes"]["children"][1] =
        json!({ "id": "again", "component": "Tag", "with": { "label": "{label}" } });
    let refused = handoff(&screen, &mapping(), &json!({})).expect_err("refused, not walked forever");
    assert!(
        refused.findings.iter().any(|f| f.problem.contains("component Tag uses itself: Tag → Tag")),
        "{:?}",
        refused.findings
    );
}

/// `description` read as a screen of its own, with nothing shared.
fn read_screen(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn handoff(screen: &Value, mapping: &Value, tokens: &Value) -> Result<Handoff, Box<Checked>> {
    underdraw_handoff::handoff(&read_screen(screen), mapping, tokens)
}
