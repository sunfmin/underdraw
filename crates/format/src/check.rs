//! Reads a whole screen description and names every fault in it.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::breakpoint::{self, Breakpoint};
use crate::components::{Components, Declared, track_faults};
use crate::conditions::{Conditions, Term, UseSite};
use crate::expand::{Frame, Instances, Met};
use crate::field::{Base, Field};
use crate::instance::build;
use crate::jobs;
use crate::operations::{BulkOf, Operation, Operations};
use crate::overlay::Anchor;
use crate::params::ScreenParams;
use crate::schema::{Schema, Scope};
use crate::screen::Screen;
use crate::subtrees::{self, Subtree, Subtrees};
use crate::template::{self, Format};
use crate::transitions;

/// What `check` makes of a screen description.
#[derive(Clone, Debug, Serialize)]
pub struct Checked {
    /// The screen's name, as `screen` gives it.
    pub screen: String,
    /// True when there is no finding.
    pub ok: bool,
    /// The fixtures, in the order written.
    pub fixtures: Vec<String>,
    /// The phases, each a tree that renders before data arrives.
    pub phases: Vec<String>,
    /// Each condition it reads and the fixtures it holds in: one over its
    /// data, and one on a type it reads through a value of it, or that one
    /// it reads uses. Empty while any finding
    /// other than coverage stands: a table worked out from a faulty
    /// description would mislead.
    pub conditions: Vec<Condition>,
    pub findings: Vec<Finding>,
}

impl Checked {
    /// `finding` added to what `check` made of the screen, by a later look
    /// at it (its layout, the files beside it). Any fault makes the screen
    /// not ok and takes the condition table away.
    pub fn fault(&mut self, finding: Finding) {
        self.ok = false;
        self.conditions.clear();
        self.findings.push(finding);
    }
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

/// The faults found so far, in the order found, each once: a fault found
/// again at the same place, as a component's is at each instance of it
/// that shares the place, is the one already standing.
#[derive(Default)]
pub struct Findings(pub Vec<Finding>);

impl Findings {
    pub fn add(&mut self, at: &str, problem: &str) {
        self.push(Finding { at: at.to_string(), problem: problem.to_string() });
    }

    pub fn push(&mut self, finding: Finding) {
        if !self.0.contains(&finding) {
            self.0.push(finding);
        }
    }
}

/// Every fault in the screen description `screen`: what reading it found
/// (the view model, the fixtures, the conditions, the components), and
/// every node's paths, templates and cases. When there is none, which
/// fixtures each condition holds in; a condition that holds in all of
/// them, or in none, is then itself a fault. Every node's component is one
/// the screen's own `"components"`, the project's or underdraw defines.
/// Where it navigates is not: one description alone cannot see the others.
pub fn check(screen: &Screen) -> Checked {
    check_among(screen, &ScreenParams::new())
}

/// What `check` makes of `screen`, among the screens under its root whose
/// params `others` holds: each `navigate` to one of them gives each of its
/// params, of its type, in `with`, and nothing else. A navigate to a screen
/// not in `others` is held to nothing.
pub fn check_among(screen: &Screen, others: &ScreenParams) -> Checked {
    let mut findings = Findings::default();
    for finding in screen.findings() {
        findings.push(finding.clone());
    }
    let Screen { components: known, schema, fixtures, conditions, operations, transitions, .. } = screen;
    let description = screen.description();
    jobs::check(operations, schema, fixtures, &mut findings);
    subtrees::check(known.declared(), schema, &mut findings);
    let components = known.subtrees();
    for Declared { name, body, at, .. } in known.declared() {
        if body.get("nodes").is_some() && known.has(name) {
            findings.add(
                at,
                &format!(
                    "{name} is underdraw's own component: give the screen's whose body is nodes its own name"
                ),
            );
        }
    }
    let mut opened = BTreeSet::new();
    collect_opened(description, &mut opened);
    let mut walk = Walk {
        opened: &opened,
        others,
        schema,
        conditions,
        operations,
        components,
        known,
        instances: Instances::new(components),
        findings: &mut findings,
        read: conditions.read_through(schema),
    };
    match description.get("root") {
        Some(root) => walk.node(root, &schema.roots(), &Known::default(), "root", ""),
        None => walk.findings.add("root", "the screen needs a root node"),
    }
    let phases = screen.phases();
    for name in &phases {
        if let Some(tree) = description.get("phases").and_then(|phases| phases.get(name)) {
            // A phase renders before data arrives: nothing in it may read data.
            walk.node(tree, &Vec::new(), &Known::default(), &format!("phases.{name}"), "");
        }
    }
    let read = walk.read;
    let fixture_names = screen.fixture_names();
    let states: Vec<String> = fixture_names.iter().chain(&phases).cloned().collect();
    transitions::check(description, components, &states, operations, &mut findings);
    // Which nodes a state shows is known only by building it, and only a
    // description with no other fault can be built.
    if findings.0.is_empty() {
        for (index, transition) in transitions.iter().enumerate() {
            let from = transition.from.as_str();
            let state = screen.state(from);
            let shown = Breakpoint::ALL.into_iter().any(|breakpoint| {
                build(screen, state, breakpoint, None)
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
    // Each node a state shows is told apart by its id: a test finds it, a
    // transition acts on it and a note is pinned to it by that id alone.
    if findings.0.is_empty() {
        let mut named = BTreeSet::new();
        for state in &states {
            for breakpoint in Breakpoint::ALL {
                if let Ok(tree) = build(screen, screen.state(state), breakpoint, None)
                    && let Some(twice) = twice(&tree, &mut BTreeSet::new())
                    && named.insert(twice.written_id.clone())
                {
                    findings.add(&twice.written_id, &format!(
                        "names more than one node in {state}, {} among them: give each its own id; inside a repeat inside a repeat, name the outer one's index (\"index\": \"row\" beside its \"each\") and write both in the id, \"cell-{{row}}-{{index}}\"",
                        twice.id
                    ));
                }
            }
        }
    }
    let mut table = Vec::new();
    if findings.0.is_empty() {
        // A condition on a type the description never reads, as the
        // domain's are to every screen that does not, has no row.
        let reached = conditions.reached(&read);
        for condition in conditions.all() {
            let mut holds_in = Vec::new();
            if let Some(on) = &condition.on {
                if !reached.contains(&condition.name) {
                    continue;
                }
                // One on a type holds of each value: where the screen reads
                // it, it must be true of one and false of another.
                let (mut of_some, mut fails_of_some) = (false, false);
                for (name, data) in fixtures {
                    let (holds, fails) = conditions.of_values(schema, &condition.name, data);
                    if holds {
                        holds_in.push(name.clone());
                    }
                    of_some = of_some || holds;
                    fails_of_some = fails_of_some || fails;
                }
                if of_some != fails_of_some && read.contains(&condition.name) {
                    let (always, add) = if of_some { ("true", "is not") } else { ("false", "is") };
                    findings.add(
                        &format!("conditions.{}", condition.name),
                        &format!("is {always} of every {on} in the fixtures, so a state it marks cannot be rendered; add one it {add}"),
                    );
                }
                table.push(Condition {
                    name: condition.name.clone(),
                    label: condition.label.clone(),
                    holds_in,
                });
                continue;
            }
            for (name, data) in fixtures {
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
        screen: screen.name().to_string(),
        ok: findings.0.is_empty(),
        fixtures: fixture_names,
        phases,
        conditions: table,
        findings: findings.0,
    }
}

/// The first node in `node`, in document order, whose id one before it in
/// `seen` already has.
fn twice<'n>(
    node: &'n crate::instance::Node,
    seen: &mut BTreeSet<&'n str>,
) -> Option<&'n crate::instance::Node> {
    if !seen.insert(&node.id) {
        return Some(node);
    }
    node.children.iter().find_map(|child| twice(child, seen))
}

/// Properties whose value is always text: placeholders in them follow the
/// format rules. Anything else given exactly one placeholder takes the value.
const TEXT: [&str; 5] = ["text", "title", "subtitle", "label", "detail"];
/// Keys that hold no template and no node.
const INERT: [&str; 12] =
    ["$comment", "$note", "$why", "assumed", "id", "component", "ref", "with", "only", "each", "index", "if"];

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
    /// (its component's look has a backdrop) and that a node opens by its id,
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
    /// The params of the other screens under the root, by name.
    others: &'a ScreenParams,
    schema: &'a Schema,
    conditions: &'a Conditions,
    operations: &'a Operations,
    components: &'a Subtrees,
    /// Every component a node may be: underdraw's and the screen's look components.
    known: &'a Components,
    /// The instances whose nodes are being walked, outermost first.
    instances: Instances<'a, Around>,
    findings: &'a mut Findings,
    /// The conditions on a type the screen reads through a value of it
    /// (`stop.here`): only those must vary in its fixtures.
    read: BTreeSet<String>,
}

/// What the walk keeps where an instance is.
struct Around {
    /// The params given a condition of the screen's (`"{hasDates}"`), each
    /// with that condition: where one holds, so does the other.
    conditions: BTreeMap<String, String>,
    /// The scope and knowledge where the instance is: what it gives in a
    /// slot is walked there.
    scope: Scope,
    known: Known,
}

/// Each param of the instance `frame` given a path whole, not a condition,
/// with that path (`order` and `item`).
fn paths(frame: &Frame<Around>) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    for (param, path) in &frame.paths {
        if !frame.at.conditions.contains_key(param) {
            paths.push((param.clone(), path.clone()));
        }
    }
    paths
}

impl<'a> Walk<'a> {
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
        let at = match object.get("id").and_then(Value::as_str) {
            Some(id) => self.instances.ids(id, id).0,
            None => at.to_string(),
        };
        let at = at.as_str();
        if object.contains_key("ref") {
            return self.findings.add(
                &format!("{at}.ref"),
                "ref is gone: declare the component in components as {\"params\": {…}, \"nodes\": {…}} and use it by name, {\"id\": …, \"component\": \"<Name>\", \"with\": {…}}",
            );
        }
        match self.instances.met(object) {
            Met::Slot(slot) => return self.slotted(&slot, at),
            Met::Instance(component) => return self.instance(component, object, scope, known, at, key),
            Met::Node => {}
        }
        if object.contains_key("with") && object.contains_key("component") {
            self.findings.add(
                &format!("{at}.with"),
                "with gives the params of a component the screen declares in components, and this node is not one",
            );
        }
        if object.contains_key("kind") {
            self.findings.add(
                &format!("{at}.kind"),
                "a node names the component it is: write \"component\" in place of \"kind\"",
            );
        }
        if let Some(Value::String(kind)) = object.get("component")
            && !self.known.has(kind)
            && !self.components.has(kind)
        {
            self.findings.add(
                at,
                &format!("component {kind} is not defined: define it in the screen's \"components\", or share it in the project's components/{kind}.json, with its params and its look or nodes (`underdraw check --help`, COMPONENTS)"),
            );
        }
        match object.get("assumed") {
            Some(Value::String(assumed)) => {
                if let Some(problem) = crate::assumption::too_short(assumed) {
                    self.findings.add(&format!("{at}.assumed"), problem);
                }
            }
            Some(_) => self.findings.add(
                &format!("{at}.assumed"),
                "is text: say in a sentence what was guessed and what else it could be (`underdraw check --help`, \"assumed\")",
            ),
            None => {}
        }
        if let Some(Value::String(kind)) = object.get("component")
            && key != "overlay"
            && self.known.backdrop(kind)
        {
            self.findings.add(
                at,
                &format!("component {kind} has a backdrop, which only an overlay draws: put it in \"overlay\", or take backdrop out of its look"),
            );
        }
        if let Some(Value::String(kind)) = object.get("component")
            && self.known.look(kind).is_some_and(|look| !look.places_children())
        {
            for (slot, held) in object {
                if !crate::instance::SLOTS.contains(&slot.as_str()) {
                    continue;
                }
                let held = match held {
                    Value::Array(nodes) => nodes.iter().collect(),
                    node => vec![node],
                };
                if held.is_empty() {
                    continue;
                }
                let ids: Vec<&str> = held.iter().filter_map(|node| node.get("id")?.as_str()).collect();
                let what = if ids.is_empty() { "what it holds".to_string() } else { ids.join(", ") };
                self.findings.add(
                    &format!("{at}.{slot}"),
                    &format!("{kind}'s look places no \"$children\", so {what} in {slot} is never drawn: move it to a container, or define a component whose look places \"$children\""),
                );
            }
        }
        let mut known = known.clone();
        if key == "overlay"
            && let Some(Value::String(kind)) = object.get("component")
            && self.known.backdrop(kind)
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
                    // The index named, so a repeat inside can still reach it.
                    match object.get("index") {
                        None => {}
                        Some(Value::String(name))
                            if crate::field::is_identifier(name)
                                && !crate::look::REPEATED.contains(&name.as_str())
                                && !scope.iter().any(|(taken, _)| taken == name) =>
                        {
                            let index = Field { lo: Some(0.0), ..Field::of(Base::Int) };
                            item_scope.push((name.clone(), index));
                        }
                        Some(name) => self.findings.add(
                            &format!("{at}.index"),
                            &format!("{name} cannot name this repeat's index: give a word no data root, enclosing repeat or item, index and n already take, such as \"row\""),
                        ),
                    }
                }
                _ => self.findings.add(at, &format!("each iterates an array, and {path} is not one")),
            }
        } else if object.contains_key("item") {
            self.findings.add(
                at,
                "\"item\" is the template each repeats, and this node has no \"each\": name the array with \"each\"",
            );
        }
        if object.contains_key("index") && !object.contains_key("each") {
            self.findings.add(
                &format!("{at}.index"),
                "\"index\" names the index of the repeat \"each\" makes, and this node has no \"each\"",
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
                    if let Some(Value::String(used)) = object.get("disabled")
                        && let Some(name) = UseSite::parse(used).only_negated()
                    {
                        enabled.extend(self.held(name, scope));
                    }
                    for (event, handler) in events {
                        self.handler(handler, scope, &enabled, &format!("{at}.on.{event}"), at);
                    }
                }
                _ => self.node(v, scope, &known, &format!("{at}.{k}"), k),
            }
        }
    }

    /// The instance `object`, at `at`, of one of the screen's components:
    /// its `with` gives each param, of its type, and nothing else, and its
    /// component's nodes read only those params.
    fn instance(
        &mut self,
        component: &'a Subtree,
        object: &Map<String, Value>,
        scope: &Scope,
        known: &Known,
        at: &str,
        key: &str,
    ) {
        let name = component.name.to_string();
        let params = component.params.clone();
        let root = Value::Object(component.nodes.clone());
        for reached in subtrees::reached_into(object) {
            self.findings.add(
                &format!("{at}.{reached}"),
                &format!("a screen cannot reach inside an instance of {name}: what it says of {name} is a param, given in \"with\""),
            );
        }
        if let Some(only) = object.get("only")
            && only.as_str().and_then(Breakpoint::parse).is_none()
        {
            self.findings.add(
                &format!("{at}.only"),
                &format!("{only} is not a breakpoint: write one of {}", Breakpoint::names()),
            );
        }
        let mut known = known.clone();
        if let Some(Value::String(used)) = object.get("if") {
            known.extend(self.use_site(used, scope, &format!("{at}.if")));
        }
        if let Some(fault) = self.instances.uses_itself(component) {
            return self.findings.add(at, &fault);
        }
        let empty = Map::new();
        let given = match object.get("with") {
            None => &empty,
            Some(Value::Object(given)) => given,
            Some(_) => {
                return self.findings.add(&format!("{at}.with"), "with is an object: one value per param");
            }
        };
        let mut inner: Scope = Vec::new();
        let mut conditions = BTreeMap::new();
        for (param, field) in &params {
            let mut field = field.clone();
            match given.get(param) {
                None if field.default.is_none() => self.findings.add(
                    &format!("{at}.with"),
                    &format!("{name} reads {param} ({}), which with does not give", field.describe()),
                ),
                None => {}
                Some(value) => {
                    let path = value.as_str().and_then(template::whole);
                    match path.as_deref().and_then(|path| self.condition(path)) {
                        Some(condition) => {
                            if !field.is_plain_bool() {
                                self.findings.add(
                                    &format!("{at}.with.{param}"),
                                    &format!(
                                        "is the condition {condition}, a Bool, and the param is {}",
                                        field.describe()
                                    ),
                                );
                            }
                            conditions.insert(param.clone(), condition);
                        }
                        None => {
                            self.argument(value, &field, scope, &known, &format!("{at}.with.{param}"));
                            if let Some(path) = path {
                                field.view = self.schema.in_view_state(scope, &path);
                            }
                        }
                    }
                }
            }
            inner.push((param.clone(), field));
        }
        for param in given.keys() {
            if !params.iter().any(|(declared, _)| declared == param) {
                let declared: Vec<&str> = params.iter().map(|(declared, _)| declared.as_str()).collect();
                let declared = if declared.is_empty() { "none".to_string() } else { declared.join(", ") };
                self.findings.add(
                    &format!("{at}.with.{param}"),
                    &format!("{name} declares no param {param}: its params are {declared}"),
                );
            }
        }
        let mut placed = BTreeSet::new();
        slots_placed(&root, &mut placed);
        for (slot, _) in object {
            if crate::instance::SLOTS.contains(&slot.as_str()) && !placed.contains(slot) {
                self.findings.add(
                    &format!("{at}.{slot}"),
                    &format!("{name} places no {slot}: its nodes hold {{\"slot\": \"{slot}\"}} where what an instance gives goes"),
                );
            }
        }
        let around = Around { conditions, scope: scope.clone(), known: known.clone() };
        if let Err(fault) = self.instances.enter(component, object, at.to_string(), at.to_string(), around) {
            return self.findings.add(at, &fault);
        }
        // What is known not null where the instance is, is known of the
        // params given it.
        let paths = self.instances.innermost().map(paths).unwrap_or_default();
        let inside = Known {
            not_null: through(&known.not_null, &paths),
            confirming: known.confirming,
            ..Known::default()
        };
        self.node(&root, &inner, &inside, at, key);
        self.instances.leave();
    }

    /// The `{"slot": …}` node at `at`, for the slot `slot`: inside a
    /// component, what the instance gives there, walked where the instance is.
    fn slotted(&mut self, slot: &str, at: &str) {
        let Some(frame) = self.instances.leave() else {
            return self.findings.add(
                at,
                "{\"slot\": …} stands for what an instance gives, and only a component's nodes hold one",
            );
        };
        if let Some(given) = frame.given(slot) {
            self.node(given, &frame.at.scope, &frame.at.known, &format!("{}.{slot}", frame.id), slot);
        }
        self.instances.resume(frame);
    }

    /// The condition of the screen's the path `path` names, read here: one
    /// of the screen's own outside any component, or, inside one, a param
    /// given one.
    fn condition(&self, path: &str) -> Option<String> {
        match self.instances.innermost() {
            Some(frame) => frame.at.conditions.get(path).cloned(),
            None if self.conditions.contains(path) && !self.conditions.is_on_type(path) => {
                Some(path.to_string())
            }
            None => None,
        }
    }

    fn cases(&mut self, branches: &[Value], scope: &Scope, known: &Known, at: &str, key: &str) {
        if !branches.last().is_some_and(|b| b.get("else").is_some()) {
            self.findings
                .add(at, "the last branch of cases must be {\"else\": …}, so every state has a value");
        }
        for (index, branch) in branches.iter().enumerate() {
            let mut known = known.clone();
            known.extend(self.branch_if(branch, scope, at, index));
            if let Some(value) = branch.get("then").or_else(|| branch.get("else")) {
                self.node(value, scope, &known, at, key);
            }
        }
    }

    /// What the `"if"` of `branch`, the `index`th of the cases at `at`,
    /// makes known where it holds; one that is not a string is no
    /// condition, and is found at that branch.
    fn branch_if(&mut self, branch: &Value, scope: &Scope, at: &str, index: usize) -> Known {
        match branch.get("if") {
            Some(Value::String(used)) => self.use_site(used, scope, &format!("{at}.cases")),
            Some(other) => {
                let what = crate::conditions::what_if_is(other);
                self.findings.add(
                    &format!("{at}.cases[{index}].if"),
                    &format!("{what} is not a condition: {}", crate::conditions::IF_TAKES),
                );
                Known::default()
            }
            None => Known::default(),
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
            for (index, branch) in branches.iter().enumerate() {
                let mut known = known.clone();
                known.extend(self.branch_if(branch, scope, at, index));
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
                let others = self.others;
                let opened =
                    actions.get("navigate").and_then(Value::as_str).and_then(|to| others.get_key_value(to));
                match opened {
                    Some((to, params)) => {
                        self.navigate(to, params, actions.get("with"), scope, known, at);
                        let mut rest = actions.clone();
                        rest.remove("with");
                        self.node(&Value::Object(rest), scope, known, at, "")
                    }
                    None => self.node(handler, scope, known, at, ""),
                }
            }
        }
    }

    /// `{"navigate": <to>, "with": {…}}` at `at`, to a screen opened with
    /// `params`: it gives each, of its type, and nothing else.
    fn navigate(
        &mut self,
        to: &str,
        params: &[(String, Field)],
        with: Option<&Value>,
        scope: &Scope,
        known: &Known,
        at: &str,
    ) {
        let empty = Map::new();
        let given = match with {
            None => &empty,
            Some(Value::Object(given)) => given,
            Some(_) => {
                return self.findings.add(&format!("{at}.with"), "with is an object: one value per param");
            }
        };
        for (name, param) in params {
            if !given.contains_key(name) {
                self.findings.add(
                    &format!("{at}.with"),
                    &format!("{to} is opened with {name} ({}), which with does not give", param.describe()),
                );
            }
        }
        for (name, value) in given {
            let at = format!("{at}.with.{name}");
            match params.iter().find(|(param, _)| param == name) {
                Some((_, param)) => self.argument(value, param, scope, known, &at),
                None => {
                    let declared: Vec<&str> = params.iter().map(|(param, _)| param.as_str()).collect();
                    let declared = match declared.as_slice() {
                        [] => "none".to_string(),
                        _ => declared.join(", "),
                    };
                    self.findings.add(
                        &at,
                        &format!(
                            "{to} declares no param {name}: it is opened with {declared}, its \"params\""
                        ),
                    );
                }
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
                    // Inside a component, the path is a param's: the screen
                    // passed what it writes, and holds the way out.
                    if let Some(frame) = self.instances.innermost() {
                        let param = path.split('.').next().unwrap_or(path);
                        let given = match paths(frame).into_iter().find(|(name, _)| name == param) {
                            Some((_, given)) => format!("is given {{{given}}}, which is domain data"),
                            None => "is given no field of the screen's".to_string(),
                        };
                        self.findings.add(
                            &at,
                            &format!(
                                "{name} writes {path}, and {param} {given}: a component's set and toggle act on what was passed, so pass {param} a field of the screen's data marked \"view\": true; or change it with a command, called with \"call\""
                            ),
                        );
                        continue;
                    }
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
            // A condition on a type is gated through the value the call is
            // about: the one of that type it passes a field of.
            let gate = match required.split_once('.') {
                Some((type_name, short)) => {
                    let value =
                        self.value_of(type_name, with, scope).unwrap_or_else(|| "<value>".to_string());
                    format!("{value}.{short}")
                }
                None => required.clone(),
            };
            let through =
                if gate == *required { String::new() } else { ", through the value it acts on".to_string() };
            self.findings.add(
                node,
                &format!(
                    "calls {}, which requires {required}: gate this node by it{through}, \"disabled\": \"!{gate}\" or \"if\": \"{gate}\"",
                    operation.name
                ),
            );
        }
        if operation.destructive && !known.confirming {
            self.findings.add(
                node,
                &format!(
                    "calls {}, which is destructive: call it only from inside its confirmation, an overlay whose component has a backdrop and that a node opens with {{\"open\": \"<the overlay's id>\"}}; here, open that instead",
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
                            // Not null where a condition gating the node tests it.
                            self.schema.resolve(scope, &placeholder.path).map(|resolved| Field {
                                nullable: resolved.field.nullable
                                    && !known.not_null.contains(&placeholder.path),
                                ..resolved.field
                            })
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

    /// The value of the type `type_name` a call's `with` passes a field of
    /// (`item` for `{item.id}`), if one does.
    fn value_of(&self, type_name: &str, with: Option<&Value>, scope: &Scope) -> Option<String> {
        let Some(Value::Object(given)) = with else {
            return None;
        };
        for text in given.values() {
            let Some(text) = text.as_str() else {
                continue;
            };
            let Ok(placeholders) = template::placeholders(text) else {
                continue;
            };
            for placeholder in placeholders {
                let mut value = placeholder.path.as_str();
                while let Some((prefix, _)) = value.rsplit_once('.') {
                    value = prefix;
                    if let Some(resolved) = self.schema.resolve(scope, value)
                        && !resolved.field.array
                        && resolved.field.base.name() == type_name
                    {
                        return Some(value.to_string());
                    }
                }
            }
        }
        None
    }

    /// A yes/no at `at`: terms joined by `&&`, each `!`, then a condition's
    /// name or a Bool path. What it guarantees when it holds: what each term
    /// does, the paths not null and the conditions true.
    fn use_site(&mut self, used: &str, scope: &Scope, at: &str) -> Known {
        let UseSite { terms } = UseSite::parse(used);
        if !terms.iter().all(|term| term.name.split('.').all(crate::field::is_identifier)) {
            self.findings.add(
                at,
                &format!(
                    "{used:?}: here go conditions' names or Bool paths, each with an optional !, joined by &&; name ||, comparisons and anything more in conditions"
                ),
            );
            return Known::default();
        }
        let mut known = Known::default();
        for Term { negated, name } in terms {
            known.extend(self.use_term(negated, name, scope, at));
        }
        known
    }

    /// One term of a use site at `at`: what it guarantees when it holds.
    fn use_term(&mut self, negated: bool, name: &str, scope: &Scope, at: &str) -> Known {
        if self.conditions.is_on_type(name) {
            let problem = self.schema.misused(scope, name).unwrap_or_default();
            self.findings.add(at, &problem);
            return Known::default();
        }
        // Inside a component, a use site reads only its params: a Bool one,
        // or one given a condition of the screen's.
        let resolved = self.resolve(scope, name);
        if let Some(frame) = self.instances.innermost() {
            let problem = if frame.at.conditions.contains_key(name) {
                None
            } else if let Some(resolved) = resolved {
                (!resolved.field.is_plain_bool())
                    .then(|| format!("{name} is {}, not a Bool", resolved.field.describe()))
            } else if self.conditions.contains(name) {
                Some(format!(
                    "{name} is a condition of the screen's, past the params of {}: declare a bool param and give it \"{{{name}}}\" in the instance's \"with\"",
                    frame.name
                ))
            } else {
                Some(self.schema.misused(scope, name).unwrap_or_else(|| {
                    format!(
                        "{name} reaches past the params of {}: a component reads only the params it declares",
                        frame.name
                    )
                }))
            };
            if let Some(problem) = problem {
                self.findings.add(at, &problem);
            }
            return if negated { Known::default() } else { self.held(name, scope) };
        }
        if !self.conditions.contains(name) {
            match resolved {
                Some(resolved) if resolved.field.is_plain_bool() => {}
                Some(resolved) => self.findings.add(
                    at,
                    &format!(
                        "{name} is {}, not a Bool; name the test in conditions",
                        resolved.field.describe()
                    ),
                ),
                None => match self.schema.misused(scope, name) {
                    Some(problem) => self.findings.add(at, &problem),
                    None => self.findings.add(at, &format!("{name} is neither a condition nor a path")),
                },
            }
        }
        if negated { Known::default() } else { self.held(name, scope) }
    }

    /// What `path` refers to, from `scope`; a condition on a type it ends
    /// in is one the screen reads.
    fn resolve(&mut self, scope: &Scope, path: &str) -> Option<crate::schema::Resolved> {
        if let Some(condition) = self.schema.type_condition(scope, path) {
            self.read.insert(condition);
        }
        self.schema.resolve(scope, path)
    }

    /// What holds wherever the use site `name` (a condition, or a Bool
    /// path) does: the paths not null, and the conditions true. A path
    /// that ends in a condition on a type holds that condition, and the
    /// paths it guards, read through the value (`item.note`).
    fn held(&self, name: &str, scope: &Scope) -> Known {
        // Inside a component, a param given a condition holds it; no other
        // condition of the screen's is read there.
        let inside = self.instances.innermost();
        if let Some(frame) = inside
            && let Some(condition) = frame.at.conditions.get(name)
        {
            return Known {
                not_null: through(&self.conditions.guards(condition), &paths(frame)),
                holds: self.conditions.implied(condition),
                confirming: false,
            };
        }
        if inside.is_none() && self.conditions.contains(name) {
            return Known {
                not_null: self.conditions.guards(name),
                holds: self.conditions.implied(name),
                confirming: false,
            };
        }
        match (self.schema.type_condition(scope, name), name.rsplit_once('.')) {
            (Some(condition), Some((value, _))) => Known {
                not_null: self.conditions.guards_through(&condition, value),
                holds: self.conditions.implied(&condition),
                confirming: false,
            },
            _ => Known::default(),
        }
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
            let Some(resolved) = self.resolve(scope, path) else {
                let problem = match self.instances.innermost() {
                    Some(frame) => format!(
                        "{path} reaches past the params of {}: a component reads only the params it declares; declare one and give it in the instance's \"with\"",
                        frame.name
                    ),
                    None => format!("{path} is not a path here"),
                };
                self.findings.add(at, &problem);
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

/// Every slot a `{"slot": …}` node in the nodes `value` stands for.
fn slots_placed(value: &Value, placed: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if subtrees::is_slot(object)
                && let Some(Value::String(slot)) = object.get("slot")
            {
                placed.insert(slot.clone());
            }
            for (key, inner) in object {
                if key != "on" {
                    slots_placed(inner, placed);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                slots_placed(item, placed);
            }
        }
        _ => {}
    }
}

/// The paths among `outer` that a param given a path whole (`paths`) reaches,
/// as read through the param: `promo.discount` is `promo.discount` inside
/// an instance given `"promo": "{promo}"`, and `item.note` is `order.note`
/// inside one given `"order": "{item}"`.
fn through(outer: &BTreeSet<String>, paths: &[(String, String)]) -> BTreeSet<String> {
    let mut inner = BTreeSet::new();
    for path in outer {
        for (param, given) in paths {
            if path == given {
                inner.insert(param.clone());
            } else if let Some(rest) =
                path.strip_prefix(given.as_str()).and_then(|rest| rest.strip_prefix('.'))
            {
                inner.insert(format!("{param}.{rest}"));
            }
        }
    }
    inner
}
