//! The screen's operations (docs/adr/0009): the queries and commands behind
//! it, each with its parameters, its result, the condition it requires and
//! the ways it can fail. Read once, and checked against the view model.

use serde_json::Value;

use crate::check::Findings;
use crate::conditions::Conditions;
use crate::field::{self, Base, Field};
use crate::schema::{Schema, object};

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
    /// Each way it can fail, by name, with when.
    pub fails: Vec<(String, String)>,
}

pub struct Operations(Vec<Operation>);

/// The keys an operation is written with.
const KEYS: [&str; 7] = ["query", "command", "params", "result", "requires", "fails", "$comment"];

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
                    findings.add(&at, &format!("an operation has no {key}: it holds query or command, params, result, requires, fails"));
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
            read.push(Operation { name: name.clone(), does, doc, params, result, requires, fails });
        }
        Operations(read)
    }

    pub fn all(&self) -> &[Operation] {
        &self.0
    }

    pub fn get(&self, name: &str) -> Option<&Operation> {
        self.0.iter().find(|operation| operation.name == name)
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
    Some(field)
}
