//! A tree: a repeat whose rows hold rows of their own type (`"nest"`),
//! each followed by its own at any depth, a level further in; a row folded
//! (`"fold"`) is drawn without its rows, which the fixture still lists.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Checked, Node, Screen, Shared, State};

/// A file tree three levels deep: `src/` holding `main.rs` and `lib/`,
/// which holds `a.rs`; then `README.md`. `lib/` is folded in `folded`.
/// `tree` is merged over the repeating node.
fn files(tree: Value) -> Value {
    let rows = json!([
        { "name": "src/", "children": [
            { "name": "main.rs", "children": [] },
            { "name": "lib/", "children": [ { "name": "a.rs", "children": [] } ] } ] },
        { "name": "README.md", "children": [] }
    ]);
    let mut folded = rows.clone();
    folded[0]["children"][1]["folded"] = json!(true);
    let mut list = json!({ "id": "files", "component": "List", "each": "files",
        "item": { "id": "file-{index}", "component": "Text", "text": "{item.name}" } });
    for (key, value) in tree.as_object().unwrap() {
        list[key] = value.clone();
    }
    json!({
        "screen": "Files",
        "data": { "files": "Entry[]" },
        "types": { "Entry": { "name": "string 1..", "size": "int = 0", "children": "Entry[]",
            "folded": { "type": "bool = false", "view": true }, "open": "bool = false" } },
        "fixtures": { "default": { "files": rows }, "folded": { "files": folded } },
        "root": { "id": "page", "component": "Screen", "children": [list] }
    })
}

fn tree() -> Value {
    json!({ "nest": "children", "fold": "folded", "indent": "space.4" })
}

fn read(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&read(description))
}

/// The rows `fixture` draws: each one's id, text and level.
fn rows(description: &Value, fixture: &str) -> Vec<(String, String, usize)> {
    let page = read(description).instantiate(State::Fixture(fixture), Breakpoint::Compact, None).unwrap();
    let list: &Node = &page.children[0];
    list.children
        .iter()
        .map(|row| (row.id.clone(), row.props["text"].as_str().unwrap().to_string(), row.level))
        .collect()
}

fn row(id: &str, text: &str, level: usize) -> (String, String, usize) {
    (id.to_string(), text.to_string(), level)
}

#[test]
fn each_row_is_followed_by_its_own_a_level_further_in_at_any_depth() {
    let screen = files(tree());
    let checked = check(&screen);
    assert!(checked.ok, "{:#?}", checked.findings);
    assert_eq!(
        rows(&screen, "default"),
        [
            row("file-0", "src/", 0),
            row("file-1", "main.rs", 1),
            row("file-2", "lib/", 1),
            row("file-3", "a.rs", 2),
            row("file-4", "README.md", 0),
        ]
    );
}

#[test]
fn a_folded_row_is_drawn_without_its_rows_and_the_others_keep_their_ids() {
    assert_eq!(
        rows(&files(tree()), "folded"),
        [
            row("file-0", "src/", 0),
            row("file-1", "main.rs", 1),
            row("file-2", "lib/", 1),
            row("file-4", "README.md", 0)
        ]
    );
}

#[test]
fn without_nest_a_list_stays_flat() {
    assert_eq!(rows(&files(json!({})), "default"), [row("file-0", "src/", 0), row("file-1", "README.md", 0)]);
}

#[test]
fn limit_counts_the_rows_at_the_top() {
    let mut limited = tree();
    limited["limit"] = json!(1);
    let shown: Vec<String> = rows(&files(limited), "default").into_iter().map(|(_, text, _)| text).collect();
    assert_eq!(shown, ["src/", "main.rs", "lib/", "a.rs"]);
}

#[test]
fn check_names_a_tree_written_wrong() {
    // (what the repeating node says, where the finding is, what it says)
    let cases = [
        (
            json!({ "nest": "name" }),
            "files.nest",
            "a list of its own type (Entry[]), such as \"children\": it is string",
        ),
        (json!({ "nest": "kids" }), "files.nest", "Entry has no field kids"),
        (json!({ "nest": "children", "fold": "open" }), "files.fold", "a bool in view state"),
        (json!({ "nest": "children", "fold": "size" }), "files.fold", "a bool in view state"),
        (json!({ "fold": "folded" }), "files.fold", "this node has no \"nest\""),
        (json!({ "indent": 12 }), "files.indent", "this node has no \"nest\""),
        (json!({ "nest": "children", "indent": -4 }), "files.indent", "a space token"),
    ];
    for (written, at, said) in cases {
        let checked = check(&files(written.clone()));
        assert!(
            checked.findings.iter().any(|f| f.at == at && f.problem.contains(said)),
            "{written}: {:#?}",
            checked.findings
        );
    }
}

#[test]
fn check_names_nest_on_a_node_that_does_not_repeat() {
    let mut screen = files(tree());
    screen["root"]["children"][0]["children"] =
        json!([{ "id": "lone", "component": "List", "nest": "children" }]);
    let checked = check(&screen);
    assert!(
        checked.findings.iter().any(|f| f.at == "lone.nest" && f.problem.contains("has no \"each\"")),
        "{:#?}",
        checked.findings
    );
}
