//! A screen's own components whose body is nodes (docs/adr/0021): a
//! subtree declared once in the screen's `"components"` with the params it
//! reads, and used where a node goes as
//! `{"id": "order-3", "component": "OrderRow", "with": {"order": "{item}"}}`.
//! The subtree's root is the instance, named by its id; each node inside is
//! named by the instance's id joined to its own (`order-3-cancel`). A node
//! `{"slot": "children"}` inside stands for what the instance gives in that
//! slot: the screen's nodes, read in the screen's scope, under their own ids.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::check::Findings;
use crate::field::{self, Base, Field};
use crate::instance::SLOTS;
use crate::schema::{Schema, object};

/// Where a screen declares its components.
pub(crate) const KEY: &str = "components";

/// The keys an instance may hold besides the slots it fills: everything
/// else it would say of the subtree, it passes as a param.
const INSTANCE: [&str; 9] = ["id", "component", "with", "if", "only", "assumed", "$note", "$why", "$comment"];

/// The keys a component's declaration holds.
const DECLARATION: [&str; 5] = ["params", "nodes", "$note", "$why", "$comment"];

/// One component the screen declares: the params it reads, and its nodes.
pub struct Component<'a> {
    pub name: &'a str,
    pub params: Vec<(String, Field)>,
    /// The subtree's root, which the instance's id names.
    pub nodes: &'a Map<String, Value>,
}

/// The components a screen declares whose body is nodes, by name.
#[derive(Default)]
pub struct Components<'a>(BTreeMap<&'a str, Component<'a>>);

impl<'a> Components<'a> {
    /// The components `screen` declares; one without nodes is left out, and
    /// a param that is not a field is too, for [`check`] to name.
    pub fn read(screen: &'a Value) -> Components<'a> {
        let mut read = BTreeMap::new();
        for (name, declared) in object(screen, KEY) {
            let Some(Value::Object(nodes)) = declared.get("nodes") else {
                continue;
            };
            let mut params = Vec::new();
            for (param, spec) in object(declared, "params") {
                if let Ok(field) = field::read(spec) {
                    params.push((param.clone(), field));
                }
            }
            read.insert(name.as_str(), Component { name, params, nodes });
        }
        Components(read)
    }

    /// The component the node `node` is an instance of, if it is one.
    pub fn of(&self, node: &Map<String, Value>) -> Option<&Component<'a>> {
        node.get("component").and_then(Value::as_str).and_then(|name| self.0.get(name))
    }

    pub(crate) fn has(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
}

/// The id of a node written `id` inside the instance named `prefix`:
/// `order-3` and `cancel` make `order-3-cancel`. Outside any, its own.
pub fn join(prefix: Option<&str>, id: &str) -> String {
    match prefix {
        Some(prefix) => format!("{prefix}-{id}"),
        None => id.to_string(),
    }
}

/// What the instance `instance` gives in the slot a `{"slot": …}` node
/// `placeholder` stands for: nothing, when it gives none.
pub(crate) fn slotted<'v>(
    placeholder: &Map<String, Value>,
    instance: &'v Map<String, Value>,
) -> Option<&'v Value> {
    placeholder.get("slot").and_then(Value::as_str).and_then(|slot| instance.get(slot))
}

/// Whether `node` is a `{"slot": …}` node: a place in a component's nodes
/// for what an instance gives.
pub(crate) fn is_slot(node: &Map<String, Value>) -> bool {
    node.contains_key("slot") && !node.contains_key("component")
}

/// Every fault in the components `screen` declares: each is
/// `{"params": {…}, "nodes": {…}}`, named with a capital, its params
/// fields of types the screen knows and none of them view state, its root
/// without an id of its own.
pub(crate) fn check(screen: &Value, schema: &Schema, findings: &mut Findings) {
    if screen.get(KEY).is_some_and(|declared| !declared.is_object()) {
        return findings
            .add(KEY, "components maps each component's name to {\"params\": {…}, \"nodes\": {…}}");
    }
    for (name, declared) in object(screen, KEY) {
        let at = format!("{KEY}.{name}");
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) || !field::is_identifier(name) {
            findings.add(&at, "a component's name starts with a capital letter: OrderRow");
        }
        let Some(declared) = declared.as_object() else {
            findings.add(&at, "a component is {\"params\": {…}, \"nodes\": {…}}");
            continue;
        };
        if declared.contains_key("id") || declared.contains_key("component") {
            findings.add(
                &at,
                "a component is {\"params\": {…}, \"nodes\": <its root node>}, not a node: put the node in \"nodes\", without its id, and use it as {\"id\": …, \"component\": \"<Name>\", \"with\": {…}}",
            );
            continue;
        }
        for key in declared.keys() {
            if DECLARATION.contains(&key.as_str()) {
                continue;
            }
            let problem = match key.as_str() {
                "data" | "view" => {
                    "a component holds no view state: the screen holds it in its data, marked \"view\": true, and passes it in a param".to_string()
                }
                _ => format!("a component holds only params and nodes, not {key}"),
            };
            findings.add(&format!("{at}.{key}"), &problem);
        }
        match declared.get("nodes") {
            Some(Value::Object(root)) => {
                if root.contains_key("id") {
                    findings.add(
                        &format!("{at}.nodes.id"),
                        "the root of a component's nodes is named by the instance's id: take its id out",
                    );
                }
            }
            _ => findings
                .add(&format!("{at}.nodes"), "a component's nodes are one node, the root of its subtree"),
        }
        match declared.get("params") {
            None => {}
            Some(Value::Object(params)) => {
                for (param, spec) in params {
                    let at = format!("{at}.params.{param}");
                    let field = match field::read(spec) {
                        Ok(field) => field,
                        Err(problem) => {
                            findings.add(&at, &problem);
                            continue;
                        }
                    };
                    if field.view {
                        findings.add(
                            &at,
                            "a component holds no view state: declare the field in the screen's data, marked \"view\": true, and pass it in; its set and toggle then act on what was passed",
                        );
                    }
                    if field.from.is_some() || field.derived.is_some() {
                        findings
                            .add(&at, "a param is given by the instance: from and derived belong to data");
                    }
                    if let Base::Named(type_name) = &field.base
                        && !schema.has(type_name)
                    {
                        findings.add(&at, &format!("unknown type {type_name}"));
                    }
                }
            }
            Some(_) => findings.add(
                &format!("{at}.params"),
                "params maps each param's name to its type: {\"order\": \"Order\"}",
            ),
        }
    }
}

/// Every key of the instance `instance` that it may not hold: what it would
/// say of the subtree is a param, given in `with`.
pub(crate) fn reached_into(instance: &Map<String, Value>) -> Vec<&String> {
    let mut reached = Vec::new();
    for key in instance.keys() {
        let slot = SLOTS.contains(&key.as_str()) && key != "item";
        if !slot && !INSTANCE.contains(&key.as_str()) {
            reached.push(key);
        }
    }
    reached
}

/// Every node `screen` writes, in its root, its phases and, through each
/// instance, its components, with its id as written and joined (a template
/// inside an item: `order-{index}-cancel`); an instance is its component's
/// root, under the instance's id.
pub(crate) fn written(screen: &Value) -> Vec<(String, &Map<String, Value>)> {
    let components = Components::read(screen);
    let mut walk = Written { components: &components, using: Vec::new(), found: Vec::new() };
    if let Some(root) = screen.get("root") {
        walk.value(root, None);
    }
    for (_, tree) in object(screen, "phases") {
        walk.value(tree, None);
    }
    walk.found
}

struct Written<'c, 'a> {
    components: &'c Components<'a>,
    /// The components being walked through, outermost first: one met again
    /// is a cycle, which `check` names, and is not walked into.
    using: Vec<&'a str>,
    found: Vec<(String, &'a Map<String, Value>)>,
}

impl<'a> Written<'_, 'a> {
    fn value(&mut self, value: &'a Value, prefix: Option<&str>) {
        match value {
            Value::Array(items) => {
                for item in items {
                    self.value(item, prefix);
                }
            }
            Value::Object(node) => self.node(node, prefix),
            _ => {}
        }
    }

    fn node(&mut self, node: &'a Map<String, Value>, prefix: Option<&str>) {
        let id = join(prefix, node.get("id").and_then(Value::as_str).unwrap_or_default());
        if let Some(component) = self.components.of(node) {
            let (name, root) = (component.name, component.nodes);
            for (key, inside) in node {
                if SLOTS.contains(&key.as_str()) {
                    self.value(inside, prefix);
                }
            }
            if self.using.contains(&name) {
                return;
            }
            self.found.push((id.clone(), root));
            self.using.push(name);
            self.inside(root, Some(&id));
            self.using.pop();
            return;
        }
        if ["id", "component", "on"].iter().any(|key| node.contains_key(*key)) {
            self.found.push((id, node));
        }
        self.inside(node, prefix);
    }

    /// What the node `node` holds, every key but its handlers.
    fn inside(&mut self, node: &'a Map<String, Value>, prefix: Option<&str>) {
        for (key, inside) in node {
            if key != "on" && key != "with" {
                self.value(inside, prefix);
            }
        }
    }
}
