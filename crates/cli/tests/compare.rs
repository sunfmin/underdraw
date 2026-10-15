//! Compares captures of the Airbnb draft with the real binary: one made from
//! its own wireframe (`render`'s boxes, `handoff`'s states), then the same
//! one wrong. Needs the GPU, as `render` does.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Map, Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/airbnb-listing");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

fn file(name: &str, value: &Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("underdraw-compare-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, value.to_string()).unwrap();
    path
}

/// The capture of an Android UI built exactly as described, showing the
/// `unavailable` fixture 1120 points wide.
fn faithful() -> Value {
    let png = file("unavailable.png", &json!(null));
    let (render, _) = underdraw(&[
        "render",
        DRAFT,
        "--fixture",
        "unavailable",
        "--width",
        "1120",
        "--scale",
        "1",
        "--out",
        png.to_str().unwrap(),
    ]);
    let (handoff, _) = underdraw(&["handoff", DRAFT, "--platform", "android"]);
    let state = handoff["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["state"] == json!({ "fixture": "unavailable" }) && s["breakpoint"] == "regular")
        .unwrap();
    let mut elements = Vec::new();
    for id in state["present"].as_array().unwrap() {
        let placed = render["nodes"].as_array().unwrap().iter().find(|n| &n["id"] == id).unwrap();
        let rect = &placed["box"];
        let mut element = Map::new();
        for key in ["x", "y", "width", "height"] {
            element.insert(key.into(), rect[key].clone());
        }
        element.insert("id".into(), id.clone());
        if let Some(Value::Object(shown)) = state["shows"].get(id.as_str().unwrap()) {
            if let Some(Value::Bool(disabled)) = shown.get("disabled") {
                element.insert("enabled".into(), json!(!disabled));
            }
            if let Some(Value::String(text)) = shown.get("text") {
                element.insert("text".into(), json!(text));
            }
        }
        elements.push(Value::Object(element));
    }
    json!({ "width": 1120, "elements": elements })
}

fn run(capture: &Value) -> (Value, i32) {
    let path = file("capture.json", capture);
    underdraw(&[
        "compare",
        DRAFT,
        "--platform",
        "android",
        "--fixture",
        "unavailable",
        "--capture",
        path.to_str().unwrap(),
    ])
}

#[test]
fn a_faithful_capture_then_a_wrong_one() {
    let mut capture = faithful();
    let (answer, code) = run(&capture);
    assert_eq!((code, &answer["ok"]), (0, &json!(true)), "{}", answer["findings"]);
    assert_eq!(answer["breakpoint"], "regular");
    assert!(answer["checked"]["order"].as_u64().unwrap() > 5, "{}", answer["checked"]);

    let elements = capture["elements"].as_array_mut().unwrap();
    let button = elements.iter_mut().find(|e| e["id"] == "reserve-button").unwrap();
    button["enabled"] = json!(true);
    elements.retain(|e| e["id"] != "summary-title");
    let (answer, code) = run(&capture);
    assert_eq!((code, &answer["ok"]), (1, &json!(false)));
    let found: Vec<_> = answer["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["at"].clone(), f["check"].clone()))
        .collect();
    assert!(found.contains(&(json!("reserve-button"), json!("enabled"))), "{found:?}");
    assert!(found.contains(&(json!("summary-title"), json!("present"))), "{found:?}");
}

#[test]
fn refuses_what_it_cannot_compare() {
    let (answer, code) = run(&json!({ "elements": [] }));
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("\"width\""), "{answer}");

    let path = file("empty.json", &json!({ "width": 390, "elements": [] }));
    let (answer, code) = underdraw(&[
        "compare",
        DRAFT,
        "--platform",
        "android",
        "--fixture",
        "nope",
        "--capture",
        path.to_str().unwrap(),
    ]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("the fixtures are"), "{answer}");
}
