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
        "tokenNaming": { "pattern": "{group}.{name}" }, "components": {},
        "actions": { "call": "await operations[operation](with)" }
    });
    let handoff = handoff(&draft("screen.json"), &mapping, &draft("tokens.json"))
        .unwrap_or_else(|c| panic!("{:#?}", c.findings));
    let calls = |id: &str| -> Vec<(String, Option<String>, Value)> {
        let node = handoff.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"));
        node.events.iter().map(|e| (e.action.clone(), e.operation.clone(), e.with.clone())).collect()
    };
    assert_eq!(
        calls("confirm-delete-confirm"),
        [(
            "call".to_string(),
            Some("deleteDoc".to_string()),
            json!({ "target": "deleteDoc", "with": { "id": "{rowMenu.docId}" } })
        )]
    );
    assert_eq!(
        calls("doc-{item.doc.id}-delete")[0].0,
        "open",
        "the menu's delete only opens its confirmation"
    );
    assert_eq!(calls("sort-{item.id}")[0].1.as_deref(), Some("listDocs"));
    assert_eq!(calls("nav-new")[0].1.as_deref(), Some("newDocument"));
    assert_eq!(calls("doc-{item.doc.id}")[0].1, None, "navigate is not an operation");
    let operations = serde_json::to_value(&handoff.operations).unwrap();
    assert_eq!(
        operations[1],
        json!({
            "name": "deleteDoc", "does": "command", "doc": "Deletes a document for everyone in the team",
            "params": { "id": "string" }, "requires": "canDelete", "destructive": true, "bulk": "deleteDocs",
            "fails": { "notFound": "the document is already gone", "forbidden": "the person no longer owns it" }
        })
    );
    assert_eq!(
        operations[2],
        json!({
            "name": "deleteDocs", "does": "command",
            "doc": "Deletes a document for everyone in the team: each of several, one by one",
            "params": { "ids": "string[]" }, "requires": "canDelete", "destructive": true, "bulkOf": "deleteDoc",
            "fails": { "notFound": "the document is already gone", "forbidden": "the person no longer owns it" }
        }),
        "the bulk form, derived: many ids, and each item's precondition and failures"
    );
    assert_eq!(calls("confirm-delete-selected-confirm")[0].1.as_deref(), Some("deleteDocs"));
    assert_eq!(calls("delete-selected")[0].0, "open", "the toolbar's delete only opens its confirmation");
    assert_eq!(operations[0]["result"], "DocPage");
    assert_eq!(operations[0]["pages"], "cursor");
    assert_eq!(
        operations[0]["params"],
        json!({ "sort": "SortKey = \"updated\"", "cursor": "string?", "limit": "int = 50" }),
        "a choice's default is shown, and paging adds cursor and limit"
    );
    assert_eq!(
        operations[0]["fills"],
        json!(["result.docs → docs[].doc", "result.next → next"]),
        "each field of the result fills its own, through the wrapper"
    );
    assert_eq!(operations[3].get("fills"), None, "a command fills no field");
}

#[test]
fn each_state_expects_the_calls_its_nodes_make() {
    let handoff = handoff(&draft("screen.json"), &draft("platforms/web.json"), &draft("tokens.json"))
        .unwrap_or_else(|c| panic!("{:#?}", c.findings));
    assert!(handoff.ok, "{:#?}", handoff.findings);
    let state =
        |fixture: &str| handoff.states.iter().find(|s| s.state == json!({ "fixture": fixture })).unwrap();
    let own = &state("row-menu-own").calls;
    assert!(!own.contains_key("doc-c-delete"), "the menu's delete opens its confirmation and calls nothing");
    assert_eq!(
        state("confirm-delete").calls["confirm-delete-confirm"],
        json!([{ "on": "tap", "operation": "deleteDoc", "with": { "id": "c" } }])
    );
    assert_eq!(own["nav-new"], json!([{ "on": "tap", "operation": "newDocument", "with": {} }]));
    assert!(!own.contains_key("doc-c-copy"), "copying a link calls no operation");
    let sorting = &state("sort-open").calls;
    assert_eq!(sorting["sort-title"][0]["with"], json!({ "sort": "title", "cursor": null }));
    assert_eq!(
        state("more-to-come").calls["load-more"],
        json!([{ "on": "tap", "operation": "listDocs", "with": { "cursor": "p2" } }]),
        "loading more passes on where the last page said the next starts"
    );
    assert!(!state("default").calls.contains_key("load-more"), "the last page offers no more");
    assert!(!state("default").calls.contains_key("doc-a-delete"), "a closed menu's items are not present");
    assert_eq!(
        state("confirm-delete-selected").calls["confirm-delete-selected-confirm"],
        json!([{ "on": "tap", "operation": "deleteDocs", "with": { "ids": ["c", "e"] } }]),
        "the bulk form, given the selection"
    );
}
