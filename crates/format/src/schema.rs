//! The view model's types (docs/format/data.md): the named `types`, the roots
//! in `data`, and what type a path through them has.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::check::Findings;
use crate::field::{self, Base, Field};

pub enum Shape {
    Object(Vec<(String, Field)>),
    Enum(Vec<String>),
}

/// Names a path may start from, with their types. A later name hides an
/// earlier one of the same name, as `item` inside a nested `each` does.
pub type Scope = Vec<(String, Field)>;

pub struct Schema {
    shapes: BTreeMap<String, Shape>,
    roots: Scope,
}

/// What a path refers to, and every prefix of it (the path included) that may
/// be null: each must be tested `!= null` before the path is shown.
pub struct Resolved {
    pub field: Field,
    pub nullable: Vec<String>,
}

impl Schema {
    pub fn read(screen: &Value, findings: &mut Findings) -> Schema {
        let mut shapes = BTreeMap::new();
        for (name, body) in object(screen, "types") {
            let at = format!("types.{name}");
            if !name.starts_with(|c: char| c.is_ascii_uppercase()) || !field::is_identifier(name) {
                findings.add(&at, "a type name starts with a capital letter");
            }
            let shape = match body {
                Value::String(text) => match enum_members(text) {
                    Some(members) => Shape::Enum(members),
                    None => {
                        findings.add(&at, "a named type is an object of fields, or enum(a, b, …)");
                        continue;
                    }
                },
                Value::Object(fields) => Shape::Object(read_fields(fields, &at, findings)),
                _ => {
                    findings.add(&at, "a named type is an object of fields, or enum(a, b, …)");
                    continue;
                }
            };
            shapes.insert(name.clone(), shape);
        }
        let roots = match screen.get("data") {
            Some(Value::Object(fields)) => read_fields(fields, "data", findings),
            _ => {
                findings.add("data", "the screen needs a `data` object of root fields");
                Vec::new()
            }
        };
        let schema = Schema { shapes, roots };
        schema.check_names(findings);
        schema
    }

    /// Every named type a field uses is declared.
    fn check_names(&self, findings: &mut Findings) {
        let declared = self.shapes.iter().filter_map(|(name, shape)| match shape {
            Shape::Object(fields) => Some((format!("types.{name}"), fields)),
            Shape::Enum(_) => None,
        });
        for (at, fields) in declared.chain(std::iter::once(("data".to_string(), &self.roots))) {
            for (name, field) in fields {
                if let Base::Named(type_name) = &field.base
                    && !self.shapes.contains_key(type_name)
                {
                    findings.add(&format!("{at}.{name}"), &format!("unknown type {type_name}"));
                }
            }
        }
    }

    pub fn roots(&self) -> Scope {
        self.roots.clone()
    }

    pub fn shape(&self, field: &Field) -> Option<&Shape> {
        match &field.base {
            Base::Named(name) if !field.array => self.shapes.get(name),
            _ => None,
        }
    }

    /// The members of `field`'s enum, when it is one.
    pub fn members(&self, field: &Field) -> Option<&[String]> {
        match self.shape(field) {
            Some(Shape::Enum(members)) => Some(members),
            _ => None,
        }
    }

    pub fn resolve(&self, scope: &Scope, path: &str) -> Option<Resolved> {
        let mut segments = path.split('.');
        let first = segments.next()?;
        let (_, root) = scope.iter().rev().find(|(name, _)| name == first)?;
        let mut field = root.clone();
        let mut walked = first.to_string();
        let mut nullable = Vec::new();
        for segment in segments {
            if field.nullable {
                nullable.push(walked.clone());
            }
            let Some(Shape::Object(fields)) = self.shape(&field) else {
                return None;
            };
            field = fields.iter().find(|(name, _)| name == segment)?.1.clone();
            walked.push('.');
            walked.push_str(segment);
        }
        if field.nullable {
            nullable.push(walked);
        }
        Some(Resolved { field, nullable })
    }
}

fn read_fields(
    fields: &serde_json::Map<String, Value>,
    at: &str,
    findings: &mut Findings,
) -> Vec<(String, Field)> {
    let mut read = Vec::new();
    for (name, spec) in fields {
        match field::read(spec) {
            Ok(field) => read.push((name.clone(), field)),
            Err(problem) => findings.add(&format!("{at}.{name}"), &problem),
        }
    }
    read
}

fn enum_members(text: &str) -> Option<Vec<String>> {
    let inside = text.trim().strip_prefix("enum(")?.strip_suffix(')')?;
    let members: Vec<String> = inside.split(',').map(|m| m.trim().to_string()).collect();
    members.iter().all(|m| field::is_identifier(m)).then_some(members)
}

/// The object under `key`, or nothing.
pub fn object<'a>(screen: &'a Value, key: &str) -> impl Iterator<Item = (&'a String, &'a Value)> {
    screen.get(key).and_then(Value::as_object).into_iter().flatten()
}
