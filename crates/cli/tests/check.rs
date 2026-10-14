//! Runs the real binary on the Airbnb draft, whole and with one fault planted
//! at a time, and reads its JSON and exit code.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/airbnb-listing");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// A copy of the draft's screen.json with `from` replaced by `to`, where
/// `from` appears exactly once.
fn planted(name: &str, from: &str, to: &str) -> PathBuf {
    let text = std::fs::read_to_string(Path::new(DRAFT).join("screen.json")).unwrap();
    assert_eq!(text.matches(from).count(), 1, "{name}: {from:?} must appear exactly once in the draft");
    let dir = std::env::temp_dir().join(format!("underdraw-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("screen.json"), text.replace(from, to)).unwrap();
    dir
}

#[test]
fn the_draft_is_sound_and_every_condition_varies() {
    let (answer, code) = underdraw(&["check", DRAFT]);
    assert_eq!(code, 0);
    assert_eq!(answer["ok"], true, "{answer:#}");
    assert_eq!(answer["fixtures"], json!(["default", "no-dates", "unavailable", "new-listing"]));
    assert_eq!(answer["phases"], json!(["loading", "error"]));
    let holds: Vec<(&str, &Value)> = answer["conditions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["name"].as_str().unwrap(), &c["holds_in"]))
        .collect();
    assert_eq!(
        holds,
        [
            ("hasReviews", &json!(["default", "no-dates", "unavailable"])),
            ("hasDates", &json!(["default", "unavailable", "new-listing"])),
            ("canReserve", &json!(["default", "new-listing"])),
            ("datesUnavailable", &json!(["unavailable"])),
        ]
    );
}

#[test]
fn a_fault_exits_1_with_where_it_is() {
    // Every fault check finds is tested in underdraw-format; here, only what the binary adds.
    let dir = planted(
        "exit-1",
        r#""text": "清除日期", "if": "hasDates""#,
        r#""text": "清除日期", "if": "booking.nights > 0""#,
    );
    let (answer, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(code, 1, "findings exit 1, so a gate stops on them");
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["findings"][0]["at"], "calendar-clear.if", "{answer:#}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_cannot_be_read_is_an_error_with_its_fix() {
    let (answer, code) = underdraw(&["check", "/nonexistent/screen"]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("pass a screen's directory"), "{answer}");

    let dir = std::env::temp_dir().join(format!("underdraw-{}-not-json", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("screen.json"), "{ nope").unwrap();
    let (answer, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("is not JSON"), "{answer}");
    assert!(answer["error"].as_str().unwrap().contains("line 1"), "{answer}");
    std::fs::remove_dir_all(dir).unwrap();

    let (answer, code) = underdraw(&["chek"]);
    assert_eq!(code, 2);
    assert_eq!(answer["fix"], "underdraw --help");
}

#[test]
fn doctor_and_version() {
    let (answer, code) = underdraw(&["doctor"]);
    assert_eq!((code, &answer["ok"], &answer["findings"]), (0, &json!(true), &json!([])), "{answer}");
    assert!(answer["gpu"]["name"].is_string(), "doctor names the GPU render uses: {answer}");
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).arg("--version").output().unwrap();
    let version = String::from_utf8(out.stdout).unwrap();
    assert!(version.starts_with(&format!("underdraw {}", env!("CARGO_PKG_VERSION"))), "{version}");
}

#[test]
fn help_names_exactly_the_commands_that_exist() {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).arg("--help").output().unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    // A command is named where a line starts with `underdraw <command>`.
    let mut named: Vec<String> = help
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("underdraw "))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|word| word.chars().all(|c| c.is_ascii_lowercase()))
        .map(str::to_string)
        .collect();
    named.sort();
    named.dedup();
    let mut exist = underdraw::commands();
    exist.sort();
    assert_eq!(named, exist);
}

#[test]
fn the_todo_draft_defines_its_own_text_field() {
    let todo = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/todo");
    let (answer, code) = underdraw(&["check", todo]);
    assert_eq!((code, &answer["findings"]), (0, &json!([])), "{answer:#}");
}

#[test]
fn an_overlay_taller_than_what_it_floats_over_is_found() {
    let menu = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../drafts/menu");
    let dir = std::env::temp_dir().join(format!("underdraw-{}-tall-sheet", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["kinds.json", "tokens.json"] {
        std::fs::copy(menu.join(file), dir.join(file)).unwrap();
    }
    let mut screen: Value =
        serde_json::from_str(&std::fs::read_to_string(menu.join("screen.json")).unwrap()).unwrap();
    // Behind the sheet, one dish and no cart bar: the page is shorter than the sheet.
    screen["fixtures"]["sheet-adding"]["sections"] = json!([{ "id": "sides", "title": "小食", "dishes": [
        { "id": "d4", "name": "煎饺", "description": "", "price": { "amount": 450, "currency": "JPY" } } ] }]);
    std::fs::write(dir.join("screen.json"), screen.to_string()).unwrap();

    let (answer, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(code, 1, "{answer:#}");
    let problems: Vec<&str> = answer["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["at"] == "dish-sheet")
        .map(|f| f["problem"].as_str().unwrap())
        .collect();
    assert!(
        problems.iter().any(|p| p.contains("sheet-adding") && p.contains("390") && p.contains("cut")),
        "{problems:#?}"
    );
    assert!(problems.iter().all(|p| p.contains("sheet-adding")), "only the short fixture: {problems:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_menu_draft_fits_its_sheet_in_every_state() {
    let menu = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../drafts/menu");
    let (answer, code) = underdraw(&["check", menu.to_str().unwrap()]);
    assert_eq!((code, &answer["findings"]), (0, &json!([])), "{answer:#}");
}
