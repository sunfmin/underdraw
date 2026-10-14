//! Long-running commands (docs/adr/0015): a command marked `"long": true`
//! answers with a job, and the job's type and the query that reads it are
//! derived from it here, once, so neither is written by hand. The
//! description is expanded with them before anything reads it, and `check`
//! holds the fixtures to showing the job running, done and failed.

use serde_json::{Map, Value, json};

use crate::check::Findings;
use crate::field::Base;
use crate::operations::Operations;
use crate::schema::{Schema, object};

/// The enum every job's `status` is: one type for every long command.
pub const STATUS: &str = "JobStatus";
/// The members of `STATUS`, in the order a job goes through them.
pub const STATES: [&str; 3] = ["running", "done", "failed"];

/// The type of the job `command` answers with: `exportDocs` → `ExportDocsJob`.
pub fn job_type(command: &str) -> String {
    let mut chars = command.chars();
    let first: String = chars.next().map(|c| c.to_ascii_uppercase()).into_iter().collect();
    format!("{first}{}Job", chars.as_str())
}

/// The query that reads the job `command` started: `exportDocs` → `exportDocsJob`.
pub fn job_query(command: &str) -> String {
    format!("{command}Job")
}

/// `screen` with what each long command derives written in: its job type,
/// the `JobStatus` enum and the query that reads the job. A type or an
/// operation of a derived name written by hand is a finding, and the
/// derived one takes its place.
pub(crate) fn expand(screen: &Value, findings: &mut Findings) -> Value {
    let mut long = Vec::new();
    for (name, body) in object(screen, "operations") {
        if body.get("long") == Some(&Value::Bool(true)) && body.get("command").is_some() {
            long.push((name.clone(), body.get("result").and_then(Value::as_str).map(str::to_string)));
        }
    }
    let mut expanded = screen.clone();
    if long.is_empty() {
        return expanded;
    }
    let Some(root) = expanded.as_object_mut() else {
        return expanded;
    };
    let mut types = take_object(root, "types");
    let mut operations = take_object(root, "operations");
    let derived_from =
        |name: &str| format!("is derived from {name}'s \"long\": true, so it is not written by hand");
    if types.contains_key(STATUS)
        && let Some((first, _)) = long.first()
    {
        findings.add(&format!("types.{STATUS}"), &derived_from(first));
    }
    types.insert(STATUS.to_string(), Value::from(format!("enum({})", STATES.join(", "))));
    for (name, result) in &long {
        let type_name = job_type(name);
        if types.contains_key(&type_name) {
            findings.add(&format!("types.{type_name}"), &derived_from(name));
        }
        let mut fields = Map::new();
        fields.insert(
            "id".to_string(),
            json!({ "type": "string", "doc": "The job's id, to read it again by" }),
        );
        fields.insert(
            "status".to_string(),
            json!({ "type": STATUS, "doc": "running until it ends, then done or failed" }),
        );
        fields.insert(
            "progress".to_string(),
            json!({ "type": "int 0..100", "doc": "How far along it is, in percent" }),
        );
        if let Some(result) = result {
            fields.insert(
                "result".to_string(),
                json!({ "type": nullable(result), "doc": format!("What {name} answers with, once done") }),
            );
        }
        fields.insert(
            "error".to_string(),
            json!({ "type": "string?", "doc": format!("Which of {name}'s fails it ended in, once failed") }),
        );
        types.insert(type_name.clone(), Value::Object(fields));
        let query = job_query(name);
        if operations.contains_key(&query) {
            findings.add(&format!("operations.{query}"), &derived_from(name));
        }
        operations.insert(
            query,
            json!({
                "query": format!("Reads the job {name} started: how far along it is, and how it ended"),
                "params": { "id": "string" },
                "result": type_name,
                "fails": { "notFound": "no job has this id" }
            }),
        );
    }
    root.insert("types".to_string(), Value::Object(types));
    root.insert("operations".to_string(), Value::Object(operations));
    expanded
}

fn take_object(root: &mut Map<String, Value>, key: &str) -> Map<String, Value> {
    match root.remove(key) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// A field type as written, made nullable: `Doc[] ..50` → `Doc[]? ..50`.
fn nullable(written: &str) -> String {
    let trimmed = written.trim();
    let (head, rest) = match trimmed.split_once(' ') {
        Some((head, rest)) => (head, format!(" {rest}")),
        None => (trimmed, String::new()),
    };
    if head.ends_with('?') { trimmed.to_string() } else { format!("{head}?{rest}") }
}

/// Each long command's job is shown: a field of `data` holds it, and the
/// fixtures hold it running, done and failed; a done job holds its result,
/// and a failed one names one of the command's fails.
pub(crate) fn check(
    operations: &Operations,
    schema: &Schema,
    fixtures: &[(String, Value)],
    findings: &mut Findings,
) {
    for operation in operations.all() {
        if !operation.long {
            continue;
        }
        let type_name = job_type(&operation.name);
        let mut holders = Vec::new();
        for (name, field) in schema.roots() {
            if field.base == Base::Named(type_name.clone()) && !field.array {
                holders.push(name);
            }
        }
        let Some(first) = holders.first() else {
            findings.add(
                &format!("operations.{}.long", operation.name),
                &format!(
                    "no field of data holds its job, so the screen cannot show it: add one, \"{}\": \"{type_name}?\", and show its progress",
                    job_query(&operation.name)
                ),
            );
            continue;
        };
        let mut seen = Vec::new();
        for (fixture, data) in fixtures {
            for holder in &holders {
                let Some(job) = data.get(holder).and_then(Value::as_object) else {
                    continue;
                };
                let status = job.get("status").and_then(Value::as_str).unwrap_or_default();
                if !seen.iter().any(|known| known == status) {
                    seen.push(status.to_string());
                }
                let at = format!("fixtures.{fixture}.{holder}");
                if status == "failed" {
                    let error = job.get("error").and_then(Value::as_str);
                    if !error.is_some_and(|error| operation.fails.iter().any(|(name, _)| name == error)) {
                        let names: Vec<&str> =
                            operation.fails.iter().map(|(name, _)| name.as_str()).collect();
                        findings.add(
                            &format!("{at}.error"),
                            &format!(
                                "a failed job names how it failed: one of {}'s fails ({})",
                                operation.name,
                                if names.is_empty() {
                                    "it has none: give it \"fails\"".to_string()
                                } else {
                                    names.join(", ")
                                }
                            ),
                        );
                    }
                }
                if status == "done"
                    && operation.result.as_ref().is_some_and(|result| !result.nullable)
                    && job.get("result").is_none_or(Value::is_null)
                {
                    findings.add(
                        &format!("{at}.result"),
                        &format!("a done job holds what {} answers with", operation.name),
                    );
                }
            }
        }
        for state in STATES {
            if !seen.iter().any(|known| known == state) {
                findings.add(
                    "fixtures",
                    &format!(
                        "no fixture holds {}'s job {state}: add one where {first}.status is \"{state}\"",
                        operation.name
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_is_made_nullable_where_its_type_ends() {
        assert_eq!(nullable("Doc"), "Doc?");
        assert_eq!(nullable("Doc[] ..50"), "Doc[]? ..50");
        assert_eq!(nullable("Doc?"), "Doc?");
    }

    #[test]
    fn names_are_derived_from_the_command() {
        assert_eq!(job_type("exportDocs"), "ExportDocsJob");
        assert_eq!(job_query("exportDocs"), "exportDocsJob");
    }
}
