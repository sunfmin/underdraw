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
        [
            (
                "list-docs",
                "list-docs [--sort <updated|created|title|owner>] [--cursor <cursor>] [--limit <limit>]"
            ),
            ("delete-doc", "delete-doc <id> --yes"),
            ("delete-docs", "delete-docs <ids>… --yes"),
            ("new-document", "new-document")
        ]
    );
    assert_eq!(
        commands[0]["args"],
        json!([{ "param": "sort", "flag": "--sort", "type": "SortKey",
                 "choices": ["updated", "created", "title", "owner"], "default": "updated" },
               { "param": "cursor", "flag": "--cursor", "type": "string?" },
               { "param": "limit", "flag": "--limit", "type": "int", "default": 50 }])
    );
    let help = commands[0]["help"].as_str().unwrap();
    assert!(help.contains("--cursor <cursor>  where the page starts"), "{help}");
    assert!(help.contains("--limit <limit>  at most this many rows; default 50"), "{help}");
    assert_eq!(
        commands[1]["fails"][0],
        json!({ "error": "requires canDelete", "when": "refused before it is tried: \"May delete it\" does not hold",
                "exit": 1, "stdout": { "error": "requires canDelete" } })
    );
    assert_eq!(commands[1]["destructive"], true);
    assert_eq!(
        commands[1]["fails"][1],
        json!({ "error": "needs --yes",
                "when": "not tried without --yes, as it cannot be undone: says what it would do",
                "exit": 1,
                "stdout": { "error": "needs --yes",
                            "dryRun": { "operation": "deleteDoc", "does": "Deletes a document for everyone in the team",
                                        "with": { "id": "<id>" } } } })
    );
    assert_eq!(commands[1]["fails"][2]["exit"], 2);
    let help = commands[1]["help"].as_str().unwrap();
    assert!(help.contains("<cli> delete-doc <id> --yes"), "{help}");
    assert!(help.contains("  --yes  do it"), "{help}");
    assert_eq!(commands[0].get("destructive"), None, "only a destructive command says so");
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
    for fixture in
        ["default", "more-to-come", "sort-open", "row-menu", "row-menu-own", "confirm-delete", "empty"]
    {
        assert!(
            golden.iter().any(|g| g["command"] == "list-docs" && g["fixture"] == fixture),
            "no golden list in {fixture}"
        );
    }
    let list = golden.iter().find(|g| g["command"] == "list-docs" && g["fixture"] == "default").unwrap();
    assert_eq!(list["exit"], 0);
    assert_eq!(
        list["stdout"]["result"]["docs"][1],
        json!({
            "id": "b", "title": "Q4 预算与人力规划", "icon": "table_chart", "owner": "王芳",
            "updated": "2026-10-13", "shared": true
        })
    );
    let empty = golden.iter().find(|g| g["command"] == "list-docs" && g["fixture"] == "empty").unwrap();
    assert_eq!(empty["stdout"], json!({ "result": { "docs": [], "next": null } }));
    // The last page says so with a null next; a page with more to come, where.
    assert_eq!(list["stdout"]["result"]["next"], Value::Null);
    let more = golden.iter().find(|g| g["command"] == "list-docs" && g["fixture"] == "more-to-come").unwrap();
    assert_eq!(more["stdout"]["result"]["next"], "p2");
    assert_eq!(more["with"], json!({ "sort": "updated", "limit": 50 }));
    let deletes: Vec<(&Value, &Value, &Value, &Value)> = golden
        .iter()
        .filter(|g| g["command"] == "delete-doc" && g.get("fixture").is_some())
        .map(|g| (&g["fixture"], &g["args"], &g["exit"], &g["stdout"]))
        .collect();
    assert_eq!(
        deletes,
        [
            (
                &json!("confirm-delete"),
                &json!(["c"]),
                &json!(1),
                &json!({ "error": "needs --yes",
                         "dryRun": { "operation": "deleteDoc", "does": "Deletes a document for everyone in the team",
                                     "with": { "id": "c" } } })
            ),
            (&json!("confirm-delete"), &json!(["c", "--yes"]), &json!(0), &json!({})),
        ],
        "deleted only from its confirmation, and only given --yes"
    );
    let failures: Vec<(&Value, &Value)> = golden
        .iter()
        .filter(|g| g.get("failure").is_some())
        .map(|g| (&g["command"], &g["failure"]))
        .collect();
    assert_eq!(failures.len(), 7, "{failures:?}");
}

#[test]
fn a_bulk_form_takes_many_ids_and_answers_for_each() {
    let interface = interface();
    let bulk = serde_json::to_value(&interface.commands[2]).unwrap();
    assert_eq!(
        (&bulk["name"], &bulk["operation"], &bulk["bulkOf"]),
        (&json!("delete-docs"), &json!("deleteDocs"), &json!("deleteDoc"))
    );
    assert_eq!(bulk["destructive"], true, "deleting many needs --yes as deleting one does");
    assert_eq!(bulk["args"], json!([{ "param": "ids", "type": "string[]", "repeated": true }]));
    let fails: Vec<(&Value, &Value)> =
        bulk["fails"].as_array().unwrap().iter().map(|f| (&f["error"], &f["exit"])).collect();
    assert_eq!(
        fails,
        [
            (&json!("requires canDelete"), &json!(2)),
            (&json!("needs --yes"), &json!(1)),
            (&json!("notFound"), &json!(2)),
            (&json!("forbidden"), &json!(2))
        ],
        "the precondition and each failure are an item's; only --yes refuses the whole command"
    );
    let help = bulk["help"].as_str().unwrap();
    assert!(help.contains("<cli> delete-docs <ids>… --yes"), "{help}");
    assert!(help.contains("<ids>  string; one or more"), "{help}");

    let args = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
    let command = &interface.commands[2];
    assert_eq!(command.with_args(&args(&["c", "e", "--yes"])), Ok(json!({ "ids": ["c", "e"] })));
    assert_eq!(command.with_args(&args(&["c"])), Ok(json!({ "ids": ["c"] })));
    assert!(command.with_args(&[]).unwrap_err().contains("needs <ids>"), "at least one id");

    let golden: Vec<Value> = interface.golden.iter().map(|g| serde_json::to_value(g).unwrap()).collect();
    let runs: Vec<(&Value, &Value, &Value, &Value)> = golden
        .iter()
        .filter(|g| g["command"] == "delete-docs")
        .map(|g| (&g["failure"], &g["args"], &g["exit"], &g["stdout"]))
        .collect();
    let picked = json!(["c", "e", "--yes"]);
    assert_eq!(
        runs,
        [
            (
                &Value::Null,
                &json!(["c", "e"]),
                &json!(1),
                &json!({ "error": "needs --yes",
                         "dryRun": { "operation": "deleteDocs",
                                     "does": "Deletes a document for everyone in the team: each of several, one by one",
                                     "with": { "ids": ["c", "e"] } } })
            ),
            (&Value::Null, &picked, &json!(0), &json!({ "results": [{ "id": "c" }, { "id": "e" }] })),
            (
                &json!("requires canDelete"),
                &picked,
                &json!(2),
                &json!({ "results": [{ "id": "c", "error": "requires canDelete" }, { "id": "e" }] })
            ),
            (
                &json!("notFound"),
                &picked,
                &json!(2),
                &json!({ "results": [{ "id": "c", "error": "notFound" }, { "id": "e" }] })
            ),
            (
                &json!("forbidden"),
                &picked,
                &json!(2),
                &json!({ "results": [{ "id": "c", "error": "forbidden" }, { "id": "e" }] })
            ),
        ],
        "the picked documents deleted only from their confirmation, given --yes: all of them, or all but one"
    );
    assert!(
        golden
            .iter()
            .filter(|g| g["command"] == "delete-docs")
            .all(|g| g["fixture"] == "confirm-delete-selected")
    );
}

#[test]
fn arguments_are_read_as_the_command_takes_them() {
    let interface = interface();
    let list = &interface.commands[0];
    let args = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
    assert_eq!(list.with_args(&[]), Ok(json!({ "sort": "updated", "limit": 50 })));
    assert_eq!(list.with_args(&args(&["--sort", "title"])), Ok(json!({ "sort": "title", "limit": 50 })));
    assert_eq!(list.with_args(&args(&["--sort=owner"])), Ok(json!({ "sort": "owner", "limit": 50 })));
    assert!(list.with_args(&args(&["--sort", "size"])).unwrap_err().contains("one of updated"));
    assert_eq!(
        list.with_args(&args(&["--cursor", "p2", "--limit", "3"])),
        Ok(json!({ "sort": "updated", "cursor": "p2", "limit": 3 }))
    );
    assert!(list.with_args(&args(&["--limit", "many"])).is_err());
    assert!(list.with_args(&args(&["--page", "2"])).is_err());
    let delete = &interface.commands[1];
    assert_eq!(delete.with_args(&args(&["b"])), Ok(json!({ "id": "b" })));
    assert!(delete.with_args(&[]).unwrap_err().contains("needs <id>"));
    assert!(delete.with_args(&args(&["b", "c"])).is_err());
    assert_eq!(delete.with_args(&args(&["b", "--yes"])), Ok(json!({ "id": "b" })), "--yes is no parameter");
    assert!(delete.yes(&args(&["b", "--yes"])) && !delete.yes(&args(&["b"])));
    assert!(list.with_args(&args(&["--yes"])).is_err(), "only a destructive command takes --yes");
}

/// The draft with its menu's delete calling deleteDoc itself, and deleteDoc
/// marked destructive or not.
fn deleting_from_the_menu(destructive: bool) -> Value {
    let mut text =
        std::fs::read_to_string(format!("{}/../../drafts/docs/screen.json", env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .replace(r#""open": "confirm-delete""#, r#""call": "deleteDoc""#);
    if !destructive {
        text = text.replace(
            r#""destructive": true,
      "bulk""#,
            r#""bulk""#,
        );
    }
    serde_json::from_str(&text).unwrap()
}

#[test]
fn a_command_whose_precondition_does_not_hold_is_refused() {
    let model = model(&deleting_from_the_menu(false), Some(&draft("kinds.json")))
        .unwrap_or_else(|c| panic!("{:#?}", c.findings));
    let interface = cli(&model);
    let golden: Vec<Value> = interface.golden.iter().map(|g| serde_json::to_value(g).unwrap()).collect();
    let deletes: Vec<(&Value, &Value, &Value, &Value)> = golden
        .iter()
        .filter(|g| g["command"] == "delete-doc" && g.get("fixture").is_some())
        .map(|g| (&g["fixture"], &g["args"], &g["exit"], &g["stdout"]))
        .collect();
    assert_eq!(
        deletes[..2],
        [
            (&json!("row-menu"), &json!(["b"]), &json!(1), &json!({ "error": "requires canDelete" })),
            (&json!("row-menu-own"), &json!(["c"]), &json!(0), &json!({})),
        ]
    );
}

#[test]
fn a_destructive_command_called_outside_its_confirmation_derives_no_cli() {
    let refused = model(&deleting_from_the_menu(true), Some(&draft("kinds.json"))).err().unwrap();
    assert!(
        refused
            .findings
            .iter()
            .any(|f| f.at == "doc-{item.doc.id}-delete" && f.problem.contains("destructive")),
        "{:#?}",
        refused.findings
    );
}

#[test]
fn a_destructive_command_refused_by_its_precondition_is_refused_with_yes_too() {
    // The confirmation opens on a document the person may not delete, and
    // its button is disabled there.
    let text =
        std::fs::read_to_string(format!("{}/../../drafts/docs/screen.json", env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .replace(
                r#""when": "canDelete && rowMenu.confirming""#,
                r#""when": "rowMenuOpen && rowMenu.confirming""#,
            )
            .replace(
                r#""docId": "c",
        "canDelete": true,
        "confirming": true"#,
                r#""docId": "c",
        "confirming": true"#,
            )
            .replace(
                r#""destructive": true,
              "text": "删除""#,
                r#""disabled": "!canDelete",
              "text": "删除""#,
            );
    let model = model(&serde_json::from_str(&text).unwrap(), Some(&draft("kinds.json")))
        .unwrap_or_else(|c| panic!("{:#?}", c.findings));
    let golden: Vec<Value> = cli(&model).golden.iter().map(|g| serde_json::to_value(g).unwrap()).collect();
    let deletes: Vec<(&Value, &Value, &Value)> = golden
        .iter()
        .filter(|g| g["command"] == "delete-doc" && g.get("fixture").is_some())
        .map(|g| (&g["args"], &g["exit"], &g["stdout"]))
        .collect();
    let refused = json!({ "error": "requires canDelete" });
    assert_eq!(
        deletes,
        [(&json!(["c"]), &json!(1), &refused), (&json!(["c", "--yes"]), &json!(1), &refused)]
    );
}

#[test]
fn it_answers_each_fixture_resolved_as_handoff_does() {
    let model = model(&draft("screen.json"), Some(&draft("kinds.json"))).unwrap();
    let fixtures = cli(&model).fixtures;
    let names: Vec<&String> = fixtures.keys().collect();
    let written: Vec<&String> = model.fixtures.iter().map(|(name, _)| name).collect();
    assert_eq!(names, written);
    for (name, data) in &model.fixtures {
        assert_eq!(&fixtures[name], data, "{name}");
    }
}
