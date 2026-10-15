//! `check` with the layout in it, on the menu draft: an overlay the picture
//! would cut is found in the state and at the width it is cut.

use serde_json::{Value, json};
use underdraw_format::ScreenParams;
use underdraw_layout::check;

mod fixed;
use fixed::Fixed;

const MENU: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/menu");

fn read(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{MENU}/{name}")).unwrap()).unwrap()
}

#[test]
fn the_menu_draft_fits_its_sheet_in_every_state() {
    let checked =
        check(&read("screen.json"), &read("tokens.json"), Some(&read("kinds.json")), &none(), &mut Fixed)
            .unwrap();
    assert!(checked.ok, "{:#?}", checked.findings);
}

#[test]
fn an_overlay_taller_than_what_it_floats_over_is_found() {
    let mut screen = read("screen.json");
    // Behind the sheet, one dish and no cart bar: the page is shorter than the sheet.
    screen["fixtures"]["sheet-adding"]["sections"] = json!([{ "id": "sides", "title": "小食", "dishes": [
        { "id": "d4", "name": "煎饺", "description": "", "price": { "amount": 450, "currency": "JPY" } } ] }]);
    let checked =
        check(&screen, &read("tokens.json"), Some(&read("kinds.json")), &none(), &mut Fixed).unwrap();
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

/// No other screen: the menu alone.
fn none() -> ScreenParams {
    ScreenParams::new()
}
