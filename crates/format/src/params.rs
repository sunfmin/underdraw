//! A screen's params: what it is opened with. Each is a field of `data`
//! that every fixture gives, and that each `navigate` to the screen gives,
//! of its type, in `with`. One description alone cannot see who opens it,
//! so what a navigate gives is checked only among the descriptions under a
//! root (`check_among`).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::check::Findings;
use crate::field::Field;
use crate::schema::{Schema, object};

/// The params each screen declares, by the screen's name, each with its
/// field; a screen that declares none (no "params") is not in it, so
/// nothing that navigates to it is held to any.
pub type ScreenParams = BTreeMap<String, Vec<(String, Field)>>;

/// The params `screen` declares, each a field of `data` given by every
/// fixture and none that only the screen holds, fills from a query or
/// derives; each fault found added to `findings`.
pub(crate) fn read(screen: &Value, schema: &Schema, findings: &mut Findings) -> Option<Vec<(String, Field)>> {
    #[cfg(test)]
    crate::screen::tests::count("params");
    let declared = screen.get("params")?;
    let Some(names) = declared.as_array() else {
        findings.add("params", "params is a list of data's fields: [\"orderId\"]");
        return Some(Vec::new());
    };
    let roots = schema.roots();
    let given = given(screen);
    let mut params: Vec<(String, Field)> = Vec::new();
    for name in names {
        let Some(name) = name.as_str() else {
            findings.add("params", "params is a list of data's fields: [\"orderId\"]");
            continue;
        };
        let at = format!("params.{name}");
        if params.iter().any(|(param, _)| param == name) {
            findings.add(&at, "is named twice: name each param once");
            continue;
        }
        let Some((_, field)) = roots.iter().find(|(root, _)| root == name) else {
            findings.add(
                &at,
                &format!("{name} is not a field of data: a param is one, which each fixture gives"),
            );
            continue;
        };
        let held = if field.view {
            Some("is view state, which only the screen holds")
        } else if field.from.is_some() {
            Some("is filled by a query, once the screen is open")
        } else if field.derived.is_some() {
            Some("is derived, computed from other values")
        } else {
            None
        };
        if let Some(held) = held {
            findings
                .add(&at, &format!("{name} {held}, so nothing opens the screen with it: name another field"));
            continue;
        }
        for (fixture, keys) in &given {
            if !keys.contains(name) {
                findings.add(
                    &format!("fixtures.{fixture}"),
                    &format!("gives no {name}, a param: every fixture gives each param, as each navigate to the screen does"),
                );
            }
        }
        params.push((name.to_string(), field.clone()));
    }
    Some(params)
}

/// The fields of `data` each fixture writes, its `$patch`'s and `$sample`'s
/// included, by
/// fixture in the order written.
fn given(screen: &Value) -> Vec<(String, BTreeSet<String>)> {
    let mut given: Vec<(String, BTreeSet<String>)> = Vec::new();
    for (name, written) in object(screen, "fixtures") {
        let Some(written) = written.as_object() else {
            continue;
        };
        let mut keys = BTreeSet::new();
        for key in written.keys() {
            if key != "$patch" && key != "$sample" {
                keys.insert(key.clone());
            }
        }
        if let Some(Value::String(sample)) = written.get("$sample")
            && let Some(Value::Object(from)) = screen.get("samples").and_then(|samples| samples.get(sample))
        {
            keys.extend(from.keys().cloned());
        }
        if let Some(Value::String(base)) = written.get("$patch")
            && let Some((_, from)) = given.iter().find(|(fixture, _)| fixture == base)
        {
            for key in from {
                keys.insert(key.clone());
            }
        }
        given.push((name.clone(), keys));
    }
    given
}
