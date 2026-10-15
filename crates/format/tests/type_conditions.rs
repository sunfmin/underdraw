//! Conditions declared on a type (ADR 0020): `Order.cancellable` in the
//! domain, read through a value of it (`item.cancellable`), required by an
//! operation and gating each call to it as a screen's own condition does.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Checked, DOMAIN, Node, State, check, model};

fn domain() -> Value {
    json!({
        "types": {
            "Order": { "id": "string", "status": "OrderStatus", "note": "string?", "customer": "Customer" },
            "OrderStatus": "enum(pending, shipped)",
            "Customer": { "name": "string" }
        },
        "conditions": {
            "Order.cancellable": { "when": "status == \"pending\"", "label": "Not shipped yet" },
            "Order.hasNote": { "when": "note != null", "label": "Has a note" },
            "Order.quiet": { "when": "cancellable && !hasNote", "label": "Cancellable, no note" }
        },
        "operations": {
            "cancelOrder": { "command": "Cancels an order", "params": { "id": "string" },
                             "requires": "Order.cancellable" }
        }
    })
}

/// A list of orders whose rows each carry `cancel`, a cancel button, and
/// `note`, the order's note under `if: item.hasNote`.
fn screen(cancel: Value) -> Value {
    let order = |id: &str, status: &str, note: Value| json!({ "id": id, "status": status, "note": note, "customer": { "name": "Ana" } });
    json!({
        "screen": "Orders",
        DOMAIN: domain(),
        "data": { "orders": "Order[] ..20" },
        "fixtures": {
            "default": { "orders": [order("a", "pending", json!(null)), order("b", "shipped", json!("leave it at the door"))] }
        },
        "root": { "id": "root", "component": "Screen", "children": [
            { "id": "list", "component": "List", "each": "orders", "item": {
                "id": "order-{index}", "component": "Card", "children": [
                    { "id": "order-{index}-note", "component": "Text", "if": "item.hasNote", "text": "{item.note}" },
                    cancel
                ] } }
        ] }
    })
}

fn button(extra: Value) -> Value {
    let mut button = json!({ "id": "order-{index}-cancel", "component": "Button", "text": "Cancel",
                             "on": { "tap": { "call": "cancelOrder", "with": { "id": "{item.id}" } } } });
    for (key, value) in extra.as_object().unwrap() {
        button[key] = value.clone();
    }
    button
}

/// Where check names the cancel button's "if".
const IF: &str = "order-{index}-cancel.if";

fn found(checked: &Checked, at: &str, said: &str) -> bool {
    checked.findings.iter().any(|f| f.at == at && f.problem.contains(said))
}

fn ids(node: &Node, out: &mut Vec<String>) {
    out.push(node.id.clone());
    for child in &node.children {
        ids(child, out);
    }
}

#[test]
fn a_row_gates_its_cancel_button_on_the_orders_condition() {
    for gate in [json!({ "if": "item.cancellable" }), json!({ "disabled": "!item.cancellable" })] {
        let screen = screen(button(gate.clone()));
        let checked = check(&screen);
        assert!(checked.ok, "{gate}: {:#?}", checked.findings);
        let table: Vec<(&str, &[String])> =
            checked.conditions.iter().map(|c| (c.name.as_str(), c.holds_in.as_slice())).collect();
        let default = ["default".to_string()];
        assert_eq!(
            table,
            [
                ("Order.cancellable", &default[..]),
                ("Order.hasNote", &default[..]),
                ("Order.quiet", &default[..])
            ]
        );
    }
    let model = model(&screen(button(json!({ "if": "item.cancellable" })))).unwrap();
    let orders = &model.fixtures[0].1["orders"];
    assert_eq!(orders[0]["cancellable"], true, "each Order reads it as a Bool field");
    assert_eq!(orders[1]["cancellable"], false);
    assert_eq!(orders[0]["quiet"], true, "one condition on a type names another by its short name");
    let mut shown = Vec::new();
    ids(&model.instantiate(State::Fixture("default"), Breakpoint::Compact).unwrap(), &mut shown);
    assert!(shown.contains(&"order-0-cancel".to_string()), "{shown:?}");
    assert!(!shown.contains(&"order-1-cancel".to_string()), "a shipped order has no cancel: {shown:?}");
    assert!(shown.contains(&"order-1-note".to_string()) && !shown.contains(&"order-0-note".to_string()));
}

#[test]
fn check_names_an_ungated_call_and_a_condition_on_the_wrong_value() {
    let ungated = check(&screen(button(json!({}))));
    let at = "order-{index}-cancel";
    assert!(
        found(&ungated, at, "calls cancelOrder, which requires Order.cancellable")
            && found(&ungated, at, "\"disabled\": \"!item.cancellable\""),
        "{:#?}",
        ungated.findings
    );

    let other = check(&screen(button(json!({ "if": "item.customer.cancellable" }))));
    assert!(
        found(&other, IF, "cancellable is a condition on Order, and item.customer is Customer"),
        "{:#?}",
        other.findings
    );

    let bare = check(&screen(button(json!({ "if": "Order.cancellable" }))));
    assert!(found(&bare, IF, "is a condition on Order: use it through a value"), "{:#?}", bare.findings);
}

#[test]
fn check_names_a_condition_on_a_type_that_is_written_wrong() {
    let planted = |name: &str, body: Value| {
        let mut screen = screen(button(json!({ "if": "item.cancellable" })));
        screen["conditions"] = json!({ name: body });
        check(&screen)
    };
    let cases = [
        ("Nothing.x", json!({ "when": "true", "label": "x" }), "Nothing is not an object type"),
        ("Order.status", json!({ "when": "true", "label": "x" }), "Order has a field status"),
        (
            "Order.big",
            json!({ "when": "hasOrders", "label": "x" }),
            "hasOrders is a condition over the screen's data",
        ),
        ("Order.pending", json!({ "when": "status == \"pending\" && id != \"\"", "label": "x" }), ""),
        (
            "Order.always",
            json!({ "when": "id != \"never\"", "label": "x" }),
            "is true of every Order in the fixtures",
        ),
    ];
    for (name, body, said) in cases {
        let mut checked = planted(name, body.clone());
        if name == "Order.big" {
            let mut screen = screen(button(json!({ "if": "item.cancellable" })));
            screen["conditions"] = json!({
                "hasOrders": { "when": "orders != null", "label": "x" },
                name: body
            });
            checked = check(&screen);
        }
        let at = format!("conditions.{name}");
        if said.is_empty() {
            assert!(checked.ok, "{name}: {:#?}", checked.findings);
        } else {
            assert!(found(&checked, &at, said), "{name}: {:#?}", checked.findings);
        }
    }

    let mut screen = screen(button(json!({ "if": "item.cancellable" })));
    screen["conditions"] = json!({
        "anyCancellable": { "when": "Order.cancellable", "label": "x" }
    });
    let checked = check(&screen);
    assert!(
        found(
            &checked,
            "conditions.anyCancellable",
            "Order.cancellable is a condition on Order: use it through a value of Order"
        ),
        "{:#?}",
        checked.findings
    );
}
