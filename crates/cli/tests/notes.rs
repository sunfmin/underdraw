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
/// `orders/` with two notes waiting on the agent, as the gallery writes them.
fn root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("underdraw-notes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("orders");
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["screen.json", "tokens.json"] {
        std::fs::copy(Path::new(ORDERS).join(file), dir.join(file)).unwrap();
    }
    let note = |id: &str, says: &str| {
        json!({
            "id": id,
            "state": { "fixture": "default" },
            "node": { "id": "order-{index}-cancel", "at": "/root/children/1/item/children/1", "instance": [2], "label": 7, "width": 390 },
            "messages": [{ "by": "person", "says": says }],
        })
    };
    let notes = json!({ "notes": [note("N1", "too red"), note("N2", "say what it cancels")] });
    std::fs::write(dir.join("notes.json"), notes.to_string()).unwrap();
    root.canonicalize().unwrap()
}

#[test]
fn notes_lists_the_waiting_ones_under_a_root_and_answer_adds_the_agents_message_once() {
    let root = root("answer");
    let dir = root.join("orders");
    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!((&answer["waiting"], &answer["answered"]), (&json!(2), &json!(0)));
    assert_eq!(answer["notes"][0]["stands"], "waiting");
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
    assert_eq!((&answered["changed"], &answered["note"]["stands"]), (&json!(true), &json!("unread")));
    assert_eq!(
        answered["note"]["messages"][1],
        json!({ "by": "agent", "says": "made it gray", "unread": true }),
        "unread until the person has seen it"
    );
    let file = std::fs::read_to_string(dir.join("notes.json")).unwrap();
    let (again, code) =
        underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N1", "--reply", "made it gray"]);
    assert_eq!((code, &again["changed"]), (0, &json!(false)), "{again}");
    assert_eq!(std::fs::read_to_string(dir.join("notes.json")).unwrap(), file, "twice changes nothing more");

    let (answer, _) = underdraw(&["notes", root.to_str().unwrap()]);
    let ids: Vec<&Value> = answer["notes"].as_array().unwrap().iter().map(|note| &note["id"]).collect();
    assert_eq!((ids, &answer["answered"]), (vec![&json!("N2")], &json!(1)), "only the waiting ones listed");

    let (refused, code) = underdraw(&["notes", root.to_str().unwrap(), "--answer", "N2", "--reply", "x"]);
    assert_eq!(code, 2);
    assert!(refused["error"].as_str().unwrap().contains("the directory the note is in"), "{refused}");
    let (refused, code) = underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N9", "--reply", "x"]);
    assert_eq!(code, 2);
    assert!(refused["error"].as_str().unwrap().contains("no note N9"), "{refused}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn answer_keeps_a_reply_of_many_lines_intact() {
    let root = root("lines");
    let dir = root.join("orders");
    let reply = "Made it gray.\n\nTwo things to ask:\n  1. gray at 1120 too?\n  2. keep the icon?";
    let (answered, code) = underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N1", "--reply", reply]);
    assert_eq!(code, 0, "{answered}");
    assert_eq!(answered["note"]["messages"][1]["says"], reply);
    let file: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("notes.json")).unwrap()).unwrap();
    assert_eq!(
        file["notes"][0]["messages"][1],
        json!({ "by": "agent", "says": reply, "unread": true }),
        "stored as given"
    );
    for key in ["status", "thread", "closed_because"] {
        assert!(file["notes"][0].get(key).is_none(), "{key} is not stored: {file}");
    }
    let (more, _) = underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N1", "--reply", "and one more"]);
    assert_eq!(more["note"]["messages"].as_array().unwrap().len(), 3, "the agent may say more: {more}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_agent_cannot_start_a_note_and_nothing_closes_or_reopens_one() {
    let root = root("refused");
    let dir = root.join("orders");
    let file = std::fs::read_to_string(dir.join("notes.json")).unwrap();
    for args in [vec!["--create"], vec!["--add", "the price"]] {
        let mut all = vec!["notes", dir.to_str().unwrap()];
        all.extend(&args);
        let (answer, code) = underdraw(&all);
        assert_eq!(code, 2, "{args:?}: {answer}");
        assert!(
            answer["error"].as_str().unwrap().contains("the person does that in the gallery"),
            "{args:?}: {answer}"
        );
    }
    for args in [vec!["--close", "N1"], vec!["--reopen", "N1"], vec!["--closed"]] {
        let mut all = vec!["notes", dir.to_str().unwrap()];
        all.extend(&args);
        let (answer, code) = underdraw(&all);
        assert_eq!(code, 2, "{args:?}: {answer}");
        assert!(answer["error"].as_str().unwrap().contains("unexpected argument"), "{args:?}: {answer}");
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
    assert_eq!((code, &one["ok"]), (0, &json!(true)), "notes waiting are no fault: {one}");
    assert_eq!(one["notes"], json!({ "waiting": 1, "answered": 1 }));
    let (all, code) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{all}");
    assert_eq!(all["notes"], json!({ "waiting": 1, "answered": 1 }));
    assert_eq!(all["descriptions"][0]["notes"], json!({ "waiting": 1, "answered": 1 }));
    let (none, _) = underdraw(&["check", ORDERS]);
    assert_eq!(none["notes"], json!({ "waiting": 0, "answered": 0 }), "no notes.json, no notes");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_note_written_on_a_region_before_regions_went_is_listed_on_its_whole_state() {
    let root = root("region");
    let dir = root.join("orders");
    let region = json!({
        "id": "N3",
        "state": { "fixture": "default" },
        "region": {
            "box": { "x": 10, "y": 145, "width": 370, "height": 30 },
            "width": 390,
            "nodes": [{ "id": "match-count", "at": "/root/children/1/children/2", "instance": [] }],
        },
        "messages": [{ "by": "person", "says": "too much air" }],
    });
    let mut notes: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("notes.json")).unwrap()).unwrap();
    notes["notes"].as_array_mut().unwrap().push(region);
    std::fs::write(dir.join("notes.json"), notes.to_string()).unwrap();
    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{answer}");
    let listed = &answer["notes"][2];
    assert_eq!(listed["id"], json!("N3"));
    assert!(listed.get("region").is_none() && listed.get("node").is_none(), "{listed}");
    assert_eq!(listed["reference"], format!("{} · fixture default", dir.display()), "on the whole state");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_note_on_the_whole_screen_is_listed_answered_and_counted() {
    let root = root("screen");
    let dir = root.join("orders");
    let screen = json!({
        "id": "N3",
        "whole": "screen",
        "messages": [{ "by": "person", "says": "this screen tries to do too much" }],
    });
    let mut notes: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("notes.json")).unwrap()).unwrap();
    notes["notes"].as_array_mut().unwrap().push(screen);
    std::fs::write(dir.join("notes.json"), notes.to_string()).unwrap();
    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{answer}");
    let listed = &answer["notes"][2];
    assert_eq!((&listed["id"], &listed["whole"]), (&json!("N3"), &json!("screen")), "{listed}");
    assert!(listed.get("state").is_none(), "apart from any one state: {listed}");
    assert_eq!(listed["reference"], format!("{} · the whole screen", dir.display()));
    let (counted, _) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(counted["notes"], json!({ "waiting": 3, "answered": 0 }));

    let (answered, code) =
        underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N3", "--reply", "split it in two"]);
    assert_eq!((code, &answered["note"]["stands"]), (0, &json!("unread")), "{answered}");
    assert_eq!(answered["note"]["whole"], "screen");
    let (counted, _) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(counted["notes"], json!({ "waiting": 2, "answered": 1 }));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_note_on_a_directory_is_listed_answered_and_counted() {
    let root = root("screens");
    let shop = root.join("shop");
    std::fs::create_dir_all(&shop).unwrap();
    std::fs::rename(root.join("orders"), shop.join("orders")).unwrap();
    let screens = |id: &str, says: &str| json!({ "notes": [{ "id": id, "whole": "screens", "messages": [{ "by": "person", "says": says }] }] });
    std::fs::write(shop.join("notes.json"), screens("N1", "these repeat a header").to_string()).unwrap();
    std::fs::write(root.join("notes.json"), screens("N1", "the app needs a tab bar").to_string()).unwrap();

    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!((code, &answer["waiting"]), (0, &json!(4)), "{answer}");
    let listed: Vec<(&Value, &Value, &Value)> = answer["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| (&note["description"], &note["id"], &note["whole"]))
        .collect();
    assert_eq!(
        listed,
        [
            (&json!("."), &json!("N1"), &json!("screens")),
            (&json!("shop"), &json!("N1"), &json!("screens")),
            (&json!("shop/orders"), &json!("N1"), &Value::Null),
            (&json!("shop/orders"), &json!("N2"), &Value::Null),
        ],
        "the root's, then in the tree's order"
    );
    assert_eq!(answer["notes"][1]["reference"], format!("{} · these screens", shop.display()));
    let (counted, _) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(counted["notes"], json!({ "waiting": 4, "answered": 0 }), "{counted}");
    assert_eq!(
        counted["directories"],
        json!([
            { "directory": ".", "notes": { "waiting": 1, "answered": 0 } },
            { "directory": "shop", "notes": { "waiting": 1, "answered": 0 } },
        ])
    );

    let (answered, code) =
        underdraw(&["notes", shop.to_str().unwrap(), "--answer", "N1", "--reply", "one header now"]);
    assert_eq!((code, &answered["note"]["stands"]), (0, &json!("unread")), "{answered}");
    assert_eq!(answered["note"]["whole"], "screens");
    let (answered, code) =
        underdraw(&["notes", root.to_str().unwrap(), "--answer", "N1", "--reply", "added one"]);
    assert_eq!((code, &answered["note"]["stands"]), (0, &json!("unread")), "the root's: {answered}");
    let (counted, _) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(counted["notes"], json!({ "waiting": 2, "answered": 2 }));

    let empty = root.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let (refused, code) = underdraw(&["notes", empty.to_str().unwrap(), "--answer", "N1", "--reply", "x"]);
    assert_eq!(code, 2, "{refused}");
    assert!(refused["error"].as_str().unwrap().contains("keeps no notes.json"), "{refused}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_note_on_a_journey_is_listed_answered_and_counted() {
    let root = root("journey");
    let shop = root.join("shop");
    std::fs::create_dir_all(&shop).unwrap();
    std::fs::rename(root.join("orders"), shop.join("orders")).unwrap();
    let journey = json!({ "notes": [{ "id": "N1", "journey": "cancel-an-order", "messages": [{ "by": "person", "says": "cancelling takes four steps; too many" }] }] });
    std::fs::write(shop.join("notes.json"), journey.to_string()).unwrap();

    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!((code, &answer["waiting"]), (0, &json!(3)), "{answer}");
    let note = &answer["notes"][0];
    assert_eq!((&note["description"], &note["journey"]), (&json!("shop"), &json!("cancel-an-order")), "{answer}");
    assert_eq!(note["reference"], format!("{} · journey cancel-an-order", shop.display()));
    let (counted, _) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(counted["notes"], json!({ "waiting": 3, "answered": 0 }), "{counted}");
    assert_eq!(counted["directories"], json!([{ "directory": "shop", "notes": { "waiting": 1, "answered": 0 } }]));

    let (answered, code) =
        underdraw(&["notes", shop.to_str().unwrap(), "--answer", "N1", "--reply", "two steps now"]);
    assert_eq!((code, &answered["note"]["stands"]), (0, &json!("unread")), "{answered}");
    assert_eq!(answered["note"]["journey"], "cancel-an-order");
    let (counted, _) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(counted["notes"], json!({ "waiting": 2, "answered": 1 }));
    std::fs::remove_dir_all(root).unwrap();
}

/// `underdraw notes <root> --wait` started in the background.
fn waiting(root: &Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_underdraw"))
        .args(["notes", root.to_str().unwrap(), "--wait"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap()
}

/// Whether `child` is still waiting after `millis`.
fn still_waiting(child: &mut std::process::Child, millis: u64) -> bool {
    std::thread::sleep(std::time::Duration::from_millis(millis));
    child.try_wait().unwrap().is_none()
}

/// What `child` answered once it exited, within ten seconds.
fn answered(mut child: std::process::Child) -> (Value, i32) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        assert!(std::time::Instant::now() < deadline, "--wait did not answer within 10s");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// The orders notes.json under `root` with `by` having said `says` last in
/// note `id`, as the gallery writes the person's and `--answer` the agent's.
fn say(root: &Path, id: &str, by: &str, says: &str) {
    let file = root.join("orders/notes.json");
    let mut notes: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    for note in notes["notes"].as_array_mut().unwrap() {
        if note["id"] == id {
            note["messages"].as_array_mut().unwrap().push(json!({ "by": by, "says": says }));
        }
    }
    // Put in place whole, as the gallery does, so --wait never reads half.
    let written = file.with_extension("json.new");
    std::fs::write(&written, notes.to_string()).unwrap();
    std::fs::rename(written, file).unwrap();
}

#[test]
fn wait_answers_as_soon_as_the_person_spoke_last_in_a_note() {
    let root = root("wait");
    say(&root, "N1", "agent", "made it gray");
    say(&root, "N2", "agent", "it says the order now");
    let mut child = waiting(&root);
    assert!(still_waiting(&mut child, 600), "the agent spoke last in every note: it waits");
    say(&root, "N1", "agent", "anything else?");
    assert!(still_waiting(&mut child, 600), "the agent saying more wakes nothing");
    say(&root, "N2", "person", "and the date");
    let (answer, code) = answered(child);
    assert_eq!(code, 0, "{answer}");
    let ids: Vec<&Value> = answer["notes"].as_array().unwrap().iter().map(|note| &note["id"]).collect();
    assert_eq!((ids, &answer["waiting"], &answer["answered"]), (vec![&json!("N2")], &json!(1), &json!(1)));
    let messages = answer["notes"][0]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3, "the whole conversation: {answer}");
    assert_eq!(answer["notes"][0]["stands"], "taken", "handed to the agent: {answer}");
    let taken = &messages[2]["taken"];
    assert!(
        taken.as_u64().is_some_and(|taken| taken > 1_700_000_000),
        "when, in seconds since 1970: {answer}"
    );
    let file: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("orders/notes.json")).unwrap()).unwrap();
    assert_eq!(
        &file["notes"][1]["messages"][2]["taken"], taken,
        "kept in notes.json, for the gallery to read"
    );

    let (now, code) = answered(waiting(&root));
    assert_eq!((code, &now), (0, &answer), "a note waiting already: it answers at once, as `notes` does");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn wait_killed_and_run_again_loses_no_note() {
    let root = root("wait-killed");
    say(&root, "N1", "agent", "done");
    say(&root, "N2", "agent", "done");
    let mut child = waiting(&root);
    assert!(still_waiting(&mut child, 300));
    child.kill().unwrap();
    child.wait().unwrap();
    say(&root, "N1", "person", "not quite");
    let (answer, code) = answered(waiting(&root));
    assert_eq!((code, &answer["notes"][0]["id"]), (0, &json!("N1")), "{answer}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn wait_is_refused_with_answer() {
    let root = root("wait-answer");
    let dir = root.join("orders");
    let (refused, code) =
        underdraw(&["notes", dir.to_str().unwrap(), "--wait", "--answer", "N1", "--reply", "x"]);
    assert_eq!(code, 2);
    assert!(refused["error"].as_str().unwrap().contains("--wait"), "{refused}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn notes_lists_a_note_on_a_whole_state_a_transition_or_a_navigate_with_its_reference() {
    let root = root("arrows");
    let dir = root.join("orders");
    let note = |id: &str, on: Value| {
        let mut note = json!({ "id": id, "messages": [{ "by": "person", "says": "why?" }] });
        note.as_object_mut().unwrap().extend(on.as_object().unwrap().clone());
        note
    };
    let notes = json!({ "notes": [
        note("N1", json!({ "state": { "phase": "loading" } })),
        note("N2", json!({ "transition": { "from": "error", "on": "retry.tap", "outcome": "ok", "to": "default" } })),
        note("N3", json!({ "navigate": { "from": "OrderList", "node": "nav-export", "to": "ExportDocuments" } })),
    ] });
    std::fs::write(dir.join("notes.json"), notes.to_string()).unwrap();
    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{answer}");
    let listed = answer["notes"].as_array().unwrap();
    assert_eq!(listed[0]["state"], json!({ "phase": "loading" }));
    assert!(listed[0].get("node").is_none(), "{answer}");
    assert_eq!(listed[0]["reference"], format!("{} · phase loading", dir.display()));
    assert_eq!(listed[1]["transition"]["on"], "retry.tap");
    assert_eq!(
        listed[1]["reference"],
        format!("{} · transition from error on retry.tap outcome ok to default", dir.display())
    );
    assert_eq!(listed[2]["navigate"]["node"], "nav-export");
    assert_eq!(
        listed[2]["reference"],
        format!("{} · navigate from OrderList on nav-export to ExportDocuments", dir.display())
    );
    let (answered, code) =
        underdraw(&["notes", dir.to_str().unwrap(), "--answer", "N3", "--reply", "it goes there"]);
    assert_eq!((code, &answered["note"]["stands"]), (0, &json!("unread")), "{answered}");
    let file: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("notes.json")).unwrap()).unwrap();
    assert_eq!(file["notes"][2]["navigate"], notes["notes"][2]["navigate"], "kept as written");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_note_on_a_half_edited_description_is_left_where_it_is() {
    let root = root("gone");
    let dir = root.join("orders");
    let file = dir.join("screen.json");
    let screen =
        std::fs::read_to_string(&file).unwrap().replace("order-{index}-cancel", "order-{index}-void");
    std::fs::write(file, screen).unwrap();
    let notes = std::fs::read_to_string(dir.join("notes.json")).unwrap();
    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!((code, &answer["waiting"]), (0, &json!(2)), "{answer}");
    underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(
        std::fs::read_to_string(dir.join("notes.json")).unwrap(),
        notes,
        "a transition still names the old id: nothing moves until it checks clean"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_note_on_what_is_gone_joins_its_states_note_listed_like_any_other() {
    let root = root("joins");
    let dir = root.join("orders");
    let note = |id: &str, on: Value, messages: Value| {
        let mut note = json!({ "id": id, "messages": messages });
        note.as_object_mut().unwrap().extend(on.as_object().unwrap().clone());
        note
    };
    let person = |says: &str| json!({ "by": "person", "says": says });
    let agent = |says: &str| json!({ "by": "agent", "says": says });
    let line = |says: &str| json!({ "by": "underdraw", "says": says });
    let export = json!({ "state": { "fixture": "default" }, "node": { "id": "export", "at": "/x", "instance": [], "label": 3, "width": 390 } });
    let notes = json!({ "notes": [
        note("N1", export, json!([person("say export what"), agent("it says Export")])),
        note("N2", json!({ "state": { "fixture": "default" } }), json!([person("too dense")])),
        note("N3", json!({ "transition": { "from": "error", "on": "retry.tap", "outcome": "offline", "to": "error" } }), json!([person("why?")])),
        note("N4", json!({ "navigate": { "from": "OrderList", "node": "export", "to": "ExportDocuments" } }), json!([person("where?")])),
    ] });
    std::fs::write(dir.join("notes.json"), notes.to_string()).unwrap();
    let file = dir.join("screen.json");
    let screen =
        std::fs::read_to_string(&file).unwrap().replace(r#""id": "export""#, r#""id": "download""#).replace(
            r#",
    { "from": "error", "on": "retry.tap", "outcome": "offline", "to": "error" }"#,
            "",
        );
    std::fs::write(&file, screen).unwrap();
    let (checked, _) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(checked["ok"], true, "{checked}");

    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{answer}");
    let listed: Vec<(&Value, &Value)> =
        answer["notes"].as_array().unwrap().iter().map(|note| (&note["id"], &note["state"])).collect();
    assert_eq!(
        listed,
        [(&json!("N2"), &json!({ "fixture": "default" })), (&json!("N3"), &json!({ "phase": "error" }))],
        "{answer}"
    );
    assert_eq!(
        answer["notes"][0]["messages"],
        json!([
            line("on `export`, now gone"),
            person("say export what"),
            agent("it says Export"),
            line("on navigate from OrderList on export to ExportDocuments, now gone"),
            person("where?"),
            person("too dense"),
        ]),
        "the person spoke last: it waits on the agent"
    );
    assert_eq!(
        answer["notes"][1]["messages"],
        json!([
            line("on transition from error on retry.tap outcome offline to error, now gone"),
            person("why?")
        ]),
        "a state with no note gets one, keeping its id"
    );
    assert_eq!(answer["notes"][1]["reference"], format!("{} · phase error", dir.display()));
    let joined = std::fs::read_to_string(dir.join("notes.json")).unwrap();
    underdraw(&["notes", root.to_str().unwrap()]);
    let (checked, _) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(checked["notes"], json!({ "waiting": 2, "answered": 0 }), "{checked}");
    assert_eq!(std::fs::read_to_string(dir.join("notes.json")).unwrap(), joined, "nothing moves twice");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_reports_notes_json_in_the_old_form_as_a_fault_to_delete() {
    let root = root("old");
    let dir = root.join("orders");
    let old = json!({ "notes": [{
        "id": "N1", "status": "answered", "state": { "phase": "error" },
        "thread": [{ "by": "person", "says": "x" }, { "by": "agent", "says": "y" }],
    }] });
    std::fs::write(dir.join("notes.json"), old.to_string()).unwrap();
    for path in [&dir, &root] {
        let (answer, code) = underdraw(&["check", path.to_str().unwrap()]);
        assert_eq!((code, &answer["ok"]), (1, &json!(false)), "{answer}");
        let findings =
            if path == &dir { &answer["findings"] } else { &answer["descriptions"][0]["findings"] };
        let found = findings
            .as_array()
            .unwrap()
            .iter()
            .find(|finding| finding["at"] == "notes.json")
            .unwrap_or_else(|| panic!("no finding on notes.json: {answer}"));
        let problem = found["problem"].as_str().unwrap();
        assert!(problem.contains("old form") && problem.contains("delete it"), "{problem}");
        assert!(problem.contains(dir.join("notes.json").to_str().unwrap()), "names the file: {problem}");
    }
    let (refused, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 2, "not read: {refused}");
    assert!(refused["error"].as_str().unwrap().contains("old form"), "{refused}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn gallery_help_names_everything_a_note_can_be_pinned_to() {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(["gallery", "--help"]).output().unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    for pinned in ["- a node:", "- a whole state:", "- a transition:", "- a navigate:"] {
        assert!(help.contains(pinned), "{pinned} not in gallery --help");
    }
}

#[test]
fn notes_carries_each_messages_mark_and_its_help_says_what_each_means() {
    let root = root("marks");
    let dir = root.join("orders");
    let answers = json!({ "assumed": "a screen not drawn yet", "guess": { "undrawn": ["ExportDocuments"] }, "answer": "wrong" });
    let proposes = json!({ "on": "order-{index}-cancel.tap", "to": "cancel-dialog" });
    let mut notes: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("notes.json")).unwrap()).unwrap();
    let messages = notes["notes"][0]["messages"].as_array_mut().unwrap();
    messages.push(json!({ "by": "person", "says": "it exports the orders shown", "answers": answers }));
    messages.push(json!({ "by": "person", "says": "it should open the dialog", "proposes": proposes }));
    std::fs::write(dir.join("notes.json"), notes.to_string()).unwrap();

    let (answer, code) = underdraw(&["notes", root.to_str().unwrap()]);
    assert_eq!(code, 0, "{answer}");
    let messages = &answer["notes"][0]["messages"];
    assert_eq!(messages[0], json!({ "by": "person", "says": "too red" }), "plain talk carries no mark");
    assert_eq!(messages[1]["answers"], answers);
    assert_eq!(messages[2]["proposes"], proposes);
    assert_eq!(answer["notes"][0].get("proposes"), None, "the marks are the messages'");

    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(["notes", "--help"]).output().unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    for said in
        ["MARKS", "\"answers\"   the person's answer", "\"proposes\"  where acting", "right  ✓", "wrong  ✗"]
    {
        assert!(help.contains(said), "{said:?} not in notes --help");
    }
    std::fs::remove_dir_all(root).unwrap();
}
