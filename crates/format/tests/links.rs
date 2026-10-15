//! Where a description's navigates go, and how one to a screen not drawn
//! yet is marked.

use std::collections::BTreeSet;

use serde_json::json;
use underdraw_format::{Navigate, assume_undrawn, navigates};

fn screen() -> serde_json::Value {
    json!({
        "screen": "Home",
        "root": { "id": "page", "kind": "Column", "children": [
            { "id": "open-a", "kind": "Button", "on": { "tap": { "navigate": "A", "with": { "id": "1" } } } },
            { "id": "pick", "kind": "Button", "on": { "tap": { "cases": [
                { "if": "ready", "then": { "navigate": "B" } },
                { "else": { "navigate": "C" } } ] } } },
            { "id": "guessed", "kind": "Button", "assumed": "its own", "on": { "tap": { "navigate": "D" } } } ] },
        "phases": { "error": { "id": "retry", "kind": "Button", "on": { "tap": { "navigate": "A" } } } },
        "fixtures": { "default": { "on": { "navigate": "NotANode" } } }
    })
}

#[test]
fn every_navigate_on_every_node_cases_included() {
    let found: Vec<(String, String)> =
        navigates(&screen()).into_iter().map(|Navigate { node, to }| (node, to)).collect();
    let expected = [("open-a", "A"), ("pick", "B"), ("pick", "C"), ("guessed", "D"), ("retry", "A")];
    assert_eq!(
        found,
        expected.map(|(node, to)| (node.to_string(), to.to_string())),
        "fixtures hold no nodes"
    );
}

#[test]
fn a_node_navigating_to_a_screen_not_drawn_is_assumed() {
    let drawn = BTreeSet::from(["A".to_string(), "B".to_string()]);
    let marked = assume_undrawn(&screen(), &drawn);
    let children = &marked["root"]["children"];
    assert_eq!(children[0].get("assumed"), None, "A is drawn");
    let why = children[1]["assumed"].as_str().unwrap();
    assert!(why.contains('C') && !why.contains("B,"), "{why}");
    assert_eq!(children[2]["assumed"], "its own", "a node's own assumption is kept");
    assert_eq!(marked["phases"]["error"].get("assumed"), None);
    assert_eq!(marked["fixtures"], screen()["fixtures"], "fixtures untouched");
}
