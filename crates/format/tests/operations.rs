//! `check` on the documents draft's operations, with one fault planted at a
//! time: each is found where it was planted, and says what is wrong.

use serde_json::Value;
use underdraw_format::{Checked, Does, Fill, Screen, Shared};

const DRAFT: &str = include_str!("../../../drafts/docs/screen.json");

/// The draft with `from` replaced by `to`, where `from` appears exactly once.
fn planted(name: &str, from: &str, to: &str) -> Value {
    assert_eq!(DRAFT.matches(from).count(), 1, "{name}: {from:?} must appear exactly once in the draft");
    serde_json::from_str(&DRAFT.replace(from, to)).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"))
}

fn found(checked: &Checked, at: &str, said: &str) -> bool {
    checked.findings.iter().any(|f| f.at == at && f.problem.contains(said))
}

#[test]
fn the_draft_declares_its_operations_and_passes() {
    let screen: Value = serde_json::from_str(DRAFT).unwrap();
    let screen = read(&screen);
    let model = underdraw_format::model(&screen).unwrap_or_else(|c| panic!("{:#?}", c.findings));
    let read: Vec<_> = model
        .operations
        .all()
        .iter()
        .map(|o| (o.name.as_str(), o.does, o.requires.as_deref(), o.result.as_ref().map(|r| r.describe())))
        .collect();
    assert_eq!(
        read,
        [
            ("listDocs", Does::Query, None, Some("DocPage".to_string())),
            ("deleteDoc", Does::Command, Some("canDelete"), None),
            ("deleteDocs", Does::Command, Some("canDelete"), None),
            ("newDocument", Does::Command, None, Some("Doc".to_string())),
        ]
    );
    let list = model.operations.get("listDocs").unwrap();
    assert_eq!(
        list.fills,
        [
            Fill {
                field: "docs".to_string(),
                part: Some("docs".to_string()),
                path: "docs[].doc".to_string()
            },
            Fill { field: "next".to_string(), part: Some("next".to_string()), path: "next".to_string() },
        ]
    );
    // Paging by a cursor adds the cursor and the limit, by default as many
    // rows as a page may hold.
    assert!(list.pages);
    let params: Vec<_> =
        list.params.iter().map(|(name, field)| (name.as_str(), field.describe(), field.hi)).collect();
    assert_eq!(
        params,
        [
            ("sort", "SortKey".to_string(), None),
            ("cursor", "string?".to_string(), None),
            ("limit", "int".to_string(), Some(50.0))
        ]
    );
    assert_eq!(list.params[2].1.default, Some(Value::from(50)));
    let sort = &list.params[0].1;
    assert_eq!(
        (sort.describe(), sort.default.clone()),
        ("SortKey".to_string(), Some(Value::from("updated")))
    );
    assert!(model.operations.all().iter().filter(|o| o.name != "listDocs").all(|o| o.fills.is_empty()));
    let delete = model.operations.get("deleteDoc").unwrap();
    assert_eq!(delete.bulk.as_deref(), Some("deleteDocs"));
    let bulk = model.operations.get("deleteDocs").unwrap();
    let of = bulk.bulk_of.as_ref().unwrap();
    assert_eq!((of.operation.as_str(), of.key.as_str(), of.keys.as_str()), ("deleteDoc", "id", "ids"));
    let params: Vec<_> =
        bulk.params.iter().map(|(name, field)| (name.as_str(), field.describe(), field.lo)).collect();
    assert_eq!(params, [("ids", "string[]".to_string(), Some(1.0))], "a list of at least one");
    assert!(bulk.destructive, "a bulk delete is as destructive as deleting one");
    assert_eq!(bulk.fails, delete.fails, "each item fails as one does");
}

#[test]
fn each_planted_fault_is_found_where_it_was_planted() {
    // (name, what the draft says, the fault planted in its place, where it is found, what is said)
    let delete_with = r#""call": "deleteDoc",
                  "with": {
                    "id": "{rowMenu.docId}"
                  }"#;
    let cases = [
        (
            "unknown-param-type",
            r#""sort": "SortKey = \"updated\"""#,
            r#""sort": "SortKeys""#,
            "operations.listDocs.params.sort",
            "unknown type SortKeys",
        ),
        (
            "param-default-not-a-choice",
            r#""sort": "SortKey = \"updated\"""#,
            r#""sort": "SortKey = \"modified\"""#,
            "operations.listDocs.params.sort",
            "its default \"modified\" is not one of SortKey's choices: updated, created, title, owner",
        ),
        (
            "with-a-literal-not-a-choice",
            r#""sort": "{item.id}""#,
            r#""sort": "modified""#,
            "sort-{item.id}.on.tap.with.sort",
            "\"modified\" is not one of SortKey's choices",
        ),
        (
            "with-a-path-not-of-the-choices",
            r#""sort": "{item.id}""#,
            r#""sort": "{item.label}""#,
            "sort-{item.id}.on.tap.with.sort",
            "is string, and the parameter is SortKey",
        ),
        (
            "from-an-unknown-operation",
            r#""from": "listDocs.docs""#,
            r#""from": "fetchDocs.docs""#,
            "data.docs.from",
            "fetchDocs is not an operation",
        ),
        (
            "from-a-command",
            r#""from": "listDocs.docs""#,
            r#""from": "newDocument""#,
            "data.docs.from",
            "newDocument is a command",
        ),
        (
            "filled-field-not-the-result",
            r#""type": "DocRow[]",
      "from""#,
            r#""type": "DocRow",
      "from""#,
            "data.docs",
            "is DocRow, and listDocs.docs answers with Doc[]",
        ),
        (
            "filled-field-of-another-type",
            r#""type": "DocRow[]",
      "from""#,
            r#""type": "SortOption[]",
      "from""#,
            "data.docs",
            "is SortOption[], and listDocs.docs answers with Doc[]",
        ),
        (
            "from-on-a-type's-field",
            r#""doc": "Doc",
      "menuOpen""#,
            r#""doc": { "type": "Doc", "from": "listDocs" },
      "menuOpen""#,
            "types.DocRow.doc",
            "only a field of data is filled by a query",
        ),
        (
            "result-reaches-app-derived",
            r#""result": "Doc",
      "fails""#,
            r#""result": "Summary",
      "fails""#,
            "operations.newDocument.result",
            "Summary holds Summary.count, which the app derives",
        ),
        (
            "derived-without-who",
            r#""derived": { "app": "docs' length" }"#,
            r#""derived": "docs' length""#,
            "types.Summary.count",
            "derived says who computes it",
        ),
        (
            "view-state-derived-by-the-server",
            r#""derived": { "app": "rowMenu is open on this document" }"#,
            r#""derived": { "server": "rowMenu is open on this document" }"#,
            "types.DocRow.menuOpen",
            "the app derives it, not the server",
        ),
        (
            "unknown-result-type",
            r#""result": "DocPage""#,
            r#""result": "DocPages""#,
            "operations.listDocs.result",
            "unknown type DocPages",
        ),
        (
            "built-in-result",
            r#""result": "DocPage""#,
            r#""result": "string""#,
            "operations.listDocs.result",
            "a result is one of types",
        ),
        (
            "result-holds-view-state",
            r#""result": "DocPage""#,
            r#""result": "DocRow""#,
            "operations.listDocs.result",
            "DocRow holds view state (DocRow.menuOpen)",
        ),
        (
            "result-reaches-view-state",
            r#""result": "Doc",
      "fails""#,
            r#""result": "Sort",
      "fails""#,
            "operations.newDocument.result",
            "Sort holds view state (Sort.menuOpen)",
        ),
        (
            "unknown-precondition",
            r#""requires": "canDelete""#,
            r#""requires": "mayDelete""#,
            "operations.deleteDoc.requires",
            "is not a condition",
        ),
        (
            "neither-query-nor-command",
            r#""command": "Deletes a document for everyone in the team""#,
            r#""does": "Deletes a document for everyone in the team""#,
            "operations.deleteDoc",
            "exactly one of \"query\"",
        ),
        (
            "call-unknown-operation",
            r#""call": "newDocument"
            }
          }
        }
      }
    ]"#,
            r#""call": "createDoc"
            }
          }
        }
      }
    ]"#,
            "empty-new.on.tap.call",
            "\"createDoc\" is not an operation",
        ),
        (
            "with-missing-a-parameter",
            delete_with,
            r#""call": "deleteDoc""#,
            "confirm-delete-confirm.on.tap.with",
            "deleteDoc needs id (string)",
        ),
        (
            "with-an-extra-parameter",
            delete_with,
            r#""call": "deleteDoc", "with": { "id": "{rowMenu.docId}", "force": true }"#,
            "confirm-delete-confirm.on.tap.with.force",
            "deleteDoc has no parameter force",
        ),
        (
            "with-of-the-wrong-type",
            delete_with,
            r#""call": "deleteDoc", "with": { "id": "{rowMenu.canDelete}" }"#,
            "confirm-delete-confirm.on.tap.with.id",
            "is bool, and the parameter is string",
        ),
        (
            "with-an-object",
            delete_with,
            r#""call": "deleteDoc", "with": { "id": "{rowMenu}" }"#,
            "confirm-delete-confirm.on.tap.with.id",
            "is RowMenu, and the parameter is string",
        ),
        (
            "with-a-path-that-is-not-one",
            delete_with,
            r#""call": "deleteDoc", "with": { "id": "{rowMenu.id}" }"#,
            "confirm-delete-confirm.on.tap.with.id",
            "rowMenu.id is not a path here",
        ),
        (
            "gated-by-another-condition",
            r#""if": "confirmingDelete","#,
            r#""if": "rowMenuOpen","#,
            "confirm-delete-confirm",
            "calls deleteDoc, which requires canDelete",
        ),
        (
            "destructive-not-a-bool",
            r#""destructive": true,
      "bulk""#,
            r#""destructive": "yes",
      "bulk""#,
            "operations.deleteDoc.destructive",
            "destructive is true or false",
        ),
        (
            "bulk-not-a-name",
            r#""bulk": "deleteDocs""#,
            r#""bulk": true"#,
            "operations.deleteDoc.bulk",
            "bulk names the operation's bulk form",
        ),
        (
            "bulk-name-taken",
            r#""bulk": "deleteDocs""#,
            r#""bulk": "newDocument""#,
            "operations.deleteDoc.bulk",
            "newDocument is already an operation",
        ),
        (
            "bulk-of-a-long-command",
            r#""bulk": "deleteDocs""#,
            r#""long": true, "bulk": "deleteDocs""#,
            "operations.deleteDoc.bulk",
            "mark it long or bulk, not both",
        ),
        (
            "bulk-query",
            r#""params": {
        "sort""#,
            r#""bulk": "listMany",
      "params": {
        "sort""#,
            "operations.listDocs.bulk",
            "bulk marks a command",
        ),
        (
            "bulk-key-with-a-default",
            r#""id": "string"
      },"#,
            r#""id": "string = \"a\""
      },"#,
            "operations.deleteDoc.bulk",
            "make the first parameter the one each item is",
        ),
        (
            "bulk-confirmation-without-a-backdrop",
            r#""id": "confirm-delete-selected",
        "component": "Dialog""#,
            r#""id": "confirm-delete-selected",
        "component": "Card""#,
            "confirm-delete-selected-confirm",
            "calls deleteDocs, which is destructive",
        ),
        (
            "bulk-call-not-gated",
            r#""if": "confirmingDeleteSelected","#,
            r#""if": "confirmingDelete","#,
            "confirm-delete-selected-confirm",
            "calls deleteDocs with selection.ids, and is not gated on a non-empty selection",
        ),
        (
            "bulk-call-on-a-selection-that-may-be-empty",
            r#""type": "string[] 1..50""#,
            r#""type": "string[] ..50""#,
            "confirm-delete-selected-confirm",
            "is not gated on a non-empty selection",
        ),
        (
            "bulk-call-not-given-the-selection",
            r#""ids": "{selection.ids}""#,
            r#""ids": "c""#,
            "confirm-delete-selected-confirm",
            "give ids the selection, as one path",
        ),
        (
            "bulk-call-given-domain-data",
            r#""ids": "{selection.ids}""#,
            r#""ids": "{docs}""#,
            "confirm-delete-selected-confirm",
            "calls deleteDocs with docs, which is domain data",
        ),
        (
            "destructive-query",
            r#""params": {
        "sort""#,
            r#""destructive": true,
      "params": {
        "sort""#,
            "operations.listDocs.destructive",
            "destructive marks a command",
        ),
        (
            "destructive-called-outside-its-confirmation",
            r#""open": "confirm-delete""#,
            r#""call": "deleteDoc""#,
            "doc-{item.doc.id}-delete",
            "calls deleteDoc, which is destructive: call it only from inside its confirmation",
        ),
        (
            "confirmation-no-node-opens",
            r#""open": "confirm-delete""#,
            r#""open": "confirm""#,
            "confirm-delete-confirm",
            "calls deleteDoc, which is destructive",
        ),
        (
            "confirmation-without-a-backdrop",
            r#""id": "confirm-delete",
        "component": "Dialog""#,
            r#""id": "confirm-delete",
        "component": "Card""#,
            "confirm-delete-confirm",
            "calls deleteDoc, which is destructive",
        ),
        (
            "view-not-a-bool",
            r#""view": true,
      "doc": "The document"#,
            r#""view": "yes",
      "doc": "The document"#,
            "data.rowMenu",
            "view is true or false",
        ),
        (
            "toggle-domain-data",
            r#""toggle": "sort.menuOpen""#,
            r#""toggle": "summary.count""#,
            "sort-button.on.tap.toggle",
            "toggle writes summary.count, which is domain data",
        ),
        (
            "set-domain-data",
            r#""toggle": "sort.menuOpen""#,
            r#""set": "sort.label", "to": "名称""#,
            "sort-button.on.tap.set",
            "set writes sort.label, which is domain data",
        ),
        (
            "set-a-path-that-is-not-one",
            r#""toggle": "sort.menuOpen""#,
            r#""set": { "sort.open": true }"#,
            "sort-button.on.tap.set",
            "sort.open is not one",
        ),
        (
            "reload-is-not-free",
            r#""call": "listDocs",
                "with": {
                  "sort": "updated",
                  "cursor": null
                }"#,
            r#""reload": true"#,
            "retry.on.tap.reload",
            "reload is not an action the screen does by itself",
        ),
        (
            "submit-is-not-free",
            delete_with,
            r#""submit": "deleteDoc", "with": { "id": "{rowMenu.docId}" }"#,
            "confirm-delete-confirm.on.tap.submit",
            "what reads or changes domain data is an operation, called with \"call\"",
        ),
        (
            "pages-by-another-means",
            r#""pages": "cursor""#,
            r#""pages": "offset""#,
            "operations.listDocs.pages",
            "pages is \"cursor\"",
        ),
        (
            "a-command-that-pages",
            r#""command": "Deletes a document for everyone in the team","#,
            r#""command": "Deletes a document for everyone in the team", "pages": "cursor","#,
            "operations.deleteDoc.pages",
            "only a query pages",
        ),
        (
            "paging-written-in-params",
            r#""sort": "SortKey = \"updated\"""#,
            r#""sort": "SortKey = \"updated\"", "cursor": "string?""#,
            "operations.listDocs.params.cursor",
            "pages adds cursor and limit itself",
        ),
        (
            "a-paged-result-without-next",
            r#""next": "string?""#,
            r#""after": "string?""#,
            "operations.listDocs.result",
            "DocPage has no \"next\": \"string?\"",
        ),
        (
            "a-paged-result-that-is-a-list",
            r#""result": "DocPage""#,
            r#""result": "Doc[] ..50""#,
            "operations.listDocs.result",
            "listDocs pages, so it answers with one page",
        ),
        (
            "a-page-without-its-most-rows",
            r#""docs": "Doc[] ..50""#,
            r#""docs": "Doc[]""#,
            "operations.listDocs.result",
            "DocPage.docs says how many rows a page holds at most",
        ),
        (
            "from-a-part-that-is-not-one",
            r#""from": "listDocs.next""#,
            r#""from": "listDocs.after""#,
            "data.next.from",
            "after is not a field of listDocs's result (DocPage)",
        ),
        (
            "load-more-without-the-cursor",
            r#""cursor": "{next}""#,
            r#""sort": "title""#,
            "load-more.on.tap.with",
            "listDocs pages: give cursor, null for the first page, or \"{next}\"",
        ),
        (
            "load-more-with-another-cursor",
            r#""cursor": "{next}""#,
            r#""cursor": "p2""#,
            "load-more.on.tap.with.cursor",
            "the cursor is null for the first page, or \"{next}\"",
        ),
        (
            "load-more-past-the-last-page",
            r#""if": "hasMore","#,
            "",
            "load-more.on.tap.with.cursor",
            "next is null on the last page: offer more only while it is not",
        ),
    ];
    for (name, from, to, at, said) in cases {
        let checked = check(&planted(name, from, to));
        assert!(!checked.ok, "{name}");
        assert!(
            found(&checked, at, said),
            "{name}: expected a finding at {at} saying {said:?}, got {:#?}",
            checked.findings
        );
    }
}

#[test]
fn a_call_is_gated_by_an_if_or_by_a_condition_that_implies_it() {
    let cases = [
        // `if` on the node itself, in place of `disabled`
        ("if-on-the-node", r#""disabled": "!canDelete","#, r#""if": "canDelete","#),
        // a condition whose && chain names the one required
        (
            "implied",
            r#""label": "May delete it"
    }"#,
            r#""label": "May delete it"
    },
    "mayDeleteNow": { "when": "canDelete && hasDocs", "label": "May delete it now" }"#,
        ),
        // a handler's cases branch
        (
            "cases-branch",
            r#""call": "deleteDoc",
                    "with": {
                      "id": "{item.doc.id}"
                    }"#,
            r#""cases": [ { "if": "canDelete", "then": { "call": "deleteDoc", "with": { "id": "{item.doc.id}" } } },
                              { "else": { "open": "why-not" } } ]"#,
        ),
    ];
    // Gating alone: deleteDoc (and so deleteDocs) not destructive, and
    // called by the menu item.
    let direct = DRAFT
        .replace(
            r#""destructive": true,
      "bulk""#,
            r#""bulk""#,
        )
        .replace(r#""open": "confirm-delete""#, r#""call": "deleteDoc""#);
    for (name, from, to) in cases {
        assert_eq!(direct.matches(from).count(), 1, "{name}: {from:?} must appear exactly once");
        let mut screen: Value = serde_json::from_str(&direct.replace(from, to)).unwrap();
        // The draft's transitions follow the confirmation this rewiring
        // takes out; they are not what is checked here.
        screen.as_object_mut().unwrap().remove("transitions");
        if name == "implied" {
            let delete = "/root/children/2/item/overlay/children/4";
            screen.pointer_mut(delete).unwrap()["disabled"] = Value::from("!mayDeleteNow");
        }
        let checked = check(&screen);
        assert!(checked.ok, "{name}: {:#?}", checked.findings);
    }
}

#[test]
fn set_and_toggle_write_view_state_and_the_rest_moves_the_ui() {
    let cases = [
        // a field marked view
        ("view-field", r#""toggle": "sort.menuOpen""#, r#""set": "sort.menuOpen", "to": false"#),
        // a field inside one marked view
        ("inside-view-state", r#""toggle": "sort.menuOpen""#, r#""set": { "rowMenu.docId": "d1" }"#),
        // actions that touch no data
        ("copy", r#""toggle": "sort.menuOpen""#, r#""copy": "DocumentList""#),
        ("close", r#""toggle": "sort.menuOpen""#, r#""close": "sort-menu""#),
    ];
    for (name, from, to) in cases {
        let checked = check(&planted(name, from, to));
        assert!(checked.ok, "{name}: {:#?}", checked.findings);
    }
}

/// `description` read as a screen of its own, with nothing shared.
fn read(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&read(description))
}
