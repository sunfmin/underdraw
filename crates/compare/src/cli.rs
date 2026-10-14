//! How a running CLI differs from the one derived from the description
//! (docs/adr/0011): what it printed and the code it exited with, for one
//! command in one fixture, against that command's golden output.
//!
//! ```json
//! { "command": ["delete", "b"], "exit": 1, "stdout": "{\"error\": \"requires canDelete\"}" }
//! ```

use serde::Serialize;
use serde_json::{Map, Value};
use underdraw_format::model;
use underdraw_handoff::{Golden, cli};

use crate::Finding;

#[derive(Debug, Serialize)]
pub struct CliComparison {
    pub screen: String,
    /// True when the CLI printed its golden output and exited with its code.
    pub ok: bool,
    /// The command line run, after the CLI's own name.
    pub command: Vec<String>,
    /// `{"fixture"}`, or `{"failure"}` for a named failure.
    pub state: Value,
    pub golden: Value,
    /// How many values of stdout were compared, and whether the exit code
    /// was.
    pub checked: CliChecked,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Default, Serialize)]
pub struct CliChecked {
    pub exit: u32,
    pub stdout: u32,
}

/// How the run `capture` holds differs from the golden output of its
/// command in `fixture`, or in the named failure the capture says it was
/// made in. Refused, with why: a description with faults, a capture that is
/// not one, a command line the derived CLI does not take, or one it has no
/// golden output for.
pub fn compare_cli(
    screen: &Value,
    kinds: Option<&Value>,
    fixture: &str,
    capture: &Value,
) -> Result<CliComparison, String> {
    let model = model(screen, kinds).map_err(|checked| {
        format!(
            "the description has {} finding(s), so it cannot say what the CLI should print: run `underdraw check` and fix them first",
            checked.findings.len()
        )
    })?;
    let interface = cli(&model);
    let mut command = Vec::new();
    match capture.get("command") {
        Some(Value::Array(words)) => {
            for word in words {
                match word.as_str() {
                    Some(word) => command.push(word.to_string()),
                    None => return Err("the capture's \"command\" is a list of strings: [\"delete\", \"b\"]".to_string()),
                }
            }
        }
        _ => return Err("the capture needs \"command\": the command line run, after the CLI's name, as [\"delete\", \"b\"]".to_string()),
    }
    let Some(exit) = capture.get("exit").and_then(Value::as_i64) else {
        return Err("the capture needs \"exit\": the code the CLI exited with".to_string());
    };
    let Some(stdout) = capture.get("stdout") else {
        return Err("the capture needs \"stdout\": what the CLI printed, as text".to_string());
    };
    let failure = match capture.get("failure") {
        None => None,
        Some(Value::String(failure)) => Some(failure.clone()),
        Some(_) => return Err("the capture's \"failure\" names one of the command's fails".to_string()),
    };
    let Some((name, args)) = command.split_first() else {
        return Err("the capture's \"command\" is empty: it starts with the command's name".to_string());
    };
    let mut spec = None;
    for candidate in &interface.commands {
        if &candidate.name == name {
            spec = Some(candidate);
        }
    }
    let Some(spec) = spec else {
        let mut names = Vec::new();
        for candidate in &interface.commands {
            names.push(candidate.name.clone());
        }
        return Err(format!("the derived CLI has no command {name:?}; its commands are {names:?}"));
    };
    let mut golden: Option<&Golden> = None;
    let state = match &failure {
        Some(failure) => {
            // A bulk form's failure is one item's, in one fixture, given
            // the ids its command line gives; any other's, whatever it is
            // given.
            let with = spec.with_args(args).ok();
            let yes = spec.yes(args);
            for candidate in &interface.golden {
                if candidate.command == spec.name
                    && candidate.failure.as_ref() == Some(failure)
                    && (candidate.fixture.is_none()
                        || (candidate.fixture.as_deref() == Some(fixture)
                            && candidate.with == with
                            && candidate.yes == yes))
                {
                    golden = Some(candidate);
                }
            }
            crate::object("failure", failure)
        }
        None => {
            let with = spec.with_args(args)?;
            let yes = spec.yes(args);
            let no_wait = spec.no_wait(args);
            for candidate in &interface.golden {
                if candidate.command == spec.name
                    && candidate.failure.is_none()
                    && candidate.fixture.as_deref() == Some(fixture)
                    && candidate.with.as_ref() == Some(&with)
                    && candidate.yes == yes
                    && candidate.no_wait == no_wait
                {
                    golden = Some(candidate);
                }
            }
            crate::object("fixture", fixture)
        }
    };
    let Some(golden) = golden else {
        let mut known = Vec::new();
        for candidate in &interface.golden {
            if candidate.command == spec.name {
                known.push(serde_json::to_value(candidate).unwrap_or(Value::Null));
            }
        }
        return Err(format!(
            "there is no golden output for {command:?} in {state}; the golden outputs of {} are {}",
            spec.name,
            Value::Array(known)
        ));
    };

    let mut checked = CliChecked::default();
    let mut findings = Vec::new();
    checked.exit = 1;
    if exit != i64::from(golden.exit) {
        findings.push(Finding {
            at: "exit".to_string(),
            check: "exit",
            problem: format!("the CLI exited with {exit}; the golden output exits with {}", golden.exit),
        });
    }
    let printed = match stdout {
        Value::String(text) => serde_json::from_str::<Value>(text).ok(),
        other => Some(other.clone()),
    };
    match printed {
        Some(printed @ Value::Object(_)) => {
            diff("stdout", &golden.stdout, &printed, &mut checked, &mut findings)
        }
        _ => findings.push(Finding {
            at: "stdout".to_string(),
            check: "stdout",
            problem: format!("the CLI printed {stdout}; it prints one JSON object, {}", golden.stdout),
        }),
    }
    Ok(CliComparison {
        screen: interface.screen.clone(),
        ok: findings.is_empty(),
        command,
        state,
        golden: serde_json::to_value(golden).unwrap_or(Value::Null),
        checked,
        findings,
    })
}

/// Each place `got` differs from `wanted`, at the JSON path `at`.
fn diff(at: &str, wanted: &Value, got: &Value, checked: &mut CliChecked, findings: &mut Vec<Finding>) {
    match (wanted, got) {
        (Value::Object(wanted), Value::Object(got)) => objects(at, wanted, got, checked, findings),
        (Value::Array(wanted), Value::Array(got)) => {
            if wanted.len() != got.len() {
                checked.stdout = checked.stdout.saturating_add(1);
                findings.push(Finding {
                    at: at.to_string(),
                    check: "stdout",
                    problem: format!("{} items; the golden output has {}", got.len(), wanted.len()),
                });
            }
            for (index, (wanted, got)) in wanted.iter().zip(got.iter()).enumerate() {
                diff(&format!("{at}[{index}]"), wanted, got, checked, findings);
            }
        }
        _ => {
            checked.stdout = checked.stdout.saturating_add(1);
            if wanted != got {
                findings.push(Finding {
                    at: at.to_string(),
                    check: "stdout",
                    problem: format!("is {got}; the golden output has {wanted}"),
                });
            }
        }
    }
}

fn objects(
    at: &str,
    wanted: &Map<String, Value>,
    got: &Map<String, Value>,
    checked: &mut CliChecked,
    findings: &mut Vec<Finding>,
) {
    for (key, wanted) in wanted {
        let path = format!("{at}.{key}");
        match got.get(key) {
            Some(got) => diff(&path, wanted, got, checked, findings),
            None => {
                checked.stdout = checked.stdout.saturating_add(1);
                findings.push(Finding {
                    at: path,
                    check: "stdout",
                    problem: format!("missing; the golden output has {wanted}"),
                });
            }
        }
    }
    for (key, got) in got {
        if !wanted.contains_key(key) {
            checked.stdout = checked.stdout.saturating_add(1);
            findings.push(Finding {
                at: format!("{at}.{key}"),
                check: "stdout",
                problem: format!("is {got}; the golden output has no {key}"),
            });
        }
    }
}
