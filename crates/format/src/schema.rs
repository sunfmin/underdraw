//! The view model's types (docs/format/data.md): the named `types`, the roots
//! in `data`, and what type a path through them has.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::check::Findings;
use crate::field::{self, Base, By, Field};

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
    /// The conditions declared on each type (`Order.cancellable`), by the
    /// type's name: a value of it reads each as a Bool field.
    on_types: BTreeMap<String, Vec<String>>,
}

/// What a path refers to, and every prefix of it (the path included) that may
/// be null: each must be tested `!= null` before the path is shown.
pub struct Resolved {
    pub field: Field,
    pub nullable: Vec<String>,
}

impl Schema {
    pub fn read(screen: &Value, findings: &mut Findings) -> Schema {
        #[cfg(test)]
        crate::screen::tests::count("schema");
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
        let mut on_types: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, _) in object(screen, "conditions") {
            if let Some((type_name, condition)) = name.split_once('.') {
                on_types.entry(type_name.to_string()).or_default().push(condition.to_string());
            }
        }
        let schema = Schema { shapes, roots, on_types };
        schema.check_fields(findings);
        schema
    }

    /// Every named type a field uses is declared, only a root of data is
    /// filled by a query, and no server derives view state.
    fn check_fields(&self, findings: &mut Findings) {
        let mut declared = vec![("data".to_string(), &self.roots)];
        for (name, shape) in &self.shapes {
            if let Shape::Object(fields) = shape {
                declared.push((format!("types.{name}"), fields));
            }
        }
        for (at, fields) in declared {
            for (name, field) in fields {
                let at = format!("{at}.{name}");
                if let Base::Named(type_name) = &field.base
                    && !self.shapes.contains_key(type_name)
                {
                    findings.add(&at, &format!("unknown type {type_name}"));
                }
                if field.from.is_some() && !at.starts_with("data.") {
                    findings.add(&at, "only a field of data is filled by a query: write from there");
                }
                if field.view && field.derived.as_ref().is_some_and(|derived| derived.by == By::Server) {
                    findings.add(&at, "view state is the screen's alone: the app derives it, not the server");
                }
            }
        }
    }

    /// The named types, by name.
    pub fn types(&self) -> Vec<(&String, &Shape)> {
        let mut types = Vec::new();
        for entry in &self.shapes {
            types.push(entry);
        }
        types
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

    /// Whether `name` is a declared type.
    pub fn has(&self, name: &str) -> bool {
        self.shapes.contains_key(name)
    }

    /// The fields of the object type `type_name`, as a scope its own
    /// conditions start their paths from.
    pub fn fields_of(&self, type_name: &str) -> Option<Scope> {
        match self.shapes.get(type_name) {
            Some(Shape::Object(fields)) => Some(fields.clone()),
            _ => None,
        }
    }

    /// Whether the type `type_name` declares the condition `name`.
    fn declares(&self, type_name: &str, name: &str) -> bool {
        self.on_types.get(type_name).is_some_and(|names| names.iter().any(|n| n == name))
    }

    /// The type condition the path `path` uses (`Order.cancellable` for
    /// `item.cancellable`, where `item` is an `Order`), if it ends in one.
    pub fn type_condition(&self, scope: &Scope, path: &str) -> Option<String> {
        let (value, name) = path.rsplit_once('.')?;
        let resolved = self.resolve(scope, value)?;
        let type_name = match &resolved.field.base {
            Base::Named(type_name) if !resolved.field.array => type_name,
            _ => return None,
        };
        self.declares(type_name, name).then(|| format!("{type_name}.{name}"))
    }

    /// Why the path `path` does not resolve, when it ends in a condition
    /// declared on a type the value before it is not: which types declare
    /// it, and what the value is.
    pub fn misused(&self, scope: &Scope, path: &str) -> Option<String> {
        let (value, name) = path.rsplit_once('.')?;
        let mut on = Vec::new();
        for (type_name, names) in &self.on_types {
            if names.iter().any(|n| n == name) {
                on.push(type_name.clone());
            }
        }
        if on.is_empty() {
            return None;
        }
        let is = match self.resolve(scope, value) {
            Some(resolved) => resolved.field.describe(),
            None if self.shapes.contains_key(value) => {
                return Some(format!(
                    "{path} is a condition on {value}: use it through a value of {value}, as item.{name}"
                ));
            }
            None => return None,
        };
        Some(format!(
            "{name} is a condition on {}, and {value} is {is}: use it only through a value of {}",
            on.join(", "),
            on.join(" or ")
        ))
    }

    /// Each value of the type `type_name` in `value`, a value of `field`:
    /// it, and those inside it, through every list and object.
    pub fn values_of<'v>(
        &self,
        type_name: &str,
        field: &Field,
        value: &'v Value,
        found: &mut Vec<&'v Value>,
    ) {
        if field.array {
            let element = field.element();
            if let Value::Array(items) = value {
                for item in items {
                    self.values_of(type_name, &element, item, found);
                }
            }
            return;
        }
        let Some(Shape::Object(fields)) = self.shapes.get(field.base.name()) else {
            return;
        };
        if !value.is_object() {
            return;
        }
        if field.base.name() == type_name {
            found.push(value);
        }
        for (name, inner) in fields {
            if let Some(inner_value) = value.get(name) {
                self.values_of(type_name, inner, inner_value, found);
            }
        }
    }

    /// The first field marked view state inside a value of `field`'s type,
    /// as a dotted path from that type (`Doc.menuOpen`); none when every
    /// field it can reach is domain data.
    pub fn view_state(&self, field: &Field) -> Option<String> {
        self.reaches_in(field.base.name(), Mark::View, &mut Vec::new())
    }

    /// The first field the app derives inside a value of `field`'s type, as
    /// a dotted path from that type (`Summary.count`): what the app works
    /// out for the screen, which no server answers with.
    pub fn app_derived(&self, field: &Field) -> Option<String> {
        self.reaches_in(field.base.name(), Mark::AppDerived, &mut Vec::new())
    }

    /// The first field with `mark` inside a value of the type `type_name`,
    /// through every type it reaches.
    fn reaches_in(&self, type_name: &str, mark: Mark, seen: &mut Vec<String>) -> Option<String> {
        if seen.iter().any(|name| name == type_name) {
            return None;
        }
        seen.push(type_name.to_string());
        let Some(Shape::Object(fields)) = self.shapes.get(type_name) else {
            return None;
        };
        for (name, field) in fields {
            if mark.on(field) {
                return Some(format!("{type_name}.{name}"));
            }
            if let Base::Named(inner) = &field.base
                && let Some(found) = self.reaches_in(inner, mark, seen)
            {
                return Some(found);
            }
        }
        None
    }

    /// Whether the path `path` is in view state: it, or a field it passes
    /// through (`rowMenu` in `rowMenu.docId`), is marked view. A path that
    /// is not one is not.
    pub fn in_view_state(&self, scope: &Scope, path: &str) -> bool {
        let mut segments = path.split('.');
        let Some(first) = segments.next() else {
            return false;
        };
        let Some((_, root)) = scope.iter().rev().find(|(name, _)| name == first) else {
            return false;
        };
        let mut field = root.clone();
        let mut view = field.view;
        for segment in segments {
            let Some(Shape::Object(fields)) = self.shape(&field) else {
                return false;
            };
            let Some((_, next)) = fields.iter().find(|(name, _)| name == segment) else {
                return false;
            };
            field = next.clone();
            view = view || field.view;
        }
        view
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
            let mut next = None;
            for (name, inner) in fields {
                if name == segment {
                    next = Some(inner.clone());
                }
            }
            // A condition declared on the type reads as a Bool field.
            if next.is_none() && self.declares(field.base.name(), segment) {
                next = Some(Field::of(Base::Bool));
            }
            field = next?;
            walked.push('.');
            walked.push_str(segment);
        }
        if field.nullable {
            nullable.push(walked);
        }
        Some(Resolved { field, nullable })
    }
}

/// What no operation's result may reach.
#[derive(Clone, Copy)]
enum Mark {
    View,
    AppDerived,
}

impl Mark {
    fn on(self, field: &Field) -> bool {
        match self {
            Mark::View => field.view,
            Mark::AppDerived => field.derived.as_ref().is_some_and(|derived| derived.by == By::App),
        }
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

/// The entries of the object under `key`, in the order written; none when
/// there is no such object.
pub fn object<'a>(screen: &'a Value, key: &str) -> Vec<(&'a String, &'a Value)> {
    let mut entries = Vec::new();
    if let Some(Value::Object(map)) = screen.get(key) {
        for entry in map {
            entries.push(entry);
        }
    }
    entries
}
