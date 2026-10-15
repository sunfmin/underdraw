//! A screen's own components whose body is nodes: each node inside an
//! instance is named by the instance's id joined to its own, reads only the
//! params the instance gives, and `check` names what breaks that.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Checked, Node, Screen, Shared, State};

const ORDERS: &str = include_str!("../../../drafts/orders/screen.json");

fn find<'a>(node: &'a Node, id: &str) -> Option<&'a Node> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, id))
}

fn found(checked: &Checked, at: &str, said: &str) -> bool {
    checked.findings.iter().any(|f| f.at == at && f.problem.contains(said))
}

#[test]
fn the_orders_rows_are_named_by_their_instance_and_their_transition_names_them() {
    let screen: Value = serde_json::from_str(ORDERS).unwrap();
    let checked = check(&screen);
    assert!(checked.ok, "{:#?}", checked.findings);
    let root = instantiate(&screen, State::Fixture("default"), Breakpoint::Regular).unwrap();
    let row = find(&root, "order-0").expect("the first row, named by its instance");
    assert_eq!(row.component, "OrderTableRow", "the instance is its component's root");
    assert_eq!(row.written_at, "/components/OrderRow/nodes");
    assert_eq!(
        row.props["title"],
        json!(format!("#{}", screen["fixtures"]["default"]["orders"][0]["number"].as_str().unwrap()))
    );
    let cancel = find(&root, "order-0-cancel").expect("the row's cancel button, its id joined to the row's");
    assert_eq!(cancel.written_id, "order-{index}-cancel");
    assert_eq!(cancel.instance, [0]);
    let screen = read(&screen);
    assert!(screen.writes("order-{index}-cancel"), "a transition can name it");
    assert!(!screen.writes("cancel"), "and never by its own id alone");
    assert_eq!(screen.transitions[0].node, "order-{index}-cancel");
    assert!(screen.navigates().iter().any(|n| n.node == "order-{index}" && n.to == "OrderDetail"));
}

/// A screen with one component, `ItemRow`, used once in a list of items.
fn screen(row: Value, instance: Value) -> Value {
    json!({
        "screen": "Items",
        "types": { "Item": { "id": "string", "name": "string 1..", "done": "bool" } },
        "data": {
            "items": "Item[]",
            "count": "int",
            "title": "string",
            "menuOpen": { "type": "bool = false", "view": true },
        },
        "conditions": { "hasItems": { "when": "count > 0", "label": "有条目" } },
        "fixtures": {
            "some": { "title": "Today", "count": 1, "items": [ { "id": "a", "name": "Milk", "done": false } ] },
            "none": { "title": "Today", "count": 0, "items": [] }
        },
        "components": { "ItemRow": row },
        "root": { "id": "screen", "component": "Screen", "children": [
            { "id": "list", "component": "List", "each": "items", "item": instance }
        ] }
    })
}

fn row() -> Value {
    json!({
        "params": { "item": "Item", "open": "bool" },
        "nodes": { "component": "Row", "children": [
            { "id": "name", "component": "Text", "text": "{item.name}" },
            { "id": "more", "component": "IconButton", "icon": "more_vert", "on": { "tap": { "toggle": "open" } } },
            { "slot": "children" }
        ] }
    })
}

#[test]
fn a_component_reads_its_params_toggles_what_was_passed_and_places_its_slot() {
    let instance = json!({
        "id": "row-{index}", "component": "ItemRow", "with": { "item": "{item}", "open": "{menuOpen}" },
        "children": { "id": "row-{index}-title", "component": "Text", "text": "{title}" }
    });
    let screen = screen(row(), instance);
    let checked = check(&screen);
    assert!(checked.ok, "{:#?}", checked.findings);
    let root = instantiate(&screen, State::Fixture("some"), Breakpoint::Compact).unwrap();
    assert_eq!(find(&root, "row-0-name").unwrap().props["text"], json!("Milk"));
    assert_eq!(
        find(&root, "row-0-more").unwrap().props["on"],
        json!({ "tap": { "toggle": "menuOpen" } }),
        "the component holds no view state: its toggle acts on what was passed"
    );
    let title = find(&root, "row-0-title").expect("what the instance gives in a slot keeps its own id");
    assert_eq!(title.props["text"], json!("Today"), "and reads the screen's data");
}

#[test]
fn each_fault_of_a_component_is_found() {
    let good = json!({ "id": "row-{index}", "component": "ItemRow", "with": { "item": "{item}", "open": "{menuOpen}" } });
    let with = |with: Value| {
        let mut instance = good.clone();
        instance["with"] = with;
        instance
    };
    let mut reaching = row();
    reaching["nodes"]["children"][0]["text"] = json!("{title}");
    let mut viewing = row();
    viewing["params"]["open"] = json!({ "type": "bool", "view": true });
    let mut conditioned = row();
    conditioned["nodes"]["children"][0]["if"] = json!("hasItems");
    let mut cycling = row();
    cycling["nodes"]["children"][2] =
        json!({ "id": "inner", "component": "ItemRow", "with": { "item": "{item}", "open": "{open}" } });
    let mut identified = row();
    identified["nodes"]["id"] = json!("row");
    // (name, the component, the instance, where it is found, what is said)
    let cases = [
        (
            "missing param",
            row(),
            with(json!({ "item": "{item}" })),
            "row-{index}.with",
            "ItemRow reads open (bool), which with does not give",
        ),
        (
            "ill-typed param",
            row(),
            with(json!({ "item": "{item.name}", "open": "{menuOpen}" })),
            "row-{index}.with.item",
            "is string, and the parameter is Item",
        ),
        (
            "undeclared param",
            row(),
            with(json!({ "item": "{item}", "open": "{menuOpen}", "colour": "red" })),
            "row-{index}.with.colour",
            "ItemRow declares no param colour",
        ),
        (
            "past the params",
            reaching,
            good.clone(),
            "row-{index}-name.text",
            "title reaches past the params of ItemRow",
        ),
        (
            "a screen's condition",
            conditioned,
            good.clone(),
            "row-{index}-name.if",
            "hasItems is a condition of the screen's, past the params of ItemRow",
        ),
        (
            "view state inside",
            viewing,
            good.clone(),
            "components.ItemRow.params.open",
            "a component holds no view state",
        ),
        (
            "reaching inside",
            row(),
            json!({ "id": "row-{index}", "component": "ItemRow", "with": { "item": "{item}", "open": "{menuOpen}" }, "text": "x" }),
            "row-{index}.text",
            "a screen cannot reach inside an instance of ItemRow",
        ),
        (
            "a slot it does not place",
            row(),
            json!({ "id": "row-{index}", "component": "ItemRow", "with": { "item": "{item}", "open": "{menuOpen}" }, "trailing": { "id": "t", "component": "Text", "text": "x" } }),
            "row-{index}.trailing",
            "ItemRow places no trailing",
        ),
        (
            "a cycle",
            cycling,
            good.clone(),
            "row-{index}-inner",
            "component ItemRow uses itself: ItemRow → ItemRow",
        ),
        (
            "an id on the root",
            identified,
            good.clone(),
            "components.ItemRow.nodes.id",
            "named by the instance's id",
        ),
        ("ref", row(), json!({ "ref": "ItemRow" }), "list.item.ref", "ref is gone"),
    ];
    for (name, component, instance, at, said) in cases {
        let checked = check(&screen(component, instance));
        assert!(found(&checked, at, said), "{name}: expected {said:?} at {at}, got {:#?}", checked.findings);
    }
}

#[test]
fn a_bool_param_takes_a_condition_of_the_screens() {
    let mut row = row();
    row["params"]["shown"] = json!("bool");
    row["nodes"]["children"][0]["if"] = json!("shown");
    let instance = json!({ "id": "row-{index}", "component": "ItemRow",
        "with": { "item": "{item}", "open": "{menuOpen}", "shown": "{hasItems}" } });
    let screen = screen(row, instance);
    let checked = check(&screen);
    assert!(checked.ok, "{:#?}", checked.findings);
    let root = instantiate(&screen, State::Fixture("some"), Breakpoint::Compact).unwrap();
    assert!(find(&root, "row-0-name").is_some(), "hasItems holds where there is a row");
}

/// `description` read as a screen of its own, with nothing shared.
fn read(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&read(description))
}

fn instantiate(description: &Value, state: State, breakpoint: Breakpoint) -> Result<Node, String> {
    read(description).instantiate(state, breakpoint, None)
}
