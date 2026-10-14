//! `check` on the export draft's long command (docs/adr/0015): its job
//! type and the query that reads it are derived, and the fixtures must show
//! the job running, done and failed. Each fault is planted one at a time.

use serde_json::Value;
use underdraw_format::{Checked, Does, Shape, check, model};

const DRAFT: &str = include_str!("../../../drafts/export/screen.json");
const KINDS: &str = include_str!("../../../drafts/export/kinds.json");

fn kinds() -> Value {
    serde_json::from_str(KINDS).unwrap()
}

/// The draft with `from` replaced by `to`, where `from` appears exactly once.
fn planted(name: &str, from: &str, to: &str) -> Value {
    assert_eq!(DRAFT.matches(from).count(), 1, "{name}: {from:?} must appear exactly once in the draft");
    serde_json::from_str(&DRAFT.replace(from, to)).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"))
}

fn found(checked: &Checked, at: &str, said: &str) -> bool {
    checked.findings.iter().any(|f| f.at == at && f.problem.contains(said))
}

#[test]
fn a_long_command_derives_its_job_type_and_the_query_that_reads_it() {
    let screen: Value = serde_json::from_str(DRAFT).unwrap();
    let model = model(&screen, Some(&kinds())).unwrap_or_else(|c| panic!("{:#?}", c.findings));
    let export = model.operations.get("exportDocs").unwrap();
    assert!(export.long);
    assert_eq!(
        (export.job_type().as_deref(), export.job_query().as_deref()),
        (Some("ExportDocsJob"), Some("exportDocsJob"))
    );
    let query = model.operations.get("exportDocsJob").unwrap();
    assert_eq!(query.does, Does::Query);
    assert_eq!(query.job_of.as_deref(), Some("exportDocs"));
    assert_eq!(
        query.params.iter().map(|(name, field)| (name.as_str(), field.describe())).collect::<Vec<_>>(),
        [("id", "string".to_string())]
    );
    assert_eq!(query.result.as_ref().map(|r| r.describe()), Some("ExportDocsJob".to_string()));
    let mut job = None;
    let mut status = None;
    for (name, shape) in model.schema.types() {
        match (name.as_str(), shape) {
            ("ExportDocsJob", Shape::Object(fields)) => {
                job = Some(fields.iter().map(|(n, f)| format!("{n}: {}", f.describe())).collect::<Vec<_>>())
            }
            ("JobStatus", Shape::Enum(members)) => status = Some(members.clone()),
            _ => {}
        }
    }
    assert_eq!(
        job.unwrap(),
        ["id: string", "status: JobStatus", "progress: int", "result: Archive?", "error: string?"]
    );
    assert_eq!(status.unwrap(), ["running", "done", "failed"]);
}

#[test]
fn each_planted_fault_is_found_where_it_was_planted() {
    let failed = r#""status": "failed",
        "progress": 65,
        "error": "tooLarge""#;
    let done = r#""progress": 100,
        "result": {
          "url": "https://example.com/exports/j42.zip",
          "files": 128,
          "size": "18.4 MB"
        }"#;
    let cases = [
        (
            "long on a query",
            r#""query": "The team whose documents are exported","#,
            r#""query": "The team whose documents are exported", "long": true,"#,
            "operations.getTeam.long",
            "a query only reads",
        ),
        (
            "long not a bool",
            r#""long": true"#,
            r#""long": "yes""#,
            "operations.exportDocs.long",
            "true or false",
        ),
        (
            "the job type written by hand",
            r#""Team": {"#,
            r#""ExportDocsJob": { "id": "string" }, "Team": {"#,
            "types.ExportDocsJob",
            "is derived from exportDocs's \"long\"",
        ),
        (
            "the job query written by hand",
            r#""getTeam": {"#,
            r#""exportDocsJob": { "query": "x", "result": "Team" }, "getTeam": {"#,
            "operations.exportDocsJob",
            "is derived from exportDocs's \"long\"",
        ),
        (
            "no field holds the job",
            r#""type": "ExportDocsJob?","#,
            r#""type": "Team?","#,
            "operations.exportDocs.long",
            "no field of data holds its job",
        ),
        (
            "no failed fixture",
            failed,
            r#""status": "running", "progress": 65"#,
            "fixtures",
            "no fixture holds exportDocs's job failed: add one where job.status is \"failed\"",
        ),
        (
            "a failed job names no failure of the command",
            failed,
            r#""status": "failed", "progress": 65, "error": "boom""#,
            "fixtures.failed.job.error",
            "one of exportDocs's fails (offline, tooLarge)",
        ),
        (
            "a done job without its result",
            done,
            r#""progress": 100"#,
            "fixtures.done.job.result",
            "a done job holds what exportDocs answers with",
        ),
    ];
    for (name, from, to, at, said) in cases {
        let checked = check(&planted(name, from, to), Some(&kinds()));
        assert!(found(&checked, at, said), "{name}: wanted {at}: {said}; got {:#?}", checked.findings);
    }
}
