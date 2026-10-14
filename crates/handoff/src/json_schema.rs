//! The view model as a JSON Schema (draft 2020-12), for whoever validates
//! data against it outside underdraw: API responses, fixtures written by
//! hand, an editor. Derived from the description's `types` and `data`, so it
//! says what `check` says of a fixture's data, except whether an icon is a
//! known Material Symbols name.

use serde_json::{Map, Value};
use underdraw_format::{Base, Field, Model, Shape};

use crate::code::root_name;
use crate::object;

/// Where money's shape is defined. Lower case, so no declared type, which
/// starts with a capital letter, can take its name.
const MONEY: &str = "money";

/// The schema of the root model: one object of the `data` fields, with every
/// named type, and money, under `$defs`.
pub fn json_schema(model: &Model) -> Value {
    let mut defs = Map::new();
    for (name, shape) in model.schema.types() {
        let def = match shape {
            Shape::Object(fields) => object_of(fields),
            Shape::Enum(members) => {
                object([("type", Value::from("string")), ("enum", Value::from(members.clone()))])
            }
        };
        defs.insert(name.clone(), def);
    }
    let money = [
        ("amount".to_string(), Field::of(Base::Number)),
        ("currency".to_string(), Field { lo: Some(3.0), hi: Some(3.0), ..Field::of(Base::String) }),
    ];
    defs.insert(MONEY.to_string(), object_of(&money));
    let mut root = Map::new();
    root.insert("$schema".to_string(), Value::from("https://json-schema.org/draft/2020-12/schema"));
    root.insert("title".to_string(), Value::from(root_name(model)));
    if let Value::Object(body) = object_of(&model.schema.roots()) {
        root.extend(body);
    }
    root.insert("$defs".to_string(), Value::Object(defs));
    Value::Object(root)
}

/// An object of `fields`: none other allowed, and each without a default
/// required, as a fixture must write it.
fn object_of(fields: &[(String, Field)]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (name, field) in fields {
        properties.insert(name.clone(), property(field));
        if field.default.is_none() {
            required.push(Value::from(name.as_str()));
        }
    }
    object([
        ("type", Value::from("object")),
        ("properties", Value::Object(properties)),
        ("required", Value::Array(required)),
        ("additionalProperties", Value::Bool(false)),
    ])
}

/// One field: its type, its range (a list's count, or each value's), null
/// when it may be, then what it means and its default.
fn property(field: &Field) -> Value {
    let mut schema = if field.array {
        let mut list = object([("type", Value::from("array")), ("items", base(&field.base))]);
        range(&mut list, "minItems", "maxItems", field);
        list
    } else {
        let mut value = base(&field.base);
        match field.base {
            Base::String => range(&mut value, "minLength", "maxLength", field),
            Base::Int | Base::Number => range(&mut value, "minimum", "maximum", field),
            _ => {}
        }
        value
    };
    if field.nullable {
        schema = object([("anyOf", Value::Array(vec![schema, object([("type", Value::from("null"))])]))]);
    }
    let derived = field.derived.as_ref().map(|derived| format!("Derived by the {}: {}", derived.by.name(), derived.how));
    let description: Vec<&str> = [field.doc.as_deref(), derived.as_deref()].into_iter().flatten().collect();
    if let Value::Object(map) = &mut schema {
        if !description.is_empty() {
            map.insert("description".to_string(), Value::from(description.join(". ")));
        }
        if let Some(default) = &field.default {
            map.insert("default".to_string(), default.clone());
        }
    }
    schema
}

/// One value of `base`, without range.
fn base(base: &Base) -> Value {
    let string = || ("type", Value::from("string"));
    match base {
        Base::String | Base::Image | Base::Icon => object([string()]),
        Base::Url => object([string(), ("format", Value::from("uri"))]),
        Base::Date => object([string(), ("pattern", Value::from("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))]),
        Base::Int => object([("type", Value::from("integer"))]),
        Base::Number => object([("type", Value::from("number"))]),
        Base::Bool => object([("type", Value::from("boolean"))]),
        Base::Money => object([("$ref", Value::from(format!("#/$defs/{MONEY}")))]),
        Base::Named(name) => object([("$ref", Value::from(format!("#/$defs/{name}")))]),
    }
}

/// The field's range on `schema`, under the keywords that bound this kind of
/// value.
fn range(schema: &mut Value, lo_key: &str, hi_key: &str, field: &Field) {
    if let Value::Object(map) = schema {
        if let Some(lo) = field.lo {
            map.insert(lo_key.to_string(), bound(lo));
        }
        if let Some(hi) = field.hi {
            map.insert(hi_key.to_string(), bound(hi));
        }
    }
}

/// A bound as a whole number when it is one: `50`, not `50.0`, since
/// `minLength` and `minItems` take only integers.
fn bound(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 { Value::from(n as i64) } else { Value::from(n) }
}
