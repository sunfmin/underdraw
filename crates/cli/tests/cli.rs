//! Derives the documents draft's CLI with the real binary, then holds two
//! hand-written stub CLIs to its golden outputs through `compare`: one
//! faithful, one wrong.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/docs");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// The fixture's data, as the server a stub CLI talks to would hold it.
fn server(fixture: &str) -> Value {
    let (handoff, _) = underdraw(&["handoff", DRAFT, "--platform", "web"]);
    handoff["fixtures"][fixture].clone()
}

/// A CLI written by hand for the documents draft: what it prints and its
/// exit code, run as `<cli> <argv>` against `data`. `wrong` plants the
/// mistakes a careless one makes.
fn stub(data: &Value, argv: &[&str], fail: Option<&str>, wrong: bool) -> (String, i32) {
    if let Some(failure) = fail {
        return (json!({ "error": failure }).to_string(), if wrong { 1 } else { 2 });
    }
    match argv {
        ["list"] => {
            let rows = data["docs"].as_array().unwrap();
            let docs: Vec<Value> = if wrong {
                // Leaks the row's view state, and drops the last document.
                rows.iter()
                    .take(rows.len().saturating_sub(1))
                    .map(|row| {
                        let mut doc = row["doc"].clone();
                        doc["menuOpen"] = row.get("menuOpen").cloned().unwrap_or(json!(false));
                        doc
                    })
                    .collect()
            } else {
                rows.iter().map(|row| row["doc"].clone()).collect()
            };
            (json!({ "result": docs }).to_string(), 0)
        }
        ["delete", id] => {
            let may = data["rowMenu"]["docId"] == *id && data["rowMenu"]["canDelete"] == true;
            if may || wrong {
                ("{}".to_string(), 0)
            } else {
                (json!({ "error": "requires canDelete" }).to_string(), 1)
            }
        }
        _ => (json!({ "error": "usage" }).to_string(), 2),
    }
}

fn capture(name: &str, argv: &[&str], fail: Option<&str>, run: (String, i32)) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("underdraw-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.json"));
    let mut written = json!({ "command": argv, "exit": run.1, "stdout": run.0 });
    if let Some(failure) = fail {
        written["failure"] = json!(failure);
    }
    std::fs::write(&path, written.to_string()).unwrap();
    path
}

/// Each run a test makes: fixture, command line, named failure.
const RUNS: [(&str, &[&str], Option<&str>); 6] = [
    ("default", &["list"], None),
    ("empty", &["list"], None),
    ("row-menu", &["list"], None),
    ("row-menu", &["delete", "b"], None),
    ("row-menu-own", &["delete", "c"], None),
    ("default", &["delete", "a"], Some("notFound")),
];

#[test]
fn the_documents_draft_derives_list_delete_and_new() {
    let (answer, code) = underdraw(&["cli", DRAFT]);
    assert_eq!(code, 0, "{answer}");
    let usages: Vec<&str> =
        answer["commands"].as_array().unwrap().iter().map(|c| c["usage"].as_str().unwrap()).collect();
    assert_eq!(usages, ["list [--sort <updated|created|title|owner>]", "delete <id>", "new"]);
}

#[test]
fn a_faithful_stub_passes_compare() {
    for (index, (fixture, argv, fail)) in RUNS.iter().enumerate() {
        let run = stub(&server(fixture), argv, *fail, false);
        let path = capture(&format!("right-{index}"), argv, *fail, run);
        let (answer, code) =
            underdraw(&["compare", DRAFT, "--capture", path.to_str().unwrap(), "--fixture", fixture]);
        assert_eq!((code, &answer["ok"]), (0, &json!(true)), "{fixture} {argv:?}: {answer}");
    }
}

#[test]
fn a_wrong_stub_is_found_out() {
    let mut found = Vec::new();
    for (index, (fixture, argv, fail)) in RUNS.iter().enumerate() {
        let run = stub(&server(fixture), argv, *fail, true);
        let path = capture(&format!("wrong-{index}"), argv, *fail, run);
        let (answer, code) =
            underdraw(&["compare", DRAFT, "--capture", path.to_str().unwrap(), "--fixture", fixture]);
        if *fixture == "empty" || *fixture == "row-menu-own" {
            assert_eq!(code, 0, "{fixture} {argv:?}: {answer}");
            continue;
        }
        assert_eq!((code, &answer["ok"]), (1, &json!(false)), "{fixture} {argv:?}: {answer}");
        for finding in answer["findings"].as_array().unwrap() {
            found.push((
                finding["check"].as_str().unwrap().to_string(),
                finding["at"].as_str().unwrap().to_string(),
            ));
        }
    }
    for wanted in [
        ("stdout", "stdout.result"),
        ("stdout", "stdout.result[0].menuOpen"),
        ("exit", "exit"),
        ("stdout", "stdout.error"),
    ] {
        assert!(
            found.iter().any(|(check, at)| (check.as_str(), at.as_str()) == wanted),
            "{wanted:?} not in {found:?}"
        );
    }
}

#[test]
fn a_command_line_with_no_golden_output_is_refused() {
    let path = capture("unknown", &["list", "--sort", "size"], None, ("{}".to_string(), 0));
    let (answer, code) = underdraw(&["compare", DRAFT, "--capture", path.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("one of updated"), "{answer}");
    let path = capture("untested", &["delete", "z"], None, ("{}".to_string(), 0));
    let (answer, code) = underdraw(&["compare", DRAFT, "--capture", path.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("no golden output"), "{answer}");
}
