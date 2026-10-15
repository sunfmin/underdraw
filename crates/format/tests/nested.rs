//! A repeat inside a repeat: the outer one names its index, so a node
//! inside says both and each instance has an id of its own, which a
//! transition names as written; an id two nodes of a state share is
//! refused.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Checked, Node, Screen, Shared, State};

/// A grid of rows of cells: `rows` repeats the rows, naming its index as
/// `index` says (none: unnamed), and each row repeats its cells, `cell`.
fn grid(index: Option<&str>, cell: Value) -> Value {
    let mut rows = json!({ "id": "rows", "component": "List", "each": "rows",
        "item": { "id": "row-{index}", "component": "List", "each": "item.cells", "item": cell } });
    if let Some(index) = index {
        rows["index"] = json!(index);
    }
    let rows_data = json!([{ "cells": ["a", "b"] }, { "cells": ["c", "d"] }]);
    json!({
        "screen": "Grid",
        "data": { "rows": { "type": "Row[]" }, "picked": { "type": "string?", "view": true } },
        "types": { "Row": { "cells": "string[]" } },
        "fixtures": {
            "default": { "rows": rows_data, "picked": null },
            "picked": { "rows": rows_data, "picked": "a" }
        },
        "root": { "id": "page", "component": "Screen", "children": [rows] },
        "transitions": []
    })
}

fn button(id: &str) -> Value {
    json!({ "id": id, "component": "Button", "text": "{item}", "on": { "tap": { "set": "picked", "to": "{item}" } } })
}

fn read(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&read(description))
}

/// Every id the default state shows, in document order.
fn ids(description: &Value) -> Vec<String> {
    fn walk(node: &Node, ids: &mut Vec<String>) {
        ids.push(node.id.clone());
        for child in &node.children {
            walk(child, ids);
        }
    }
    let tree = read(description).instantiate(State::Fixture("default"), Breakpoint::Compact, None).unwrap();
    let mut found = Vec::new();
    walk(&tree, &mut found);
    found
}

#[test]
fn an_inner_node_names_the_outer_index_and_each_instance_has_its_own_id() {
    let screen = grid(Some("row"), button("cell-{row}-{index}"));
    let checked = check(&screen);
    assert!(checked.ok, "{:#?}", checked.findings);
    assert_eq!(
        ids(&screen),
        ["page", "rows", "row-0", "cell-0-0", "cell-0-1", "row-1", "cell-1-0", "cell-1-1"]
    );
}

#[test]
fn an_id_two_nodes_of_a_state_share_is_refused() {
    let checked = check(&grid(None, button("cell-{index}")));
    assert_eq!(checked.findings.len(), 1, "{:#?}", checked.findings);
    let finding = &checked.findings[0];
    assert_eq!(finding.at, "cell-{index}");
    assert!(
        finding.problem.starts_with("names more than one node in default, cell-0 among them"),
        "{}",
        finding.problem
    );
    assert!(finding.problem.contains("\"index\": \"row\""), "{}", finding.problem);
}

#[test]
fn a_transition_names_the_inner_node_as_written() {
    let mut screen = grid(Some("row"), button("cell-{row}-{index}"));
    screen["transitions"] = json!([{ "from": "default", "on": "cell-{row}-{index}.tap", "to": "picked" }]);
    let checked = check(&screen);
    assert!(checked.ok, "{:#?}", checked.findings);

    screen["transitions"] = json!([{ "from": "default", "on": "cell-{index}.tap", "to": "picked" }]);
    let checked = check(&screen);
    assert!(
        checked
            .findings
            .iter()
            .any(|f| f.at == "transitions.0.on" && f.problem.contains("no node has the id cell-{index}")),
        "{:#?}",
        checked.findings
    );
}

#[test]
fn a_node_given_an_id_inside_names_the_outer_index_too() {
    let screen = grid(Some("row"), json!({ "component": "Text", "text": "{item}" }));
    let checked = check(&screen);
    assert!(checked.ok, "{:#?}", checked.findings);
    let ids = ids(&screen);
    assert_eq!(ids[3..5], ["row-0-text-item-0", "row-0-text-item-1"], "{ids:?}");
    assert_eq!(ids[6..8], ["row-1-text-item-0", "row-1-text-item-1"], "{ids:?}");
}

#[test]
fn an_index_is_named_by_a_word_no_other_name_takes() {
    for name in [json!("item"), json!("index"), json!("rows"), json!("a-b"), json!(3)] {
        let mut screen = grid(None, button("cell-{index}"));
        screen["root"]["children"][0]["index"] = name.clone();
        let checked = check(&screen);
        assert!(
            checked
                .findings
                .iter()
                .any(|f| f.at == "rows.index" && f.problem.contains("cannot name this repeat's index")),
            "{name}: {:#?}",
            checked.findings
        );
    }
    let mut screen = grid(None, button("cell-{index}"));
    screen["root"]["index"] = json!("page");
    let checked = check(&screen);
    assert!(
        checked.findings.iter().any(|f| f.at == "page.index" && f.problem.contains("has no \"each\"")),
        "{:#?}",
        checked.findings
    );
}
