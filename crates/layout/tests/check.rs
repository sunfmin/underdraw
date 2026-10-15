//! `check` with the layout in it, on the menu draft: an overlay the picture
//! would cut is found in the state and at the width it is cut.

use serde_json::{Value, json};
use underdraw_format::{Checked, Screen, ScreenParams, Shared};

mod fixed;
use fixed::Fixed;

const MENU: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/menu");

fn read(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{MENU}/{name}")).unwrap()).unwrap()
}

#[test]
fn the_menu_draft_fits_its_sheet_in_every_state() {
    let checked = check(&read("screen.json"), &read("tokens.json"), &none(), &mut Fixed).unwrap();
    assert!(checked.ok, "{:#?}", checked.findings);
}

#[test]
fn an_overlay_taller_than_what_it_floats_over_is_found() {
    let mut screen = read("screen.json");
    // Behind the sheet, one dish and no cart bar: the page is shorter than the sheet.
    screen["fixtures"]["sheet-adding"]["sections"] = json!([{ "id": "sides", "title": "小食", "dishes": [
        { "id": "d4", "name": "煎饺", "description": "", "price": { "amount": 450, "currency": "JPY" } } ] }]);
    let checked = check(&screen, &read("tokens.json"), &none(), &mut Fixed).unwrap();
    assert!(!checked.ok);
    assert!(checked.conditions.is_empty(), "no condition table beside a finding");
    let problems: Vec<&str> =
        checked.findings.iter().filter(|f| f.at == "dish-sheet").map(|f| f.problem.as_str()).collect();
    assert!(
        problems.iter().any(|p| p.contains("sheet-adding") && p.contains("390") && p.contains("cut")),
        "{problems:#?}"
    );
    assert!(problems.iter().all(|p| p.contains("sheet-adding")), "only the short fixture: {problems:#?}");
}

#[test]
fn the_widths_tokens_name_are_the_widths_checked() {
    let mut screen = read("screen.json");
    screen["fixtures"]["sheet-adding"]["sections"] = json!([{ "id": "sides", "title": "小食", "dishes": [
        { "id": "d4", "name": "煎饺", "description": "", "price": { "amount": 450, "currency": "JPY" } } ] }]);
    let mut tokens = read("tokens.json");
    tokens["widths"] = json!([500]);
    let checked = check(&screen, &tokens, &none(), &mut Fixed).unwrap();
    let problems: Vec<&str> =
        checked.findings.iter().filter(|f| f.at == "dish-sheet").map(|f| f.problem.as_str()).collect();
    assert!(!problems.is_empty(), "cut at the width named: {:#?}", checked.findings);
    assert!(problems.iter().all(|p| p.contains("width 500")), "only at 500: {problems:#?}");
}

#[test]
fn widths_that_cannot_be_used_are_a_fault() {
    for widths in [json!([]), json!("390"), json!([390, 100]), json!([390.5])] {
        let mut tokens = read("tokens.json");
        tokens["widths"] = widths.clone();
        let checked = check(&read("screen.json"), &tokens, &none(), &mut Fixed).unwrap();
        assert!(!checked.ok, "{widths}");
        assert!(
            checked
                .findings
                .iter()
                .any(|f| f.at == "tokens.json widths" && f.problem.contains("[390, 1440]")),
            "{widths}: {:#?}",
            checked.findings
        );
    }
}

#[test]
fn without_widths_the_defaults_are_used() {
    assert_eq!(underdraw_layout::widths(&json!({})), [390, 1120]);
    assert_eq!(underdraw_layout::widths(&json!({ "widths": [390, 1440] })), [390, 1440]);
}

/// No other screen: the menu alone.
fn none() -> ScreenParams {
    ScreenParams::new()
}

/// `underdraw_layout::check` of the description `screen`, read as a
/// screen of its own.
fn check(
    screen: &Value,
    tokens: &Value,
    others: &ScreenParams,
    measure: &mut Fixed,
) -> Result<Checked, String> {
    underdraw_layout::check(
        &Screen::read(screen, Shared::default()),
        tokens,
        others,
        measure,
        &underdraw_layout::NoPictures,
    )
}
