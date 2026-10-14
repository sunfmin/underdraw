//! The JSON Schema derived from a description agrees with `check`: it
//! accepts every fixture of every draft, and refuses data exactly when
//! `check` finds a fault in a fixture holding it.

use std::path::Path;

use serde_json::{Value, json};
use underdraw_format::{check, model};
use underdraw_handoff::json_schema;

const DRAFTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts");

fn read(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// Whether `check` and the schema agree on `data`, written as a fixture of
/// its own; what each says when they do not.
fn agree(screen: &Value, kinds: Option<&Value>, data: &Value) -> Result<bool, String> {
    let schema = json_schema(&model(screen, kinds).map_err(|c| format!("{:?}", c.findings))?);
    let validator = jsonschema::validator_for(&schema).unwrap();
    let mut probed = screen.clone();
    probed["fixtures"]["probe"] = data.clone();
    let findings: Vec<_> =
        check(&probed, kinds).findings.into_iter().filter(|f| f.at.starts_with("fixtures.probe")).collect();
    let by_schema = validator.is_valid(data);
    if by_schema == findings.is_empty() {
        Ok(by_schema)
    } else {
        Err(format!("schema says {by_schema}, check finds {findings:?}, of {data}"))
    }
}

#[test]
fn every_draft_fixture_is_valid_and_every_planted_fault_is_refused() {
    let mut drafts = 0;
    for entry in std::fs::read_dir(DRAFTS).unwrap() {
        let dir = entry.unwrap().path();
        let Some(screen) = read(&dir.join("screen.json")) else { continue };
        let kinds = read(&dir.join("kinds.json"));
        drafts += 1;
        let model = model(&screen, kinds.as_ref()).unwrap_or_else(|c| panic!("{dir:?}: {:?}", c.findings));
        let schema = json_schema(&model);
        let validator = jsonschema::validator_for(&schema).unwrap();
        for (name, data) in &model.fixtures {
            let errors: Vec<String> = validator.iter_errors(data).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "{dir:?} fixture {name}: {errors:?}");
        }
        let (_, base) = &model.fixtures[0];
        let mut planted = vec![];
        let mut unknown = base.clone();
        unknown["notDeclared"] = json!(1);
        planted.push(unknown);
        for (field, spec) in schema["properties"].as_object().unwrap() {
            if schema["required"].as_array().unwrap().contains(&json!(field)) {
                let mut missing = base.clone();
                missing.as_object_mut().unwrap().remove(field);
                planted.push(missing);
            }
            if spec.get("anyOf").is_none() {
                let mut null = base.clone();
                null[field] = Value::Null;
                planted.push(null);
            }
        }
        for data in &planted {
            assert_eq!(agree(&screen, kinds.as_ref(), data), Ok(false), "{dir:?}");
        }
    }
    assert!(drafts >= 7, "found only {drafts} drafts under {DRAFTS}");
}

#[test]
fn each_type_is_bounded_as_check_bounds_it() {
    let screen = json!({
        "screen": "Probe",
        "data": {
            "name": "string 1..5",
            "count": "int 0..3",
            "score": "number? 0..5",
            "on": "bool = false",
            "day": "date",
            "price": "money",
            "plan": "Plan",
            "tags": "string[] ..2",
            "owner": { "type": "Owner?", "doc": "Who owns it", "derived": { "server": "from the session" } }
        },
        "types": {
            "Plan": "enum(monthly, yearly)",
            "Owner": { "id": "string" }
        },
        "fixtures": {
            "a": { "name": "ab", "count": 1, "score": 4.5, "on": true, "day": "2026-01-02",
                   "price": { "amount": 100, "currency": "JPY" }, "plan": "yearly", "tags": [], "owner": { "id": "u" } },
            "b": { "$patch": "a", "on": false, "owner": null }
        },
        "conditions": { "isOn": { "when": "on", "label": "on" } },
        "root": { "id": "root", "kind": "Screen", "children": [
            { "id": "t", "kind": "Text", "text": "{name}", "if": "isOn" } ] }
    });
    let base = json!({ "name": "ab", "count": 1, "day": "2026-01-02",
                       "price": { "amount": 100, "currency": "JPY" }, "plan": "yearly", "tags": [] });
    let cases = [
        (json!({}), true),
        (json!({ "score": null, "owner": null, "on": true }), true),
        (json!({ "name": "" }), false),
        (json!({ "name": "abcdef" }), false),
        (json!({ "count": 4 }), false),
        (json!({ "count": 1.5 }), false),
        (json!({ "score": 6 }), false),
        (json!({ "day": "2026/01/02" }), false),
        (json!({ "price": { "amount": 1, "currency": "YEN!" } }), false),
        (json!({ "price": { "amount": 1, "currency": "JPY", "x": 1 } }), false),
        (json!({ "plan": "weekly" }), false),
        (json!({ "tags": ["a", "b", "c"] }), false),
        (json!({ "owner": { "id": "u", "name": "x" } }), false),
        (json!({ "owner": {} }), false),
    ];
    for (change, valid) in cases {
        let mut data = base.clone();
        for (key, value) in change.as_object().unwrap() {
            data[key] = value.clone();
        }
        assert_eq!(agree(&screen, None, &data), Ok(valid), "{change}");
    }
    let schema = json_schema(&model(&screen, None).unwrap_or_else(|c| panic!("{:?}", c.findings)));
    assert_eq!(schema["properties"]["owner"]["description"], "Who owns it. Derived by the server: from the session");
    assert_eq!(schema["properties"]["on"]["default"], false);
}
