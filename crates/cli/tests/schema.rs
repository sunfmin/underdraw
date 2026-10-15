//! Derives the view model's JSON Schema with the real binary, and refuses a
//! description with faults.

use std::process::Command;

use serde_json::{Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/todo");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

#[test]
fn answers_the_schema_of_the_root_model() {
    let (answer, code) = underdraw(&["schema", DRAFT]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["screen"], "TodoList");
    let schema = &answer["schema"];
    assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!(schema["required"], json!(["todos", "summary", "composer"]));
    assert_eq!(schema["properties"]["todos"]["items"], json!({ "$ref": "#/$defs/Todo" }));
    assert_eq!(schema["$defs"]["Todo"]["properties"]["title"]["maxLength"], 80);
}

#[test]
fn refuses_a_description_with_faults() {
    let dir = std::env::temp_dir().join(format!("underdraw-{}-schema-fault", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("screen.json"), r#"{"screen": "Broken", "data": {"x": "Nope"}}"#).unwrap();
    let (answer, code) = underdraw(&["schema", dir.to_str().unwrap()]);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(code, 2, "{answer}");
    assert!(answer["error"].as_str().unwrap().contains("underdraw check"), "{answer}");
}
