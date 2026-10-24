//! A tree's rows are drawn one step in per level, the step written once on
//! the repeating node, whatever component draws each row.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Lang, Screen, Shared, State};
use underdraw_layout::{Page, layout};

mod fixed;
use fixed::Fixed;

/// Three levels of rows; a row with `"note"` is drawn as a Card holding
/// its text, any other as Text. `indent` is the repeating node's, if any.
fn page(indent: Option<Value>) -> Page {
    let mut list = json!({ "id": "rows", "component": "List", "gap": 0, "each": "rows", "nest": "children",
        "item": { "id": "row-{index}", "component": "Stack", "children": [
            { "id": "row-{index}-text", "component": "Text", "text": "{item.name}", "if": "!item.note" },
            { "id": "row-{index}-card", "component": "Card", "if": "item.note", "children": [
                { "id": "row-{index}-said", "component": "Text", "text": "{item.name}" } ] } ] } });
    if let Some(indent) = indent {
        list["indent"] = indent;
    }
    let screen = json!({
        "screen": "Tree",
        "data": { "rows": "Line[]" },
        "types": { "Line": { "name": "string", "note": "bool = false", "children": "Line[]" } },
        "fixtures": { "default": { "rows": [
            { "name": "a", "children": [
                { "name": "b", "children": [ { "name": "c", "note": true, "children": [] } ] } ] },
            { "name": "d", "note": true, "children": [] } ] } },
        "root": { "id": "page", "component": "Screen", "children": [list] }
    });
    let tokens = json!({ "space": { "3": 12 } });
    let screen = Screen::read(&screen, Shared::default());
    let root = screen.instantiate(State::Fixture("default"), Breakpoint::Compact, None).unwrap();
    layout(&root, &tokens, &screen, 390.0, Lang::default(), &mut Fixed, &underdraw_layout::NoPictures)
        .unwrap()
}

/// How far in each row is from the list, by its id.
fn insets(page: &Page) -> Vec<(String, f32)> {
    let list = page.nodes.iter().find(|n| n.id == "rows").unwrap().rect.x;
    page.nodes
        .iter()
        .filter(|n| n.id.starts_with("row-") && n.id.matches('-').count() == 1)
        .map(|n| (n.id.clone(), n.rect.x - list))
        .collect()
}

fn inset(id: &str, x: f32) -> (String, f32) {
    (id.to_string(), x)
}

#[test]
fn each_level_is_one_step_in_by_the_token_the_tree_names() {
    assert_eq!(
        insets(&page(Some(json!("space.3")))),
        [inset("row-0", 0.0), inset("row-1", 12.0), inset("row-2", 24.0), inset("row-3", 0.0)]
    );
}

#[test]
fn a_tree_that_names_no_step_steps_16_points() {
    assert_eq!(
        insets(&page(None)),
        [inset("row-0", 0.0), inset("row-1", 16.0), inset("row-2", 32.0), inset("row-3", 0.0)]
    );
}
