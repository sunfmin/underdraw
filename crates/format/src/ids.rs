//! The id underdraw gives a node written without one. Only a node a
//! person can act on, or answer about, needs an id written by hand: one
//! with `on`, `bind` or `assumed`, which transitions, journeys and
//! assumptions name it by. Any other may leave it out, and is given one
//! here, the first step after the domain is written in, so every command
//! (check's findings, render, handoff, the gallery and its notes) names it
//! by the same id.
//!
//! The id is made from where the node is and what it shows, never from
//! how many nodes come before it, so it is the same on every run and
//! unchanged when an unrelated node is added: the id of the node it is in,
//! joined to its component in kebab case and the first words of what it
//! shows (its `text`, `title`, `label` or `icon`): `page-text-hello-name`
//! for `{"component": "Text", "text": "Hello {name}"}` in `page`. Text with
//! no ASCII word in it gives four hex digits of its hash in their place.
//! A node that is an `item` ends in `-{index}`, one id per element. Where
//! that id is taken, by a node written or one given before it in the
//! order written, `-2`, `-3`… follows it. The root of a component's nodes
//! is named by the instance and is given none; the nodes inside it are
//! given theirs without the root's.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::check::Findings;
use crate::instance::SLOTS;
use crate::schema::object;
use crate::showcase::kebab;
use crate::subtrees;

/// The keys that make a node one a person acts on or answers about: it
/// needs an id written by hand.
const NAMED: [&str; 3] = ["on", "bind", "assumed"];

/// The keys whose text names a node given an id, the first held.
const SHOWS: [&str; 4] = ["text", "title", "label", "icon"];

/// At most this many words of what a node shows go into its id.
const WORDS: usize = 3;

/// `description` with an id given to each node written without one: in
/// its root and phases, which share one set of ids, and in each of its
/// own components' nodes, each with its own; and the components the
/// project `shared` (each `components/<Name>.json` by its name) with
/// theirs given, answered. A node with on, bind or assumed and no id is a
/// finding, and so is an instance of a component holding one, whose nodes
/// are named by the instance's id; each is given one all the same.
pub(crate) fn give(
    description: &mut Value,
    shared: &Map<String, Value>,
    findings: &mut Findings,
) -> Map<String, Value> {
    let acted = acted(description, shared);
    let mut giving = Giving { taken: BTreeSet::new(), acted: &acted, findings };
    if let Some(root) = description.get("root") {
        written(root, &mut giving.taken);
    }
    for (_, tree) in object(description, "phases") {
        written(tree, &mut giving.taken);
    }
    if let Some(root) = description.get_mut("root") {
        giving.value(root, "", "root", "");
    }
    if let Some(Value::Object(phases)) = description.get_mut("phases") {
        for (name, tree) in phases.iter_mut() {
            if !name.starts_with('$') {
                giving.value(tree, "", &format!("phases.{name}"), "");
            }
        }
    }
    if let Some(Value::Object(components)) = description.get_mut(subtrees::KEY) {
        for (name, component) in components.iter_mut() {
            nodes(component, &format!("{}.{name}", subtrees::KEY), &acted, giving.findings);
        }
    }
    let mut given = shared.clone();
    for (name, component) in given.iter_mut() {
        nodes(component, &format!("components/{name}.json"), &acted, findings);
    }
    given
}

/// The components, the project's and the screen's own, whose nodes hold
/// one a person acts on or answers about, there or in an instance of
/// another such: an instance of one needs an id written by hand.
fn acted(description: &Value, shared: &Map<String, Value>) -> BTreeSet<String> {
    let mut bodies = Vec::new();
    for (name, component) in shared.iter().chain(object(description, subtrees::KEY)) {
        if let Some(root) = component.get("nodes") {
            bodies.push((name, root));
        }
    }
    let mut acted = BTreeSet::new();
    loop {
        let mut found = Vec::new();
        for (name, root) in &bodies {
            if !acted.contains(*name) && acts(root, &acted) {
                found.push((*name).clone());
            }
        }
        if found.is_empty() {
            return acted;
        }
        acted.extend(found);
    }
}

/// Whether the tree `value` holds a node a person acts on or answers
/// about, or an instance of one of the components `acted`.
fn acts(value: &Value, acted: &BTreeSet<String>) -> bool {
    match value {
        Value::Array(items) => items.iter().any(|item| acts(item, acted)),
        Value::Object(node) => {
            NAMED.iter().any(|named| node.contains_key(*named))
                || node.get("component").and_then(Value::as_str).is_some_and(|name| acted.contains(name))
                || SLOTS.iter().any(|slot| node.get(*slot).is_some_and(|held| acts(held, acted)))
        }
        _ => false,
    }
}

/// An id given to each node inside `component`'s nodes, its root apart,
/// findings named under `at`.
fn nodes(component: &mut Value, at: &str, acted: &BTreeSet<String>, findings: &mut Findings) {
    let mut giving = Giving { taken: BTreeSet::new(), acted, findings };
    if let Some(root) = component.get("nodes") {
        written(root, &mut giving.taken);
    }
    if let Some(Value::Object(root)) = component.get_mut("nodes") {
        giving.slots(root, "", &format!("{at}.nodes"));
    }
}

/// Every id written in the tree `value`, added to `taken`.
fn written(value: &Value, taken: &mut BTreeSet<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                written(item, taken);
            }
        }
        Value::Object(node) => {
            if let Some(Value::String(id)) = node.get("id") {
                taken.insert(id.clone());
            }
            for slot in SLOTS {
                if let Some(held) = node.get(slot) {
                    written(held, taken);
                }
            }
        }
        _ => {}
    }
}

struct Giving<'f> {
    /// Every id written in the tree, and each given so far.
    taken: BTreeSet<String>,
    /// The components an instance of which needs an id written by hand.
    acted: &'f BTreeSet<String>,
    findings: &'f mut Findings,
}

impl Giving<'_> {
    /// An id given to each node in `value`, held under `key` by the node
    /// named `parent` (none for a root), at `at`.
    fn value(&mut self, value: &mut Value, parent: &str, at: &str, key: &str) {
        match value {
            Value::Array(items) => {
                for (index, item) in items.iter_mut().enumerate() {
                    self.value(item, parent, &format!("{at}.{index}"), key);
                }
            }
            Value::Object(node) => self.node(node, parent, at, key),
            _ => {}
        }
    }

    /// The node `node` given an id, when it has none, and then each node
    /// it holds.
    fn node(&mut self, node: &mut Map<String, Value>, parent: &str, at: &str, key: &str) {
        if !node.contains_key("component") {
            return;
        }
        let id = match node.get("id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => {
                let component = node.get("component").and_then(Value::as_str).unwrap_or_default();
                if NAMED.iter().any(|named| node.contains_key(*named)) {
                    self.findings.add(
                        at,
                        "a node with on, bind or assumed is one a person acts on or answers about, and transitions, journeys and notes name it by its id: give it an \"id\"",
                    );
                } else if self.acted.contains(component) {
                    self.findings.add(
                        at,
                        &format!("{component} holds a node with on, bind or assumed, named by this instance's id joined to its own: give the instance an \"id\""),
                    );
                }
                let id = self.given(node, parent, key == "item");
                node.insert("id".to_string(), Value::String(id.clone()));
                id
            }
        };
        self.slots(node, &id, at);
    }

    /// An id given to each node the node `node`, named `id`, holds.
    fn slots(&mut self, node: &mut Map<String, Value>, id: &str, at: &str) {
        for slot in SLOTS {
            if let Some(held) = node.get_mut(slot) {
                self.value(held, id, &format!("{at}.{slot}"), slot);
            }
        }
    }

    /// The id the node `node`, held by the node named `parent`, is given,
    /// ending in `-{index}` when it is an `item`; taken from now on.
    fn given(&mut self, node: &Map<String, Value>, parent: &str, item: bool) -> String {
        let component = node.get("component").and_then(Value::as_str).unwrap_or_default();
        let mut stem = kebab(component);
        if let Some(words) = SHOWS.iter().find_map(|key| node.get(*key)?.as_str()).and_then(words) {
            stem = format!("{stem}-{words}");
        }
        if !parent.is_empty() {
            stem = format!("{parent}-{stem}");
        }
        let end = if item { "-{index}" } else { "" };
        let first = format!("{stem}{end}");
        let id = match self.taken.contains(&first) {
            false => first,
            true => (2u32..)
                .map(|n| format!("{stem}-{n}{end}"))
                .find(|id| !self.taken.contains(id))
                .unwrap_or(first),
        };
        self.taken.insert(id.clone());
        id
    }
}

/// What `shown` says, as words of an id: its first ASCII words, lower
/// case, joined by `-`; four hex digits of its hash when it has text but
/// no such word; none when it is blank.
fn words(shown: &str) -> Option<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    for c in shown.chars().chain([' ']) {
        if c.is_ascii_alphanumeric() {
            word.push(c.to_ascii_lowercase());
        } else if !word.is_empty() && words.len() < WORDS {
            words.push(std::mem::take(&mut word));
        } else {
            word.clear();
        }
    }
    if !words.is_empty() {
        return Some(words.join("-"));
    }
    if shown.trim().is_empty() {
        return None;
    }
    let mut hash = 0x811c_9dc5_u32;
    for byte in shown.bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    Some(format!("{:04x}", hash >> 16))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// `description` with its ids given, and what giving them found.
    fn given(mut description: Value) -> (Value, Vec<String>) {
        let mut findings = Findings::default();
        give(&mut description, &Map::new(), &mut findings);
        (description, findings.0.into_iter().map(|finding| finding.at).collect())
    }

    fn page(children: Value) -> Value {
        json!({ "root": { "id": "page", "component": "Stack", "children": children } })
    }

    fn ids(description: &Value) -> Vec<String> {
        let children = description.pointer("/root/children").and_then(Value::as_array).unwrap();
        children.iter().map(|child| child["id"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn a_node_is_named_by_its_parent_its_component_and_what_it_shows() {
        let (description, findings) = given(page(json!([
            { "component": "Text", "text": "Hello {name}, how are you" },
            { "component": "Icon", "icon": "arrow_back" },
            { "component": "Divider" },
            { "component": "Text", "text": "待办" },
        ])));
        assert_eq!(findings, Vec::<String>::new());
        let ids = ids(&description);
        assert_eq!(ids[..3], ["page-text-hello-name-how", "page-icon-arrow-back", "page-divider"]);
        assert!(ids[3].starts_with("page-text-") && ids[3].len() == "page-text-".len() + 4, "{}", ids[3]);
    }

    #[test]
    fn an_unrelated_node_added_changes_no_id_given() {
        let before =
            given(page(json!([{ "component": "Text", "text": "Total" }, { "component": "Divider" }]))).0;
        let after = given(page(json!([
            { "component": "Text", "text": "Title" },
            { "component": "Text", "text": "Total" },
            { "id": "save", "component": "Button", "on": { "tap": { "back": true } } },
            { "component": "Divider" },
        ])))
        .0;
        assert_eq!(ids(&before), ["page-text-total", "page-divider"]);
        assert_eq!(ids(&after), ["page-text-title", "page-text-total", "save", "page-divider"]);
    }

    #[test]
    fn a_taken_id_is_followed_by_a_number_and_an_item_by_its_index() {
        let (description, _) = given(page(json!([
            { "component": "Divider" },
            { "id": "page-divider-2", "component": "Divider" },
            { "component": "Divider" },
            { "id": "rows", "component": "List", "each": "items", "item": { "component": "Text", "text": "{item}" } },
        ])));
        assert_eq!(ids(&description), ["page-divider", "page-divider-2", "page-divider-3", "rows"]);
        assert_eq!(description.pointer("/root/children/3/item/id").unwrap(), "rows-text-item-{index}");
    }

    #[test]
    fn a_node_a_person_acts_on_still_needs_an_id() {
        let (_, findings) = given(page(json!([
            { "component": "Button", "on": { "tap": { "back": true } } },
            { "component": "TextField", "bind": "query" },
            { "component": "Text", "text": "Ok", "assumed": "a guess, said in a whole sentence here" },
            { "component": "Text", "text": "Fine" },
        ])));
        assert_eq!(findings, ["root.children.0", "root.children.1", "root.children.2"]);
    }

    #[test]
    fn an_instance_of_a_component_holding_a_node_a_person_acts_on_still_needs_an_id() {
        let mut description = page(json!([
            { "component": "Row" },
            { "component": "Wrap" },
            { "component": "Plain" },
            { "id": "row", "component": "Row" },
        ]));
        description["components"] = json!({
            "Wrap": { "nodes": { "component": "Stack", "children": [{ "id": "row", "component": "Row" }] } },
            "Plain": { "nodes": { "component": "Stack", "children": [{ "component": "Text", "text": "Hi" }] } },
        });
        let shared =
            json!({ "Row": { "nodes": { "component": "Card", "on": { "tap": { "back": true } } } } });
        let mut findings = Findings::default();
        give(&mut description, shared.as_object().unwrap(), &mut findings);
        let at: Vec<&str> = findings.0.iter().map(|finding| finding.at.as_str()).collect();
        assert_eq!(at, ["root.children.0", "root.children.1"], "Row's root acts, and Wrap holds a Row");
    }

    #[test]
    fn a_components_root_is_given_none_and_its_nodes_theirs_without_it() {
        let (description, findings) = given(json!({
            "root": { "id": "page", "component": "Stack" },
            "components": { "Row": { "nodes": { "component": "Stack", "children": [{ "component": "Text", "text": "Hi" }] } } },
        }));
        assert_eq!(findings, Vec::<String>::new());
        let root = description.pointer("/components/Row/nodes").unwrap();
        assert_eq!(root.get("id"), None);
        assert_eq!(root.pointer("/children/0/id").unwrap(), "text-hi");
    }
}
