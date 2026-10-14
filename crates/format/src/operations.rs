//! The screen's operations (docs/adr/0009): the queries and commands behind
//! it, each with its parameters, its result, the condition it requires and
//! the ways it can fail. Read once, and checked against the view model.

use serde_json::Value;

use crate::check::Findings;
use crate::conditions::Conditions;
use crate::field::{self, Base, By, Field};
use crate::jobs;
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
    /// A command that takes long (docs/adr/0015): it answers with a job,
    /// whose type and the query reading it are derived from it.
    pub long: bool,
    /// For the query derived from a long command, that command's name.
    pub job_of: Option<String>,
    /// Each way it can fail, by name, with when. A bulk form's fail item
    /// by item.
    pub fails: Vec<(String, String)>,
    /// The name of its bulk form, for an operation marked `"bulk"`.
    pub bulk: Option<String>,
    /// For a bulk form, derived from the operation marked `"bulk"`: what it
    /// is the bulk form of.
    pub bulk_of: Option<BulkOf>,
    /// The fields of `data` its result fills, for a query: each written
    /// `"from": <this query>`, or `"from": <this query>.<part>`.
    pub fills: Vec<Fill>,
    /// True of a query that pages by a cursor (`"pages": "cursor"`): it
    /// answers one page at a time, its result says in `next` where the next
    /// page starts, and it takes the `cursor` and `limit` paging adds.
    pub pages: bool,
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
    /// The field of the result that fills it, when it is filled by one
    /// part of the result (`"from": "listDocs.next"`) rather than all of it.
    pub part: Option<String>,
    /// Where in the view model the result lands: the field itself (`docs`)
    /// when it is of the result's type, or the field of each element (`[]`)
    /// that holds it when its type wraps the result's: `docs[].doc` for
    /// `DocRow { doc: Doc, … }`.
    pub path: String,
}

impl Operation {
    /// The type of the job a long command answers with: `ExportDocsJob`.
    pub fn job_type(&self) -> Option<String> {
        self.long.then(|| jobs::job_type(&self.name))
    }

    /// The query that reads the job a long command started: `exportDocsJob`.
    pub fn job_query(&self) -> Option<String> {
        self.long.then(|| jobs::job_query(&self.name))
    }
}

pub struct Operations(Vec<Operation>);

/// The keys an operation is written with.
const KEYS: [&str; 11] = [
    "query",
    "command",
    "params",
    "result",
    "requires",
    "destructive",
    "long",
    "bulk",
    "fails",
    "pages",
    "$comment",
];

/// The parameters paging by a cursor adds, which the description does not
/// write.
pub const PAGING: [&str; 2] = ["cursor", "limit"];

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
                    findings.add(&at, &format!("an operation has no {key}: it holds query or command, params, result, requires, destructive, long, bulk, fails, pages"));
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
            let mut params = params(body.get("params"), schema, &format!("{at}.params"), findings);
            let result = result(body.get("result"), schema, &format!("{at}.result"), findings);
            if does == Does::Query && result.is_none() && body.get("result").is_none() {
                findings.add(&at, "a query answers with something: give it a \"result\", one of types");
            }
            let pages = match body.get("pages") {
                None => false,
                Some(Value::String(by)) if by == "cursor" => true,
                Some(_) => {
                    findings.add(
                        &format!("{at}.pages"),
                        "pages is \"cursor\": the query answers one page at a time, and says in next where the next one starts",
                    );
                    false
                }
            };
            if pages && does != Does::Query {
                findings.add(&format!("{at}.pages"), "only a query pages: a command changes something once");
            }
            if pages {
                for added in PAGING {
                    if params.iter().any(|(param, _)| param == added) {
                        findings.add(
                            &format!("{at}.params.{added}"),
                            "pages adds cursor and limit itself: leave them out of params",
                        );
                    }
                }
                if let Some(result) = &result {
                    params.extend(paging(schema, name, result, &format!("{at}.result"), findings));
                }
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
            let long = match body.get("long") {
                None => false,
                Some(Value::Bool(long)) => *long,
                Some(other) => {
                    findings.add(&format!("{at}.long"), &format!("{other}: long is true or false"));
                    false
                }
            };
            if long && does == Does::Query {
                findings.add(
                    &format!("{at}.long"),
                    "a query only reads, and answers at once: long marks a command that answers with a job",
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
                long: long && does == Does::Command,
                job_of: None,
                fails,
                bulk,
                bulk_of: None,
                fills: Vec::new(),
                pages,
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
        let mut long = Vec::new();
        for operation in &read {
            if operation.long {
                long.push(operation.name.clone());
            }
        }
        for operation in &mut read {
            for command in &long {
                if operation.name == jobs::job_query(command) {
                    operation.job_of = Some(command.clone());
                }
            }
        }
        let mut operations = Operations(read);
        operations.fill(schema, findings);
        operations
    }

    /// Each field of `data` written `"from": <query>`, to that query: the
    /// query exists, and its result is of the field's type or wrapped by it.
    /// Written `"from": <query>.<part>`, the same of that field of its result.
    fn fill(&mut self, schema: &Schema, findings: &mut Findings) {
        for (name, field) in schema.roots() {
            let Some(from) = &field.from else {
                continue;
            };
            let (query, part) = match from.split_once('.') {
                Some((query, part)) => (query, Some(part)),
                None => (from.as_str(), None),
            };
            let at = format!("data.{name}");
            let Some(operation) = self.0.iter_mut().find(|operation| operation.name == query) else {
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
            let result = match part {
                None => result.clone(),
                Some(part) => match schema.shape(result) {
                    Some(Shape::Object(fields))
                        if !result.nullable
                            && let Some((_, inner)) = fields.iter().find(|(inner, _)| inner == part) =>
                    {
                        inner.clone()
                    }
                    _ => {
                        findings.add(
                            &format!("{at}.from"),
                            &format!(
                                "{part} is not a field of {query}'s result ({}): from names a query, or one field of its result, \"listDocs.next\"",
                                result.describe()
                            ),
                        );
                        continue;
                    }
                },
            };
            let result = &result;
            match through(schema, &field, result) {
                Ok(through) => {
                    let path = match through {
                        None => name.clone(),
                        Some(through) => format!("{name}{}.{through}", if field.array { "[]" } else { "" }),
                    };
                    operation.fills.push(Fill { field: name.clone(), part: part.map(str::to_string), path });
                }
                Err(()) => findings.add(
                    &at,
                    &format!(
                        "is {}, and {from} answers with {}: a field a query fills is of the result's type, or of a type that wraps it (one field of that type, beside view state and what the app derives)",
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
    if single.long {
        findings.add(
            &at,
            "a long command answers with one job, and its bulk form would answer with one per item: mark it long or bulk, not both",
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
    for (index, (param, field)) in single.params.iter().enumerate() {
        if index > 0 {
            params.push((param.clone(), field.clone()));
        }
    }
    Some(Operation {
        name: name.to_string(),
        does: single.does,
        doc: format!("{}: each of several, one by one", single.doc),
        params,
        result: single.result.clone(),
        requires: single.requires.clone(),
        destructive: single.destructive,
        long: false,
        job_of: None,
        fails: single.fails.clone(),
        bulk: None,
        bulk_of: Some(BulkOf { operation: single.name.clone(), key: key.clone(), keys }),
        fills: Vec::new(),
        pages: false,
    })
}

/// The parameters a query that pages by a cursor takes besides its own:
/// `cursor`, where the page starts (null for the first), and `limit`, how
/// many rows it holds at most, by default as many as its list may hold.
/// Its result is one page: an object of one list, its rows, and `next`.
fn paging(
    schema: &Schema,
    query: &str,
    result: &Field,
    at: &str,
    findings: &mut Findings,
) -> Vec<(String, Field)> {
    let shape = if result.nullable { None } else { schema.shape(result) };
    let Some(Shape::Object(fields)) = shape else {
        findings.add(
            at,
            &format!(
                "{query} pages, so it answers with one page: a type holding its rows' list and \"next\": \"string?\", not {}",
                result.describe()
            ),
        );
        return Vec::new();
    };
    let page = result.base.name();
    let next = fields.iter().find(|(name, _)| name == "next").map(|(_, next)| next);
    if !next.is_some_and(|next| next.base == Base::String && next.nullable && !next.array) {
        findings.add(
            at,
            &format!(
                "{page} has no \"next\": \"string?\": a paged query's result says where the next page starts, and null on the last page"
            ),
        );
    }
    let lists: Vec<&(String, Field)> = fields.iter().filter(|(_, inner)| inner.array).collect();
    let [(rows, list)] = lists.as_slice() else {
        findings.add(at, &format!("{page} holds exactly one list, the page's rows, beside next"));
        return Vec::new();
    };
    let Some(most) = list.hi else {
        findings.add(
            at,
            &format!(
                "{page}.{rows} says how many rows a page holds at most, which is limit's default: \"{}[] ..50\"",
                list.base.name()
            ),
        );
        return Vec::new();
    };
    let mut added = Vec::new();
    for (name, written) in [("cursor", "string?".to_string()), ("limit", format!("int 1..{most} = {most}"))] {
        if let Ok(field) = field::parse(&written) {
            added.push((name.to_string(), field));
        }
    }
    added
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
