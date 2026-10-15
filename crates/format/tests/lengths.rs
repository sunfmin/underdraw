//! Only a look computes, and only a length: `check` names an operand
//! without bounds, a division and a fraction in a look's length, and
//! arithmetic anywhere outside a look.

use serde_json::{Value, json};
use underdraw_format::{Checked, Screen, Shared};

fn check(description: &Value) -> Checked {
    underdraw_format::check(&Screen::read(description, Shared::default()))
}

fn found(checked: &Checked, at: &str, said: &str) -> bool {
    checked.findings.iter().any(|f| f.at == at && f.problem.contains(said))
}

/// A screen with one look component, `Line`, declaring `params`, whose
/// look's padding is `padding`; `node` adds to its one node, and
/// `conditions` are the screen's.
fn screen(params: Value, padding: Value, node: Value, conditions: Value) -> Value {
    let mut line = json!({ "id": "line", "component": "Line", "depth": 1 });
    line.as_object_mut().unwrap().extend(node.as_object().unwrap().clone());
    json!({
        "screen": "Lines",
        "data": { "depth": "int 0..4", "name": "string" },
        "conditions": conditions,
        "fixtures": { "default": { "depth": 1, "name": "a" } },
        "components": { "Line": { "params": params, "look": { "box": "row", "padding": padding } } },
        "root": { "id": "page", "component": "Screen", "children": [line] }
    })
}

fn padded(params: Value, padding: Value) -> Checked {
    check(&screen(params, padding, json!({}), json!({})))
}

#[test]
fn a_length_over_bounded_ints_and_tokens_checks_clean() {
    let checked = padded(json!({ "depth": "int 0..4" }), json!([8, "{depth} * space.4 + 12"]));
    assert!(checked.ok, "{:#?}", checked.findings);
}

#[test]
fn an_operand_without_bounds_is_named() {
    let at = "components.Line.look.padding";
    assert!(found(
        &padded(json!({ "depth": "int 0.." }), json!("{depth} * space.4")),
        at,
        "{depth} has no bounds"
    ));
    assert!(found(&padded(json!(["depth"]), json!("{depth} * space.4")), at, "{depth} has no bounds"));
    assert!(found(&padded(json!({ "depth": "number 0..4" }), json!("{depth} * 4")), at, "{depth} is number"));
    assert!(found(
        &padded(json!({ "depth": "int 0..4" }), json!([0, "{depth} * {depth}"])),
        &format!("{at}[1]"),
        "is an int, not a length"
    ));
}

#[test]
fn a_division_and_a_fraction_are_named() {
    let params = json!({ "depth": "int 0..4" });
    let at = "components.Line.look.padding";
    assert!(found(&padded(params.clone(), json!("space.4 / 2")), at, "never divided"));
    assert!(found(&padded(params, json!("{depth} * 1.5")), at, "1.5 is a fraction"));
}

#[test]
fn arithmetic_outside_a_look_is_named() {
    let params = json!({ "depth": "int 0..4" });
    let condition = json!({ "deep": { "when": "depth * 2 > 4", "label": "Deep" } });
    let checked = check(&screen(params.clone(), json!(0), json!({}), condition));
    assert!(
        checked.findings.iter().any(|f| f.problem.contains("* is arithmetic, and a yes/no never computes")),
        "{:#?}",
        checked.findings
    );

    let checked = check(&screen(params.clone(), json!(0), json!({ "depth": "{depth} + 1" }), json!({})));
    assert!(
        found(&checked, "line.depth", "is arithmetic, and a node never computes"),
        "{:#?}",
        checked.findings
    );

    let template = json!({ "children": [{ "id": "said", "component": "Text", "text": "{depth * 2}" }] });
    let checked = check(&screen(params, json!(0), template, json!({})));
    assert!(
        found(&checked, "said.text", "is arithmetic, and a template never computes"),
        "{:#?}",
        checked.findings
    );
}
