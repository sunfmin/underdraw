//! Runs the real binary over a copy of the orders draft holding notes as
//! the gallery writes them, and reads its JSON and exit code.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const ORDERS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/orders");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// A root of its own named `name`, holding a copy of the orders draft at
/// `orders/` with two open notes, as the gallery writes them.
fn root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("underdraw-notes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("orders");
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["screen.json", "tokens.json", "kinds.json"] {
        std::fs::copy(Path::new(ORDERS).join(file), dir.join(file)).unwrap();
    }
    let note = |id: &str, says: &str| {
        json!({
            "id": id,
            "status": "open",
            "state": { "fixture": "default" },
            "node": { "id": "order-{index}-cancel", "at": "/root/children/1/item/children/1", "instance": [2], "label": 7, "width": 390 },
            "thread": [{ "by": "person", "says": says }],
        })
    };
    let notes = json!({ "notes": [note("N1", "too red"), note("N2", "say what it cancels")] });
    std::fs::write(dir.join("notes.json"), notes.to_string()).unwrap();
    root.canonicalize().unwrap()
}

#[test]
fn notes_lists_the_open_ones_under_a_root_and_answer_marks_one_answered_once() {
    let root = root("answer");
    let dir = root.join("orders");
    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!((&answer["open"], &answer["answered"]), (&json!(2), &json!(0)));
    let first = &answer["notes"][0];
    assert_eq!((&first["description"], &first["id"]), (&json!("orders"), &json!("N1")));
    assert_eq!(first["node"]["id"], "order-{index}-cancel");
    assert_eq!(
        first["reference"],
        format!(
            "{} · fixture default · order-{{index}}-cancel at /root/children/1/item/children/1 in screen.json · pointed at [2], label 7, width 390",
            dir.display()
        ),
        "the same line ⌘C copies"
    );

    let (answered, code) =
        underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N1", "--reply", "made it gray"]);
    assert_eq!(code, 0, "{answered}");
    assert_eq!((&answered["changed"], &answered["note"]["status"]), (&json!(true), &json!("answered")));
    assert_eq!(answered["note"]["thread"][1], json!({ "by": "agent", "says": "made it gray" }));
    let file = std::fs::read_to_string(dir.join("notes.json")).unwrap();
    let (again, code) =
        underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N1", "--reply", "made it gray"]);
    assert_eq!((code, &again["changed"]), (0, &json!(false)), "{again}");
    assert_eq!(std::fs::read_to_string(dir.join("notes.json")).unwrap(), file, "twice changes nothing more");

    let (answer, _) = underdraw(&["notes", root.to_str().unwrap()]);
    let ids: Vec<&Value> = answer["notes"].as_array().unwrap().iter().map(|note| &note["id"]).collect();
    assert_eq!((ids, &answer["answered"]), (vec![&json!("N2")], &json!(1)), "only the open ones listed");

    let (refused, code) = underdraw(&["notes", root.to_str().unwrap(), "--answer", "N2", "--reply", "x"]);
    assert_eq!(code, 2);
    assert!(refused["error"].as_str().unwrap().contains("the directory of the description"), "{refused}");
    let (refused, code) = underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N9", "--reply", "x"]);
    assert_eq!(code, 2);
    assert!(refused["error"].as_str().unwrap().contains("no note N9"), "{refused}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_agent_cannot_create_close_or_reopen_a_note() {
    let root = root("refused");
    let dir = root.join("orders");
    let file = std::fs::read_to_string(dir.join("notes.json")).unwrap();
    for args in [vec!["--create"], vec!["--add", "the price"], vec!["--close", "N1"], vec!["--reopen", "N1"]]
    {
        let mut all = vec!["notes", dir.to_str().unwrap()];
        all.extend(&args);
        let (answer, code) = underdraw(&all);
        assert_eq!(code, 2, "{args:?}: {answer}");
        assert!(
            answer["error"].as_str().unwrap().contains("the person does that in the gallery"),
            "{args:?}: {answer}"
        );
    }
    assert_eq!(std::fs::read_to_string(dir.join("notes.json")).unwrap(), file, "nothing written");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_counts_the_notes_and_exits_as_it_would_without_them() {
    let root = root("check");
    let dir = root.join("orders");
    underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N2", "--reply", "it says the order now"]);
    let (one, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!((code, &one["ok"]), (0, &json!(true)), "open notes are no fault: {one}");
    assert_eq!(one["notes"], json!({ "open": 1, "answered": 1 }));
    let (all, code) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{all}");
    assert_eq!(all["notes"], json!({ "open": 1, "answered": 1 }));
    assert_eq!(all["descriptions"][0]["notes"], json!({ "open": 1, "answered": 1 }));
    let (none, _) = underdraw(&["check", ORDERS]);
    assert_eq!(none["notes"], json!({ "open": 0, "answered": 0 }), "no notes.json, no notes");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn notes_lists_a_region_note_with_its_box_and_the_nodes_it_covered() {
    let root = root("region");
    let dir = root.join("orders");
    let region = json!({
        "id": "N3",
        "status": "open",
        "state": { "fixture": "default" },
        "region": {
            "box": { "x": 10, "y": 145, "width": 370, "height": 30 },
            "width": 390,
            "nodes": [{ "id": "match-count", "at": "/root/children/1/children/2", "instance": [] }],
        },
        "thread": [{ "by": "person", "says": "too much air" }],
    });
    let mut notes: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("notes.json")).unwrap()).unwrap();
    notes["notes"].as_array_mut().unwrap().push(region.clone());
    std::fs::write(dir.join("notes.json"), notes.to_string()).unwrap();
    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{answer}");
    let listed = &answer["notes"][2];
    assert_eq!((&listed["id"], &listed["region"]), (&json!("N3"), &region["region"]), "its box and nodes");
    assert!(listed.get("node").is_none(), "{listed}");
    assert_eq!(
        listed["reference"],
        format!(
            "{} · fixture default · region 10, 145 · 370 × 30 at width 390 · covering match-count at /root/children/1/children/2",
            dir.display()
        )
    );
    let (answered, code) =
        underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N3", "--reply", "tightened it"]);
    assert_eq!((code, &answered["note"]["status"]), (0, &json!("answered")), "{answered}");
    std::fs::remove_dir_all(root).unwrap();
}
