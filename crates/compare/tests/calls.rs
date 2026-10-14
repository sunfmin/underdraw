//! Compares the documents draft against its hand-written captures on the
//! web, of the `row-menu-own` state (a row's menu open) and of the
//! `confirm-delete` state (the delete confirmation open): in each, one whose
//! interactions call what the description says, and one that gets each call
//! wrong a different way.

use serde_json::{Value, json};
use underdraw_compare::{Comparison, compare};
use underdraw_format::State;
use underdraw_layout::{Font, Measure, Size};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/docs");

/// Every character half the font size wide: the captures have no frames, so
/// nothing here depends on it.
struct Fixed;

impl Measure for Fixed {
    fn text(&mut self, text: &str, font: Font, _max_width: Option<f32>) -> Size {
        Size { width: text.chars().count() as f32 * font.size / 2.0, height: font.line_height }
    }
}

fn read(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{DRAFT}/{name}")).unwrap()).unwrap()
}

fn run(capture: &Value) -> Result<Comparison, String> {
    run_in("row-menu-own", capture)
}

fn run_in(fixture: &str, capture: &Value) -> Result<Comparison, String> {
    let (screen, tokens, kinds) = (read("screen.json"), read("tokens.json"), read("kinds.json"));
    let mapping = read("platforms/web.json");
    compare(&screen, &mapping, &tokens, Some(&kinds), State::Fixture(fixture), capture, &mut Fixed)
}

#[test]
fn calls_made_as_described_have_no_findings() {
    let comparison = run(&read("captures/web-row-menu-own.json")).unwrap();
    assert!(comparison.ok, "{:#?}", comparison.findings);
    assert_eq!(comparison.checked.calls, 3);
    let comparison = run_in("confirm-delete", &read("captures/web-confirm-delete.json")).unwrap();
    assert!(comparison.ok, "{:#?}", comparison.findings);
    assert_eq!(comparison.checked.calls, 2);
}

#[test]
fn deleting_from_the_menu_without_confirming_a_missing_call_and_an_extra_one() {
    let comparison = run(&read("captures/web-row-menu-own-wrong.json")).unwrap();
    let found: Vec<_> = comparison.findings.iter().map(|f| (f.at.as_str(), f.check)).collect();
    assert_eq!(
        found,
        [("doc-c-delete", "call-extra"), ("nav-new", "call-missing"), ("doc-c-copy", "call-extra")],
        "{:#?}",
        comparison.findings
    );
}

#[test]
fn a_call_with_other_arguments_and_cancel_that_deletes() {
    let comparison = run_in("confirm-delete", &read("captures/web-confirm-delete-wrong.json")).unwrap();
    let found: Vec<_> = comparison.findings.iter().map(|f| (f.at.as_str(), f.check)).collect();
    assert_eq!(
        found,
        [("confirm-delete-confirm", "call-with"), ("confirm-delete-cancel", "call-extra")],
        "{:#?}",
        comparison.findings
    );
    assert!(comparison.findings[0].problem.contains(r#"{"id":"c"}"#), "{}", comparison.findings[0].problem);
}

#[test]
fn a_call_on_another_event_is_not_the_tap_one() {
    let mut capture = read("captures/web-row-menu-own.json");
    capture["acted"] = json!([
        { "id": "nav-new", "on": "longPress", "calls": [] },
        { "id": "nav-new", "calls": [{ "operation": "newDocument", "with": {} }] }
    ]);
    let comparison = run(&capture).unwrap();
    assert!(comparison.ok, "the event defaults to tap: {:#?}", comparison.findings);
}

#[test]
fn acted_that_is_not_written_as_the_help_says_is_refused() {
    let mut capture = read("captures/web-row-menu-own.json");
    capture["acted"] = json!([
        { "on": "tap", "calls": [] },
        { "id": "nav-new" },
        { "id": "nav-new", "calls": [{ "with": {} }] },
        { "id": "nav-new", "calls": [{ "operation": "newDocument", "with": "c" }] }
    ]);
    let refused = run(&capture).err().unwrap();
    assert!(refused.contains("acted[0]: needs \"id\""), "{refused}");
    assert!(refused.contains("acted[1]: nav-new: needs \"calls\""), "{refused}");
    assert!(refused.contains("acted[2]: nav-new: each call needs \"operation\""), "{refused}");
    assert!(refused.contains("acted[3]: nav-new: a call's \"with\""), "{refused}");
}
