//! A command-line interface derived from the screen's operations
//! (docs/adr/0011): one command per operation, its stdout as a JSON Schema,
//! its failures as exit codes, a help draft, and the golden outputs each
//! fixture projects to. The screen and its CLI come from one description,
//! so they cannot drift apart.

use serde::Serialize;
use serde_json::{Map, Value};
use underdraw_format::{Base, BulkOf, Does, Field, Model, Operation, Shape};

use crate::json_schema::{MONEY, def, money, object_of};
use crate::{is_false, kebab, object};

/// Exit 1: the command was refused before it was tried, as its
/// precondition does not hold.
pub const REFUSED: u8 = 1;
/// Exit 2: the command was tried and failed, in one of its named ways.
pub const FAILED: u8 = 2;
/// The flag a destructive command needs before it does anything.
pub const YES: &str = "--yes";
/// What `"error"` says when a destructive command is run without `--yes`.
const NEEDS_YES: &str = "needs --yes";
/// The flag that makes a long command hand back its job instead of waiting.
pub const NO_WAIT: &str = "--no-wait";

#[derive(Debug, Serialize)]
pub struct Interface {
    pub screen: String,
    /// One per operation, in the order written.
    pub commands: Vec<CommandSpec>,
    /// What a faithful CLI prints: for each fixture, each query it can
    /// answer and each command its nodes call; and each named failure.
    pub golden: Vec<Golden>,
}

#[derive(Debug, Serialize)]
pub struct CommandSpec {
    /// The command's name: the operation's first word (`list` for
    /// `listDocs`), or the whole name in kebab case when two operations
    /// start with the same word.
    pub name: String,
    pub operation: String,
    /// `query` or `command`.
    pub does: String,
    pub doc: String,
    /// It cannot be undone, so it does nothing without `--yes`: it exits 1
    /// and says what it would do, as a dry run.
    #[serde(skip_serializing_if = "is_false")]
    pub destructive: bool,
    /// For a bulk form: the operation it does for each id it is given. It
    /// answers with one result per item, and exits 0 only when each was
    /// done.
    #[serde(rename = "bulkOf", skip_serializing_if = "Option::is_none")]
    pub bulk_of: Option<String>,
    /// It takes long (docs/adr/0015): it waits for its job and prints the
    /// result, or, given `--no-wait`, prints the job to read again later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long: Option<Long>,
    /// For the command that reads a long command's job: that command.
    #[serde(rename = "jobOf", skip_serializing_if = "Option::is_none")]
    pub job_of: Option<String>,
    /// `list [--sort <updated|created|title|owner>]`, `delete <id> --yes`,
    /// `delete-docs <ids>… --yes`.
    pub usage: String,
    pub args: Vec<Arg>,
    /// What it prints on success: a JSON Schema of `{"result": …}`, or of
    /// `{}` for an operation that answers with nothing.
    pub stdout: Value,
    /// Every way it ends without its result: its precondition first, then
    /// its dry run without `--yes` when it is destructive, then each named
    /// failure. A bulk form's precondition and named failures are an
    /// item's: exit 2, the others still tried.
    pub fails: Vec<Failure>,
    /// A draft of `<command> --help`.
    pub help: String,
}

/// What a long command hands back with `--no-wait`, and how to read it again.
#[derive(Debug, Serialize)]
pub struct Long {
    /// The job's type: `ExportDocsJob`.
    pub job: String,
    /// The command that reads the job again: `export-job <id>`.
    pub poll: String,
    /// What it prints with `--no-wait`: a JSON Schema of `{"result": <job>}`.
    #[serde(rename = "noWait")]
    pub no_wait: Value,
    /// What it prints on stderr while it waits.
    pub progress: String,
}

/// One parameter, as a positional argument (written without a default) or
/// a flag (with one, or nullable, or a list); a bulk form's ids are
/// positional, as many as are given.
#[derive(Debug, Serialize)]
pub struct Arg {
    pub param: String,
    /// `--sort`; none for a positional argument.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    /// The type, as the description writes it: `SortKey`, `string`.
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// A list: the flag is given once per value; positional, each value
    /// is one argument.
    #[serde(skip_serializing_if = "is_false")]
    pub repeated: bool,
    #[serde(skip)]
    field: Field,
}

/// One way a command ends without its result.
#[derive(Debug, Serialize)]
pub struct Failure {
    /// What `"error"` says: the failure's name, `requires <condition>`, or
    /// `needs --yes`.
    pub error: String,
    pub when: String,
    pub exit: u8,
    pub stdout: Value,
}

/// What a faithful CLI prints for one command: in one fixture, given
/// `args`, or in one named failure, whatever it is given. A bulk form's
/// named failure is in one fixture, given `args`: the first item fails.
#[derive(Debug, Serialize)]
pub struct Golden {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    /// The arguments after the command's name, defaults left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// The parameters those arguments give, defaults filled in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with: Option<Value>,
    /// The arguments include `--yes`.
    #[serde(skip_serializing_if = "is_false")]
    pub yes: bool,
    /// The arguments include `--no-wait`.
    #[serde(rename = "noWait", skip_serializing_if = "is_false")]
    pub no_wait: bool,
    pub exit: u8,
    pub stdout: Value,
}

/// The CLI derived from `model`'s operations.
pub fn cli(model: &Model) -> Interface {
    let operations = model.operations.all();
    let mut commands = Vec::new();
    for operation in operations {
        commands.push(command(model, operation, &name(operation, operations)));
    }
    let mut golden = Vec::new();
    for spec in &commands {
        if let Some(operation) = model.operations.get(&spec.operation) {
            queried(model, operation, spec, &mut golden);
        }
    }
    called(model, &commands, &mut golden);
    polled(model, &commands, &mut golden);
    for spec in &commands {
        for failure in &spec.fails {
            if failure.exit == FAILED && spec.bulk_of.is_none() {
                golden.push(Golden {
                    command: spec.name.clone(),
                    fixture: None,
                    failure: Some(failure.error.clone()),
                    args: None,
                    with: None,
                    yes: false,
                    no_wait: false,
                    exit: failure.exit,
                    stdout: failure.stdout.clone(),
                });
            }
        }
    }
    Interface { screen: model.screen().to_string(), commands, golden }
}

/// The first word of `operation`'s name, unless another operation starts
/// with it too; then the whole name, in kebab case. A bulk form's is always
/// its whole name (`delete-docs`), and leaves the name of the operation it
/// is derived from as it was (`delete`). The query that reads a long
/// command's job is that command's name and `-job`.
fn name(operation: &Operation, all: &[Operation]) -> String {
    if operation.bulk_of.is_some() {
        return kebab(&operation.name);
    }
    if let Some(command) = &operation.job_of {
        for other in all {
            if &other.name == command {
                return format!("{}-job", name(other, all));
            }
        }
    }
    let word = first_word(&operation.name);
    let mut sharing: u32 = 0;
    for other in all {
        if other.bulk_of.is_none() && other.job_of.is_none() && first_word(&other.name) == word {
            sharing = sharing.saturating_add(1);
        }
    }
    if sharing > 1 { kebab(&operation.name) } else { word }
}

fn first_word(name: &str) -> String {
    let mut word = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            break;
        }
        word.push(c);
    }
    word
}

fn command(model: &Model, operation: &Operation, name: &str) -> CommandSpec {
    let bulk = operation.bulk_of.as_ref();
    let mut args = Vec::new();
    for (param, field) in &operation.params {
        let choices = model.schema.members(field).map(<[String]>::to_vec).unwrap_or_default();
        let default = field.default.clone().filter(|default| !default.is_null());
        let ids = bulk.is_some_and(|of| &of.keys == param);
        let positional = ids || (default.is_none() && !field.nullable && !field.array);
        args.push(Arg {
            param: param.clone(),
            flag: if positional { None } else { Some(format!("--{}", kebab(param))) },
            type_name: field.describe(),
            choices,
            default,
            repeated: field.array,
            field: field.clone(),
        });
    }
    let mut fails = Vec::new();
    if let Some(condition) = &operation.requires {
        let mut label = condition.clone();
        for defined in model.conditions() {
            if &defined.name == condition {
                label = defined.label.clone();
            }
        }
        let error = format!("requires {condition}");
        fails.push(match bulk {
            None => Failure {
                stdout: object([("error", Value::from(error.as_str()))]),
                error,
                when: format!("refused before it is tried: \"{label}\" does not hold"),
                exit: REFUSED,
            },
            Some(of) => Failure {
                stdout: item_failed(of, &error),
                error,
                when: format!("an item \"{label}\" does not hold for is refused; the others are still tried"),
                exit: FAILED,
            },
        });
    }
    if operation.destructive {
        let mut with = Map::new();
        for (param, _) in &operation.params {
            with.insert(param.clone(), Value::from(format!("<{param}>")));
        }
        fails.push(Failure {
            error: NEEDS_YES.to_string(),
            when: format!("not tried without {YES}, as it cannot be undone: says what it would do"),
            exit: REFUSED,
            stdout: dry_run(operation, Value::Object(with)),
        });
    }
    for (failure, when) in &operation.fails {
        fails.push(Failure {
            error: failure.clone(),
            when: match bulk {
                None => when.clone(),
                Some(_) => format!("{when}, for an item; the others are still tried"),
            },
            exit: FAILED,
            stdout: match bulk {
                None => object([("error", Value::from(failure.as_str()))]),
                Some(of) => item_failed(of, failure),
            },
        });
    }
    let mut usage = usage(name, &args);
    if operation.destructive {
        usage.push_str(&format!(" {YES}"));
    }
    let long = match (operation.job_type(), operation.job_query()) {
        (Some(job), Some(query)) => {
            usage.push_str(&format!(" [{NO_WAIT}]"));
            let mut poll = format!("{}-job <id>", name);
            for other in model.operations.all() {
                if other.name == query {
                    poll = format!("{} <id>", self::name(other, model.operations.all()));
                }
            }
            let job_field = Field::of(Base::Named(job.clone()));
            Some(Long { no_wait: stdout(model, Some(&job_field)), progress: PROGRESS.to_string(), job, poll })
        }
        _ => None,
    };
    let stdout = match bulk {
        Some(of) => bulk_stdout(model, operation, of, &fails),
        None => stdout(model, operation.result.as_ref()),
    };
    let help = help(operation, &usage, &args, &fails, long.as_ref());
    CommandSpec {
        name: name.to_string(),
        operation: operation.name.clone(),
        does: operation.does.name().to_string(),
        doc: operation.doc.clone(),
        destructive: operation.destructive,
        bulk_of: bulk.map(|of| of.operation.clone()),
        long,
        job_of: operation.job_of.as_ref().map(|command| {
            let mut named = command.clone();
            for other in model.operations.all() {
                if &other.name == command {
                    named = self::name(other, model.operations.all());
                }
            }
            named
        }),
        usage,
        args,
        stdout,
        fails,
        help,
    }
}

/// What a destructive command run without `--yes` prints: that it needs
/// it, and what it would have done, given `with`.
fn dry_run(operation: &Operation, with: Value) -> Value {
    object([
        ("error", Value::from(NEEDS_YES)),
        (
            "dryRun",
            object([
                ("operation", Value::from(operation.name.as_str())),
                ("does", Value::from(operation.doc.as_str())),
                ("with", with),
            ]),
        ),
    ])
}

/// What a bulk form prints when one item failed with `error`: that item's
/// result, standing for every item's.
fn item_failed(of: &BulkOf, error: &str) -> Value {
    let item =
        object([(of.key.as_str(), Value::from(format!("<{}>", of.key))), ("error", Value::from(error))]);
    object([("results", Value::Array(vec![item]))])
}

/// What a bulk form prints for `ids`, each done but the one at `failed`,
/// which failed with the error given.
fn results(of: &BulkOf, ids: &[Value], failed: Option<(usize, &str)>) -> Value {
    let mut items = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let mut item = Map::new();
        item.insert(of.key.clone(), id.clone());
        if let Some((at, error)) = failed
            && at == index
        {
            item.insert("error".to_string(), Value::from(error));
        }
        items.push(Value::Object(item));
    }
    object([("results", Value::Array(items))])
}

/// `delete <id>`, `list [--sort <updated|created>]`, `delete-docs <ids>…`.
fn usage(name: &str, args: &[Arg]) -> String {
    let mut usage = name.to_string();
    for arg in args {
        let value = if arg.choices.is_empty() { arg.param.clone() } else { arg.choices.join("|") };
        match &arg.flag {
            None if arg.repeated => usage.push_str(&format!(" <{value}>…")),
            None => usage.push_str(&format!(" <{value}>")),
            Some(flag) => {
                let more = if arg.repeated { " …" } else { "" };
                usage.push_str(&format!(" [{flag} <{value}>{more}]"));
            }
        }
    }
    usage
}

/// The schema of `{"result": <result>}`, with every type the result
/// reaches and no other: so none that holds view state.
fn stdout(model: &Model, result: Option<&Field>) -> Value {
    let mut fields = Vec::new();
    if let Some(result) = result {
        fields.push(("result".to_string(), Field { doc: None, derived: None, from: None, ..result.clone() }));
    }
    let mut schema = Map::new();
    schema.insert("$schema".to_string(), Value::from("https://json-schema.org/draft/2020-12/schema"));
    if let Value::Object(body) = object_of(&fields) {
        schema.extend(body);
    }
    let mut reached: Vec<String> = Vec::new();
    let mut money_used = false;
    if let Some(result) = result {
        reach(model, &result.base, &mut reached, &mut money_used);
    }
    if !reached.is_empty() || money_used {
        let mut defs = Map::new();
        for (name, shape) in model.schema.types() {
            if reached.contains(name) {
                defs.insert(name.clone(), def(shape));
            }
        }
        if money_used {
            defs.insert(MONEY.to_string(), money());
        }
        schema.insert("$defs".to_string(), Value::Object(defs));
    }
    Value::Object(schema)
}

/// The schema of what the bulk form `operation` prints: `{"results": [<one
/// per item>]}`, each item its key, its result when the operation answers
/// with one, and the error it was not done with, one of `fails`.
fn bulk_stdout(model: &Model, operation: &Operation, of: &BulkOf, fails: &[Failure]) -> Value {
    let Value::Object(mut schema) = stdout(model, operation.result.as_ref()) else {
        return Value::Null;
    };
    let key = operation.params.iter().find(|(param, _)| param == &of.keys).map(|(_, field)| field.element());
    let key = Field { lo: None, hi: None, ..key.unwrap_or(Field::of(Base::String)) };
    let mut item = object_of(&[(of.key.clone(), key)]);
    let mut errors = Vec::new();
    for failure in fails {
        if failure.exit == FAILED {
            errors.push(Value::from(failure.error.as_str()));
        }
    }
    if let Some(Value::Object(properties)) = item.get_mut("properties") {
        if let Some(result) = schema.get("properties").and_then(|answered| answered.get("result")) {
            properties.insert("result".to_string(), result.clone());
        }
        properties.insert(
            "error".to_string(),
            object([("type", Value::from("string")), ("enum", Value::Array(errors))]),
        );
    }
    schema.insert(
        "properties".to_string(),
        object([("results", object([("type", Value::from("array")), ("items", item)]))]),
    );
    schema.insert("required".to_string(), Value::from(vec!["results"]));
    Value::Object(schema)
}

/// Every named type a value of `base` reaches, into `reached`; whether it
/// reaches money, into `money_used`.
fn reach(model: &Model, base: &Base, reached: &mut Vec<String>, money_used: &mut bool) {
    let name = match base {
        Base::Money => {
            *money_used = true;
            return;
        }
        Base::Named(name) => name,
        _ => return,
    };
    if reached.contains(name) {
        return;
    }
    reached.push(name.clone());
    for (declared, shape) in model.schema.types() {
        if declared == name
            && let Shape::Object(fields) = shape
        {
            for (_, field) in fields {
                reach(model, &field.base, reached, money_used);
            }
        }
    }
}

/// What a long command prints on stderr while it waits.
const PROGRESS: &str = "one JSON object a line, each time the job moves: {\"job\": \"<id>\", \"status\": \"running\", \"progress\": 40}";

fn help(operation: &Operation, usage: &str, args: &[Arg], fails: &[Failure], long: Option<&Long>) -> String {
    let mut help = format!("{}\n\nUSAGE\n  <cli> {usage}\n", operation.doc);
    if !args.is_empty() || operation.destructive || long.is_some() {
        help.push_str("\nARGUMENTS\n");
        for arg in args {
            let written = match &arg.flag {
                None => format!("<{}>", arg.param),
                Some(flag) => format!("{flag} <{}>", arg.param),
            };
            let mut said = if arg.repeated && arg.flag.is_none() {
                arg.field.element().describe()
            } else {
                arg.type_name.clone()
            };
            if !arg.choices.is_empty() {
                said = format!("one of {}", arg.choices.join(", "));
            }
            if operation.pages && arg.param == "cursor" {
                said = "where the page starts: the \"next\" the page before answered with; the first page when left out"
                    .to_string();
            }
            if operation.pages && arg.param == "limit" {
                said = "at most this many rows".to_string();
            }
            if let Some(default) = &arg.default {
                said.push_str(&format!("; default {}", text_of(default)));
            }
            if arg.repeated {
                said.push_str(if arg.flag.is_some() { "; give it once per value" } else { "; one or more" });
            }
            help.push_str(&format!("  {written}  {said}\n"));
        }
        if operation.destructive {
            help.push_str(&format!(
                "  {YES}  do it: it cannot be undone; without {YES} nothing is done, and it says what would be\n"
            ));
        }
        if let Some(long) = long {
            help.push_str(&format!(
                "  {NO_WAIT}  do not wait: print the job, {{\"result\": {}}}, and read it again with `<cli> {}`\n",
                long.job, long.poll
            ));
        }
    }
    if let Some(long) = long {
        help.push_str(&format!(
            "\nWAITING\n  It takes long: it waits for its job to end, and prints its progress on stderr,\n  {}.\n  A job that fails exits 2 with the failure it names.\n",
            long.progress
        ));
    }
    let answer = match (&operation.bulk_of, &operation.result) {
        (Some(of), Some(result)) => format!(
            "{{\"results\": [{{\"{}\", \"result\": {}}}, …]}}, one per item in the order given, each done; exit 2 when any was not, it with \"error\"",
            of.key,
            result.describe()
        ),
        (Some(of), None) => format!(
            "{{\"results\": [{{\"{}\"}}, …]}}, one per item in the order given, each done; exit 2 when any was not, it with \"error\"",
            of.key
        ),
        (None, Some(result)) => format!("{{\"result\": {}}}", result.describe()),
        (None, None) => "{}".to_string(),
    };
    help.push_str(&format!("\nANSWERS\n  One JSON object on stdout. Exit 0: {answer}\n"));
    if operation.pages {
        help.push_str("  One page at a time: \"next\" is where the next page starts, null on the last; pass it to --cursor for more.\n");
    }
    for failure in fails {
        help.push_str(&format!("  Exit {}: {}  {}\n", failure.exit, failure.stdout, failure.when));
    }
    help
}

/// A value as written on a command line: a string without its quotes.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// For a query whose result fills `data` and that needs no argument: in
/// each fixture, the result that fixture holds, projected through what it
/// fills; or, filled part by part (`"from": "listDocs.next"`), each part
/// so, when every field of the result is filled.
fn queried(model: &Model, operation: &Operation, spec: &CommandSpec, golden: &mut Vec<Golden>) {
    if operation.does != Does::Query {
        return;
    }
    let Some(paths) = paths(model, operation) else {
        return;
    };
    let Ok(with) = spec.with_args(&[]) else {
        return;
    };
    for (fixture, data) in &model.fixtures {
        let result = match &paths {
            Answer::Whole(path) => project(data, path),
            Answer::Parts(parts) => {
                let mut result = Map::new();
                for (part, path) in parts {
                    result.insert(part.clone(), project(data, path));
                }
                Value::Object(result)
            }
        };
        golden.push(Golden {
            command: spec.name.clone(),
            fixture: Some(fixture.clone()),
            failure: None,
            args: Some(Vec::new()),
            with: Some(with.clone()),
            yes: false,
            no_wait: false,
            exit: 0,
            stdout: object([("result", result)]),
        });
    }
}

/// Where in the view model a query's answer is held.
enum Answer {
    /// All of it, at one path.
    Whole(String),
    /// Each field of it, by name, at its own path.
    Parts(Vec<(String, String)>),
}

/// Where the view model holds all of `operation`'s result; none when it
/// holds none of it, or only some of its fields.
fn paths(model: &Model, operation: &Operation) -> Option<Answer> {
    if let Some(fill) = operation.fills.iter().find(|fill| fill.part.is_none()) {
        return Some(Answer::Whole(fill.path.clone()));
    }
    let result = operation.result.as_ref()?;
    let Some(Shape::Object(fields)) = model.schema.shape(result) else {
        return None;
    };
    let mut parts = Vec::new();
    for (name, _) in fields {
        let fill = operation.fills.iter().find(|fill| fill.part.as_deref() == Some(name.as_str()))?;
        parts.push((name.clone(), fill.path.clone()));
    }
    Some(Answer::Parts(parts))
}

/// The value at `path` (`docs`, `docs[].doc`) in `data`: through `[]`,
/// each element's.
fn project(data: &Value, path: &str) -> Value {
    let (head, rest) = match path.split_once('.') {
        Some((head, rest)) => (head, Some(rest)),
        None => (path, None),
    };
    let (key, each) = match head.strip_suffix("[]") {
        Some(key) => (key, true),
        None => (head, false),
    };
    let value = data.get(key).cloned().unwrap_or(Value::Null);
    let Some(rest) = rest else {
        return value;
    };
    match value {
        Value::Array(items) if each => {
            let mut projected = Vec::new();
            for item in &items {
                projected.push(project(item, rest));
            }
            Value::Array(projected)
        }
        Value::Null => Value::Null,
        other => project(&other, rest),
    }
}

/// For each command a node calls in a fixture, what it prints: refused
/// where its precondition does not hold; `{}` where it holds and the
/// command answers with nothing. A command that answers with something the
/// fixture does not hold has no golden output.
fn called(model: &Model, commands: &[CommandSpec], golden: &mut Vec<Golden>) {
    for expected in crate::states(model, &[]) {
        let Some(Value::String(fixture)) = expected.state.get("fixture") else {
            continue;
        };
        for calls in expected.calls.values() {
            let Value::Array(calls) = calls else {
                continue;
            };
            for call in calls {
                let name = call.get("operation").and_then(Value::as_str).unwrap_or_default();
                let Some(operation) = model.operations.get(name) else {
                    continue;
                };
                if operation.does != Does::Command {
                    continue;
                }
                let mut spec = None;
                for candidate in commands {
                    if candidate.operation == name {
                        spec = Some(candidate);
                    }
                }
                let Some(spec) = spec else {
                    continue;
                };
                let given = call.get("with").cloned().unwrap_or(Value::Object(Map::new()));
                let args = spec.args_for(&given);
                let Ok(with) = spec.with_args(&args) else {
                    continue;
                };
                // A bulk form's precondition is each item's: the fixture's
                // selection is of items it holds for.
                let refused = operation.bulk_of.is_none()
                    && operation.requires.as_ref().is_some_and(|condition| !holds(model, condition, fixture));
                // A destructive command is run both ways: without --yes and
                // with it; a long one, waiting and with --no-wait. Any other
                // is run as it is.
                let mut runs = vec![(false, false)];
                if operation.destructive {
                    runs.push((true, false));
                }
                if operation.long {
                    runs.push((operation.destructive, true));
                }
                let job = job_in(model, operation, fixture);
                for (yes, no_wait) in runs {
                    let (exit, stdout) = if refused {
                        let mut stdout = Value::Null;
                        for failure in &spec.fails {
                            if failure.exit == REFUSED && failure.error != NEEDS_YES {
                                stdout = failure.stdout.clone();
                            }
                        }
                        (REFUSED, stdout)
                    } else if operation.destructive && !yes {
                        (REFUSED, dry_run(operation, with.clone()))
                    } else if operation.long {
                        // What the job the fixture holds ends with; none
                        // where it holds none, or one still running.
                        let Some(job) = &job else {
                            continue;
                        };
                        match (no_wait, job.get("status").and_then(Value::as_str)) {
                            (true, _) => (0, object([("result", job.clone())])),
                            (false, Some("done")) => match &operation.result {
                                Some(_) => (
                                    0,
                                    object([("result", job.get("result").cloned().unwrap_or(Value::Null))]),
                                ),
                                None => (0, Value::Object(Map::new())),
                            },
                            (false, Some("failed")) => (
                                FAILED,
                                object([("error", job.get("error").cloned().unwrap_or(Value::Null))]),
                            ),
                            _ => continue,
                        }
                    } else if operation.result.is_some() {
                        continue;
                    } else if let Some(of) = &operation.bulk_of {
                        (0, results(of, ids(&with, of), None))
                    } else {
                        (0, Value::Object(Map::new()))
                    };
                    let mut seen = false;
                    for known in golden.iter() {
                        if known.command == spec.name
                            && known.fixture.as_deref() == Some(fixture.as_str())
                            && known.with.as_ref() == Some(&with)
                            && known.yes == yes
                            && known.no_wait == no_wait
                        {
                            seen = true;
                        }
                    }
                    if !seen {
                        let mut args = args.clone();
                        if yes {
                            args.push(YES.to_string());
                        }
                        if no_wait {
                            args.push(NO_WAIT.to_string());
                        }
                        golden.push(Golden {
                            command: spec.name.clone(),
                            fixture: Some(fixture.clone()),
                            failure: None,
                            args: Some(args),
                            with: Some(with.clone()),
                            yes,
                            no_wait,
                            exit,
                            stdout,
                        });
                    }
                }
                if let Some(of) = &operation.bulk_of {
                    partly(spec, operation, of, fixture, &args, &with, golden);
                }
            }
        }
    }
}

/// The ids a bulk form's parameters `with` give.
fn ids<'a>(with: &'a Value, of: &BulkOf) -> &'a [Value] {
    with.get(&of.keys).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// For a bulk form called with at least two ids, once for each way an item
/// can fail, the first time a fixture's call gives them: the first item
/// fails that way, the others are done, and it exits 2.
fn partly(
    spec: &CommandSpec,
    operation: &Operation,
    of: &BulkOf,
    fixture: &str,
    args: &[String],
    with: &Value,
    golden: &mut Vec<Golden>,
) {
    let ids = ids(with, of);
    if ids.len() < 2 || operation.result.is_some() {
        return;
    }
    for failure in &spec.fails {
        if failure.exit != FAILED
            || golden
                .iter()
                .any(|known| known.command == spec.name && known.failure.as_ref() == Some(&failure.error))
        {
            continue;
        }
        let mut args = args.to_vec();
        if operation.destructive {
            args.push(YES.to_string());
        }
        golden.push(Golden {
            command: spec.name.clone(),
            fixture: Some(fixture.to_string()),
            failure: Some(failure.error.clone()),
            args: Some(args),
            with: Some(with.clone()),
            yes: operation.destructive,
            no_wait: false,
            exit: FAILED,
            stdout: results(of, ids, Some((0, &failure.error))),
        });
    }
}

/// The job of the long command `operation` that `fixture` holds: the value
/// of the field of `data` of its job type, when it is not null.
fn job_in(model: &Model, operation: &Operation, fixture: &str) -> Option<Value> {
    let job = operation.job_type()?;
    let (_, data) = model.fixtures.iter().find(|(name, _)| name == fixture)?;
    for (name, field) in model.schema.roots() {
        if field.base == Base::Named(job.clone())
            && !field.array
            && let Some(value) = data.get(&name).filter(|value| value.is_object())
        {
            return Some(value.clone());
        }
    }
    None
}

/// For the command that reads a long command's job: in each fixture that
/// holds a job, given its id, that job as it stands, running, done or failed.
fn polled(model: &Model, commands: &[CommandSpec], golden: &mut Vec<Golden>) {
    for spec in commands {
        let Some(command) = model.operations.get(&spec.operation).and_then(|query| query.job_of.as_ref())
        else {
            continue;
        };
        let Some(operation) = model.operations.get(command) else {
            continue;
        };
        for (fixture, _) in &model.fixtures {
            let Some(job) = job_in(model, operation, fixture) else {
                continue;
            };
            let args = vec![text_of(job.get("id").unwrap_or(&Value::Null))];
            let Ok(with) = spec.with_args(&args) else {
                continue;
            };
            golden.push(Golden {
                command: spec.name.clone(),
                fixture: Some(fixture.clone()),
                failure: None,
                args: Some(args),
                with: Some(with),
                yes: false,
                no_wait: false,
                exit: 0,
                stdout: object([("result", job)]),
            });
        }
    }
}

fn holds(model: &Model, condition: &str, fixture: &str) -> bool {
    for defined in &model.checked.conditions {
        if defined.name == condition {
            return defined.holds_in.iter().any(|name| name == fixture);
        }
    }
    false
}

impl CommandSpec {
    /// Whether `args` give `--yes`, which a destructive command needs
    /// before it does anything.
    pub fn yes(&self, args: &[String]) -> bool {
        self.destructive && args.iter().any(|word| word == YES)
    }

    /// Whether `args` give `--no-wait`, which makes a long command print
    /// its job instead of waiting for it.
    pub fn no_wait(&self, args: &[String]) -> bool {
        self.long.is_some() && args.iter().any(|word| word == NO_WAIT)
    }

    /// The parameters `args` (what follows the command's name) give, each
    /// default filled in; why, when they are not this command's arguments.
    /// A destructive command's `--yes` gives none.
    pub fn with_args(&self, args: &[String]) -> Result<Value, String> {
        let mut with = Map::new();
        let mut positional = Vec::new();
        for arg in &self.args {
            if arg.flag.is_none() {
                positional.push(arg);
            }
        }
        let mut next_positional: usize = 0;
        let mut index: usize = 0;
        while index < args.len() {
            let word = &args[index];
            index = index.saturating_add(1);
            if (self.destructive && word == YES) || (self.long.is_some() && word == NO_WAIT) {
                continue;
            }
            if let Some(flag) = word.strip_prefix("--") {
                let (flag, inline) = match flag.split_once('=') {
                    Some((flag, value)) => (flag, Some(value.to_string())),
                    None => (flag, None),
                };
                let mut found = None;
                for arg in &self.args {
                    if arg.flag.as_deref() == Some(format!("--{flag}").as_str()) {
                        found = Some(arg);
                    }
                }
                let Some(arg) = found else {
                    return Err(format!("{} takes no --{flag}: {}", self.name, self.usage));
                };
                let text = match inline {
                    Some(text) => text,
                    None if index < args.len() => {
                        let text = args[index].clone();
                        index = index.saturating_add(1);
                        text
                    }
                    None => return Err(format!("--{flag} needs a value: {}", self.usage)),
                };
                let value = arg.read(&text)?;
                if arg.repeated {
                    let entry = with.entry(arg.param.clone()).or_insert_with(|| Value::Array(Vec::new()));
                    if let Value::Array(values) = entry {
                        values.push(value);
                    }
                } else {
                    with.insert(arg.param.clone(), value);
                }
            } else {
                let Some(arg) = positional.get(next_positional) else {
                    return Err(format!(
                        "{} takes no more arguments than {}: {word:?}",
                        self.name, self.usage
                    ));
                };
                let value = arg.read(word)?;
                if arg.repeated {
                    // As many as are given: each is one more.
                    let entry = with.entry(arg.param.clone()).or_insert_with(|| Value::Array(Vec::new()));
                    if let Value::Array(values) = entry {
                        values.push(value);
                    }
                } else {
                    next_positional = next_positional.saturating_add(1);
                    with.insert(arg.param.clone(), value);
                }
            }
        }
        for arg in &self.args {
            if with.contains_key(&arg.param) {
                continue;
            }
            match &arg.default {
                Some(default) => {
                    with.insert(arg.param.clone(), default.clone());
                }
                None if arg.flag.is_none() => {
                    return Err(format!("{} needs <{}>: {}", self.name, arg.param, self.usage));
                }
                None => {}
            }
        }
        Ok(Value::Object(with))
    }

    /// The arguments that give `with`: each positional in order, then each
    /// flag whose value is not its default.
    fn args_for(&self, with: &Value) -> Vec<String> {
        let mut args = Vec::new();
        for arg in &self.args {
            let Some(value) = with.get(&arg.param) else {
                continue;
            };
            match &arg.flag {
                None if arg.repeated => {
                    for value in value.as_array().map_or(&[][..], Vec::as_slice) {
                        args.push(text_of(value));
                    }
                }
                None => args.push(text_of(value)),
                Some(flag) => {
                    if arg.default.as_ref() == Some(value) || value.is_null() {
                        continue;
                    }
                    let values = match value {
                        Value::Array(values) if arg.repeated => values.clone(),
                        other => vec![other.clone()],
                    };
                    for value in &values {
                        args.push(flag.clone());
                        args.push(text_of(value));
                    }
                }
            }
        }
        args
    }
}

impl Arg {
    /// `text`, as written on the command line, read as a value of this
    /// argument's type.
    fn read(&self, text: &str) -> Result<Value, String> {
        let at = match &self.flag {
            Some(flag) => flag.clone(),
            None => format!("<{}>", self.param),
        };
        if !self.choices.is_empty() {
            return if self.choices.iter().any(|choice| choice == text) {
                Ok(Value::from(text))
            } else {
                Err(format!("{at} is one of {}, not {text:?}", self.choices.join(", ")))
            };
        }
        match self.field.base {
            Base::Int => text
                .parse::<i64>()
                .map(Value::from)
                .map_err(|_| format!("{at} is a whole number, not {text:?}")),
            Base::Number => {
                text.parse::<f64>().map(Value::from).map_err(|_| format!("{at} is a number, not {text:?}"))
            }
            Base::Bool => match text {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => Err(format!("{at} is true or false, not {text:?}")),
            },
            Base::Money | Base::Named(_) => serde_json::from_str(text)
                .map_err(|_| format!("{at} is a JSON value of {}, not {text:?}", self.type_name)),
            _ => Ok(Value::from(text)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_through_a_list_projects_each_element() {
        let data = serde_json::json!({ "docs": [{ "doc": { "id": "a" }, "menuOpen": true }, { "doc": { "id": "b" } }] });
        assert_eq!(project(&data, "docs[].doc"), serde_json::json!([{ "id": "a" }, { "id": "b" }]));
        assert_eq!(project(&data, "docs"), data["docs"]);
    }
}
