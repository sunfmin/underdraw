//! Fixtures (docs/format/data.md): each one a value of the view model, with
//! `$patch` starting from another. Read whole: merged, defaults filled, and
//! every field checked against its type.

use serde_json::{Map, Value};

use crate::check::{Finding, Findings};
use crate::field::{Base, Field};
use crate::schema::{Schema, Shape, object};

/// Every fixture by name, in the order written, with defaults filled in.
pub fn read(screen: &Value, schema: &Schema, findings: &mut Findings) -> Vec<(String, Value)> {
    #[cfg(test)]
    crate::screen::tests::count("fixtures");
    let roots = schema.roots();
    let mut merged: Vec<(String, Value)> = Vec::new();
    let mut read: Vec<(String, Value)> = Vec::new();
    for (name, written) in object(screen, "fixtures") {
        let at = format!("fixtures.{name}");
        let Some(written) = written.as_object() else {
            findings.add(&at, "a fixture is an object");
            continue;
        };
        let mut own = written.clone();
        let base = own.remove("$patch");
        let value = match &base {
            None => Value::Object(own),
            Some(Value::String(base)) => match merged.iter().find(|(n, _)| n == base) {
                Some((_, from)) => merge(from.clone(), Value::Object(own)),
                None => {
                    findings.add(&at, &format!("$patch names {base}, which is not a fixture above it"));
                    continue;
                }
            },
            Some(_) => {
                findings.add(&at, "$patch names a fixture");
                continue;
            }
        };
        let mut own = Findings::default();
        let checked = match value.as_object() {
            Some(object) => Value::Object(fields(schema, &roots, "data", object, &at, &mut own)),
            None => value.clone(),
        };
        // A fault inherited from the base fixture is the base's to report.
        let base_at = base.as_ref().and_then(Value::as_str).map(|b| format!("fixtures.{b}"));
        for finding in own.0 {
            let as_base: Option<Finding> = base_at.as_ref().map(|b| finding.renamed(&at, b));
            if !as_base.is_some_and(|as_base| findings.0.contains(&as_base)) {
                findings.0.push(finding);
            }
        }
        merged.push((name.clone(), value));
        read.push((name.clone(), checked));
    }
    read
}

/// `over` laid on `under`: objects merge key by key, anything else replaces.
fn merge(under: Value, over: Value) -> Value {
    match (under, over) {
        (Value::Object(mut under), Value::Object(over)) => {
            for (key, value) in over {
                let merged = match under.remove(&key) {
                    Some(old) => merge(old, value),
                    None => value,
                };
                under.insert(key, merged);
            }
            Value::Object(under)
        }
        (_, over) => over,
    }
}

/// `value` checked against `field`, with the defaults of absent fields filled.
fn validate(schema: &Schema, value: &Value, field: &Field, at: &str, findings: &mut Findings) -> Value {
    if value.is_null() {
        if !field.nullable {
            findings.add(at, &format!("{} cannot be null", field.describe()));
        }
        return Value::Null;
    }
    if field.array {
        let Some(items) = value.as_array() else {
            findings.add(at, &format!("expected an array, {}", field.describe()));
            return value.clone();
        };
        in_range(field, items.len() as f64, at, "count", findings);
        let element = field.element();
        let checked = items
            .iter()
            .enumerate()
            .map(|(i, item)| validate(schema, item, &element, &format!("{at}[{i}]"), findings));
        return Value::Array(checked.collect());
    }
    let fits = match &field.base {
        Base::String | Base::Url | Base::Image | Base::Icon => value.is_string(),
        Base::Int => value.is_i64() || value.is_u64(),
        Base::Number => value.is_number(),
        Base::Bool => value.is_boolean(),
        Base::Date => value.as_str().is_some_and(is_date),
        Base::Money => is_money(value),
        Base::Named(_) => true,
    };
    if !fits {
        findings.add(at, &format!("{value} is not {}", field.base.name()));
        return value.clone();
    }
    match &field.base {
        Base::String => {
            let length = value.as_str().map_or(0, |s| s.chars().count());
            in_range(field, length as f64, at, "length", findings);
        }
        Base::Int | Base::Number => {
            in_range(field, value.as_f64().unwrap_or(0.0), at, "value", findings);
        }
        Base::Icon => {
            if let Some(icon) = value.as_str()
                && !crate::icons::known(icon)
            {
                findings.add(at, &crate::icons::unknown(icon));
            }
        }
        _ => {}
    }
    match schema.shape(field) {
        Some(Shape::Enum(members)) => {
            if !value.as_str().is_some_and(|s| members.iter().any(|m| m == s)) {
                let name = field.base.name();
                findings.add(at, &format!("{value} is not one of {name}'s members {members:?}"));
            }
            value.clone()
        }
        Some(Shape::Object(declared)) => match value.as_object() {
            Some(object) => Value::Object(fields(schema, declared, field.base.name(), object, at, findings)),
            None => {
                findings.add(at, &format!("expected a {} object", field.base.name()));
                value.clone()
            }
        },
        None => value.clone(),
    }
}

/// An object checked against `declared`: no unknown field, each declared one
/// present or defaulted.
fn fields(
    schema: &Schema,
    declared: &[(String, Field)],
    type_name: &str,
    object: &Map<String, Value>,
    at: &str,
    findings: &mut Findings,
) -> Map<String, Value> {
    for key in object.keys() {
        if !declared.iter().any(|(name, _)| name == key) {
            findings.add(at, &format!("{type_name} has no field {key}"));
        }
    }
    let mut filled = Map::new();
    for (name, field) in declared {
        let value = match (object.get(name), &field.default) {
            (Some(value), _) => validate(schema, value, field, &format!("{at}.{name}"), findings),
            (None, Some(default)) => default.clone(),
            (None, None) => {
                findings.add(at, &format!("missing {name} ({}), which has no default", field.describe()));
                continue;
            }
        };
        filled.insert(name.clone(), value);
    }
    filled
}

fn in_range(field: &Field, n: f64, at: &str, what: &str, findings: &mut Findings) {
    let below = field.lo.is_some_and(|lo| n < lo);
    let above = field.hi.is_some_and(|hi| n > hi);
    if below || above {
        let lo = field.lo.map(|lo| lo.to_string()).unwrap_or_default();
        let hi = field.hi.map(|hi| hi.to_string()).unwrap_or_default();
        findings.add(at, &format!("{what} {n} is outside {lo}..{hi}"));
    }
}

/// `YYYY-MM-DD`.
fn is_date(s: &str) -> bool {
    s.chars().count() == 10
        && s.chars().enumerate().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() })
}

fn is_money(value: &Value) -> bool {
    value.as_object().is_some_and(|o| {
        o.len() == 2
            && o.get("amount").is_some_and(Value::is_number)
            && o.get("currency").and_then(Value::as_str).is_some_and(|c| c.chars().count() == 3)
    })
}
