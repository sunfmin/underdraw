//! What an agent needs to build one screen on one platform, resolved from
//! the description and the platform's mapping: the view model and its
//! conditions as code, the tokens by their platform names, every node as
//! the platform builds it, and what each state must show, to test against.

extern crate flux_alloc;
extern crate flux_core;

mod code;
mod nodes;

use serde::Serialize;
use serde_json::{Map, Value};
use underdraw_format::{Breakpoint, Checked, Finding, Model, Node, State, instantiate};

pub use code::Language;
pub use nodes::{Event, Hidden, Spec};

#[derive(Debug, Serialize)]
pub struct Handoff {
    pub screen: String,
    pub platform: String,
    pub language: String,
    /// True when the platform mapping covers every kind and action used.
    pub ok: bool,
    pub findings: Vec<Finding>,
    /// The platform's conventions, as its mapping states them.
    pub rules: Map<String, Value>,
    /// The generated source file: the view model's types and the conditions.
    pub code: Code,
    pub conditions: Vec<Meaning>,
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

#[derive(Debug, Serialize)]
pub struct Code {
    pub file: String,
    pub text: String,
}

/// A condition: what it means, and the fixtures it holds in.
#[derive(Debug, Serialize)]
pub struct Meaning {
    pub name: String,
    pub label: String,
    pub holds_in: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenName {
    pub token: String,
    pub name: String,
    pub value: Value,
}

/// What one state of the screen shows at one breakpoint: the nodes present,
/// and the words and values they show.
#[derive(Debug, Serialize)]
pub struct Expected {
    pub state: Value,
    pub breakpoint: Breakpoint,
    pub present: Vec<String>,
    pub shows: Map<String, Value>,
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
    for key in ["platform", "testId", "kinds", "actions"] {
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

    let components = screen.get("components").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut walk = nodes::Walk {
        platform: &platform,
        mapping,
        components: &components,
        specs: Vec::new(),
        hidden: Vec::new(),
        findings: Vec::new(),
    };
    if let Some(root) = screen.get("root") {
        walk.node(root, None, "root");
    }
    let phases = phases(screen);
    for phase in &phases {
        if let Some(tree) = screen.pointer(&format!("/phases/{phase}")) {
            walk.node(tree, None, "phase");
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
    let states = states(screen, &model, &phases, &hidden_ids);
    let mut fixtures = Map::new();
    for (name, data) in &model.fixtures {
        fixtures.insert(name.clone(), data.clone());
    }
    let mut rules = Map::new();
    for key in ["stack", "testId", "breakpoints", "elementTree"] {
        if let Some(value) = mapping.get(key) {
            rules.insert(key.to_string(), value.clone());
        }
    }
    let root = code::root_name(&model);
    Ok(Handoff {
        screen: model.screen.clone(),
        platform: platform.clone(),
        language: language_name,
        ok: findings.is_empty(),
        findings,
        rules,
        code: Code { file: language.file(&root), text: code::file(&model, language) },
        conditions: meanings(&model),
        tokens: token_names(tokens, pattern.unwrap_or("{group}.{name}"), language),
        nodes: walk.specs,
        hidden: walk.hidden,
        states,
        fixtures,
        unconfirmed,
    })
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn phases(screen: &Value) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(Value::Object(phases)) = screen.get("phases") {
        for name in phases.keys() {
            if !name.starts_with('$') {
                names.push(name.clone());
            }
        }
    }
    names
}

fn ids(hidden: &[Hidden]) -> Vec<String> {
    let mut ids = Vec::new();
    for node in hidden {
        ids.push(node.id.clone());
    }
    ids
}

fn meanings(model: &Model) -> Vec<Meaning> {
    let mut meanings = Vec::new();
    for definition in model.conditions() {
        let mut holds_in = Vec::new();
        for (name, data) in &model.fixtures {
            if model.conditions.holds(&definition.name, data) {
                holds_in.push(name.clone());
            }
        }
        meanings.push(Meaning { name: definition.name.clone(), label: definition.label.clone(), holds_in });
    }
    meanings
}

/// Each fixture, then each phase, at each breakpoint.
fn states(screen: &Value, model: &Model, phases: &[String], hidden: &[String]) -> Vec<Expected> {
    let mut wanted = Vec::new();
    for (name, _) in &model.fixtures {
        wanted.push((State::Fixture(name), object([("fixture", Value::from(name.as_str()))])));
    }
    for name in phases {
        wanted.push((State::Phase(name), object([("phase", Value::from(name.as_str()))])));
    }
    let mut expected = Vec::new();
    for (state, named) in wanted {
        for breakpoint in [Breakpoint::Compact, Breakpoint::Regular] {
            let Ok(root) = instantiate(screen, state, breakpoint) else {
                continue;
            };
            let mut present = Vec::new();
            let mut shows = Map::new();
            collect(&root, hidden, &mut present, &mut shows);
            expected.push(Expected { state: named.clone(), breakpoint, present, shows });
        }
    }
    expected
}

fn collect(node: &Node, hidden: &[String], present: &mut Vec<String>, shows: &mut Map<String, Value>) {
    if hidden.contains(&node.id) {
        return;
    }
    if !node.id.is_empty() {
        present.push(node.id.clone());
        let mut shown = Map::new();
        for key in SHOWN {
            if let Some(value) = node.props.get(key)
                && !value.is_null()
            {
                shown.insert(key.to_string(), value.clone());
            }
        }
        if !shown.is_empty() {
            shows.insert(node.id.clone(), Value::Object(shown));
        }
    }
    for child in &node.children {
        collect(child, hidden, present, shows);
    }
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

/// `textSecondary` as `text-secondary`.
fn kebab(name: &str) -> String {
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
