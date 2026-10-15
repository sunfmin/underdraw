//! What an agent needs to build one screen on one platform, resolved from
//! the description and the platform's mapping: the view model and its
//! conditions as code, the tokens by their platform names, every node as
//! the platform builds it, and what each state must show, to test against.

extern crate flux_alloc;
extern crate flux_core;

mod cli;
mod code;
mod json_schema;
mod nodes;

use serde::Serialize;
use serde_json::{Map, Value};
use underdraw_format::{Breakpoint, Checked, Condition, Finding, Model, Node, State};

pub use cli::{Arg, CommandSpec, Failure, Golden, Interface, cli};
pub use code::Language;
pub use json_schema::json_schema;
pub use nodes::{Event, Hidden, Spec};

#[derive(Debug, Serialize)]
pub struct Handoff {
    pub screen: String,
    pub platform: String,
    pub language: String,
    /// True when the platform mapping covers every component and action used.
    pub ok: bool,
    pub findings: Vec<Finding>,
    /// The platform's conventions, as its mapping states them.
    pub rules: Map<String, Value>,
    /// The generated source file: the view model's types and the conditions.
    pub code: Code,
    /// Each condition and the fixtures it holds in.
    pub conditions: Vec<Condition>,
    /// The queries and commands the screen calls, which its events name.
    pub operations: Vec<OperationSpec>,
    pub tokens: Vec<TokenName>,
    pub nodes: Vec<Spec>,
    pub hidden: Vec<Hidden>,
    pub states: Vec<Expected>,
    /// Each fixture's data, merged and defaulted, ready to decode into the
    /// generated model for previews and tests.
    pub fixtures: Map<String, Value>,
    /// What the agent guessed and the person has not confirmed.
    pub unconfirmed: Vec<Unconfirmed>,
}

/// One operation, its types written as in the description.
#[derive(Debug, Serialize)]
pub struct OperationSpec {
    pub name: String,
    /// `query` or `command`.
    pub does: String,
    /// What it does, in one line.
    pub doc: String,
    pub params: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// The condition that must hold before it is called.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    /// It cannot be undone: the screen calls it only from its confirmation.
    #[serde(skip_serializing_if = "is_false")]
    pub destructive: bool,
    /// The name of its bulk form, which does it for each of several items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bulk: Option<String>,
    /// For a bulk form: the operation it does for each item, which the
    /// precondition, the failures and the result are each item's.
    #[serde(rename = "bulkOf", skip_serializing_if = "Option::is_none")]
    pub bulk_of: Option<String>,
    /// It takes long: it answers with a job, `{"type": "ExportDocsJob",
    /// "query": "exportDocsJob"}`, which the screen shows and the query reads
    /// again until it is done or failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<Value>,
    /// For the query derived from a long command: that command.
    #[serde(rename = "jobOf", skip_serializing_if = "Option::is_none")]
    pub job_of: Option<String>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub fails: Map<String, Value>,
    /// The fields of `data` a query's result fills: `docs`, or `docs[].doc`
    /// where the field's type wraps the result; filled by one field of the
    /// result, `result.next → next`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<String>,
    /// `cursor` for a query that pages by a cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Code {
    pub file: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct TokenName {
    pub token: String,
    pub name: String,
    pub value: Value,
}

/// What one state of the screen shows at one breakpoint: the nodes present,
/// the words and values they show, and the operation each calls when acted on.
#[derive(Debug, Serialize)]
pub struct Expected {
    pub state: Value,
    pub breakpoint: Breakpoint,
    pub present: Vec<String>,
    pub shows: Map<String, Value>,
    /// For each present node that calls an operation: `[{"on", "operation",
    /// "with"}, …]`, its arguments filled for this state.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub calls: Map<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct Unconfirmed {
    pub id: String,
    pub assumed: String,
}

/// Properties whose value a test can read off the real UI.
const SHOWN: [&str; 7] = ["text", "title", "subtitle", "label", "detail", "value", "disabled"];

/// The handoff of `screen` to the platform `mapping` describes, with
/// `tokens`. A description with faults is refused with what `check` says.
pub fn handoff(screen: &Value, mapping: &Value, tokens: &Value) -> Result<Handoff, Box<Checked>> {
    let model = underdraw_format::model(screen)?;
    let mut findings = Vec::new();
    let platform = text(mapping, "platform");
    let language_name = text(mapping, "language");
    let language = match Language::named(&language_name) {
        Some(language) => language,
        None => {
            findings.push(Finding {
                at: "language".to_string(),
                problem: format!(
                    "platforms/{platform}.json says \"language\": {language_name:?}; it is one of typescript, swift, kotlin"
                ),
            });
            Language::TypeScript
        }
    };
    for key in ["platform", "testId", "components", "actions"] {
        if mapping.get(key).is_none() {
            findings.push(Finding {
                at: key.to_string(),
                problem: format!("platforms/{platform}.json needs \"{key}\""),
            });
        }
    }
    let pattern = mapping.pointer("/tokenNaming/pattern").and_then(Value::as_str);
    if pattern.is_none() {
        findings.push(Finding {
            at: "tokenNaming".to_string(),
            problem: format!(
                "platforms/{platform}.json needs \"tokenNaming\": {{\"pattern\": …}}, such as \"Theme.{{group}}.{{name}}\""
            ),
        });
    }

    let components = underdraw_format::Subtrees::read(screen);
    let mut walk = nodes::Walk {
        platform: &platform,
        mapping,
        components: &components,
        specs: Vec::new(),
        hidden: Vec::new(),
        findings: Vec::new(),
    };
    if let Some(root) = screen.get("root") {
        walk.node(root, None, "root", None);
    }
    for phase in &model.checked.phases {
        if let Some(tree) = screen.pointer(&format!("/phases/{phase}")) {
            walk.node(tree, None, "phase", None);
        }
    }
    findings.append(&mut walk.findings);

    let mut unconfirmed = Vec::new();
    for spec in &walk.specs {
        if let Some(assumed) = &spec.assumed {
            unconfirmed.push(Unconfirmed { id: spec.id.clone(), assumed: assumed.clone() });
        }
    }
    let hidden_ids = ids(&walk.hidden);
    let states = states(&model, &hidden_ids);
    let mut rules = Map::new();
    for key in ["stack", "testId", "breakpoints", "elementTree"] {
        if let Some(value) = mapping.get(key) {
            rules.insert(key.to_string(), value.clone());
        }
    }
    let root = code::root_name(&model);
    Ok(Handoff {
        screen: model.screen().to_string(),
        platform: platform.clone(),
        language: language_name,
        ok: findings.is_empty(),
        findings,
        rules,
        code: Code { file: language.file(&root), text: code::file(&model, language) },
        conditions: model.checked.conditions.clone(),
        operations: operations(&model),
        tokens: token_names(tokens, pattern.unwrap_or("{group}.{name}"), language),
        nodes: walk.specs,
        hidden: walk.hidden,
        states,
        fixtures: fixtures(&model),
        unconfirmed,
    })
}

/// Each fixture's data by name, `$patch` applied and every default filled:
/// what `handoff` and `cli` both answer under "fixtures".
fn fixtures(model: &Model) -> Map<String, Value> {
    model.fixtures.iter().map(|(name, data)| (name.clone(), data.clone())).collect()
}

fn operations(model: &Model) -> Vec<OperationSpec> {
    let mut specs = Vec::new();
    for operation in model.operations.all() {
        let mut params = Map::new();
        for (name, field) in &operation.params {
            let default = match &field.default {
                Some(default) if !default.is_null() => format!(" = {default}"),
                _ => String::new(),
            };
            params.insert(name.clone(), Value::from(format!("{}{default}", field.describe())));
        }
        let mut fails = Map::new();
        for (name, when) in &operation.fails {
            fails.insert(name.clone(), Value::from(when.as_str()));
        }
        specs.push(OperationSpec {
            name: operation.name.clone(),
            does: operation.does.name().to_string(),
            doc: operation.doc.clone(),
            params,
            result: operation.result.as_ref().map(|field| field.describe()),
            requires: operation.requires.clone(),
            destructive: operation.destructive,
            bulk: operation.bulk.clone(),
            bulk_of: operation.bulk_of.as_ref().map(|of| of.operation.clone()),
            job: match (operation.job_type(), operation.job_query()) {
                (Some(job), Some(query)) => {
                    Some(object([("type", Value::from(job)), ("query", Value::from(query))]))
                }
                _ => None,
            },
            job_of: operation.job_of.clone(),
            fails,
            fills: operation
                .fills
                .iter()
                .map(|fill| match &fill.part {
                    None => fill.path.clone(),
                    Some(part) => format!("result.{part} → {}", fill.path),
                })
                .collect(),
            pages: operation.pages.then(|| "cursor".to_string()),
        });
    }
    specs
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn ids(hidden: &[Hidden]) -> Vec<String> {
    let mut ids = Vec::new();
    for node in hidden {
        ids.push(node.id.clone());
    }
    ids
}

/// Each fixture, then each phase, at each breakpoint.
pub(crate) fn states(model: &Model, hidden: &[String]) -> Vec<Expected> {
    let mut expected = Vec::new();
    for state in model.states() {
        let named = match state {
            State::Fixture(name) => object([("fixture", Value::from(name))]),
            State::Phase(name) => object([("phase", Value::from(name))]),
        };
        for breakpoint in Breakpoint::ALL {
            let Ok(root) = model.instantiate(state, breakpoint) else {
                continue;
            };
            let mut found = Expected {
                state: named.clone(),
                breakpoint,
                present: Vec::new(),
                shows: Map::new(),
                calls: Map::new(),
            };
            collect(&root, hidden, &mut found);
            expected.push(found);
        }
    }
    expected
}

fn collect(node: &Node, hidden: &[String], into: &mut Expected) {
    if hidden.contains(&node.id) {
        return;
    }
    if !node.id.is_empty() {
        into.present.push(node.id.clone());
        let mut shown = Map::new();
        for key in SHOWN {
            if let Some(value) = node.props.get(key)
                && !value.is_null()
            {
                shown.insert(key.to_string(), value.clone());
            }
        }
        if !shown.is_empty() {
            into.shows.insert(node.id.clone(), Value::Object(shown));
        }
        let calls = calls(node);
        if !calls.is_empty() {
            into.calls.insert(node.id.clone(), Value::Array(calls));
        }
    }
    for child in &node.children {
        collect(child, hidden, into);
    }
}

/// Each operation `node` calls, by the event that calls it, given what.
fn calls(node: &Node) -> Vec<Value> {
    node.calls()
        .into_iter()
        .map(|call| {
            object([
                ("on", Value::from(call.on)),
                ("operation", Value::from(call.operation)),
                ("with", call.with.cloned().unwrap_or_else(|| Value::Object(Map::new()))),
            ])
        })
        .collect()
}

/// Every design token by its platform name, `pattern` filled with its
/// `{group}` and `{name}` (or `{kebab-name}`). In Swift and Kotlin a name
/// that starts with a digit takes `_` before it, to be an identifier.
fn token_names(tokens: &Value, pattern: &str, language: Language) -> Vec<TokenName> {
    let mut names = Vec::new();
    let Some(groups) = tokens.as_object() else {
        return names;
    };
    for (group, members) in groups {
        let Some(members) = members.as_object() else {
            continue;
        };
        if group.starts_with('$') || group == "breakpoint" {
            continue;
        }
        for (name, value) in members {
            let starts_with_digit = name.starts_with(|c: char| c.is_ascii_digit());
            let identifier = if starts_with_digit && language != Language::TypeScript {
                format!("_{name}")
            } else {
                name.clone()
            };
            let platform_name = pattern
                .replace("{group}", group)
                .replace("{kebab-name}", &kebab(name))
                .replace("{name}", &identifier);
            names.push(TokenName {
                token: format!("{group}.{name}"),
                name: platform_name,
                value: value.clone(),
            });
        }
    }
    names
}

/// For serde: leave out a flag that is false.
pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

/// `textSecondary` as `text-secondary`.
pub(crate) fn kebab(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A JSON object of `pairs`. (`serde_json::json!` unwraps a conversion that
/// cannot fail, which Flux cannot see.)
pub(crate) fn object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert(key.to_string(), value);
    }
    Value::Object(map)
}
