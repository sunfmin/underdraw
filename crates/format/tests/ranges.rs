//! `check` on the range of each value a call gives, on the checkout draft:
//! it lies inside the parameter's wherever the node can call it, narrowed
//! by the conditions that gate the node.

use serde_json::Value;
use underdraw_format::{Checked, Screen, Shared};

const DRAFT: &str = include_str!("../../../drafts/checkout/screen.json");

/// The draft with each `from` replaced by its `to`, where `from` appears
/// exactly once.
fn planted(name: &str, edits: &[(&str, &str)]) -> Value {
    let mut draft = DRAFT.to_string();
    for (from, to) in edits {
        assert_eq!(draft.matches(from).count(), 1, "{name}: {from:?} must appear exactly once in the draft");
        draft = draft.replace(from, to);
    }
    serde_json::from_str(&draft).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"))
}

fn checked(screen: &Value) -> Checked {
    check(screen)
}

/// Text replaced in the draft: (what it says, what is planted instead).
type Edits<'a> = Vec<(&'a str, &'a str)>;

const APPLY: &str = r#""text": "使用", "disabled": "!hasCode","#;
const REMOVE_PROMO: &str = r#""removePromo": { "command": "Takes the applied promo code off the order" }"#;
const CALL_REMOVE_PROMO: &str = r#""on": { "tap": { "call": "removePromo" } }"#;

#[test]
fn the_draft_gates_the_promo_code_it_applies_and_passes() {
    let checked = checked(&planted("as written", &[]));
    assert!(checked.ok, "{:#?}", checked.findings);
}

#[test]
fn a_value_that_may_fall_outside_the_parameters_range_is_refused() {
    // (name, edits, where it is found, what is said)
    let cases: [(&str, Edits, &str, &str); 5] = [
        (
            "a typed code, ungated",
            vec![(APPLY, r#""text": "使用","#)],
            "promo-apply.on.tap.with.code",
            r#"is string ..20, and the parameter is string 1..20: gate this node by a condition that keeps it inside, "disabled": "!<condition>", such as one that tests promoCode != """#,
        ),
        (
            "a typed code, gated by a condition that does not narrow it",
            vec![(APPLY, r#""text": "使用", "disabled": "!hasAddress","#)],
            "promo-apply.on.tap.with.code",
            "is string ..20, and the parameter is string 1..20",
        ),
        (
            "a quantity of a wider range",
            vec![(r#""quantity": "int 1..10" },"#, r#""quantity": "int 2..10" },"#)],
            "item-{index}-qty.on.change.with.quantity",
            "is int 1..10, and the parameter is int 2..10: gate this node by a condition that keeps it inside, \"disabled\": \"!<condition>\"",
        ),
        (
            "a literal too long",
            vec![(
                r#""with": { "code": "{promoCode}" }"#,
                r#""with": { "code": "ABCDEFGHIJKLMNOPQRSTUVWXYZ" }"#,
            )],
            "promo-apply.on.tap.with.code",
            "is string 26..26, and the parameter is string 1..20",
        ),
        (
            "a count not known to be above 0",
            vec![
                (
                    REMOVE_PROMO,
                    r#""removePromo": { "command": "Takes the applied promo code off the order", "params": { "n": "int 1.." } }"#,
                ),
                (
                    CALL_REMOVE_PROMO,
                    r#""on": { "tap": { "call": "removePromo", "with": { "n": "{cart.unavailableCount}" } } }"#,
                ),
            ],
            "promo-applied.on.tap.with.n",
            "is int 0.., and the parameter is int 1..: gate this node by a condition that keeps it inside, \"disabled\": \"!<condition>\", such as one that tests cart.unavailableCount >= 1",
        ),
    ];
    for (name, edits, at, said) in cases {
        let checked = checked(&planted(name, &edits));
        assert!(
            checked.findings.iter().any(|f| f.at == at && f.problem.contains(said)),
            "{name}: wanted {said:?} at {at}, got {:#?}",
            checked.findings
        );
    }
}

#[test]
fn a_condition_that_gates_the_node_narrows_the_value() {
    // (name, edits)
    let cases: [(&str, Edits); 3] = [
        (
            "a literal inside",
            vec![(r#""with": { "code": "{promoCode}" }"#, r#""with": { "code": "SAVE50" }"#)],
        ),
        (
            "a count gated above 0",
            vec![
                (
                    REMOVE_PROMO,
                    r#""removePromo": { "command": "Takes the applied promo code off the order", "params": { "n": "int 1.." } }"#,
                ),
                (
                    CALL_REMOVE_PROMO,
                    r#""disabled": "!hasUnavailable", "on": { "tap": { "call": "removePromo", "with": { "n": "{cart.unavailableCount}" } } }"#,
                ),
            ],
        ),
        (
            "a typed code gated by a condition whose && chain narrows it",
            vec![(
                r#""when": "promoCode != \"\"""#,
                r#""when": "hasAddress && 1 <= cart.unavailableCount && promoCode != \"\"""#,
            )],
        ),
    ];
    for (name, edits) in cases {
        let checked = checked(&planted(name, &edits));
        let calls: Vec<_> = checked.findings.iter().filter(|f| f.at.contains(".with.")).collect();
        assert!(calls.is_empty(), "{name}: {calls:#?}");
    }
}

/// `description` read as a screen of its own, with nothing shared.
fn read(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&read(description))
}
