//! `check` on the transitions of the orders draft, with one fault planted
//! at a time: each is found where it was planted, and says what is wrong.

use serde_json::Value;
use underdraw_format::{Checked, Screen, Shared, Transition};

const DRAFT: &str = include_str!("../../../drafts/orders/screen.json");

/// The draft with `from` replaced by `to`, where `from` appears exactly once.
fn planted(name: &str, from: &str, to: &str) -> Value {
    assert_eq!(DRAFT.matches(from).count(), 1, "{name}: {from:?} must appear exactly once in the draft");
    serde_json::from_str(&DRAFT.replace(from, to)).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"))
}

fn checked(screen: &Value) -> Checked {
    check(screen)
}

const OPEN: &str = r#"{ "from": "default", "on": "order-{index}-cancel.tap", "to": "cancel-dialog" }"#;
const CONFIRM: &str =
    r#"{ "from": "cancel-ready", "on": "cd-confirm.tap", "outcome": "shipped", "to": "cancel-ready" }"#;

#[test]
fn the_drafts_transitions_are_read_as_written() {
    let screen: Value = serde_json::from_str(DRAFT).unwrap();
    let screen = read(&screen);
    let model = underdraw_format::model(&screen).unwrap_or_else(|c| panic!("{c:#?}"));
    assert_eq!(
        model.transitions.first(),
        Some(&Transition {
            from: "default".to_string(),
            node: "order-{index}-cancel".to_string(),
            event: "tap".to_string(),
            outcome: None,
            to: "cancel-dialog".to_string(),
        })
    );
}

#[test]
fn each_planted_fault_in_a_transition_is_found() {
    // (name, what the draft says, the fault planted in its place, where it is found, what is said)
    let cases = [
        (
            "unknown state",
            OPEN,
            r#"{ "from": "default", "on": "order-{index}-cancel.tap", "to": "cancelling" }"#,
            "transitions.0.to",
            "cancelling is not a state: name a fixture or a phase",
        ),
        (
            "unknown node",
            OPEN,
            r#"{ "from": "default", "on": "cancel-button.tap", "to": "cancel-dialog" }"#,
            "transitions.0.on",
            "no node has the id cancel-button",
        ),
        (
            "no event in on",
            OPEN,
            r#"{ "from": "default", "on": "order-{index}-cancel", "to": "cancel-dialog" }"#,
            "transitions.0.on",
            "on is \"<node id>.<event>\"",
        ),
        (
            "an event the node does not have",
            OPEN,
            r#"{ "from": "default", "on": "order-{index}-cancel.longPress", "to": "cancel-dialog" }"#,
            "transitions.0.on",
            "order-{index}-cancel has no longPress event; its events: tap",
        ),
        (
            "an event that leaves the screen",
            OPEN,
            r#"{ "from": "default", "on": "nav-export.tap", "to": "cancel-dialog" }"#,
            "transitions.0.on",
            "it leaves this screen, so it is not a transition",
        ),
        (
            "back leaves the screen too",
            OPEN,
            r#"{ "from": "default", "on": "nav-back.tap", "to": "cancel-dialog" }"#,
            "transitions.0.on",
            "it leaves this screen",
        ),
        (
            "an outcome on a non-call",
            OPEN,
            r#"{ "from": "default", "on": "order-{index}-cancel.tap", "outcome": "ok", "to": "cancel-dialog" }"#,
            "transitions.0.outcome",
            "the event calls no operation, so it has no outcome",
        ),
        (
            "a call with no outcome",
            CONFIRM,
            r#"{ "from": "cancel-ready", "on": "cd-confirm.tap", "to": "cancel-ready" }"#,
            "transitions.5.outcome",
            "the event calls cancelOrder: say which outcome this is, one of ok, shipped",
        ),
        (
            "an outcome the operation does not have",
            CONFIRM,
            r#"{ "from": "cancel-ready", "on": "cd-confirm.tap", "outcome": "offline", "to": "cancel-ready" }"#,
            "transitions.5.outcome",
            "\"offline\" is not an outcome of cancelOrder: one of ok, shipped",
        ),
        (
            "a key a transition does not hold",
            OPEN,
            r#"{ "from": "default", "on": "order-{index}-cancel.tap", "to": "cancel-dialog", "when": "x" }"#,
            "transitions.0.when",
            "a transition holds only from, on, outcome and to",
        ),
        (
            "a node not in the from state",
            OPEN,
            r#"{ "from": "no-match", "on": "order-{index}-cancel.tap", "to": "cancel-dialog" }"#,
            "transitions.0.on",
            "order-{index}-cancel is not shown in no-match",
        ),
        (
            "a node of the root, from a phase",
            OPEN,
            r#"{ "from": "loading", "on": "cd-keep.tap", "to": "cancel-dialog" }"#,
            "transitions.0.on",
            "cd-keep is not shown in loading",
        ),
    ];
    for (name, from, to, at, said) in cases {
        let checked = checked(&planted(name, from, to));
        assert!(
            checked.findings.iter().any(|f| f.at == at && f.problem.contains(said)),
            "{name}: expected {said:?} at {at}, found {:#?}",
            checked.findings
        );
    }
}

#[test]
fn transitions_are_a_list() {
    let screen = planted("not a list", r#""transitions": ["#, r#""transitions": {}, "was": ["#);
    let checked = checked(&screen);
    assert!(checked.findings.iter().any(|f| f.at == "transitions" && f.problem.contains("is a list")));
}

#[test]
fn a_node_from_a_component_is_found_and_shown_in_a_phase() {
    let screen = planted(
        "from a component, in a phase",
        OPEN,
        r#"{ "from": "loading", "on": "nav-export.tap", "to": "default" }"#,
    );
    let checked = checked(&screen);
    // It is found, and shown in loading: what is wrong is only that it leaves.
    assert_eq!(checked.findings.len(), 1, "{:#?}", checked.findings);
    assert!(checked.findings[0].problem.contains("leaves this screen"));
}

/// `description` read as a screen of its own, with nothing shared.
fn read(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&read(description))
}
