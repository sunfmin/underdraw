//! Reads a whole screen description and names every fault in it.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::conditions::Conditions;
use crate::field::{Base, Field};
use crate::fixture;
use crate::kinds::Kinds;
use crate::schema::{Schema, Scope, object};
use crate::template::{self, Format};

/// What `check` makes of a screen description.
#[derive(Debug, Serialize)]
pub struct Checked {
    /// The screen's name, as `screen` gives it.
    pub screen: String,
    /// True when there is no finding.
    pub ok: bool,
    /// The fixtures, in the order written.
    pub fixtures: Vec<String>,
    /// The phases, each a tree that renders before data arrives.
    pub phases: Vec<String>,
    /// Each condition and the fixtures it holds in. Empty while any finding
    /// other than coverage stands: a table worked out from a faulty
    /// description would mislead.
    pub conditions: Vec<Condition>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Condition {
    pub name: String,
    pub label: String,
    pub holds_in: Vec<String>,
}

/// One fault: where it is, as a dotted path or a node id, and what is wrong.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Finding {
    pub at: String,
    pub problem: String,
}

impl Finding {
    /// The same finding, as it would read at `to` in place of `from`.
    pub fn renamed(&self, from: &str, to: &str) -> Finding {
        let at = match self.at.strip_prefix(from) {
            Some(rest) => format!("{to}{rest}"),
            None => self.at.clone(),
        };
        Finding { at, problem: self.problem.clone() }
    }
}

#[derive(Default)]
pub struct Findings(pub Vec<Finding>);

impl Findings {
    pub fn add(&mut self, at: &str, problem: &str) {
        self.0.push(Finding { at: at.to_string(), problem: problem.to_string() });
    }
}

/// Every fault in the screen description `screen`: the view model, the
/// fixtures, the conditions, and every node's paths, templates and cases.
/// When there is none, which fixtures each condition holds in; a condition
/// that holds in all of them, or in none, is then itself a fault. Every
/// node's kind is one `kinds` (the project's kinds.json) or underdraw
/// defines, and the project's looks are checked too.
pub fn check(screen: &Value, kinds: Option<&Value>) -> Checked {
    let mut findings = Findings::default();
    let known = Kinds::read(kinds);
    known.check(kinds, &mut findings);
    let schema = Schema::read(screen, &mut findings);
    let fixtures = fixture::read(screen, &schema, &mut findings);
    let conditions = Conditions::read(screen, &schema, &mut findings);
    let components = screen.get("components").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut walk = Walk {
        schema: &schema,
        conditions: &conditions,
        components: &components,
        kinds: &known,
        findings: &mut findings,
    };
    match screen.get("root") {
        Some(root) => walk.node(root, &schema.roots(), &BTreeSet::new(), "root", ""),
        None => walk.findings.add("root", "the screen needs a root node"),
    }
    let mut phases = Vec::new();
    for (name, tree) in object(screen, "phases") {
        if name.starts_with('$') {
            continue;
        }
        // A phase renders before data arrives: nothing in it may read data.
        walk.node(tree, &Vec::new(), &BTreeSet::new(), &format!("phases.{name}"), "");
        phases.push(name.clone());
    }

    let mut table = Vec::new();
    if findings.0.is_empty() {
        for condition in conditions.all() {
            let mut holds_in = Vec::new();
            for (name, data) in &fixtures {
                if conditions.holds(&condition.name, data) {
                    holds_in.push(name.clone());
                }
            }
            if holds_in.is_empty() || holds_in.len() == fixtures.len() {
                let always = if holds_in.is_empty() { "false" } else { "true" };
                findings.add(
                    &format!("conditions.{}", condition.name),
                    &format!("is {always} in every fixture, so a state it marks cannot be rendered; add a fixture where it is not"),
                );
            }
            table.push(Condition { name: condition.name.clone(), label: condition.label.clone(), holds_in });
        }
    }
    Checked {
        screen: screen.get("screen").and_then(Value::as_str).unwrap_or_default().to_string(),
        ok: findings.0.is_empty(),
        fixtures: fixtures.into_iter().map(|(name, _)| name).collect(),
        phases,
        conditions: table,
        findings: findings.0,
    }
}

/// Properties whose value is always text: placeholders in them follow the
/// format rules. Anything else given exactly one placeholder takes the value.
const TEXT: [&str; 5] = ["text", "title", "subtitle", "label", "detail"];
/// Keys that hold no template and no node.
const INERT: [&str; 10] = ["$comment", "$note", "$why", "assumed", "id", "kind", "ref", "only", "each", "if"];

struct Walk<'a> {
    schema: &'a Schema,
    conditions: &'a Conditions,
    components: &'a Map<String, Value>,
    kinds: &'a Kinds,
    findings: &'a mut Findings,
}

impl Walk<'_> {
    /// `value`, found under `key`, where `scope` names what paths may start
    /// from and `guarded` the paths known not to be null.
    fn node(&mut self, value: &Value, scope: &Scope, guarded: &BTreeSet<String>, at: &str, key: &str) {
        let object = match value {
            Value::Array(items) => {
                for item in items {
                    self.node(item, scope, guarded, at, key);
                }
                return;
            }
            Value::String(text) => return self.template(text, scope, guarded, at, key),
            Value::Object(object) => object,
            _ => return,
        };
        if crate::instance::is_responsive(object) && object.len() < 2 {
            self.findings
                .add(at, "a responsive value names both breakpoints: {\"compact\": …, \"regular\": …}");
        }
        let at = object.get("id").and_then(Value::as_str).unwrap_or(at);
        if let Some(name) = object.get("ref") {
            match name.as_str().and_then(|name| self.components.get(name)) {
                Some(component) => self.node(component, scope, guarded, at, key),
                None => self.findings.add(at, &format!("ref {name} is not in components")),
            }
            return;
        }
        if let Some(Value::String(kind)) = object.get("kind")
            && !self.kinds.has(kind)
        {
            self.findings.add(
                at,
                &format!("kind {kind} is not defined: define its look in kinds.json beside screen.json (docs/format/kinds.md)"),
            );
        }
        let mut guarded = guarded.clone();
        if let Some(Value::String(used)) = object.get("if") {
            guarded.extend(self.use_site(used, scope, &format!("{at}.if")));
        }
        // `item` and `index` exist only in the template `each` repeats.
        let mut item_scope = scope.clone();
        if let Some(Value::String(path)) = object.get("each") {
            if object.contains_key("children") && !object.contains_key("item") {
                self.findings.add(
                    at,
                    "each repeats \"item\" for every element and draws \"children\" once: put the template for one element in \"item\"",
                );
            }
            match self.schema.resolve(scope, path) {
                Some(resolved) if resolved.field.array => {
                    if resolved.nullable.iter().any(|p| !guarded.contains(p)) {
                        self.findings.add(
                            at,
                            &format!("{path} may be null; guard it with a condition testing != null"),
                        );
                    }
                    item_scope.push(("item".to_string(), resolved.field.element()));
                    item_scope.push(("index".to_string(), Field::of(Base::Int)));
                }
                _ => self.findings.add(at, &format!("each iterates an array, and {path} is not one")),
            }
        } else if object.contains_key("item") {
            self.findings.add(
                at,
                "\"item\" is the template each repeats, and this node has no \"each\": name the array with \"each\"",
            );
        }
        for (k, v) in object {
            if INERT.contains(&k.as_str()) {
                continue;
            }
            match (k.as_str(), v) {
                ("disabled", Value::String(used)) => {
                    self.use_site(used, scope, &format!("{at}.disabled"));
                }
                ("cases", Value::Array(branches)) => self.cases(branches, scope, &guarded, at, key),
                ("cases", _) => self.findings.add(at, "cases is a list of branches"),
                ("item", _) => self.node(v, &item_scope, &guarded, &format!("{at}.{k}"), k),
                _ => self.node(v, scope, &guarded, &format!("{at}.{k}"), k),
            }
        }
    }

    fn cases(&mut self, branches: &[Value], scope: &Scope, guarded: &BTreeSet<String>, at: &str, key: &str) {
        if !branches.last().is_some_and(|b| b.get("else").is_some()) {
            self.findings
                .add(at, "the last branch of cases must be {\"else\": …}, so every state has a value");
        }
        for branch in branches {
            let mut guarded = guarded.clone();
            if let Some(Value::String(used)) = branch.get("if") {
                guarded.extend(self.use_site(used, scope, &format!("{at}.cases")));
            }
            if let Some(value) = branch.get("then").or_else(|| branch.get("else")) {
                self.node(value, scope, &guarded, at, key);
            }
        }
    }

    /// A yes/no at `at`: `!`, then a condition's name or a Bool path. What it
    /// guarantees is not null when it holds.
    fn use_site(&mut self, used: &str, scope: &Scope, at: &str) -> BTreeSet<String> {
        let (negated, name) = match used.strip_prefix('!') {
            Some(name) => (true, name),
            None => (false, used),
        };
        if !name.split('.').all(crate::field::is_identifier) {
            self.findings.add(at, &format!("{used:?}: here goes !? and a condition's name or a Bool path; name anything more in conditions"));
            return BTreeSet::new();
        }
        if self.conditions.contains(name) {
            return if negated { BTreeSet::new() } else { self.conditions.guards(name) };
        }
        match self.schema.resolve(scope, name) {
            Some(resolved) if resolved.field.is_plain_bool() => {}
            Some(resolved) => self.findings.add(
                at,
                &format!("{name} is {}, not a Bool; name the test in conditions", resolved.field.describe()),
            ),
            None => self.findings.add(at, &format!("{name} is neither a condition nor a path")),
        }
        BTreeSet::new()
    }

    fn template(&mut self, text: &str, scope: &Scope, guarded: &BTreeSet<String>, at: &str, key: &str) {
        let placeholders = match template::placeholders(text) {
            Ok(placeholders) => placeholders,
            Err(problem) => return self.findings.add(at, &problem),
        };
        let whole =
            placeholders.len() == 1 && text.starts_with('{') && text.ends_with('}') && !TEXT.contains(&key);
        for placeholder in placeholders {
            let path = &placeholder.path;
            if path == "index" {
                continue;
            }
            let Some(resolved) = self.schema.resolve(scope, path) else {
                self.findings.add(at, &format!("{path} is not a path here"));
                continue;
            };
            let mut open = Vec::new();
            for prefix in &resolved.nullable {
                if !guarded.contains(prefix) {
                    open.push(prefix);
                }
            }
            if !open.is_empty() {
                self.findings.add(
                    at,
                    &format!("{path} may be null ({open:?}); show it under a condition that tests != null"),
                );
            }
            if whole {
                continue;
            }
            self.shown(&resolved.field, path, placeholder.format, at);
        }
    }

    /// `field`, at `path`, shown as text with `format`.
    fn shown(&mut self, field: &Field, path: &str, format: Option<Format>, at: &str) {
        let named = matches!(field.base, Base::Named(_));
        if !field.array && (named || matches!(field.base, Base::Bool | Base::Image)) {
            return self.findings.add(
                at,
                &format!("{path} is {}, which is not text; choose the words with cases", field.describe()),
            );
        }
        let kind = if field.array { "array" } else { field.base.name() };
        let fits = match format {
            None => !matches!(kind, "money" | "date" | "number" | "array"),
            Some(Format::Money) => kind == "money",
            Some(Format::Date | Format::DateShort) => kind == "date",
            Some(Format::Count) => kind == "array",
            Some(Format::Number(_)) => matches!(kind, "number" | "int"),
        };
        if fits {
            return;
        }
        let needs = match kind {
            "money" => "|money",
            "date" => "|date or |date:short",
            "number" => "|number:N or |number:N-M",
            "array" => "|count",
            _ => "no format",
        };
        self.findings.add(at, &format!("{path} is {}; it is shown with {needs}", field.describe()));
    }
}
