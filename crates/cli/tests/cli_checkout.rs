//! Derives the checkout draft's CLI with the real binary — a form's commands
//! beside its queries — then runs a hand-written stub CLI through every
//! golden output and holds each run to it with `compare`; a wrong stub is
//! found out.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Map, Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/checkout");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// `value` with `key` set to `extra` when it leaves it out: what a careless
/// stub adds to a row.
fn with_extra(value: &Value, key: &str, extra: Value) -> Value {
    let mut filled: Map<String, Value> = value.as_object().unwrap().clone();
    filled.entry(key.to_string()).or_insert(extra);
    Value::Object(filled)
}

/// A CLI written by hand for the checkout draft: what it prints and its exit
/// code, run as `<cli> <argv>` against `data`, a fixture's data as
/// `underdraw cli` resolves it. `wrong` plants the mistakes a
/// careless one makes.
fn stub(data: &Value, argv: &[&str], fail: Option<&str>, wrong: bool) -> (String, i32) {
    if let Some(failure) = fail {
        return (json!({ "error": failure }).to_string(), if wrong { 1 } else { 2 });
    }
    let result = |value: Value| (json!({ "result": value }).to_string(), 0);
    match argv {
        ["get-cart"] => result(data["cart"].clone()),
        ["get-address"] => result(data["address"].clone()),
        ["list"] => {
            let rows = data["delivery"].as_array().unwrap();
            let options = rows
                .iter()
                .map(|row| {
                    // Leaks what the app derives for the row.
                    if wrong {
                        with_extra(&row["option"], "free", json!(false))
                    } else {
                        row["option"].clone()
                    }
                })
                .collect();
            result(Value::Array(options))
        }
        ["get-promo"] => result(data["promo"].clone()),
        ["get-summary"] => result(data["summary"].clone()),
        ["choose", _] | ["set", _, _] | ["remove-item", _] | ["apply", _] | ["remove-promo"] => {
            ("{}".to_string(), 0)
        }
        ["place", _] => {
            let can_place = !data["address"].is_null() && data["cart"]["unavailableCount"] == 0;
            if can_place || wrong {
                result(json!({ "id": "o1", "total": data["summary"]["total"] }))
            } else {
                (json!({ "error": "requires canPlace" }).to_string(), 1)
            }
        }
        _ => (json!({ "error": "usage" }).to_string(), 2),
    }
}

fn capture(name: &str, argv: &[&str], fail: Option<&str>, run: (String, i32)) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("underdraw-cli-checkout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.json"));
    let mut written = json!({ "command": argv, "exit": run.1, "stdout": run.0 });
    if let Some(failure) = fail {
        written["failure"] = json!(failure);
    }
    std::fs::write(&path, written.to_string()).unwrap();
    path
}

/// One golden output: its fixture, command line and named failure.
type Run = (String, Vec<String>, Option<String>);

/// Every golden output the CLI derives, as (fixture, command line, named
/// failure), a named failure running in the first fixture; and each
/// fixture's data, as the server a stub CLI talks to holds it.
fn runs() -> (Vec<Run>, Value) {
    let (answer, code) = underdraw(&["cli", DRAFT]);
    assert_eq!(code, 0, "{answer}");
    let runs = answer["golden"]
        .as_array()
        .unwrap()
        .iter()
        .map(|golden| {
            let mut argv = vec![golden["command"].as_str().unwrap().to_string()];
            if let Some(args) = golden["args"].as_array() {
                argv.extend(args.iter().map(|a| a.as_str().unwrap().to_string()));
            }
            let fixture = golden["fixture"].as_str().unwrap_or("default").to_string();
            (fixture, argv, golden["failure"].as_str().map(str::to_string))
        })
        .collect();
    (runs, answer["fixtures"].clone())
}

/// Runs the stub on one golden output and answers what `compare` said.
fn compare(index: usize, run: &Run, fixtures: &Value, wrong: bool) -> (Value, i32) {
    let (fixture, argv, fail) = run;
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let printed = stub(&fixtures[fixture.as_str()], &argv, fail.as_deref(), wrong);
    let name = format!("{}-{index}", if wrong { "wrong" } else { "right" });
    let path = capture(&name, &argv, fail.as_deref(), printed);
    underdraw(&["compare", DRAFT, "--capture", path.to_str().unwrap(), "--fixture", fixture])
}

#[test]
fn the_checkout_draft_derives_queries_and_the_forms_commands() {
    let (answer, code) = underdraw(&["cli", DRAFT]);
    assert_eq!(code, 0, "{answer}");
    let usages: Vec<&str> =
        answer["commands"].as_array().unwrap().iter().map(|c| c["usage"].as_str().unwrap()).collect();
    assert_eq!(
        usages,
        [
            "get-cart",
            "get-address",
            "list",
            "get-promo",
            "get-summary",
            "choose <id>",
            "set <id> <quantity>",
            "remove-item <id>",
            "apply <code>",
            "remove-promo",
            "place <card|alipay|wechat>",
        ]
    );
    // The promo code the person typed is what apply is given.
    assert!(
        answer["golden"].as_array().unwrap().iter().any(|g| g["command"] == "apply"
            && g["fixture"] == "promo-invalid"
            && g["args"] == json!(["FALL2025"])),
        "{answer}"
    );
}

#[test]
fn a_faithful_stub_passes_compare_on_every_golden_output() {
    let (runs, fixtures) = runs();
    assert!(runs.len() > 50, "{runs:?}");
    for (index, run) in runs.iter().enumerate() {
        let (answer, code) = compare(index, run, &fixtures, false);
        assert_eq!((code, &answer["ok"]), (0, &json!(true)), "{run:?}: {answer}");
    }
}

#[test]
fn a_wrong_stub_is_found_out() {
    let mut found = Vec::new();
    let (runs, fixtures) = runs();
    for (index, run) in runs.iter().enumerate() {
        let (answer, _) = compare(index, run, &fixtures, true);
        for finding in answer["findings"].as_array().into_iter().flatten() {
            found.push((
                run.1[0].clone(),
                finding["check"].as_str().unwrap().to_string(),
                finding["at"].as_str().unwrap().to_string(),
            ));
        }
    }
    for wanted in [
        ("list", "stdout", "stdout.result[0].free"),
        ("place", "exit", "exit"),
        ("place", "stdout", "stdout.error"),
        ("apply", "exit", "exit"),
    ] {
        assert!(
            found
                .iter()
                .any(|(command, check, at)| (command.as_str(), check.as_str(), at.as_str()) == wanted),
            "{wanted:?} not in {found:?}"
        );
    }
}
