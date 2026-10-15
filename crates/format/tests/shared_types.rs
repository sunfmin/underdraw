//! A project's types and conditions, view state included, written once in
//! the domain (ADR 0020) and used by each screen that names them: a row
//! type wrapping a domain type with view state is the project's, and a
//! condition on it is held only by a screen that reads it.

use serde_json::{Value, json};
use underdraw_format::{Checked, Screen, Shared};

fn domain() -> Value {
    json!({
        "types": {
            "Note": { "id": "string", "status": "NoteStatus" },
            "NoteStatus": "enum(waiting, taken, queued)",
            "NoteRow": { "note": "Note", "menuOpen": { "type": "bool = false", "view": true } }
        },
        "conditions": {
            "Note.waiting": { "when": "status == \"waiting\"", "label": "Waiting" },
            "Note.taken": { "when": "status == \"taken\"", "label": "Taken" },
            "NoteRow.open": { "when": "menuOpen", "label": "Menu open" }
        }
    })
}

fn row(id: &str, status: &str, open: bool) -> Value {
    json!({ "note": { "id": id, "status": status }, "menuOpen": open })
}

/// A screen listing `rows`, each row drawing `shown`.
fn screen(name: &str, rows: Vec<Value>, shown: Vec<Value>) -> Value {
    json!({
        "screen": name,
        "data": { "rows": "NoteRow[]" },
        "fixtures": { "default": { "rows": rows } },
        "root": { "id": "root", "component": "Screen", "children": [
            { "id": "list", "component": "List", "each": "rows", "item": {
                "id": "row-{index}", "component": "Card", "children": shown } }
        ] }
    })
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&Screen::read(description, Shared { domain: Some(&domain()), components: None }))
}

fn text(id: &str, text: &str) -> Value {
    json!({ "id": id, "component": "Text", "text": text })
}

#[test]
fn several_screens_use_one_row_type_with_view_state() {
    let menu = json!({ "id": "row-{index}-menu", "component": "Text", "text": "Menu", "if": "item.open" });
    let toggle = json!({ "id": "row-{index}-more", "component": "Button", "text": "More",
                         "on": { "tap": { "toggle": "item.menuOpen" } } });
    let state = screen(
        "State",
        vec![row("a", "waiting", true), row("b", "taken", false)],
        vec![text("row-{index}-id", "{item.note.id}"), toggle, menu],
    );
    let checked = check(&state);
    assert!(checked.ok, "{:#?}", checked.findings);
    let names: Vec<&str> = checked.conditions.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["NoteRow.open"], "only what the screen reads is listed");

    // Every note here is waiting and every menu closed: `play` reads none
    // of the conditions, so it is held to none of them.
    let play =
        screen("Play", vec![row("a", "waiting", false)], vec![text("row-{index}-id", "{item.note.id}")]);
    let checked = check(&play);
    assert!(checked.ok, "a screen that reads no condition is not held to it: {:#?}", checked.findings);
    assert!(checked.conditions.is_empty());

    // The same screen reading one is held to it.
    let waiting =
        json!({ "id": "row-{index}-w", "component": "Text", "text": "Waiting", "if": "item.note.waiting" });
    let read = screen("Play", vec![row("a", "waiting", false)], vec![waiting]);
    let checked = check(&read);
    assert!(
        checked.findings.iter().any(|f| f.at == "conditions.Note.waiting"
            && f.problem.contains("is true of every Note in the fixtures")),
        "{:#?}",
        checked.findings
    );
    assert!(!checked.findings.iter().any(|f| f.at == "conditions.Note.taken"), "{:#?}", checked.findings);
}

#[test]
fn a_copy_in_the_screen_is_named_as_shadowing_the_projects() {
    let mut copy =
        screen("State", vec![row("a", "waiting", true)], vec![text("row-{index}-id", "{item.note.id}")]);
    copy["types"] =
        json!({ "NoteRow": { "note": "Note", "menuOpen": { "type": "bool = false", "view": true } } });
    let checked = check(&copy);
    assert!(
        checked
            .findings
            .iter()
            .any(|f| f.at == "types.NoteRow" && f.problem.contains("shadows the domain's NoteRow")),
        "{:#?}",
        checked.findings
    );
}
