//! The generated code for a small screen that reaches what the listing
//! draft does not: an enum, an order on a value that may be null, a path
//! through a null parent, a field named like a keyword.

use serde_json::{Value, json};
use underdraw_handoff::handoff;

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
        "root": { "id": "root", "kind": "Screen", "children": [ { "id": "t", "kind": "Text", "text": "x" } ] }
    })
}

fn code(language: &str) -> String {
    let mapping = json!({
        "platform": language, "language": language, "testId": "{id}",
        "tokenNaming": { "pattern": "{group}.{name}" },
        "kinds": { "Screen": "screen", "Text": "text" }, "actions": {}
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
fn a_mapping_that_misses_a_kind_says_so() {
    let mapping = json!({
        "platform": "ios", "language": "swift", "testId": "{id}",
        "tokenNaming": { "pattern": "{group}.{name}" }, "kinds": { "Screen": "screen" }, "actions": {}
    });
    let handoff = handoff(&screen(), &mapping, &json!({})).unwrap_or_else(|c| panic!("{:?}", c.findings));
    assert!(!handoff.ok);
    assert_eq!(handoff.findings.len(), 1);
    assert_eq!(handoff.findings[0].at, "t");
    assert!(handoff.findings[0].problem.contains("kind Text"), "{:?}", handoff.findings);
}
