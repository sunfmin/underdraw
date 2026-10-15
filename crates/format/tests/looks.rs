//! A look component's params, listed by name or typed as a nodes
//! component's are: `check` holds each node's props to the types, and a
//! prop may be given a condition of the screen's, which holds where the
//! condition does.

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

/// What the node `chip` is given, in the fixture `fixture`.
fn props(description: &Value, fixture: &str) -> serde_json::Map<String, Value> {
    let screen = Screen::read(description, Shared::default());
    let root = screen.instantiate(State::Fixture(fixture), Breakpoint::Regular, None).unwrap();
    find(&root, "chip").expect("the chip").props.clone()
}

/// A screen with one look component, `Chip`, declaring `params`, and one
/// node of it giving `given`.
fn screen(params: Value, given: Value) -> Value {
    let mut chip = json!({ "id": "chip", "component": "Chip" });
    chip.as_object_mut().unwrap().extend(given.as_object().unwrap().clone());
    json!({
        "screen": "Items",
        "data": { "count": "int", "name": "string" },
        "conditions": { "hasItems": { "when": "count > 0", "label": "有条目" } },
        "fixtures": { "some": { "count": 1, "name": "Milk" }, "none": { "count": 0, "name": "Milk" } },
        "components": { "Chip": {
            "params": params,
            "look": { "box": "row", "padding": [6, 12],
                      "fill": { "cases": [ { "if": "selected", "then": "strong" }, { "else": null } ] },
                      "children": [ { "text": "{text}" } ] } } },
        "root": { "id": "screen", "component": "Screen", "children": [chip] }
    })
}

fn typed() -> Value {
    json!({ "selected": "bool", "text": "string" })
}

#[test]
fn a_look_with_typed_params_holds_each_prop_to_its_type() {
    let description = screen(typed(), json!({ "selected": true, "text": "{name}" }));
    let checked = check(&description);
    assert!(checked.ok, "{:#?}", checked.findings);

    for (given, at, said) in [
        (json!({ "selected": 3, "text": "Milk" }), "chip.selected", "and the parameter is bool"),
        (json!({ "selected": "{count}", "text": "Milk" }), "chip.selected", "is int"),
        (json!({ "selected": true, "text": 2 }), "chip.text", "and the parameter is string"),
        (json!({ "selected": true }), "chip", "Chip reads text (string), which the node does not give"),
        (
            json!({ "selected": "!hasItems", "text": "Milk" }),
            "chip.selected",
            "is text, and selected is a bool",
        ),
        (json!({ "selected": true, "text": "{hasItems}" }), "chip.text", "is the condition hasItems, a Bool"),
    ] {
        let checked = check(&screen(typed(), given.clone()));
        assert!(found(&checked, at, said), "{given}: {:#?}", checked.findings);
    }

    let defaulted = json!({ "selected": "bool = false", "text": "string?" });
    let checked = check(&screen(defaulted, json!({})));
    assert!(checked.ok, "a default or a null need not be given: {:#?}", checked.findings);

    let unknown = json!({ "selected": "Flag", "text": "string" });
    let checked = check(&screen(unknown, json!({ "selected": true, "text": "Milk" })));
    assert!(
        found(&checked, "components.Chip.params.selected", "unknown type Flag"),
        "{:#?}",
        checked.findings
    );

    let unread = json!({ "text": "string" });
    let checked = check(&screen(unread, json!({ "text": "Milk" })));
    assert!(found(&checked, "components.Chip.look", "reads selected"), "{:#?}", checked.findings);
}

#[test]
fn a_look_listing_its_params_still_takes_any_prop() {
    let listed = json!(["selected", "text"]);
    let checked = check(&screen(listed.clone(), json!({ "selected": 3, "text": "Milk" })));
    assert!(checked.ok, "{:#?}", checked.findings);
    let checked = check(&screen(json!("selected"), json!({})));
    assert!(found(&checked, "components.Chip.params", "or types each"), "{:#?}", checked.findings);
}

#[test]
fn a_prop_given_a_condition_holds_where_the_condition_does() {
    for params in [typed(), json!(["selected", "text"])] {
        let description = screen(params.clone(), json!({ "selected": "{hasItems}", "text": "Milk" }));
        let checked = check(&description);
        assert!(checked.ok, "{params}: {:#?}", checked.findings);
        assert_eq!(props(&description, "some")["selected"], json!(true), "{params}");
        assert_eq!(props(&description, "none")["selected"], json!(false), "{params}");
    }
    let mut description = screen(typed(), json!({ "selected": "{hasItems}", "text": "Milk" }));
    description["components"]["Chip"]["params"]["selected"] = json!("string");
    let checked = check(&description);
    assert!(
        found(&checked, "chip.selected", "is the condition hasItems, a Bool, and the param is string"),
        "{:#?}",
        checked.findings
    );
}

#[test]
fn a_shared_look_with_typed_params_is_shown_in_fixtures_of_those_types() {
    let chip = json!({
        "params": { "selected": "bool", "text": "string" },
        "fixtures": { "on": { "selected": true, "text": "Direct" }, "off": { "selected": false, "text": "Direct" } },
        "look": { "box": "row", "fill": { "cases": [ { "if": "selected", "then": "strong" }, { "else": null } ] },
                  "children": [ { "text": "{text}" } ] }
    });
    let shared = json!({ "Chip": chip.clone() });
    let shown = underdraw_format::showcase("Chip", &chip);
    let checked = underdraw_format::check(&Screen::read(
        &shown,
        Shared { domain: None, components: shared.as_object() },
    ));
    assert!(checked.ok, "{:#?}", checked.findings);

    let mut wrong = chip.clone();
    wrong["fixtures"]["off"]["selected"] = json!("no");
    let shown = underdraw_format::showcase("Chip", &wrong);
    let checked = underdraw_format::check(&Screen::read(
        &shown,
        Shared { domain: None, components: shared.as_object() },
    ));
    assert!(checked.findings.iter().any(|f| f.at.starts_with("fixtures.off")), "{:#?}", checked.findings);
}
