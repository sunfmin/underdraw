//! The screen's operations (docs/adr/0009): the queries and commands behind
//! it, each with its parameters, its result, the condition it requires and
//! the ways it can fail. Read once, and checked against the view model.

use serde_json::Value;

use crate::check::Findings;
use crate::conditions::Conditions;
use crate::field::{self, Base, By, Field};
use crate::schema::{Schema, Shape, object};

/// Whether an operation only reads, or changes something.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Does {
    Query,
    Command,
}

impl Does {
    pub fn name(self) -> &'static str {
        match self {
            Does::Query => "query",
            Does::Command => "command",
        }
    }
}

pub struct Operation {
    pub name: String,
    pub does: Does,
    /// What it does, in one line, as written under `query` or `command`.
    pub doc: String,
    /// The parameters, in the order written, typed as view-model fields.
    pub params: Vec<(String, Field)>,
    /// What it answers with: one of the view model's `types`.
    pub result: Option<Field>,
    /// The condition that must hold before it may be called.
    pub requires: Option<String>,
    /// A command that cannot be undone: the screen calls it only from a
    /// confirmation, and the CLI only given `--yes`.
    pub destructive: bool,
    /// Each way it can fail, by name, with when. A bulk form's fail item
    /// by item.
    pub fails: Vec<(String, String)>,
    /// The name of its bulk form, for an operation marked `"bulk"`.
    pub bulk: Option<String>,
    /// For a bulk form, derived from the operation marked `"bulk"`: what it
    /// is the bulk form of.
    pub bulk_of: Option<BulkOf>,
    /// The fields of `data` its result fills, for a query: each written
    /// `"from": <this query>`.
    pub fills: Vec<Fill>,
}

/// What a bulk form is derived from: the operation it does for each item,
/// and the parameter each item goes in.
#[derive(Clone, Debug, PartialEq)]
pub struct BulkOf {
    /// The operation marked `"bulk"`: `deleteDoc`.
    pub operation: String,
    /// Its key parameter, the first it takes: `id`. Each item is one value
    /// of it; every other parameter is the same for all of them.
    pub key: String,
    /// The bulk form's list of them: the key's name with an `s`, `ids`.
    pub keys: String,
}

/// A field of `data` a query's result fills.
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    /// The field, a root of `data`: `docs`.
    pub field: String,
    /// Where in the view model the result lands: the field itself (`docs`)
    /// when it is of the result's type, or the field of each element (`[]`)
    /// that holds it when its type wraps the result's: `docs[].doc` for
    /// `DocRow { doc: Doc, … }`.
    pub path: String,
}

pub struct Operations(Vec<Operation>);

/// The keys an operation is written with.
const KEYS: [&str; 9] =
    ["query", "command", "params", "result", "requires", "destructive", "bulk", "fails", "$comment"];

impl Operations {
    pub fn read(
        screen: &Value,
        schema: &Schema,
        conditions: &Conditions,
        findings: &mut Findings,
    ) -> Operations {
        let mut read = Vec::new();
        for (name, body) in object(screen, "operations") {
            if name.starts_with('$') {
                continue;
            }
            let at = format!("operations.{name}");
            if !field::is_identifier(name) {
                findings.add(&at, "an operation's name is an identifier, such as deleteDoc");
            }
            let Some(body) = body.as_object() else {
                findings.add(&at, "an operation is {\"query\" or \"command\": what it does, \"params\", \"result\", \"requires\", \"fails\"}");
                continue;
            };
            for key in body.keys() {
                if !KEYS.contains(&key.as_str()) && !key.starts_with('$') {
                    findings.add(&at, &format!("an operation has no {key}: it holds query or command, params, result, requires, destructive, bulk, fails"));
                }
            }
            let (does, doc) = match (body.get("query"), body.get("command")) {
                (Some(Value::String(doc)), None) => (Does::Query, doc.clone()),
                (None, Some(Value::String(doc))) => (Does::Command, doc.clone()),
                _ => {
                    findings.add(
                        &at,
                        "write exactly one of \"query\" (it only reads) or \"command\" (it changes something), each saying in one line what it does",
                    );
                    (Does::Command, String::new())
                }
            };
            let params = params(body.get("params"), schema, &format!("{at}.params"), findings);
            let result = result(body.get("result"), schema, &format!("{at}.result"), findings);
            if does == Does::Query && result.is_none() && body.get("result").is_none() {
                findings.add(&at, "a query answers with something: give it a \"result\", one of types");
            }
            let requires = match body.get("requires") {
                None => None,
                Some(Value::String(condition)) if conditions.contains(condition) => Some(condition.clone()),
                Some(other) => {
                    findings.add(
                        &format!("{at}.requires"),
                        &format!("{other} is not a condition: requires names one in conditions"),
                    );
                    None
                }
            };
            let destructive = match body.get("destructive") {
                None => false,
                Some(Value::Bool(destructive)) => *destructive,
                Some(other) => {
                    findings
                        .add(&format!("{at}.destructive"), &format!("{other}: destructive is true or false"));
                    false
                }
            };
            if destructive && does == Does::Query {
                findings.add(
                    &format!("{at}.destructive"),
                    "a query only reads, so nothing it does needs confirming: destructive marks a command",
                );
            }
            let mut fails = Vec::new();
            match body.get("fails") {
                None => {}
                Some(Value::Object(written)) => {
                    for (failure, said) in written {
                        if let (true, Some(said)) = (field::is_identifier(failure), said.as_str()) {
                            fails.push((failure.clone(), said.to_string()));
                        } else {
                            findings.add(
                                &format!("{at}.fails.{failure}"),
                                "a failure is a name and one line saying when: \"notFound\": \"the document is gone\"",
                            );
                        }
                    }
                }
                Some(_) => findings.add(
                    &format!("{at}.fails"),
                    "fails names each way it can fail: {\"notFound\": \"the document is gone\"}",
                ),
            }
            let bulk = match body.get("bulk") {
                None => None,
                Some(Value::String(bulk)) if field::is_identifier(bulk) => Some(bulk.clone()),
                Some(other) => {
                    findings.add(
                        &format!("{at}.bulk"),
                        &format!(
                            "{other}: bulk names the operation's bulk form, an identifier such as deleteDocs"
                        ),
                    );
                    None
                }
            };
            let single = Operation {
                name: name.clone(),
                does,
                doc,
                params,
                result,
                requires,
                destructive,
                fails,
                bulk,
                bulk_of: None,
                fills: Vec::new(),
            };
            let bulk_form = single.bulk.as_ref().and_then(|bulk| bulk_form(&single, bulk, &at, findings));
            read.push(single);
            read.extend(bulk_form);
        }
        for form in &read {
            if let Some(of) = &form.bulk_of
                && read.iter().filter(|other| other.name == form.name).count() > 1
            {
                findings.add(
                    &format!("operations.{}.bulk", of.operation),
                    &format!("{} is already an operation: name the bulk form something else", form.name),
                );
            }
        }
        let mut operations = Operations(read);
        operations.fill(schema, findings);
        operations
    }

    /// Each field of `data` written `"from": <query>`, to that query: the
    /// query exists, and its result is of the field's type or wrapped by it.
    fn fill(&mut self, schema: &Schema, findings: &mut Findings) {
        for (name, field) in schema.roots() {
            let Some(query) = &field.from else {
                continue;
            };
            let at = format!("data.{name}");
            let Some(operation) = self.0.iter_mut().find(|operation| &operation.name == query) else {
                findings.add(
                    &format!("{at}.from"),
                    &format!("{query} is not an operation: from names a query in operations"),
                );
                continue;
            };
            if operation.does != Does::Query {
                findings.add(
                    &format!("{at}.from"),
                    &format!("{query} is a command: from names the query whose result fills the field"),
                );
                continue;
            }
            if field.view {
                findings.add(&at, "view state is the screen's alone: no query fills it");
                continue;
            }
            let Some(result) = &operation.result else {
                // The query's own finding says it answers with nothing.
                continue;
            };
            match through(schema, &field, result) {
                Ok(through) => {
                    let path = match through {
                        None => name.clone(),
                        Some(through) => format!("{name}{}.{through}", if field.array { "[]" } else { "" }),
                    };
                    operation.fills.push(Fill { field: name.clone(), path });
                }
                Err(()) => findings.add(
                    &at,
                    &format!(
                        "is {}, and {query} answers with {}: a field a query fills is of the result's type, or of a type that wraps it (one field of that type, beside view state and what the app derives)",
                        field.describe(),
                        result.describe()
                    ),
                ),
            }
        }
    }

    pub fn all(&self) -> &[Operation] {
        &self.0
    }

    pub fn get(&self, name: &str) -> Option<&Operation> {
        self.0.iter().find(|operation| operation.name == name)
    }
}

/// The bulk form `name` of `single`: its key parameter (the first) made a
/// list of at least one, every other parameter as it is; its precondition
/// and failures held item by item, and its result one entry per item. None,
/// with a finding at `at`, when `single` cannot have one.
fn bulk_form(single: &Operation, name: &str, at: &str, findings: &mut Findings) -> Option<Operation> {
    let at = format!("{at}.bulk");
    if single.does == Does::Query {
        findings.add(
            &at,
            "bulk marks a command: a query only reads, and one that reads many is a query of its own",
        );
        return None;
    }
    let Some((key, field)) = single.params.first() else {
        findings.add(
            &at,
            "a bulk form takes many of the operation's first parameter, and it has none: give it the one each item is, such as \"id\": \"string\"",
        );
        return None;
    };
    if field.array || field.nullable || field.default.is_some() {
        findings.add(
            &at,
            &format!(
                "a bulk form takes many of the operation's first parameter, and {key} is {}: make the first parameter the one each item is, one value and required",
                field.describe()
            ),
        );
        return None;
    }
    let keys = format!("{key}s");
    if single.params.iter().any(|(param, _)| param == &keys) {
        findings.add(
            &at,
            &format!("the bulk form takes {keys}, and the operation has a parameter of that name already"),
        );
        return None;
    }
    let mut params = vec![(
        keys.clone(),
        Field { array: true, lo: Some(1.0), hi: None, default: None, doc: None, ..field.clone() },
    )];
    params.extend(single.params.iter().skip(1).cloned());
    Some(Operation {
        name: name.to_string(),
        does: single.does,
        doc: format!("{}: each of several, one by one", single.doc),
        params,
        result: single.result.clone(),
        requires: single.requires.clone(),
        destructive: single.destructive,
        fails: single.fails.clone(),
        bulk: None,
        bulk_of: Some(BulkOf { operation: single.name.clone(), key: key.clone(), keys }),
        fills: Vec::new(),
    })
}

/// How `field` holds `result`: as it is (none), or through the one field of
/// its type that is neither view state nor derived by the app.
fn through(schema: &Schema, field: &Field, result: &Field) -> Result<Option<String>, ()> {
    if field.array != result.array || field.nullable != result.nullable {
        return Err(());
    }
    if field.base == result.base {
        return Ok(None);
    }
    let Some(Shape::Object(fields)) = schema.shape(&field.element()) else {
        return Err(());
    };
    let answered: Vec<&(String, Field)> = fields
        .iter()
        .filter(|(_, inner)| {
            !inner.view && !inner.derived.as_ref().is_some_and(|derived| derived.by == By::App)
        })
        .collect();
    match answered.as_slice() {
        [(name, inner)] if inner.base == result.base && !inner.array && !inner.nullable => {
            Ok(Some(name.clone()))
        }
        _ => Err(()),
    }
}

/// The parameters, each a field whose named type is declared.
fn params(
    written: Option<&Value>,
    schema: &Schema,
    at: &str,
    findings: &mut Findings,
) -> Vec<(String, Field)> {
    let mut read = Vec::new();
    let fields = match written {
        None => return read,
        Some(Value::Object(fields)) => fields,
        Some(_) => {
            findings.add(at, "params is an object of fields, as in data: {\"id\": \"string\"}");
            return read;
        }
    };
    for (name, spec) in fields {
        let at = format!("{at}.{name}");
        match field::read(spec) {
            Ok(field) => {
                if let Base::Named(type_name) = &field.base
                    && !schema.has(type_name)
                {
                    findings.add(&at, &format!("unknown type {type_name}"));
                }
                if field.view {
                    findings.add(&at, "a parameter is domain data, never view state");
                }
                if field.from.is_some() {
                    findings.add(&at, "a parameter is given by the call, not filled by a query");
                }
                if let (Some(members), Some(default)) = (schema.members(&field), &field.default)
                    && !default.as_str().is_some_and(|default| members.iter().any(|member| member == default))
                {
                    findings.add(
                        &at,
                        &format!(
                            "its default {default} is not one of {}'s choices: {}",
                            field.base.name(),
                            members.join(", ")
                        ),
                    );
                }
                read.push((name.clone(), field));
            }
            Err(problem) => findings.add(&at, &problem),
        }
    }
    read
}

/// The result: one of the view model's types, holding no view state.
fn result(written: Option<&Value>, schema: &Schema, at: &str, findings: &mut Findings) -> Option<Field> {
    let text = match written? {
        Value::String(text) => text,
        _ => {
            findings.add(at, "result is a type, written as in data: \"Doc\", \"Doc[] ..50\", \"Doc?\"");
            return None;
        }
    };
    let field = match field::parse(text) {
        Ok(field) => field,
        Err(problem) => {
            findings.add(at, &problem);
            return None;
        }
    };
    let Base::Named(type_name) = &field.base else {
        findings.add(at, &format!("{text:?}: a result is one of types, not a built-in"));
        return None;
    };
    if !schema.has(type_name) {
        findings.add(at, &format!("unknown type {type_name}"));
        return None;
    }
    if let Some(view) = schema.view_state(&field) {
        findings.add(
            at,
            &format!("{type_name} holds view state ({view}), which only the screen keeps: answer with a type of domain data only"),
        );
    }
    if let Some(derived) = schema.app_derived(&field) {
        findings.add(
            at,
            &format!(
                "{type_name} holds {derived}, which the app derives for the screen: answer with what the server knows, and keep the app's fields in the type that wraps it"
            ),
        );
    }
    Some(field)
}
