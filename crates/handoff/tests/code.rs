//! The generated code for a small screen that reaches what the listing
//! draft does not: an enum, an order on a value that may be null, a path
//! through a null parent, a field named like a keyword.

use serde_json::{Value, json};
use underdraw_format::{Checked, Screen, Shared};
use underdraw_handoff::Handoff;

fn screen() -> Value {
    json!({
        "screen": "Plans",
        "data": { "plan": "Plan", "user": "User?" },
        "types": {
            "Plan": "enum(monthly, yearly)",
            "User": { "name": "string", "score": "number?", "default": "bool = false", "since": "date" }
        },
        "conditions": {
            "isYearly": { "when": "plan == \"yearly\"", "label": "yearly" },
            "highScore": { "when": "user != null && user.score > 4", "label": "high" },
            "both": { "when": "isYearly && (highScore || user.default)", "label": "both" }
        },
        "fixtures": {
            "a": { "plan": "yearly", "user": { "name": "A", "score": 4.5, "default": true, "since": "2026-01-02" } },
            "b": { "plan": "monthly", "user": null }
        },
        "root": { "id": "root", "component": "Screen", "children": [ { "id": "t", "component": "Text", "text": "x" } ] }
    })
}

fn code(language: &str) -> String {
    let mapping = json!({
        "platform": language, "language": language, "testId": "{id}",
        "tokenNaming": { "pattern": "{group}.{name}" },
        "components": { "Screen": "screen", "Text": "text" }, "actions": {}
    });
    let handoff = handoff(&screen(), &mapping, &json!({})).unwrap_or_else(|c| panic!("{:?}", c.findings));
    assert!(handoff.ok, "{:?}", handoff.findings);
    handoff.code.text
}

#[test]
fn swift() {
    let code = code("swift");
    for line in [
        "enum Plan: String, Codable, Equatable {\n    case monthly\n    case yearly\n}",
        "var user: User?",
        "var `default`: Bool",
        "var isYearly: Bool { plan == .yearly }",
        "var highScore: Bool { user != nil && ((user?.score).map { $0 > 4.0 } ?? false) }",
        "var both: Bool { isYearly && (highScore || user?.`default` == true) }",
    ] {
        assert!(code.contains(line), "missing {line:?} in\n{code}");
    }
}

#[test]
fn kotlin() {
    let code = code("kotlin");
    for line in [
        "@file:Suppress(\"UNNECESSARY_SAFE_CALL\")",
        "enum class Plan { monthly, yearly }",
        "val user: User? = null,",
        "val default: Boolean = false,",
        "get() = plan == Plan.yearly",
        "get() = user != null && ((user?.score)?.let { it > 4.0 } ?: false)",
        "get() = isYearly && (highScore || user?.default == true)",
    ] {
        assert!(code.contains(line), "missing {line:?} in\n{code}");
    }
}

#[test]
fn typescript() {
    let code = code("typescript");
    for line in [
        "export type Plan = \"monthly\" | \"yearly\";",
        "user: User | null;",
        "return m.plan == \"yearly\";",
        "return m.user != null && (m.user?.score != null && m.user?.score > 4.0);",
        "return isYearly(m) && (highScore(m) || m.user?.default == true);",
    ] {
        assert!(code.contains(line), "missing {line:?} in\n{code}");
    }
}

#[test]
fn a_mapping_that_misses_a_component_says_so() {
    let mapping = json!({
        "platform": "ios", "language": "swift", "testId": "{id}",
        "tokenNaming": { "pattern": "{group}.{name}" }, "components": { "Screen": "screen" }, "actions": {}
    });
    // A second Text lacks the same entry: it is said once, at the first.
    let mut screen = screen();
    screen["root"]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "u", "component": "Text", "text": "y" }));
    let handoff = handoff(&screen, &mapping, &json!({})).unwrap_or_else(|c| panic!("{:?}", c.findings));
    assert!(!handoff.ok);
    assert_eq!(handoff.findings.len(), 1, "{:?}", handoff.findings);
    assert_eq!(handoff.findings[0].at, "t");
    assert!(handoff.findings[0].problem.contains("component Text"), "{:?}", handoff.findings);
}

/// A condition declared on a type is a property of that type, and one of
/// the screen reads it through a value of the type, here one that may be
/// null.
fn on_a_type(language: &str) -> String {
    let screen = json!({
        "screen": "Shop",
        "data": { "order": "Order?" },
        "types": { "Order": { "status": "Status", "note": "string?" }, "Status": "enum(pending, shipped)" },
        "conditions": {
            "Order.cancellable": { "when": "status == \"pending\"", "label": "Not shipped" },
            "Order.quiet": { "when": "cancellable && note == null", "label": "Quiet" },
            "canCancel": { "when": "order != null && order.cancellable", "label": "Can cancel" }
        },
        "fixtures": {
            "a": { "order": { "status": "pending", "note": null } },
            "b": { "order": { "status": "shipped", "note": "x" } },
            "c": { "order": null }
        },
        "root": { "id": "root", "component": "Screen", "children": [ { "id": "t", "component": "Text", "text": "x" } ] }
    });
    let mapping = json!({
        "platform": language, "language": language, "testId": "{id}",
        "tokenNaming": { "pattern": "{group}.{name}" },
        "components": { "Screen": "screen", "Text": "text" }, "actions": {}
    });
    let handoff = handoff(&screen, &mapping, &json!({})).unwrap_or_else(|c| panic!("{:?}", c.findings));
    assert!(handoff.ok, "{:?}", handoff.findings);
    handoff.code.text
}

#[test]
fn a_condition_on_a_type_is_a_property_of_it() {
    let expected = [
        (
            "typescript",
            vec![
                "export function orderCancellable(m: Order): boolean {\n  return m.status == \"pending\";\n}",
                "export function orderQuiet(m: Order): boolean {\n  return orderCancellable(m) && m.note == null;\n}",
                "return m.order != null && (m.order != null && orderCancellable(m.order));",
            ],
        ),
        (
            "swift",
            vec![
                "extension Order {\n    /// Not shipped\n    var cancellable: Bool { status == .pending }",
                "var quiet: Bool { cancellable && note == nil }",
                "var canCancel: Bool { order != nil && order?.cancellable == true }",
            ],
        ),
        (
            "kotlin",
            vec![
                "val Order.cancellable: Boolean\n    get() = status == Status.pending",
                "val Order.quiet: Boolean\n    get() = cancellable && note == null",
                "get() = order != null && order?.cancellable == true",
            ],
        ),
    ];
    for (language, lines) in expected {
        let code = on_a_type(language);
        for line in lines {
            assert!(code.contains(line), "{language}: missing {line:?} in\n{code}");
        }
    }
}

/// `description` read as a screen of its own, with nothing shared.
fn read_screen(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn handoff(screen: &Value, mapping: &Value, tokens: &Value) -> Result<Handoff, Box<Checked>> {
    underdraw_handoff::handoff(&read_screen(screen), mapping, tokens)
}
