//! A look computes a length from its int params and tokens
//! (`{depth} * space.4 + 8`): drawn at what it comes to, and `check` proves
//! it in range from the params' bounds.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Checked, Lang, Screen, ScreenParams, Shared, State};
use underdraw_layout::{Page, layout};

mod fixed;
use fixed::Fixed;

/// A screen of two `Line`s, at depth 0 and 2, whose look's `key` is
/// `length`, the param typed `depth`.
fn screen(depth: &str, key: &str, length: Value) -> Value {
    let mut look = json!({ "box": "row", "children": [] });
    look[key] = length;
    json!({
        "screen": "Lines",
        "data": { "title": "string" },
        "fixtures": { "default": { "title": "a" } },
        "components": { "Line": { "params": { "depth": depth }, "look": look } },
        "root": { "id": "page", "component": "Screen", "children": [
            { "id": "top", "component": "Line", "depth": 0 },
            { "id": "deep", "component": "Line", "depth": 2 } ] }
    })
}

fn tokens() -> Value {
    json!({ "space": { "4": 16 } })
}

fn page(description: &Value) -> Page {
    let screen = Screen::read(description, Shared::default());
    let root = screen.instantiate(State::Fixture("default"), Breakpoint::Compact, None).unwrap();
    layout(&root, &tokens(), &screen, 390.0, Lang::default(), &mut Fixed, &underdraw_layout::NoPictures)
        .unwrap()
}

/// The width and height of the node `id`.
fn size(page: &Page, id: &str) -> (f32, f32) {
    let rect = page.nodes.iter().find(|n| n.id == id).unwrap().rect;
    (rect.width, rect.height)
}

fn check(description: &Value) -> Checked {
    underdraw_layout::check(
        &Screen::read(description, Shared::default()),
        &tokens(),
        &ScreenParams::new(),
        &mut Fixed,
        &underdraw_layout::NoPictures,
    )
    .unwrap()
}

#[test]
fn a_size_is_drawn_at_the_length_it_computes() {
    let page = page(&screen("int 0..4", "size", json!("{depth} * space.4 + 8")));
    assert_eq!(size(&page, "top"), (8.0, 8.0));
    assert_eq!(size(&page, "deep"), (40.0, 40.0));
}

#[test]
fn a_side_of_padding_is_drawn_at_the_length_it_computes() {
    let page = page(&screen("int 0..4", "padding", json!([0, 0, "{depth} * 5", "{depth} × space.4"])));
    assert_eq!(size(&page, "deep").1, 10.0, "the bottom side, 2 × 5");
}

#[test]
fn a_length_in_range_checks_clean() {
    let checked = check(&screen("int 0..4", "size", json!("{depth} * space.4 + 8")));
    assert!(checked.ok, "{:#?}", checked.findings);
}

#[test]
fn a_length_that_can_pass_the_most_is_a_finding() {
    let checked = check(&screen("int 0..1000", "size", json!("{depth} * space.4")));
    let at = "components.Line.look.size";
    assert!(
        checked.findings.iter().any(|f| f.at == at && f.problem.contains("comes to 0..16000 points")),
        "{:#?}",
        checked.findings
    );
}

#[test]
fn a_length_that_can_fall_below_nothing_is_a_finding() {
    let checked = check(&screen("int -1..4", "size", json!("{depth} * space.4")));
    assert!(
        checked.findings.iter().any(|f| f.problem.contains("comes to -16..64")),
        "{:#?}",
        checked.findings
    );
}

#[test]
fn a_token_tokens_json_does_not_name_is_a_finding() {
    let checked = check(&screen("int 0..4", "gap", json!("{depth} * space.9")));
    assert!(
        checked.findings.iter().any(|f| f.at == "components.Line.look.gap" && f.problem.contains("space.9")),
        "{:#?}",
        checked.findings
    );
}
