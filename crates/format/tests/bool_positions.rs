//! A bool position takes an expression whatever its key is called: a look's
//! bool prop or a component's bool param given `"!hasItems"` holds where
//! that does, and `disabled` is one such prop of Button, read by its type,
//! not its name.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Checked, Node, Screen, Shared, State};

fn find<'a>(node: &'a Node, id: &str) -> Option<&'a Node> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, id))
}

fn found(checked: &Checked, at: &str, said: &str) -> bool {
    checked.findings.iter().any(|f| f.at == at && f.problem.contains(said))
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&Screen::read(description, Shared::default()))
}

/// What the node `id` is given, in the fixture `fixture`.
fn prop(description: &Value, fixture: &str, id: &str, prop: &str) -> Value {
    let screen = Screen::read(description, Shared::default());
    let root = screen.instantiate(State::Fixture(fixture), Breakpoint::Regular, None).unwrap();
    find(&root, id).unwrap_or_else(|| panic!("no node {id}")).props[prop].clone()
}

/// A screen whose `Chip` declares `params` and reads `dim` only as a
/// yes/no, with `nodes` under its root.
fn screen(params: Value, nodes: Value) -> Value {
    json!({
        "screen": "Items",
        "data": { "count": "int", "bar": "Bar" },
        "types": { "Bar": { "canBack": "bool" } },
        "conditions": { "hasItems": { "when": "count > 0", "label": "有条目" } },
        "fixtures": {
            "some": { "count": 1, "bar": { "canBack": true } },
            "none": { "count": 0, "bar": { "canBack": false } }
        },
        "components": {
            "Chip": { "params": params,
                      "look": { "box": "row", "fill": { "cases": [ { "if": "dim", "then": "fill" }, { "else": "strong" } ] } } },
            "Pair": { "params": { "dim": "bool" },
                      "nodes": { "component": "Row", "children": [ { "id": "chip", "component": "Chip", "dim": "{dim}" } ] } }
        },
        "root": { "id": "screen", "component": "Screen", "children": nodes }
    })
}

#[test]
fn a_bool_prop_given_an_expression_holds_where_it_does() {
    for given in ["!bar.canBack", "!hasItems", "!hasItems && !bar.canBack"] {
        let chip = json!({ "id": "chip", "component": "Chip", "dim": given });
        let description = screen(json!({ "dim": "bool" }), json!([chip]));
        let checked = check(&description);
        assert!(checked.ok, "{given}: {:#?}", checked.findings);
        assert_eq!(prop(&description, "some", "chip", "dim"), json!(false), "{given}");
        assert_eq!(prop(&description, "none", "chip", "dim"), json!(true), "{given}");
    }
}

#[test]
fn a_bool_param_in_with_given_an_expression_holds_where_it_does() {
    let pair = json!({ "id": "pair", "component": "Pair", "with": { "dim": "!hasItems" } });
    let description = screen(json!({ "dim": "bool" }), json!([pair]));
    let checked = check(&description);
    assert!(checked.ok, "{:#?}", checked.findings);
    assert_eq!(prop(&description, "some", "pair-chip", "dim"), json!(false));
    assert_eq!(prop(&description, "none", "pair-chip", "dim"), json!(true));
}

#[test]
fn a_bool_position_given_what_is_no_expression_is_found_there() {
    for (given, said) in [
        ("can go back", "\"can go back\" is no expression, and this is a bool"),
        ("count > 0", "\"count > 0\" is no expression, and this is a bool"),
        ("!nowhere", "nowhere is neither a condition nor a path"),
        ("!count", "count is int, not a Bool"),
    ] {
        let chip = json!({ "id": "chip", "component": "Chip", "dim": given });
        let checked = check(&screen(json!({ "dim": "bool" }), json!([chip])));
        assert!(found(&checked, "chip.dim", said), "{given}: {:#?}", checked.findings);
    }
    let pair = json!({ "id": "pair", "component": "Pair", "with": { "dim": "a || b" } });
    let checked = check(&screen(json!({ "dim": "bool" }), json!([pair])));
    assert!(
        found(&checked, "pair.with.dim", "is no expression, and this is a bool"),
        "{:#?}",
        checked.findings
    );
}

#[test]
fn a_listed_param_asked_only_as_a_yes_no_given_an_expression_is_found() {
    let chip = json!({ "id": "chip", "component": "Chip", "dim": "!hasItems" });
    let checked = check(&screen(json!(["dim"]), json!([chip])));
    assert!(
        found(
            &checked,
            "chip.dim",
            "\"!hasItems\" is read as text, which always holds, and Chip asks dim only as a yes/no: type Chip's params, {\"dim\": \"bool\", …}"
        ),
        "{:#?}",
        checked.findings
    );
}

#[test]
fn disabled_is_a_bool_prop_of_button_and_no_other_by_its_name() {
    let button = json!({ "id": "go", "component": "Button", "text": "Go", "disabled": "!hasItems" });
    let description = screen(json!({ "dim": "bool" }), json!([button]));
    assert!(check(&description).ok, "{:#?}", check(&description).findings);
    assert_eq!(prop(&description, "some", "go", "disabled"), json!(false));
    assert_eq!(prop(&description, "none", "go", "disabled"), json!(true));

    // A component that does not type `disabled` reads it as it reads any
    // prop it lists: as given.
    let chip = json!({ "id": "chip", "component": "Chip", "dim": true, "disabled": "!hasItems" });
    let description = screen(json!(["dim", "disabled"]), json!([chip]));
    assert_eq!(prop(&description, "none", "chip", "disabled"), json!("!hasItems"));
}
