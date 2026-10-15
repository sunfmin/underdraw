//! Runs the real binary on the Airbnb draft, whole and with one fault planted
//! at a time, and reads its JSON and exit code.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../format/tests/airbnb-listing");

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
fn the_menu_draft_fits_its_sheet_in_every_state() {
    // Laid out in the system's fonts, as render draws it; what is cut where is tested in underdraw-layout.
    let menu = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../drafts/menu");
    let (answer, code) = underdraw(&["check", menu.to_str().unwrap()]);
    assert_eq!((code, &answer["findings"]), (0, &json!([])), "{answer:#}");
}

const DRAFTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts");

/// The links `check <root>` answers from the description `description`, as
/// (node, to, resolved).
fn links_from<'a>(answer: &'a Value, description: &str) -> Vec<(&'a str, &'a str, bool)> {
    answer["links"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|link| link["description"] == description)
        .map(|link| {
            (
                link["node"].as_str().unwrap(),
                link["to"].as_str().unwrap(),
                link["resolved"].as_bool().unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_root_is_checked_whole_with_the_navigates_between_its_descriptions() {
    let (answer, code) = underdraw(&["check", DRAFTS]);
    assert_eq!((code, &answer["ok"]), (0, &json!(true)), "an unresolved navigate is no fault: {answer:#}");
    let described: Vec<(&str, &str)> = answer["descriptions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["description"].as_str().unwrap(), d["screen"].as_str().unwrap()))
        .collect();
    assert!(described.contains(&("orders", "OrderList")), "{described:?}");
    assert!(described.contains(&("export", "ExportDocuments")), "{described:?}");
    let orders = links_from(&answer, "orders");
    assert!(orders.contains(&("nav-export", "ExportDocuments", true)), "{orders:?}");
    let unresolved: Vec<&str> =
        orders.iter().filter(|(_, _, resolved)| !resolved).map(|(_, to, _)| *to).collect();
    assert_eq!(unresolved, ["OrderDetail"], "the one screen orders goes to that is not drawn yet");
    assert!(answer["links"].as_array().unwrap().iter().all(|link| link["from"].is_string()));
}

#[test]
fn a_root_with_a_faulty_description_exits_1_and_one_with_none_exits_2() {
    let root = std::env::temp_dir().join(format!("underdraw-{}-root", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("broken")).unwrap();
    let (answer, code) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(code, 2, "{answer}");
    assert!(answer["error"].as_str().unwrap().contains("no screen description"), "{answer}");

    std::fs::write(root.join("broken/screen.json"), "{").unwrap();
    let (answer, code) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!((code, &answer["ok"]), (1, &json!(false)), "{answer}");
    assert_eq!(answer["descriptions"][0]["description"], "broken");
    assert_eq!(answer["descriptions"][0]["findings"][0]["at"], "screen.json");
    assert_eq!(answer["links"], json!([]));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn one_description_says_nothing_of_where_it_navigates() {
    let orders = Path::new(DRAFTS).join("orders");
    let (answer, code) = underdraw(&["check", orders.to_str().unwrap()]);
    assert_eq!((code, &answer["ok"]), (0, &json!(true)), "{answer:#}");
    let mut keys: Vec<&str> = answer.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        ["conditions", "findings", "fixtures", "notes", "ok", "phases", "screen"],
        "and its notes"
    );
}

#[test]
fn a_screen_not_drawn_yet_is_assumed_in_a_sentence_in_the_language_asked() {
    let assumed = |lang: &str| {
        let (answer, code) = underdraw(&["check", DRAFTS, "--lang", lang]);
        assert_eq!(code, 0, "{answer:#}");
        let links = answer["links"].as_array().unwrap().clone();
        let unresolved = links.iter().find(|link| link["to"] == "OrderDetail").unwrap();
        let resolved = links.iter().find(|link| link["resolved"] == true).unwrap();
        assert_eq!(resolved["assumed"], Value::Null, "a resolved navigate assumes nothing");
        unresolved["assumed"].as_str().unwrap().to_string()
    };
    let (en, zh) = (assumed("en"), assumed("zh"));
    assert!(en.starts_with("This navigates to OrderDetail") && en.ends_with('.'), "{en}");
    assert!(zh.contains("OrderDetail") && zh.contains("还没画"), "{zh}");
    let ja = assumed("ja");
    assert!(
        ja.starts_with("ここは OrderDetail に遷移します") && ja.contains("まだ描かれていない画面"),
        "{ja}"
    );
    let (answer, code) = underdraw(&["check", DRAFTS, "--lang", "fr"]);
    assert_eq!(code, 2, "{answer}");
}

#[test]
fn a_root_holds_each_navigate_to_the_params_of_the_screen_it_opens() {
    let root = std::env::temp_dir().join(format!("underdraw-{}-params", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for draft in ["orders", "export"] {
        std::fs::create_dir_all(root.join(draft)).unwrap();
        for entry in std::fs::read_dir(Path::new(DRAFTS).join(draft)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), root.join(draft).join(entry.file_name())).unwrap();
            }
        }
    }
    let (answer, code) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!(
        (code, &answer["ok"]),
        (0, &json!(true)),
        "ExportDocuments is opened with nothing: {answer:#}"
    );

    let orders = root.join("orders/screen.json");
    let text = std::fs::read_to_string(&orders).unwrap();
    let from = r#""navigate": "ExportDocuments""#;
    assert_eq!(text.matches(from).count(), 1);
    std::fs::write(
        &orders,
        text.replace(from, r#""navigate": "ExportDocuments", "with": { "format": "pdf" }"#),
    )
    .unwrap();
    let (answer, code) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!((code, &answer["ok"]), (1, &json!(false)), "{answer:#}");
    let described = answer["descriptions"].as_array().unwrap();
    let orders = described.iter().find(|d| d["description"] == "orders").unwrap();
    assert_eq!(orders["findings"][0]["at"], "nav-export.on.tap.with.format", "{orders:#}");
    assert!(
        orders["findings"][0]["problem"].as_str().unwrap().contains("declares no param format"),
        "{orders:#}"
    );

    let (alone, code) = underdraw(&["check", root.join("orders").to_str().unwrap()]);
    assert_eq!((code, &alone["ok"]), (0, &json!(true)), "alone, it cannot see ExportDocuments: {alone:#}");
    std::fs::remove_dir_all(root).unwrap();
}

/// The index both helps open with names, for each line, the sections that
/// say how: each must be a heading of `check --help`, and every heading of
/// FORMAT must be reached from it, so a renamed or added section fails here.
#[test]
fn the_index_reaches_every_section_of_format() {
    let help = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let check = help(&["check", "--help"]);
    let index: Vec<&str> = check
        .lines()
        .skip_while(|line| *line != "FIND IT")
        .skip(3)
        .take_while(|line| !line.is_empty())
        .collect();
    let named: Vec<&str> =
        index.iter().flat_map(|line| line.rsplit_once("  ").unwrap().1.split(", ")).collect();
    // A heading is a line of capitals, not indented.
    let heading = |line: &&str| {
        !line.starts_with(' ')
            && line.chars().any(|c| c.is_ascii_uppercase())
            && !line.chars().any(|c| c.is_ascii_lowercase())
    };
    let headings: Vec<&str> = check.lines().filter(heading).collect();
    for section in &named {
        assert!(
            headings.contains(section),
            "the index names {section:?}, which no heading of check --help is"
        );
    }
    let format = include_str!("../src/help/format.txt");
    for section in format.lines().filter(heading) {
        assert!(named.contains(&section), "FORMAT's {section:?} is not reached from the index");
    }
    assert!(check.contains("\nFIND IT\n"), "check --help opens with the index");
    assert!(help(&["--help"]).contains(&index.join("\n")), "--help opens with the same index");
}
