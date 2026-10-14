//! The CLI the documents draft's operations derive: its commands, the
//! schema of what each prints, and the golden outputs its fixtures give.

use serde_json::{Value, json};
use underdraw_format::model;
use underdraw_handoff::cli;

fn draft(file: &str) -> Value {
    let path = format!("{}/../../drafts/docs/{file}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn interface() -> underdraw_handoff::Interface {
    let model = model(&draft("screen.json"), Some(&draft("kinds.json")))
        .unwrap_or_else(|c| panic!("{:#?}", c.findings));
    cli(&model)
}

#[test]
fn each_operation_is_a_command_with_its_arguments_and_failures() {
    let interface = interface();
    let commands: Vec<Value> = interface.commands.iter().map(|c| serde_json::to_value(c).unwrap()).collect();
    let usages: Vec<(&str, &str)> =
        commands.iter().map(|c| (c["name"].as_str().unwrap(), c["usage"].as_str().unwrap())).collect();
    assert_eq!(
        usages,
        [("list", "list [--sort <updated|created|title|owner>]"), ("delete", "delete <id>"), ("new", "new")]
    );
    assert_eq!(
        commands[0]["args"],
        json!([{ "param": "sort", "flag": "--sort", "type": "SortKey",
                 "choices": ["updated", "created", "title", "owner"], "default": "updated" }])
    );
    assert_eq!(
        commands[1]["fails"][0],
        json!({ "error": "requires canDelete", "when": "refused before it is tried: \"May delete it\" does not hold",
                "exit": 1, "stdout": { "error": "requires canDelete" } })
    );
    assert_eq!(commands[1]["fails"][1]["exit"], 2);
    assert!(commands[1]["help"].as_str().unwrap().contains("<cli> delete <id>"));
}

#[test]
fn each_stdout_schema_validates_its_golden_outputs_and_holds_no_view_state() {
    let interface = interface();
    for command in &interface.commands {
        let text = command.stdout.to_string();
        for view in ["menuOpen", "rowMenu", "DocRow", "count", "label"] {
            assert!(!text.contains(view), "{}: {view} in {text}", command.name);
        }
        let validator = jsonschema::validator_for(&command.stdout).unwrap();
        for golden in interface.golden.iter().filter(|g| g.command == command.name && g.exit == 0) {
            let errors: Vec<String> = validator.iter_errors(&golden.stdout).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "{} in {:?}: {errors:?}", command.name, golden.fixture);
        }
    }
}

#[test]
fn each_fixture_yields_a_golden_output_and_the_precondition_its_refusal() {
    let interface = interface();
    let golden: Vec<Value> = interface.golden.iter().map(|g| serde_json::to_value(g).unwrap()).collect();
    for fixture in ["default", "sort-open", "row-menu", "row-menu-own", "empty"] {
        assert!(
            golden.iter().any(|g| g["command"] == "list" && g["fixture"] == fixture),
            "no golden list in {fixture}"
        );
    }
    let list = golden.iter().find(|g| g["command"] == "list" && g["fixture"] == "default").unwrap();
    assert_eq!(list["exit"], 0);
    assert_eq!(
        list["stdout"]["result"][1],
        json!({
            "id": "b", "title": "Q4 预算与人力规划", "icon": "table_chart", "owner": "王芳",
            "updated": "2026-10-13", "shared": true
        })
    );
    let empty = golden.iter().find(|g| g["command"] == "list" && g["fixture"] == "empty").unwrap();
    assert_eq!(empty["stdout"], json!({ "result": [] }));
    let deletes: Vec<(&Value, &Value, &Value, &Value)> = golden
        .iter()
        .filter(|g| g["command"] == "delete" && g.get("fixture").is_some())
        .map(|g| (&g["fixture"], &g["args"], &g["exit"], &g["stdout"]))
        .collect();
    assert_eq!(
        deletes,
        [
            (&json!("row-menu"), &json!(["b"]), &json!(1), &json!({ "error": "requires canDelete" })),
            (&json!("row-menu-own"), &json!(["c"]), &json!(0), &json!({})),
        ]
    );
    let failures: Vec<(&Value, &Value)> = golden
        .iter()
        .filter(|g| g.get("failure").is_some())
        .map(|g| (&g["command"], &g["failure"]))
        .collect();
    assert_eq!(failures.len(), 4, "{failures:?}");
}

#[test]
fn arguments_are_read_as_the_command_takes_them() {
    let interface = interface();
    let list = &interface.commands[0];
    let args = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
    assert_eq!(list.with_args(&[]), Ok(json!({ "sort": "updated" })));
    assert_eq!(list.with_args(&args(&["--sort", "title"])), Ok(json!({ "sort": "title" })));
    assert_eq!(list.with_args(&args(&["--sort=owner"])), Ok(json!({ "sort": "owner" })));
    assert!(list.with_args(&args(&["--sort", "size"])).unwrap_err().contains("one of updated"));
    assert!(list.with_args(&args(&["--limit", "3"])).is_err());
    let delete = &interface.commands[1];
    assert_eq!(delete.with_args(&args(&["b"])), Ok(json!({ "id": "b" })));
    assert!(delete.with_args(&[]).unwrap_err().contains("needs <id>"));
    assert!(delete.with_args(&args(&["b", "c"])).is_err());
}
