//! Derives the export draft's CLI with the real binary — a long command,
//! its --no-wait and the command that reads its job (docs/adr/0014) — then
//! runs a hand-written stub CLI through every golden output and holds each
//! run to it with `compare`; a wrong stub is found out.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/export");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// The job the server a stub CLI talks to holds in `fixture`, as the server
/// answers with it: every field written, null where it has none yet.
fn job(fixture: &str) -> Value {
    let screen: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{DRAFT}/screen.json")).unwrap()).unwrap();
    let written = &screen["fixtures"][fixture]["job"];
    if written.is_null() {
        return Value::Null;
    }
    let mut job = json!({ "result": null, "error": null });
    for (key, value) in written.as_object().unwrap() {
        job[key] = value.clone();
    }
    job
}

/// A CLI written by hand for the export draft: what it prints and its exit
/// code, run as `<cli> <argv>` in `fixture`. Waiting is the server's job
/// running to its end, which the fixture already shows. `wrong` plants the
/// mistakes a careless one makes.
fn stub(fixture: &str, argv: &[&str], fail: Option<&str>, wrong: bool) -> (String, i32) {
    if let Some(failure) = fail {
        return (json!({ "error": failure }).to_string(), 2);
    }
    let job = job(fixture);
    match argv {
        ["get"] => {
            (json!({ "result": { "id": "t1", "name": "产品设计组", "docCount": 128 } }).to_string(), 0)
        }
        // Prints the job, not the result, as if --no-wait were given.
        ["export"] if wrong => (json!({ "result": job }).to_string(), 0),
        ["export"] => match job["status"].as_str() {
            Some("done") => (json!({ "result": job["result"] }).to_string(), 0),
            Some("failed") => (json!({ "error": job["error"] }).to_string(), 2),
            _ => panic!("still waiting in {fixture}"),
        },
        ["export", "--no-wait"] | ["export-job", _] => {
            let mut printed = job.clone();
            if wrong {
                // Leaves out what has no value yet.
                printed.as_object_mut().unwrap().retain(|_, value| !value.is_null());
            }
            (json!({ "result": printed }).to_string(), 0)
        }
        _ => (json!({ "error": "usage" }).to_string(), 2),
    }
}

fn capture(name: &str, argv: &[&str], fail: Option<&str>, run: (String, i32)) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("underdraw-cli-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.json"));
    let mut written = json!({ "command": argv, "exit": run.1, "stdout": run.0 });
    if let Some(failure) = fail {
        written["failure"] = json!(failure);
    }
    std::fs::write(&path, written.to_string()).unwrap();
    path
}

/// Every golden output the CLI derives, as (fixture, command line, named
/// failure); a named failure runs in the first fixture.
fn runs() -> Vec<(String, Vec<String>, Option<String>)> {
    let (answer, code) = underdraw(&["cli", DRAFT]);
    assert_eq!(code, 0, "{answer}");
    answer["golden"]
        .as_array()
        .unwrap()
        .iter()
        .map(|golden| {
            let mut argv = vec![golden["command"].as_str().unwrap().to_string()];
            if let Some(args) = golden["args"].as_array() {
                argv.extend(args.iter().map(|a| a.as_str().unwrap().to_string()));
            }
            let fixture = golden["fixture"].as_str().unwrap_or("default").to_string();
            (fixture, argv, golden["failure"].as_str().map(str::to_string))
        })
        .collect()
}

/// Runs the stub on one golden output and answers what `compare` said.
fn compare(index: usize, run: &(String, Vec<String>, Option<String>), wrong: bool) -> (Value, i32) {
    let (fixture, argv, fail) = run;
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let printed = stub(fixture, &argv, fail.as_deref(), wrong);
    let name = format!("{}-{index}", if wrong { "wrong" } else { "right" });
    let path = capture(&name, &argv, fail.as_deref(), printed);
    underdraw(&["compare", DRAFT, "--capture", path.to_str().unwrap(), "--fixture", fixture])
}

#[test]
fn a_long_command_waits_or_hands_back_its_job_and_a_command_reads_the_job() {
    let (answer, code) = underdraw(&["cli", DRAFT]);
    assert_eq!(code, 0, "{answer}");
    let commands = answer["commands"].as_array().unwrap();
    let usages: Vec<&str> = commands.iter().map(|c| c["usage"].as_str().unwrap()).collect();
    assert_eq!(usages, ["get", "export [--no-wait]", "export-job <id>"]);
    let export = &commands[1];
    assert_eq!(export["long"]["job"], "ExportDocsJob");
    assert_eq!(export["long"]["poll"], "export-job <id>");
    assert_eq!(export["long"]["noWait"]["properties"]["result"]["$ref"], "#/$defs/ExportDocsJob");
    assert!(export["help"].as_str().unwrap().contains("--no-wait"), "{export}");
    assert_eq!(commands[2]["jobOf"], "export");

    // (command line, fixture) → (exit, stdout) for every job state.
    let golden = |argv: Value, fixture: &str| {
        let found = answer["golden"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| {
                let mut line = vec![g["command"].clone()];
                line.extend(g["args"].as_array().cloned().unwrap_or_default());
                Value::Array(line) == argv && g["fixture"] == fixture
            })
            .cloned();
        found.map(|g| (g["exit"].clone(), g["stdout"].clone()))
    };
    let running = json!({ "id": "j42", "status": "running", "progress": 40, "result": null, "error": null });
    assert_eq!(golden(json!(["export"]), "running"), None, "a running job has not ended");
    assert_eq!(
        golden(json!(["export", "--no-wait"]), "running"),
        Some((json!(0), json!({ "result": running })))
    );
    assert_eq!(
        golden(json!(["export-job", "j42"]), "running"),
        Some((json!(0), json!({ "result": running })))
    );
    assert_eq!(
        golden(json!(["export"]), "done"),
        Some((
            json!(0),
            json!({ "result": { "url": "https://example.com/exports/j42.zip", "files": 128, "size": "18.4 MB" } })
        ))
    );
    assert_eq!(golden(json!(["export"]), "failed"), Some((json!(2), json!({ "error": "tooLarge" }))));
    assert_eq!(golden(json!(["export-job", "j42"]), "failed").unwrap().1["result"]["error"], "tooLarge");
    assert_eq!(golden(json!(["export"]), "default"), None, "no job is held before one is started");
}

#[test]
fn a_faithful_stub_passes_compare_on_every_golden_output() {
    let runs = runs();
    assert!(runs.len() > 12, "{runs:?}");
    for (index, run) in runs.iter().enumerate() {
        let (answer, code) = compare(index, run, false);
        assert_eq!((code, &answer["ok"]), (0, &json!(true)), "{run:?}: {answer}");
    }
}

#[test]
fn a_wrong_stub_is_found_out() {
    let mut found = Vec::new();
    for (index, run) in runs().iter().enumerate() {
        let (answer, _) = compare(index, run, true);
        for finding in answer["findings"].as_array().into_iter().flatten() {
            found.push((run.1.join(" "), run.0.clone(), finding["at"].as_str().unwrap().to_string()));
        }
    }
    for (line, fixture, at) in [
        ("export", "done", "stdout.result.id"),
        ("export", "failed", "exit"),
        ("export --no-wait", "running", "stdout.result.result"),
        ("export-job j42", "done", "stdout.result.error"),
    ] {
        assert!(
            found.iter().any(|f| (f.0.as_str(), f.1.as_str(), f.2.as_str()) == (line, fixture, at)),
            "{line} in {fixture} at {at} not in {found:?}"
        );
    }
}
