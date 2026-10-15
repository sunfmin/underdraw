//! Reads a whole screen description and names every fault in it.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::breakpoint::{self, Breakpoint};
use crate::conditions::{Conditions, UseSite};
use crate::field::{Base, Field};
use crate::fixture;
use crate::instance::{State, build};
use crate::jobs;
use crate::kinds::{Kinds, track_faults};
use crate::operations::{BulkOf, Operation, Operations};
use crate::overlay::Anchor;
use crate::schema::{Schema, Scope, object};
use crate::template::{self, Format};
use crate::transitions;

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

/// A condition: what it means, and the fixtures it holds in.
#[derive(Clone, Debug, Serialize)]
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
    let screen = &jobs::expand(screen, &mut findings);
    let known = Kinds::read(kinds);
    known.check(kinds, &mut findings);
    let schema = Schema::read(screen, &mut findings);
    let fixtures = fixture::read(screen, &schema, &mut findings);
    let conditions = Conditions::read(screen, &schema, &mut findings);
    let operations = Operations::read(screen, &schema, &conditions, &mut findings);
    jobs::check(&operations, &schema, &fixtures, &mut findings);
    let components = screen.get("components").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut opened = BTreeSet::new();
    collect_opened(screen, &mut opened);
    let mut walk = Walk {
        opened: &opened,
        schema: &schema,
        conditions: &conditions,
        operations: &operations,
        components: &components,
        kinds: &known,
        findings: &mut findings,
    };
    match screen.get("root") {
        Some(root) => walk.node(root, &schema.roots(), &Known::default(), "root", ""),
        None => walk.findings.add("root", "the screen needs a root node"),
    }
    let mut phases = Vec::new();
    for (name, tree) in object(screen, "phases") {
        if name.starts_with('$') {
            continue;
        }
        // A phase renders before data arrives: nothing in it may read data.
        walk.node(tree, &Vec::new(), &Known::default(), &format!("phases.{name}"), "");
        phases.push(name.clone());
    }
    let fixture_names: Vec<String> = fixtures.iter().map(|(name, _)| name.clone()).collect();
    let states: Vec<String> = fixture_names.iter().chain(&phases).cloned().collect();
    transitions::check(screen, &states, &operations, &mut findings);
    // Which nodes a state shows is known only by building it, and only a
    // description with no other fault can be built.
    if findings.0.is_empty() {
        for (index, transition) in transitions::read(screen).iter().enumerate() {
            let from = transition.from.as_str();
            let state = if fixture_names.iter().any(|name| name == from) {
                State::Fixture(from)
            } else {
                State::Phase(from)
            };
            let shown = Breakpoint::ALL.into_iter().any(|breakpoint| {
                build(screen, &conditions, &fixtures, state, breakpoint)
                    .is_ok_and(|tree| transitions::holds(&tree, &transition.node))
            });
            if !shown {
                findings.add(
                    &format!("transitions.{index}.on"),
                    &format!(
                        "{} is not shown in {from}, so a person cannot act on it there: name a state that shows it",
                        transition.node
                    ),
                );
            }
        }
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
        fixtures: fixture_names,
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

/// The actions that only move the UI and touch no data. With `set` and
/// `toggle` on view state, they are all a screen does by itself.
const UI_ONLY: [&str; 7] = ["navigate", "back", "open", "close", "scrollTo", "share", "copy"];

/// What is known where a node is reached: the paths tested `!= null`, and
/// the conditions that hold, by the `if`s around it; and whether it is
/// inside a confirmation.
#[derive(Clone, Default)]
struct Known {
    not_null: BTreeSet<String>,
    holds: BTreeSet<String>,
    /// Inside a confirmation: an overlay that dims the screen behind it
    /// (its kind's look has a backdrop) and that a node opens by its id,
    /// `{"open": "<id>"}`. Only here may a destructive operation be called.
    confirming: bool,
}

impl Known {
    fn extend(&mut self, more: Known) {
        self.not_null.extend(more.not_null);
        self.holds.extend(more.holds);
    }
}

/// Every name an `{"open": …}` action anywhere in `value` opens.
fn collect_opened(value: &Value, opened: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, inner) in object {
                match (key.as_str(), inner) {
                    ("open", Value::String(name)) => {
                        opened.insert(name.clone());
                    }
                    _ => collect_opened(inner, opened),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_opened(item, opened);
            }
        }
        _ => {}
    }
}

struct Walk<'a> {
    /// What the screen's `open` actions open, by name.
    opened: &'a BTreeSet<String>,
    schema: &'a Schema,
    conditions: &'a Conditions,
    operations: &'a Operations,
    components: &'a Map<String, Value>,
    kinds: &'a Kinds,
    findings: &'a mut Findings,
}

impl Walk<'_> {
    /// `value`, found under `key`, where `scope` names what paths may start
    /// from and `known` what the enclosing `if`s guarantee.
    fn node(&mut self, value: &Value, scope: &Scope, known: &Known, at: &str, key: &str) {
        let object = match value {
            Value::Array(items) => {
                for item in items {
                    self.node(item, scope, known, at, key);
                }
                return;
            }
            Value::String(text) => return self.template(text, scope, &known.not_null, at, key),
            Value::Object(object) => object,
            _ => return,
        };
        if breakpoint::is_responsive(object) && object.len() < Breakpoint::ALL.len() {
            self.findings
                .add(at, &format!("a responsive value names every breakpoint: {}", Breakpoint::names()));
        }
        if crate::instance::SLOTS.contains(&key) && object.contains_key("cases") {
            self.findings
                .add(at, "a slot holds nodes, not cases: write each node in the slot and give it an \"if\"");
            return;
        }
        let at = object.get("id").and_then(Value::as_str).unwrap_or(at);
        if let Some(name) = object.get("ref") {
            match name.as_str().and_then(|name| self.components.get(name)) {
                Some(component) => self.node(component, scope, known, at, key),
                None => self.findings.add(at, &format!("ref {name} is not in components")),
            }
            return;
        }
        if let Some(Value::String(kind)) = object.get("kind")
            && !self.kinds.has(kind)
        {
            self.findings.add(
                at,
                &format!("kind {kind} is not defined: define its look in kinds.json beside screen.json (`underdraw check --help`, KINDS.JSON)"),
            );
        }
        if let Some(Value::String(kind)) = object.get("kind")
            && key != "overlay"
            && self.kinds.backdrop(kind)
        {
            self.findings.add(
                at,
                &format!("kind {kind} has a backdrop, which only an overlay draws: put it in \"overlay\", or take backdrop out of its look"),
            );
        }
        let mut known = known.clone();
        if key == "overlay"
            && let Some(Value::String(kind)) = object.get("kind")
            && self.kinds.backdrop(kind)
            && object.get("id").and_then(Value::as_str).is_some_and(|id| self.opened.contains(id))
        {
            known.confirming = true;
        }
        if let Some(Value::String(used)) = object.get("if") {
            known.extend(self.use_site(used, scope, &format!("{at}.if")));
        }
        if let Some(only) = object.get("only")
            && only.as_str().and_then(Breakpoint::parse).is_none()
        {
            self.findings.add(
                &format!("{at}.only"),
                &format!("{only} is not a breakpoint: write one of {}", Breakpoint::names()),
            );
        }
        if let Some(limit) = object.get("limit")
            && !breakpoint::each(limit, at).iter().all(|(_, limit)| limit.as_u64().is_some())
        {
            self.findings
                .add(at, "limit is a whole number, or one per breakpoint: {\"compact\": 10, \"regular\": 9}");
        }
        if let Some(anchor) = object.get("anchor") {
            for (at, anchor) in breakpoint::each(anchor, &format!("{at}.anchor")) {
                Anchor::fault(anchor, &at, self.findings);
            }
        }
        for key in ["columns", "rows"] {
            if let Some(tracks) = object.get(key) {
                for (at, tracks) in breakpoint::each(tracks, &format!("{at}.{key}")) {
                    track_faults(tracks, &at, self.findings);
                }
            }
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
                    if resolved.nullable.iter().any(|p| !known.not_null.contains(p)) {
                        self.findings.add(
                            at,
                            &format!("{path} may be null; guard it with a condition testing != null"),
                        );
                    }
                    // An element of a list in view state is view state too.
                    let mut element = resolved.field.element();
                    element.view = self.schema.in_view_state(scope, path);
                    item_scope.push(("item".to_string(), element));
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
                ("cases", Value::Array(branches)) => self.cases(branches, scope, &known, at, key),
                ("cases", _) => self.findings.add(at, "cases is a list of branches"),
                ("item", _) => self.node(v, &item_scope, &known, &format!("{at}.{k}"), k),
                ("on", Value::Object(events)) => {
                    // A node disabled unless a condition holds is enabled
                    // only while it does: what it calls may require it, and
                    // may read what it guards.
                    let mut enabled = known.clone();
                    if let Some(Value::String(used)) = object.get("disabled") {
                        let UseSite { negated, name } = UseSite::parse(used);
                        if negated {
                            enabled.holds.extend(self.conditions.implied(name));
                            enabled.not_null.extend(self.conditions.guards(name));
                        }
                    }
                    for (event, handler) in events {
                        self.handler(handler, scope, &enabled, &format!("{at}.on.{event}"), at);
                    }
                }
                _ => self.node(v, scope, &known, &format!("{at}.{k}"), k),
            }
        }
    }

    fn cases(&mut self, branches: &[Value], scope: &Scope, known: &Known, at: &str, key: &str) {
        if !branches.last().is_some_and(|b| b.get("else").is_some()) {
            self.findings
                .add(at, "the last branch of cases must be {\"else\": …}, so every state has a value");
        }
        for branch in branches {
            let mut known = known.clone();
            if let Some(Value::String(used)) = branch.get("if") {
                known.extend(self.use_site(used, scope, &format!("{at}.cases")));
            }
            if let Some(value) = branch.get("then").or_else(|| branch.get("else")) {
                self.node(value, scope, &known, at, key);
            }
        }
    }

    /// One event's handler at `at`, on the node `node`: its actions, or
    /// `cases` choosing between them.
    fn handler(&mut self, handler: &Value, scope: &Scope, known: &Known, at: &str, node: &str) {
        let Some(actions) = handler.as_object() else {
            return self.node(handler, scope, known, at, "");
        };
        if let Some(branches) = actions.get("cases") {
            let Some(branches) = branches.as_array() else {
                return self.findings.add(at, "cases is a list of branches");
            };
            if !branches.last().is_some_and(|b| b.get("else").is_some()) {
                self.findings
                    .add(at, "the last branch of cases must be {\"else\": …}, so every state has a handler");
            }
            for branch in branches {
                let mut known = known.clone();
                if let Some(Value::String(used)) = branch.get("if") {
                    known.extend(self.use_site(used, scope, &format!("{at}.cases")));
                }
                if let Some(chosen) = branch.get("then").or_else(|| branch.get("else")) {
                    self.handler(chosen, scope, &known, at, node);
                }
            }
            return;
        }
        match actions.get("call") {
            Some(called) => self.call(called, actions.get("with"), scope, known, at, node),
            None => {
                self.ui_only(actions, scope, at);
                self.node(handler, scope, known, at, "")
            }
        }
    }

    /// The actions at `at` other than `call`: each is one the screen does by
    /// itself, and `set` and `toggle` write only view state. What reads or
    /// changes domain data is an operation.
    fn ui_only(&mut self, actions: &Map<String, Value>, scope: &Scope, at: &str) {
        for (name, argument) in actions {
            let at = format!("{at}.{name}");
            let written: Vec<&str> = match (name.as_str(), argument) {
                ("with" | "to", _) => continue,
                (name, _) if UI_ONLY.contains(&name) => continue,
                ("set", Value::Object(paths)) => paths.keys().map(String::as_str).collect(),
                ("set" | "toggle", Value::String(path)) => vec![path.as_str()],
                ("set", _) => {
                    self.findings.add(
                        &at,
                        "set names a path, with \"to\" for its value, or is an object of path: value",
                    );
                    continue;
                }
                ("toggle", _) => {
                    self.findings.add(&at, "toggle names a Bool path");
                    continue;
                }
                (name, _) => {
                    self.findings.add(
                        &at,
                        &format!(
                            "{name} is not an action the screen does by itself ({}, or set / toggle on view state): what reads or changes domain data is an operation, called with \"call\"",
                            UI_ONLY.join(", ")
                        ),
                    );
                    continue;
                }
            };
            for path in written {
                if self.schema.resolve(scope, path).is_none() {
                    self.findings
                        .add(&at, &format!("{name} writes a field of the view model, and {path} is not one"));
                } else if !self.schema.in_view_state(scope, path) {
                    self.findings.add(
                        &at,
                        &format!(
                            "{name} writes {path}, which is domain data: changing it is a command, called with \"call\"; or, if only the screen holds it, mark the field \"view\": true"
                        ),
                    );
                }
            }
        }
    }

    /// `{"call": <operation>, "with": {…}}` at `at`, on the node `node`.
    fn call(
        &mut self,
        called: &Value,
        with: Option<&Value>,
        scope: &Scope,
        known: &Known,
        at: &str,
        node: &str,
    ) {
        let operations = self.operations;
        let Some(operation) = called.as_str().and_then(|name| operations.get(name)) else {
            return self.findings.add(
                &format!("{at}.call"),
                &format!("{called} is not an operation: call names one in operations"),
            );
        };
        if let Some(of) = &operation.bulk_of {
            // A bulk form's precondition holds item by item, which no
            // condition of the screen can say: the screen's own gate is a
            // selection that is there.
            self.selection(operation, of, with, scope, known, node);
        } else if let Some(required) = &operation.requires
            && !known.holds.contains(required)
        {
            self.findings.add(
                node,
                &format!(
                    "calls {}, which requires {required}: gate this node by it, \"disabled\": \"!{required}\" or \"if\": \"{required}\"",
                    operation.name
                ),
            );
        }
        if operation.destructive && !known.confirming {
            self.findings.add(
                node,
                &format!(
                    "calls {}, which is destructive: call it only from inside its confirmation, an overlay whose kind has a backdrop and that a node opens with {{\"open\": \"<the overlay's id>\"}}; here, open that instead",
                    operation.name
                ),
            );
        }
        let empty = Map::new();
        let given = match with {
            None => &empty,
            Some(Value::Object(given)) => given,
            Some(_) => {
                return self
                    .findings
                    .add(&format!("{at}.with"), "with is an object: one value per parameter");
            }
        };
        for (name, param) in &operation.params {
            if !given.contains_key(name) && param.default.is_none() {
                self.findings.add(
                    &format!("{at}.with"),
                    &format!(
                        "{} needs {name} ({}), which with does not give",
                        operation.name,
                        param.describe()
                    ),
                );
            }
        }
        let next = format!("{}.next", operation.name);
        let next = self
            .schema
            .roots()
            .into_iter()
            .find(|(_, field)| field.from.as_deref() == Some(next.as_str()))
            .map(|(root, _)| root);
        if operation.pages && !given.contains_key("cursor") {
            self.findings.add(
                &format!("{at}.with"),
                &format!(
                    "{} pages: give cursor, null for the first page, or \"{{{}}}\" for the page after the last one loaded",
                    operation.name,
                    next.clone().unwrap_or_else(|| format!("<the field filled from {}.next>", operation.name))
                ),
            );
        }
        for (name, value) in given {
            let at = format!("{at}.with.{name}");
            let Some((_, param)) = operation.params.iter().find(|(param, _)| param == name) else {
                self.findings.add(&at, &format!("{} has no parameter {name}", operation.name));
                continue;
            };
            if operation.pages && name == "cursor" {
                self.cursor(value, &operation.name, next.as_deref(), scope, known, &at);
            } else {
                self.argument(value, param, scope, known, &at);
            }
        }
    }

    /// The cursor given to the paged query `query`: null for its first page,
    /// or the field of `data` its last page's `next` fills (`next`), and only
    /// where that is known not to be null.
    fn cursor(
        &mut self,
        value: &Value,
        query: &str,
        next: Option<&str>,
        scope: &Scope,
        known: &Known,
        at: &str,
    ) {
        if value.is_null() {
            return;
        }
        let Some(next) = next else {
            return self.findings.add(
                at,
                &format!(
                    "the cursor is null for the first page, or where {query}'s last page said the next starts: fill a field of data from it, \"from\": \"{query}.next\", and pass that"
                ),
            );
        };
        let whole = value.as_str().and_then(|text| {
            let placeholders = template::placeholders(text).ok()?;
            match placeholders.as_slice() {
                [placeholder]
                    if text.starts_with('{') && text.ends_with('}') && placeholder.format.is_none() =>
                {
                    Some(placeholder.path.clone())
                }
                _ => None,
            }
        });
        if whole.as_deref() != Some(next) || self.schema.resolve(scope, next).is_none() {
            return self.findings.add(
                at,
                &format!("the cursor is null for the first page, or \"{{{next}}}\": where {query}'s last page said the next starts"),
            );
        }
        if !known.not_null.contains(next) {
            self.findings.add(
                at,
                &format!(
                    "{next} is null on the last page: offer more only while it is not, under a condition that tests {next} != null"
                ),
            );
        }
    }

    /// The call, on `node`, to the bulk form `operation`: it is given the
    /// selection, a list in view state that holds at least one item, and is
    /// made only where that list is there.
    fn selection(
        &mut self,
        operation: &Operation,
        of: &BulkOf,
        with: Option<&Value>,
        scope: &Scope,
        known: &Known,
        node: &str,
    ) {
        let name = &operation.name;
        let keys = &of.keys;
        let given = with.and_then(|with| with.get(keys)).and_then(Value::as_str);
        let path = given.and_then(|text| {
            let placeholders = template::placeholders(text).ok()?;
            match placeholders.as_slice() {
                [only] if only.format.is_none() && text == format!("{{{}}}", only.path) => {
                    Some(only.path.clone())
                }
                _ => None,
            }
        });
        let Some(path) = path else {
            return self.findings.add(
                node,
                &format!("calls {name}, the bulk form of {}: give {keys} the selection, as one path: \"{keys}\": \"{{selection.{keys}}}\"", of.operation),
            );
        };
        let Some(resolved) = self.schema.resolve(scope, &path) else {
            // The argument's own finding says the path is not one.
            return;
        };
        if !self.schema.in_view_state(scope, &path) {
            self.findings.add(
                node,
                &format!("calls {name} with {path}, which is domain data: the selection is view state, what the person has picked on the screen, marked \"view\": true"),
            );
        }
        let empty = !resolved.field.lo.is_some_and(|lo| lo >= 1.0);
        let open = resolved.nullable.iter().any(|prefix| !known.not_null.contains(prefix));
        if empty || open || resolved.nullable.is_empty() {
            self.findings.add(
                node,
                &format!(
                    "calls {name} with {path}, and is not gated on a non-empty selection: hold the selection in a nullable field (\"Selection?\", null while nothing is picked) whose list holds at least one (\"{}[] 1..\"), and gate this node by a condition testing it != null",
                    resolved.field.base.name()
                ),
            );
        }
    }

    /// `value`, given to the parameter `param`: a template whose type fits,
    /// and whose range lies inside the parameter's wherever the node can
    /// call it.
    fn argument(&mut self, value: &Value, param: &Field, scope: &Scope, known: &Known, at: &str) {
        // The path the value is, when it is one whole placeholder.
        let mut path = None;
        let given = match value {
            Value::String(text) => {
                let Ok(placeholders) = template::placeholders(text) else {
                    return self.template(text, scope, &known.not_null, at, "");
                };
                let whole = placeholders.len() == 1 && text.starts_with('{') && text.ends_with('}');
                match placeholders.first() {
                    Some(placeholder) if whole && placeholder.format.is_none() => {
                        self.template(text, scope, &known.not_null, at, "");
                        if placeholder.path == "index" {
                            Some(Field { lo: Some(0.0), ..Field::of(Base::Int) })
                        } else {
                            path = Some(placeholder.path.clone());
                            self.schema.resolve(scope, &placeholder.path).map(|resolved| resolved.field)
                        }
                    }
                    None if let Some(members) = self.schema.members(param) => {
                        if !members.iter().any(|member| member == text) {
                            self.findings.add(
                                at,
                                &format!(
                                    "{text:?} is not one of {}'s choices: {}",
                                    param.base.name(),
                                    members.join(", ")
                                ),
                            );
                        }
                        return;
                    }
                    None => {
                        let length = Some(text.chars().count() as f64);
                        Some(Field { lo: length, hi: length, ..Field::of(Base::String) })
                    }
                    _ => {
                        self.template(text, scope, &known.not_null, at, "text");
                        Some(Field::of(Base::String))
                    }
                }
            }
            Value::Null => Some(Field { nullable: true, ..Field::of(param.base.clone()) }),
            Value::Bool(_) => Some(Field::of(Base::Bool)),
            Value::Number(n) if n.is_i64() || n.is_u64() => {
                Some(Field { lo: n.as_f64(), hi: n.as_f64(), ..Field::of(Base::Int) })
            }
            Value::Number(n) => Some(Field { lo: n.as_f64(), hi: n.as_f64(), ..Field::of(Base::Number) }),
            _ => {
                return self
                    .findings
                    .add(at, "an argument is a template, such as \"{item.id}\", or a literal");
            }
        };
        let Some(mut given) = given else {
            // The template's own finding says the path is not one.
            return;
        };
        let base = given.base == param.base || (given.base == Base::Int && param.base == Base::Number);
        let text_into = given.base == Base::String
            && matches!(param.base, Base::Url | Base::Image | Base::Icon | Base::Date)
            && !value.as_str().is_some_and(|text| text.starts_with('{'));
        if !(base || text_into) || given.array != param.array || (given.nullable && !param.nullable) {
            return self
                .findings
                .add(at, &format!("is {}, and the parameter is {}", given.describe(), param.describe()));
        }
        if value.is_null() || (param.lo.is_none() && param.hi.is_none()) {
            return;
        }
        if let Some(path) = &path {
            (given.lo, given.hi) = self.conditions.narrowed(&known.holds, path, &given);
        }
        // A string's length and an array's count are never below 0; a
        // number unbounded below may be any.
        let floor = if matches!(given.base, Base::Int | Base::Number) && !given.array {
            f64::NEG_INFINITY
        } else {
            0.0
        };
        let below = param.lo.is_some_and(|lo| given.lo.unwrap_or(floor) < lo);
        let above = param.hi.is_some_and(|hi| given.hi.is_none_or(|given| given > hi));
        if below || above {
            // A condition reads only from data's roots: name one in the
            // hint only for a path it can test.
            let testable =
                path.as_ref().filter(|path| self.schema.resolve(&self.schema.roots(), path).is_some());
            let gate = match (testable, below) {
                (Some(path), true) if floor == 0.0 && param.lo == Some(1.0) && !given.array => {
                    format!(", such as one that tests {path} != \"\"")
                }
                (Some(path), true) if floor < 0.0 => {
                    format!(", such as one that tests {path} >= {}", param.lo.unwrap_or_default())
                }
                (Some(path), false) if floor < 0.0 => {
                    format!(", such as one that tests {path} <= {}", param.hi.unwrap_or_default())
                }
                _ => String::new(),
            };
            self.findings.add(
                at,
                &format!(
                    "is {} {}, and the parameter is {} {}: gate this node by a condition that keeps it inside, \"disabled\": \"!<condition>\"{gate}",
                    given.describe(),
                    given.range().unwrap_or_else(|| "..".to_string()),
                    param.describe(),
                    param.range().unwrap_or_default()
                ),
            );
        }
    }

    /// A yes/no at `at`: `!`, then a condition's name or a Bool path. What it
    /// guarantees when it holds: the paths not null, and the conditions true.
    fn use_site(&mut self, used: &str, scope: &Scope, at: &str) -> Known {
        let UseSite { negated, name } = UseSite::parse(used);
        if !name.split('.').all(crate::field::is_identifier) {
            self.findings.add(at, &format!("{used:?}: here goes !? and a condition's name or a Bool path; name anything more in conditions"));
            return Known::default();
        }
        if self.conditions.contains(name) {
            return if negated {
                Known::default()
            } else {
                Known {
                    not_null: self.conditions.guards(name),
                    holds: self.conditions.implied(name),
                    confirming: false,
                }
            };
        }
        match self.schema.resolve(scope, name) {
            Some(resolved) if resolved.field.is_plain_bool() => {}
            Some(resolved) => self.findings.add(
                at,
                &format!("{name} is {}, not a Bool; name the test in conditions", resolved.field.describe()),
            ),
            None => self.findings.add(at, &format!("{name} is neither a condition nor a path")),
        }
        Known::default()
    }

    fn template(&mut self, text: &str, scope: &Scope, guarded: &BTreeSet<String>, at: &str, key: &str) {
        let placeholders = match template::placeholders(text) {
            Ok(placeholders) => placeholders,
            Err(problem) => return self.findings.add(at, &problem),
        };
        if placeholders.is_empty() && crate::icons::is_icon_key(key) && !crate::icons::known(text) {
            self.findings.add(at, &crate::icons::unknown(text));
        }
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
