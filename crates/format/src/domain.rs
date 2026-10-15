//! A project's domain (docs/adr/0020): the types, conditions and operations
//! every screen of the project shares, written once in domain.json at the
//! project's root. The caller hands it over in the description under
//! [`KEY`]; it is written into the description here, before anything reads
//! it, so the domain's are read exactly as a screen's own. A screen may add
//! its own, but one of a name the domain declares is a finding, and the
//! domain's takes its place.

use serde_json::{Map, Value};

use crate::check::Findings;
use crate::jobs;

/// Where the caller puts domain.json's contents in the description it
/// hands over.
pub const KEY: &str = "$domain";

/// The file the domain is written in, as findings in it name it.
const FILE: &str = "domain.json";

/// What a domain declares, each section keyed by name as a screen's is.
const SHARED: [&str; 3] = ["types", "conditions", "operations"];

/// `screen` with what it took from its domain written in, and what each
/// long command derives. The one way the description is read: everything
/// that reads one reads what this answers.
pub(crate) fn expand(screen: &Value, findings: &mut Findings) -> Value {
    jobs::expand(&share(screen, findings), findings)
}

/// `screen` with the domain under [`KEY`] written in: each section the
/// domain's entries first, in its order, then the screen's own. The domain
/// is taken out, so what reads the result never sees it twice.
fn share(screen: &Value, findings: &mut Findings) -> Value {
    let mut shared = screen.clone();
    let Some(root) = shared.as_object_mut() else {
        return shared;
    };
    let domain = match root.remove(KEY) {
        None => return shared,
        Some(Value::Object(domain)) => domain,
        Some(_) => {
            findings
                .add(FILE, "the domain is an object: {\"types\": …, \"conditions\": …, \"operations\": …}");
            return shared;
        }
    };
    for key in domain.keys() {
        if !SHARED.contains(&key.as_str()) && !key.starts_with('$') {
            findings.add(
                &format!("{FILE} {key}"),
                &format!(
                    "the domain holds only {}: a screen's own goes in its screen.json",
                    SHARED.join(", ")
                ),
            );
        }
    }
    for key in SHARED {
        let declared = match domain.get(key) {
            None => continue,
            Some(Value::Object(declared)) => declared,
            Some(_) => {
                findings
                    .add(&format!("{FILE} {key}"), &format!("\"{key}\" maps each name to what it declares"));
                continue;
            }
        };
        let mut merged = declared.clone();
        for (name, body) in take_object(root, key) {
            if merged.contains_key(&name) && !name.starts_with('$') {
                findings.add(
                    &format!("{key}.{name}"),
                    &format!(
                        "shadows the domain's {name}, declared in {FILE}: use the domain's by name, or give the screen's own another name"
                    ),
                );
                continue;
            }
            merged.insert(name, body);
        }
        root.insert(key.to_string(), Value::Object(merged));
    }
    shared
}

fn take_object(root: &mut Map<String, Value>, key: &str) -> Map<String, Value> {
    match root.remove(key) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_domains_come_first_and_a_screens_own_of_its_names_is_a_finding() {
        let screen = json!({
            "screen": "Orders",
            "types": {"Order": {"id": "string"}, "Row": {"order": "Order"}},
            "operations": {"cancelOrder": {"command": "mine"}},
            KEY: {
                "types": {"Order": {"id": "string", "total": "number"}},
                "operations": {"cancelOrder": {"command": "Cancels it"}, "listOrders": {"query": "Lists them"}},
            },
        });
        let mut findings = Findings::default();
        let shared = share(&screen, &mut findings);
        assert_eq!(shared.get(KEY), None, "the domain is taken out");
        let names = |key: &str| shared[key].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(names("types"), ["Order", "Row"]);
        assert_eq!(names("operations"), ["cancelOrder", "listOrders"]);
        assert_eq!(shared["types"]["Order"]["total"], "number", "the domain's takes its place");
        assert_eq!(shared["operations"]["cancelOrder"]["command"], "Cancels it");
        let at: Vec<&str> = findings.0.iter().map(|f| f.at.as_str()).collect();
        assert_eq!(at, ["types.Order", "operations.cancelOrder"]);
        assert!(findings.0[0].problem.contains("shadows the domain's Order"), "{:?}", findings.0);
    }

    #[test]
    fn a_domain_of_the_wrong_shape_is_named_in_domain_json() {
        let mut findings = Findings::default();
        share(&json!({KEY: {"types": [], "fixtures": {}}}), &mut findings);
        let at: Vec<&str> = findings.0.iter().map(|f| f.at.as_str()).collect();
        assert_eq!(at, ["domain.json fixtures", "domain.json types"]);

        let mut findings = Findings::default();
        let alone = json!({"screen": "Alone", "types": {"A": {"x": "string"}}});
        assert_eq!(share(&alone, &mut findings), alone, "no domain, nothing changes");
        assert!(findings.0.is_empty());
    }
}
