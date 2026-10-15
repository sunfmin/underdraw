//! Every node of the description as one platform builds it: how its component is
//! realised there, its test id, and what each of its events does there.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Map, Value};
use underdraw_format::{Finding, Instances, Met};

/// One node, as the platform builds it.
#[derive(Debug, Serialize)]
pub struct Spec {
    pub id: String,
    pub component: String,
    /// The node it sits in, and under which key (`children`, `item`, …).
    pub parent: Option<String>,
    pub slot: String,
    /// How the platform realises the component, from the platform mapping.
    pub implement: String,
    /// The test id, written as the platform writes it.
    pub test_id: String,
    /// Everything else the description says of it, as written: templates,
    /// `cases`, responsive values and tokens unresolved.
    pub props: Map<String, Value>,
    /// The keys that decide whether and how often it is shown.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub shown: Map<String, Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    /// Why it is built this way here, when the description says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assumed: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Event {
    /// `tap`, `change`, …
    pub on: String,
    pub action: String,
    /// The operation it calls, when the action is `call`: one of the
    /// handoff's `operations`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// The use site that picks this action, when `cases` chooses between
    /// actions; `else` for the last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// What the action is given, as written.
    pub with: Value,
    /// How the platform does the action, from the platform mapping.
    pub implement: String,
}

/// A node the platform does not build, and why.
#[derive(Debug, Serialize)]
pub struct Hidden {
    pub id: String,
    pub why: Option<String>,
}

/// Keys that hold child nodes.
const SLOTS: [&str; 6] = ["children", "leading", "trailing", "item", "action", "overlay"];
/// Keys that decide whether and how often a node is shown.
const SHOWN: [&str; 5] = ["if", "only", "each", "index", "limit"];
/// Keys said of the node elsewhere in a spec, or not at all.
const OTHER: [&str; 8] = ["id", "component", "with", "assumed", "platform", "on", "$note", "$why"];

pub struct Walk<'a> {
    pub platform: &'a str,
    pub mapping: &'a Value,
    /// The instances whose nodes are being walked: each node inside is
    /// named by the instance's id, as written, joined to its own.
    pub instances: Instances<'a, ()>,
    pub specs: Vec<Spec>,
    pub hidden: Vec<Hidden>,
    pub findings: Vec<Finding>,
    /// Each entry the mapping lacks, by its group and name, once found.
    pub missing: BTreeSet<(String, String)>,
}

impl Walk<'_> {
    /// `written`, found under `slot` of `parent`, and everything in it. An
    /// instance of a component is its nodes, each named by the instance's
    /// id joined to its own; what an instance gives in a slot sits where
    /// its component places the slot.
    pub fn node(&mut self, written: &Value, parent: Option<&str>, slot: &str) {
        let object = match written {
            Value::Array(items) => {
                for item in items {
                    self.node(item, parent, slot);
                }
                return;
            }
            Value::Object(object) => object,
            _ => return,
        };
        let raw = object.get("id").and_then(Value::as_str).unwrap_or_default();
        let (_, id) = self.instances.ids(raw, raw);
        match self.instances.met(object) {
            Met::Slot(key) => {
                if let Some(frame) = self.instances.leave() {
                    if let Some(given) = frame.given(&key) {
                        self.node(given, parent, &key);
                    }
                    self.instances.resume(frame);
                }
            }
            Met::Instance(component) => {
                if let Err(problem) = self.instances.enter(component, object, id.clone(), id.clone(), ()) {
                    return self.findings.push(Finding { at: id, problem });
                }
                self.spec(&component.nodes, Some(object), id, parent, slot);
                self.instances.leave();
            }
            Met::Node => self.spec(object, None, id, parent, slot),
        }
    }

    /// The node `object`, named `id`, and everything in it; for the root
    /// of a component's nodes, the instance it is, `instance`, whose `if`,
    /// `only` and `with` are said of it too.
    fn spec(
        &mut self,
        object: &Map<String, Value>,
        instance: Option<&Map<String, Value>>,
        id: String,
        parent: Option<&str>,
        slot: &str,
    ) {
        let Some(component) = object.get("component").and_then(Value::as_str) else {
            return;
        };
        // A node already listed (a component used twice) is listed once.
        for spec in &self.specs {
            if !id.is_empty() && spec.id == id {
                return;
            }
        }
        let here = object.get("platform").and_then(|p| p.get(self.platform)).and_then(Value::as_object);
        let why = here.and_then(|h| h.get("$why")).or_else(|| object.get("$why")).and_then(Value::as_str);
        if here.and_then(|h| h.get("hidden")) == Some(&Value::Bool(true)) {
            self.hidden.push(Hidden { id, why: why.map(str::to_string) });
            return;
        }
        let mut props = Map::new();
        let mut shown = Map::new();
        for (key, value) in object {
            if SHOWN.contains(&key.as_str()) {
                shown.insert(key.clone(), value.clone());
            } else if !SLOTS.contains(&key.as_str()) && !OTHER.contains(&key.as_str()) {
                props.insert(key.clone(), value.clone());
            }
        }
        // An instance's own `if` and `only` decide whether its root is
        // shown, as written where the instance is; what it passes is said
        // as written there too.
        for (key, value) in instance.into_iter().flatten() {
            if SHOWN.contains(&key.as_str()) {
                shown.insert(key.clone(), value.clone());
            } else if key == "with" {
                props.insert(key.clone(), value.clone());
            }
        }
        if let Some(here) = here {
            for (key, value) in here {
                if key != "$why" && key != "hidden" {
                    props.insert(key.clone(), value.clone());
                }
            }
        }
        let implement = self.mapped("components", "component", component, &id);
        let test_id =
            self.mapping.get("testId").and_then(Value::as_str).unwrap_or("{id}").replace("{id}", &id);
        let events = self.events(object.get("on"), &id);
        self.specs.push(Spec {
            id: id.clone(),
            component: component.to_string(),
            parent: parent.map(str::to_string),
            slot: slot.to_string(),
            implement,
            test_id,
            props,
            shown,
            events,
            why: why.map(str::to_string),
            assumed: object.get("assumed").and_then(Value::as_str).map(str::to_string),
        });
        for key in SLOTS {
            if let Some(children) = object.get(key) {
                self.node(children, Some(&id), key);
            }
        }
    }

    /// `on`: each event names one action and what it is given.
    fn events(&mut self, on: Option<&Value>, id: &str) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(Value::Object(on)) = on else {
            return events;
        };
        for (event, handler) in on {
            match handler.get("cases") {
                Some(Value::Array(branches)) => {
                    for branch in branches {
                        let when = match branch.get("if").and_then(Value::as_str) {
                            Some(used) => used.to_string(),
                            None => "else".to_string(),
                        };
                        if let Some(chosen) = branch.get("then").or_else(|| branch.get("else")) {
                            self.actions(event, chosen, Some(&when), id, &mut events);
                        }
                    }
                }
                _ => self.actions(event, handler, None, id, &mut events),
            }
        }
        events
    }

    /// The actions `handler` names for `event`: each key but `with`.
    fn actions(
        &mut self,
        event: &str,
        handler: &Value,
        when: Option<&str>,
        id: &str,
        events: &mut Vec<Event>,
    ) {
        let Some(handler) = handler.as_object() else {
            return;
        };
        for (action, given) in handler {
            if action == "with" {
                continue;
            }
            let implement = self.mapped("actions", "action", action, id);
            let with = match handler.get("with") {
                Some(with) => crate::object([("target", given.clone()), ("with", with.clone())]),
                None => given.clone(),
            };
            let operation = if action == "call" { given.as_str().map(str::to_string) } else { None };
            events.push(Event {
                on: event.to_string(),
                action: action.clone(),
                operation,
                when: when.map(str::to_string),
                with,
                implement,
            });
        }
    }

    /// How the platform does `name`, from its mapping's `group`; a finding
    /// at `at` when the mapping does not say.
    fn mapped(&mut self, group: &str, what: &str, name: &str, at: &str) -> String {
        // The mapping is read through a copy of the reference, so that the
        // finding below does not write to `self` while it is borrowed.
        let mapping = self.mapping;
        if let Some(implement) = mapping.get(group).and_then(|g| g.get(name)).and_then(Value::as_str) {
            return implement.to_string();
        }
        // One finding per missing entry, at the first node that needs it.
        if self.missing.insert((group.to_string(), name.to_string())) {
            let problem =
                format!("platforms/{}.json has no entry for {what} {name} under \"{group}\"", self.platform);
            self.findings.push(Finding { at: at.to_string(), problem });
        }
        String::new()
    }
}
