//! A screen's params: each a field of data every fixture gives, and each
//! navigate to it, among the screens under a root, gives each of its type.

use serde_json::{Value, json};
use underdraw_format::{Checked, ScreenParams, check, check_among, params};

/// OrderDetail, opened with the order's id and whether to show its history.
fn detail() -> Value {
    json!({
        "screen": "OrderDetail",
        "params": ["orderId", "history"],
        "data": { "orderId": "string 1..", "history": "bool", "note": "string?",
                  "menu": { "type": "bool = false", "view": true } },
        "fixtures": {
            "default": { "orderId": "o1", "history": false },
            "history": { "$patch": "default", "history": true }
        },
        "root": { "id": "page", "component": "Stack", "children": [ { "id": "title", "component": "Text", "text": "{orderId}" } ] }
    })
}

/// OrderList, whose rows each open OrderDetail with `with`.
fn list(with: Value) -> Value {
    json!({
        "screen": "OrderList",
        "data": { "orders": "Order[]", "count": "int" },
        "types": { "Order": { "id": "string 1..", "paid": "bool" } },
        "fixtures": { "default": { "orders": [ { "id": "o1", "paid": true } ], "count": 1 } },
        "root": { "id": "page", "component": "Stack", "children": [
            { "id": "row", "component": "Stack", "each": "orders", "item": { "id": "row-{item.id}", "component": "Button", "label": "{item.id}",
              "on": { "tap": { "navigate": "OrderDetail", "with": with } } } },
            { "id": "later", "component": "Button", "label": "Later", "on": { "tap": { "navigate": "Invoice", "with": { "anything": 1 } } } } ] }
    })
}

fn others() -> ScreenParams {
    ScreenParams::from([("OrderDetail".to_string(), params(&detail()).unwrap())])
}

fn problems(checked: &Checked) -> Vec<(&str, &str)> {
    checked.findings.iter().map(|f| (f.at.as_str(), f.problem.as_str())).collect()
}

#[test]
fn a_screen_declaring_params_its_fixtures_give_checks() {
    let checked = check(&detail());
    assert!(checked.ok, "{:#?}", checked.findings);
    let declared: Vec<String> = params(&detail())
        .unwrap()
        .into_iter()
        .map(|(name, field)| format!("{name}: {}", field.describe()))
        .collect();
    assert_eq!(declared, ["orderId: string", "history: bool"]);
    assert_eq!(params(&list(json!({}))), None, "a screen with no params declares none");
}

#[test]
fn a_param_is_a_field_of_data_every_fixture_gives() {
    let mut screen = detail();
    screen["params"] = json!(["orderId", "nope", "menu", "note"]);
    screen["fixtures"]["bare"] = json!({ "orderId": "o2", "history": true });
    let checked = check(&screen);
    let found = problems(&checked);
    assert!(
        found.iter().any(|(at, said)| *at == "params.nope" && said.contains("not a field of data")),
        "{found:#?}"
    );
    assert!(found.iter().any(|(at, said)| *at == "params.menu" && said.contains("view state")), "{found:#?}");
    let missing: Vec<&str> =
        found.iter().filter(|(_, said)| said.contains("gives no note")).map(|(at, _)| *at).collect();
    assert_eq!(
        missing,
        ["fixtures.default", "fixtures.history", "fixtures.bare"],
        "a $patch gives what its base gives"
    );

    screen["fixtures"]["default"]["note"] = json!(null);
    screen["fixtures"]["bare"]["note"] = json!("hi");
    screen["params"] = json!(["orderId", "note"]);
    assert!(check(&screen).ok, "{:#?}", check(&screen).findings);
}

#[test]
fn a_navigate_gives_each_param_of_its_type_and_no_other() {
    let good = list(json!({ "orderId": "{item.id}", "history": "{item.paid}" }));
    assert!(check_among(&good, &others()).ok, "{:#?}", check_among(&good, &others()).findings);

    let at = "row-{item.id}.on.tap.with";
    let missing = check_among(&list(json!({ "orderId": "{item.id}" })), &others());
    assert!(
        problems(&missing)
            .contains(&(at, "OrderDetail is opened with history (bool), which with does not give")),
        "{:#?}",
        missing.findings
    );

    let wrong = check_among(&list(json!({ "orderId": "{count}", "history": true })), &others());
    let found = problems(&wrong);
    assert!(
        found.iter().any(|(where_, said)| *where_ == format!("{at}.orderId") && said.contains("is int")),
        "{found:#?}"
    );

    let extra =
        check_among(&list(json!({ "orderId": "{item.id}", "history": false, "tab": "x" })), &others());
    assert!(
        problems(&extra)
            .iter()
            .any(|(where_, said)| *where_ == format!("{at}.tab") && said.contains("declares no param tab")),
        "{:#?}",
        extra.findings
    );
}

#[test]
fn a_navigate_to_a_screen_not_drawn_or_alone_is_held_to_nothing() {
    // Invoice is not under the root; alone, OrderDetail is not either.
    let bare = list(json!({ "whatever": "{count}" }));
    assert!(check(&bare).ok, "{:#?}", check(&bare).findings);
    let among = check_among(&bare, &others());
    assert!(problems(&among).iter().all(|(at, _)| !at.starts_with("later")), "{:#?}", among.findings);
}
