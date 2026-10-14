//! The documents draft handed off: the operations it declares, and the one
//! each event calls.

use serde_json::{Value, json};
use underdraw_handoff::handoff;

fn draft(file: &str) -> Value {
    let path = format!("{}/../../drafts/docs/{file}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn each_event_names_the_operation_it_calls() {
    let mapping = json!({
        "platform": "web", "language": "typescript", "testId": "{id}",
        "tokenNaming": { "pattern": "{group}.{name}" }, "kinds": {},
        "actions": { "call": "await operations[operation](with)" }
    });
    let handoff = handoff(&draft("screen.json"), &mapping, &draft("tokens.json"), Some(&draft("kinds.json")))
        .unwrap_or_else(|c| panic!("{:#?}", c.findings));
    let calls = |id: &str| -> Vec<(String, Option<String>, Value)> {
        let node = handoff.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"));
        node.events.iter().map(|e| (e.action.clone(), e.operation.clone(), e.with.clone())).collect()
    };
    assert_eq!(
        calls("doc-{item.doc.id}-delete"),
        [(
            "call".to_string(),
            Some("deleteDoc".to_string()),
            json!({ "target": "deleteDoc", "with": { "id": "{item.doc.id}" } })
        )]
    );
    assert_eq!(calls("sort-{item.id}")[0].1.as_deref(), Some("listDocs"));
    assert_eq!(calls("nav-new")[0].1.as_deref(), Some("newDocument"));
    assert_eq!(calls("doc-{item.doc.id}")[0].1, None, "navigate is not an operation");
    let operations = serde_json::to_value(&handoff.operations).unwrap();
    assert_eq!(
        operations[1],
        json!({
            "name": "deleteDoc", "does": "command", "doc": "Deletes a document for everyone in the team",
            "params": { "id": "string" }, "requires": "canDelete",
            "fails": { "notFound": "the document is already gone", "forbidden": "the person no longer owns it" }
        })
    );
    assert_eq!(operations[0]["result"], "Doc[]");
    assert_eq!(
        operations[0]["params"],
        json!({ "sort": "SortKey = \"updated\"" }),
        "a choice's default is shown"
    );
    assert_eq!(
        operations[0]["fills"],
        json!(["docs[].doc"]),
        "the field the query fills, through the wrapper"
    );
    assert_eq!(operations[2].get("fills"), None, "a command fills no field");
}

#[test]
fn each_state_expects_the_calls_its_nodes_make() {
    let handoff = handoff(
        &draft("screen.json"),
        &draft("platforms/web.json"),
        &draft("tokens.json"),
        Some(&draft("kinds.json")),
    )
    .unwrap_or_else(|c| panic!("{:#?}", c.findings));
    assert!(handoff.ok, "{:#?}", handoff.findings);
    let state =
        |fixture: &str| handoff.states.iter().find(|s| s.state == json!({ "fixture": fixture })).unwrap();
    let own = &state("row-menu-own").calls;
    assert_eq!(
        own["doc-c-delete"],
        json!([{ "on": "tap", "operation": "deleteDoc", "with": { "id": "c" } }])
    );
    assert_eq!(own["nav-new"], json!([{ "on": "tap", "operation": "newDocument", "with": {} }]));
    assert!(!own.contains_key("doc-c-copy"), "copying a link calls no operation");
    let sorting = &state("sort-open").calls;
    assert_eq!(sorting["sort-title"][0]["with"], json!({ "sort": "title" }));
    assert!(!state("default").calls.contains_key("doc-a-delete"), "a closed menu's items are not present");
}
