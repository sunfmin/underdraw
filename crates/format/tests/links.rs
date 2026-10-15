//! Where a description's navigates go, and how one to a screen not drawn
//! yet is marked.

use std::collections::BTreeSet;

use serde_json::json;
use underdraw_format::{Breakpoint, Lang, Navigate, Screen, Shared, State, Undrawn};

fn screen() -> serde_json::Value {
    json!({
        "screen": "Home",
        "root": { "id": "page", "component": "Column", "children": [
            { "id": "open-a", "component": "Button", "on": { "tap": { "navigate": "A", "with": { "id": "1" } } } },
            { "id": "pick", "component": "Button", "on": { "tap": { "cases": [
                { "if": "ready", "then": { "navigate": "B" } },
                { "else": { "navigate": "C" } } ] } } },
            { "id": "guessed", "component": "Button", "assumed": "its own", "on": { "tap": { "navigate": "D" } } } ] },
        "phases": { "error": { "id": "retry", "component": "Button", "on": { "tap": { "navigate": "A" } } } },
        "fixtures": { "default": { "on": { "navigate": "NotANode" } } }
    })
}

#[test]
fn every_navigate_on_every_node_cases_included() {
    let found: Vec<(String, String)> = Screen::read(&screen(), Shared::default())
        .navigates()
        .into_iter()
        .map(|Navigate { node, to }| (node, to))
        .collect();
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
    let screen = Screen::read(&screen(), Shared::default());
    let undrawn = Undrawn { drawn: &drawn, lang: Lang::En };
    let marked = screen.instantiate(State::Fixture("default"), Breakpoint::Compact, Some(&undrawn)).unwrap();
    let assumed: Vec<Option<&str>> = marked.children.iter().map(|child| child.assumed.as_deref()).collect();
    assert_eq!(assumed[0], None, "A is drawn");
    let why = assumed[1].unwrap();
    assert!(why.contains('C') && !why.contains("B,"), "{why}");
    assert_eq!(assumed[2], Some("its own"), "a node's own assumption is kept");
    let error = screen.instantiate(State::Phase("error"), Breakpoint::Compact, Some(&undrawn)).unwrap();
    assert_eq!(error.assumed, None);
    let unmarked = screen.instantiate(State::Fixture("default"), Breakpoint::Compact, None).unwrap();
    assert_eq!(unmarked.children[1].assumed, None, "marked only when asked")
}
