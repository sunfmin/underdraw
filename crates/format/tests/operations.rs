//! `check` on the documents draft's operations, with one fault planted at a
//! time: each is found where it was planted, and says what is wrong.

use serde_json::Value;
use underdraw_format::{Checked, Does, Fill, check, model};

const DRAFT: &str = include_str!("../../../drafts/docs/screen.json");
const KINDS: &str = include_str!("../../../drafts/docs/kinds.json");

fn kinds() -> Value {
    serde_json::from_str(KINDS).unwrap()
}

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
    let model = model(&screen, Some(&kinds())).unwrap_or_else(|c| panic!("{:#?}", c.findings));
    let read: Vec<_> = model
        .operations
        .all()
        .iter()
        .map(|o| (o.name.as_str(), o.does, o.requires.as_deref(), o.result.as_ref().map(|r| r.describe())))
        .collect();
    assert_eq!(
        read,
        [
            ("listDocs", Does::Query, None, Some("Doc[]".to_string())),
            ("deleteDoc", Does::Command, Some("canDelete"), None),
            ("newDocument", Does::Command, None, Some("Doc".to_string())),
        ]
    );
    let list = model.operations.get("listDocs").unwrap();
    assert_eq!(list.fills, [Fill { field: "docs".to_string(), path: "docs[].doc".to_string() }]);
    let sort = &list.params[0].1;
    assert_eq!(
        (sort.describe(), sort.default.clone()),
        ("SortKey".to_string(), Some(Value::from("updated")))
    );
    assert!(model.operations.all().iter().filter(|o| o.name != "listDocs").all(|o| o.fills.is_empty()));
}

#[test]
fn each_planted_fault_is_found_where_it_was_planted() {
    // (name, what the draft says, the fault planted in its place, where it is found, what is said)
    let delete_with = r#""call": "deleteDoc",
                    "with": {
                      "id": "{item.doc.id}"
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
            r#""from": "listDocs""#,
            r#""from": "fetchDocs""#,
            "data.docs.from",
            "fetchDocs is not an operation",
        ),
        (
            "from-a-command",
            r#""from": "listDocs""#,
            r#""from": "newDocument""#,
            "data.docs.from",
            "newDocument is a command",
        ),
        (
            "filled-field-not-the-result",
            r#""type": "DocRow[] ..50",
      "from""#,
            r#""type": "DocRow",
      "from""#,
            "data.docs",
            "is DocRow, and listDocs answers with Doc[]",
        ),
        (
            "filled-field-of-another-type",
            r#""type": "DocRow[] ..50",
      "from""#,
            r#""type": "SortOption[] ..50",
      "from""#,
            "data.docs",
            "is SortOption[], and listDocs answers with Doc[]",
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
            r#""result": "Doc[] ..50""#,
            r#""result": "Docs[] ..50""#,
            "operations.listDocs.result",
            "unknown type Docs",
        ),
        (
            "built-in-result",
            r#""result": "Doc[] ..50""#,
            r#""result": "string""#,
            "operations.listDocs.result",
            "a result is one of types",
        ),
        (
            "result-holds-view-state",
            r#""result": "Doc[] ..50""#,
            r#""result": "DocRow[] ..50""#,
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
            "doc-{item.doc.id}-delete.on.tap.with",
            "deleteDoc needs id (string)",
        ),
        (
            "with-an-extra-parameter",
            delete_with,
            r#""call": "deleteDoc", "with": { "id": "{item.doc.id}", "force": true }"#,
            "doc-{item.doc.id}-delete.on.tap.with.force",
            "deleteDoc has no parameter force",
        ),
        (
            "with-of-the-wrong-type",
            delete_with,
            r#""call": "deleteDoc", "with": { "id": "{item.doc.updated}" }"#,
            "doc-{item.doc.id}-delete.on.tap.with.id",
            "is date, and the parameter is string",
        ),
        (
            "with-an-object",
            delete_with,
            r#""call": "deleteDoc", "with": { "id": "{item.doc}" }"#,
            "doc-{item.doc.id}-delete.on.tap.with.id",
            "is Doc, and the parameter is string",
        ),
        (
            "with-a-path-that-is-not-one",
            delete_with,
            r#""call": "deleteDoc", "with": { "id": "{item.docId}" }"#,
            "doc-{item.doc.id}-delete.on.tap.with.id",
            "item.docId is not a path here",
        ),
        (
            "not-gated",
            r#""disabled": "!canDelete","#,
            "",
            "doc-{item.doc.id}-delete",
            "calls deleteDoc, which requires canDelete",
        ),
        (
            "gated-by-another-condition",
            r#""disabled": "!canDelete","#,
            r#""disabled": "!rowMenuOpen","#,
            "doc-{item.doc.id}-delete",
            "calls deleteDoc, which requires canDelete",
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
    ];
    for (name, from, to, at, said) in cases {
        let checked = check(&planted(name, from, to), Some(&kinds()));
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
    for (name, from, to) in cases {
        let mut screen = planted(name, from, to);
        if name == "implied" {
            let delete = "/root/children/2/item/overlay/children/4";
            screen.pointer_mut(delete).unwrap()["disabled"] = Value::from("!mayDeleteNow");
        }
        let checked = check(&screen, Some(&kinds()));
        assert!(checked.ok, "{name}: {:#?}", checked.findings);
    }
}
