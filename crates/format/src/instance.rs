//! A screen in one state: the description with one fixture's data (or one
//! phase) filled in, at one breakpoint. Conditions are decided, `cases`
//! chosen, templates filled, `each` expanded, each instance of a component
//! built from its nodes and `only` applied, so what remains is a plain tree
//! for layout.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::breakpoint::{self, Breakpoint};
use crate::components;
use crate::conditions::{Conditions, UseSite};
use crate::expand::{Instances, Met};
use crate::links::Undrawn;
use crate::schema::object;
use crate::screen::Screen;
use crate::subtrees;
use crate::template::{self, Decimals, Format};

/// Which state of the screen to show.
#[derive(Clone, Copy, Debug)]
pub enum State<'a> {
    /// The screen once data has arrived, with this fixture's data.
    Fixture(&'a str),
    /// A phase before data arrives, such as `loading`.
    Phase(&'a str),
}

/// One node of a screen in one state.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Node {
    pub id: String,
    /// Its id as the description writes it: a template inside an item
    /// (`order-{index}-cancel`), joined to its instance's inside a
    /// component, what a transition names it by.
    #[serde(skip)]
    pub written_id: String,
    /// Where it is written in screen.json, as a JSON Pointer (RFC 6901):
    /// `/root/children/1/item`, or `/components/OrderRow/nodes/…` for one
    /// of a component's nodes.
    #[serde(skip)]
    pub written_at: String,
    /// Which element of each `each` it was made for, the outermost first:
    /// `[2]` for the third row; empty outside any.
    #[serde(skip)]
    pub instance: Vec<usize>,
    /// How many levels in it is drawn, as a row of a tree (`"nest"`): 0 for
    /// a row at the top and for every node that is no row of one.
    #[serde(skip_serializing_if = "is_zero")]
    pub level: usize,
    pub component: String,
    /// Every property other than the node's structure, with templates filled
    /// and cases and responsive values chosen.
    pub props: Map<String, Value>,
    /// The nodes it holds, each with the key it came from in `slot`
    /// (`children`, `leading`, `trailing`, `item`, `action`, `overlay`, `foot`).
    pub children: Vec<Node>,
    pub slot: String,
    /// What the agent guessed here, for the person to confirm.
    pub assumed: Option<String>,
    /// Whether a person can act on it: it has an `on` handler or binds data.
    pub interactive: bool,
}

/// One operation a node calls: the event it is called on, and what it is
/// given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Call<'a> {
    pub on: &'a str,
    pub operation: &'a str,
    pub with: Option<&'a Value>,
}

impl Node {
    /// Each operation it calls, in the order its `on` lists the events.
    pub fn calls(&self) -> Vec<Call<'_>> {
        let Some(Value::Object(events)) = self.props.get("on") else {
            return Vec::new();
        };
        events
            .iter()
            .filter_map(|(on, action)| match action.get("call") {
                Some(Value::String(operation)) => Some(Call { on, operation, with: action.get("with") }),
                _ => None,
            })
            .collect()
    }
}

/// Keys that hold child nodes.
pub const SLOTS: [&str; 7] = ["children", "leading", "trailing", "item", "action", "overlay", "foot"];

/// Keys that only steer how the tree is built.
const STRUCTURE: [&str; 14] = [
    "id",
    "component",
    "with",
    "if",
    "only",
    "each",
    "index",
    "limit",
    "nest",
    "fold",
    "assumed",
    "platform",
    "$note",
    "$why",
];

/// The screen `screen` in `state`, at `breakpoint`, each node that
/// navigates to a screen `undrawn` says is not drawn yet marked assumed.
pub(crate) fn build(
    screen: &Screen,
    state: State,
    breakpoint: Breakpoint,
    undrawn: Option<&Undrawn>,
) -> Result<Node, String> {
    let (fixtures, description) = (&screen.fixtures, screen.description());
    let (tree, data) = match state {
        State::Fixture(name) => {
            let mut found = None;
            for (fixture, data) in fixtures {
                if fixture == name {
                    found = Some(data.clone());
                }
            }
            let data = found.ok_or_else(|| format!("there is no fixture {name}"))?;
            (description.get("root").ok_or("the screen has no root")?, data)
        }
        State::Phase(name) => {
            let mut found = None;
            for (phase, tree) in object(description, "phases") {
                if phase == name {
                    found = Some(tree);
                }
            }
            (found.ok_or_else(|| format!("there is no phase {name}"))?, Value::Object(Map::new()))
        }
    };
    let instances = Instances::new(screen.components.subtrees());
    let mut fill = Fill { conditions: &screen.conditions, instances, breakpoint, undrawn };
    let at = match state {
        State::Fixture(_) => "/root".to_string(),
        State::Phase(name) => format!("/phases/{}", escaped(name)),
    };
    let mut built = fill.node(tree, &data, "root", &Written { at, instance: Vec::new(), shared: false })?;
    built.pop().ok_or_else(|| "the root is hidden in this state".to_string())
}

/// Where a node is written, and for which elements of the `each`s around
/// it: what [`Node::written_at`] and [`Node::instance`] are given.
#[derive(Clone)]
struct Written {
    at: String,
    instance: Vec<usize>,
    /// Whether it is written in a component the project shares, whose
    /// nodes are never marked as navigating to a screen not drawn yet.
    shared: bool,
}

impl Written {
    /// The node under `key`, and, for one of a list, at `index` of it.
    fn under(&self, key: &str, index: Option<usize>) -> Written {
        let at = match index {
            Some(index) => format!("{}/{}/{index}", self.at, escaped(key)),
            None => format!("{}/{}", self.at, escaped(key)),
        };
        Written { at, instance: self.instance.clone(), shared: self.shared }
    }
}

/// `key` as one step of a JSON Pointer: `~` and `/` escaped.
fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

struct Fill<'c> {
    conditions: &'c Conditions,
    /// The instances being built, each with where it is written and the
    /// scope it is in: what a `{"slot": …}` node inside stands for is
    /// built there.
    instances: Instances<'c, (Written, Value)>,
    breakpoint: Breakpoint,
    undrawn: Option<&'c Undrawn<'c>>,
}

impl Fill<'_> {
    /// The nodes `written` becomes in `scope`: none when it is hidden, one
    /// otherwise, or what an instance gives in a slot.
    /// `at` is where it is written.
    fn node(
        &mut self,
        written: &Value,
        scope: &Value,
        slot: &str,
        at: &Written,
    ) -> Result<Vec<Node>, String> {
        let Some(object) = written.as_object() else {
            return Err(format!("expected a node, found {written}"));
        };
        if object.contains_key("ref") {
            return Err(
                "ref is gone: use the component, {\"id\": …, \"component\": \"<Name>\", \"with\": {…}}"
                    .to_string(),
            );
        }
        let component = match self.instances.met(object) {
            Met::Slot(key) => {
                let Some(frame) = self.instances.leave() else {
                    return Err(format!("{{\"slot\": \"{key}\"}} is outside any component"));
                };
                let built = match frame.given(&key) {
                    Some(given) => self.slot(given, &frame.at.1, &key, &frame.at.0),
                    None => Ok(Vec::new()),
                };
                self.instances.resume(frame);
                return built;
            }
            Met::Instance(component) => Some(component),
            Met::Node => None,
        };
        if !self.shown(object, scope) {
            return Ok(Vec::new());
        }
        let (id, written_id) = match object.get("id") {
            Some(Value::String(id)) => self.instances.ids(id, &self.text(id, scope)?),
            _ => return Err(format!("a node has no id: {written}")),
        };
        let Some(component) = component else {
            return Ok(vec![self.build(object, scope, slot, at, id, written_id)?]);
        };
        let mut params = Map::new();
        for (name, field) in &component.params {
            let value = match object.get("with").and_then(|with| with.get(name)) {
                Some(given) => self.argument(given, scope)?,
                None => field.default.clone().unwrap_or(Value::Null),
            };
            params.insert(name.clone(), value);
        }
        let root = &component.nodes;
        let params = Value::Object(params);
        self.instances.enter(
            component,
            object,
            id.clone(),
            written_id.clone(),
            (at.clone(), scope.clone()),
        )?;
        let inner = Written {
            at: match component.shared {
                true => format!("{}/nodes", components::shared_at(&escaped(&component.name))),
                false => format!("/{}/{}/nodes", subtrees::KEY, escaped(&component.name)),
            },
            instance: at.instance.clone(),
            shared: component.shared,
        };
        let built = if self.shown(root, &params) {
            self.build(root, &params, slot, &inner, id, written_id).map(Some)
        } else {
            Ok(None)
        };
        self.instances.leave();
        let Some(mut node) = built? else {
            return Ok(Vec::new());
        };
        if let Some(Value::String(assumed)) = object.get("assumed") {
            node.assumed = Some(assumed.clone());
        }
        Ok(vec![node])
    }

    /// What an instance gives in the slot `key`, `given`, built in the
    /// instance's scope, `scope`, where it is written, `at`.
    fn slot(&mut self, given: &Value, scope: &Value, key: &str, at: &Written) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        match given {
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    nodes.append(&mut self.node(child, scope, key, &at.under(key, Some(index)))?);
                }
            }
            one => nodes.append(&mut self.node(one, scope, key, &at.under(key, None))?),
        }
        Ok(nodes)
    }

    /// Whether the node `object` is shown in `scope`, at this breakpoint.
    fn shown(&self, object: &Map<String, Value>, scope: &Value) -> bool {
        if let Some(Value::String(only)) = object.get("only")
            && Breakpoint::parse(only) != Some(self.breakpoint)
        {
            return false;
        }
        !matches!(object.get("if"), Some(Value::String(used)) if !self.holds(used, scope))
    }

    /// What a param is given in `with`, read in the instance's scope: a
    /// condition's name in braces gives whether it holds.
    fn argument(&self, given: &Value, scope: &Value) -> Result<Value, String> {
        match given.as_str().and_then(template::whole) {
            Some(name) if self.conditions.contains(&name) => {
                Ok(Value::Bool(self.conditions.holds(&name, scope)))
            }
            _ => self.value(given, scope),
        }
    }

    /// The node `object`, named `id` (and `written_id` as written), with
    /// what it holds.
    fn build(
        &mut self,
        object: &Map<String, Value>,
        scope: &Value,
        slot: &str,
        at: &Written,
        id: String,
        written_id: String,
    ) -> Result<Node, String> {
        let component = object.get("component").and_then(Value::as_str).unwrap_or_default().to_string();
        let mut node = Node {
            id,
            written_id,
            written_at: at.at.clone(),
            instance: at.instance.clone(),
            level: 0,
            component,
            props: Map::new(),
            children: Vec::new(),
            slot: slot.to_string(),
            assumed: match (object.get("assumed"), object.get("on"), self.undrawn) {
                (Some(assumed), _, _) => assumed.as_str().map(str::to_string),
                (None, Some(on), Some(undrawn)) if !at.shared => undrawn.of(on),
                _ => None,
            },
            interactive: object.contains_key("on") || object.contains_key("bind"),
        };
        let each = match object.get("each") {
            Some(Value::String(path)) => Some(self.items(path, object, scope)?),
            _ => None,
        };
        for (key, value) in object {
            if STRUCTURE.contains(&key.as_str()) {
                continue;
            }
            if key == "item" {
                let named = object.get("index").and_then(Value::as_str);
                for row in each.iter().flatten().filter(|row| row.shown) {
                    let scope = with(scope, &row.item, row.index, named);
                    let mut at = at.under(key, None);
                    at.instance.push(row.index);
                    for mut built in self.node(value, &scope, "item", &at)? {
                        built.level = row.level;
                        node.children.push(built);
                    }
                }
            } else if SLOTS.contains(&key.as_str()) {
                node.children.append(&mut self.slot(value, scope, key, at)?);
            } else if key == "on" && self.instances.innermost().is_some() {
                let mut on = self.value(value, scope)?;
                repath(&mut on, &self.instances);
                node.props.insert(key.clone(), on);
            } else if let ("disabled", Value::String(used)) = (key.as_str(), value) {
                node.props.insert(key.clone(), Value::Bool(self.holds(used, scope)));
            } else {
                node.props.insert(key.clone(), self.value(value, scope)?);
            }
        }
        // What a bound field shows: the value at its path (or paths).
        match object.get("bind") {
            Some(Value::String(path)) => {
                node.props.insert("value".to_string(), lookup(scope, path).cloned().unwrap_or(Value::Null));
            }
            Some(Value::Array(paths)) => {
                let mut values = Vec::new();
                for path in paths {
                    let found = path.as_str().and_then(|path| lookup(scope, path));
                    values.push(found.cloned().unwrap_or(Value::Null));
                }
                node.props.insert("value".to_string(), Value::Array(values));
            }
            _ => {}
        }
        // `each` with no `item` template shows the items themselves (photos).
        if let Some(rows) = each
            && !object.contains_key("item")
        {
            let items = rows.into_iter().filter(|row| row.shown).map(|row| row.item).collect();
            node.props.insert("items".to_string(), Value::Array(items));
        }
        Ok(node)
    }

    /// The rows the repeating node `object` makes of the elements `path`
    /// holds, at most its `limit` of them: a count, or one per breakpoint.
    /// With `"nest"`, each element is followed by the rows of the list its
    /// nest field holds, a level further in, at any depth; those of an
    /// element whose `"fold"` field is true are made but not shown, so
    /// every row keeps its index whatever is folded above it.
    fn items(&self, path: &str, object: &Map<String, Value>, scope: &Value) -> Result<Vec<Row>, String> {
        let Some(Value::Array(all)) = lookup(scope, path) else {
            return Err(format!("each: {path} is not an array here"));
        };
        let limit = object.get("limit").map(|limit| breakpoint::at(limit, self.breakpoint));
        let limit = limit.and_then(Value::as_u64).map_or(all.len(), |n| n as usize);
        let top: Vec<&Value> = all.iter().take(limit).collect();
        let nest = object.get("nest").and_then(Value::as_str);
        let fold = object.get("fold").and_then(Value::as_str);
        let mut rows = Vec::new();
        rows_of(&top, nest, fold, 0, true, &mut rows);
        Ok(rows)
    }

    /// A property's value in `scope`: templates filled, `cases` and
    /// responsive objects decided, nested values in turn.
    fn value(&self, written: &Value, scope: &Value) -> Result<Value, String> {
        match written {
            Value::String(text) => self.template(text, scope),
            Value::Array(items) => {
                let mut filled = Vec::new();
                for item in items {
                    filled.push(self.value(item, scope)?);
                }
                Ok(Value::Array(filled))
            }
            Value::Object(object) => {
                if let Some(Value::Array(branches)) = object.get("cases") {
                    return self.value(&self.choose(branches, scope)?, scope);
                }
                if breakpoint::is_responsive(object) {
                    return self.value(breakpoint::at(written, self.breakpoint), scope);
                }
                let mut filled = Map::new();
                for (key, value) in object {
                    filled.insert(key.clone(), self.value(value, scope)?);
                }
                Ok(Value::Object(filled))
            }
            other => Ok(other.clone()),
        }
    }

    fn choose(&self, branches: &[Value], scope: &Value) -> Result<Value, String> {
        for branch in branches {
            match (branch.get("if"), branch.get("then"), branch.get("else")) {
                (Some(Value::String(used)), Some(then), _) if self.holds(used, scope) => {
                    return Ok(then.clone());
                }
                (None, _, Some(otherwise)) => return Ok(otherwise.clone()),
                _ => {}
            }
        }
        Err("no branch of cases holds, and there is no else".to_string())
    }

    /// Whether the use site `used` holds: each of its `&&` terms (`!`, then
    /// a condition or a Bool path) does.
    fn holds(&self, used: &str, scope: &Value) -> bool {
        UseSite::parse(used).terms.iter().all(|term| {
            let holds = if self.conditions.contains(term.name) {
                self.conditions.holds(term.name, scope)
            } else {
                lookup(scope, term.name) == Some(&Value::Bool(true))
            };
            holds != term.negated
        })
    }

    /// A template's value: the value itself when it is one placeholder alone,
    /// text otherwise.
    fn template(&self, text: &str, scope: &Value) -> Result<Value, String> {
        let placeholders = template::placeholders(text)?;
        if let [only] = placeholders.as_slice()
            && only.format.is_none()
            && text == format!("{{{}}}", only.path)
        {
            return Ok(lookup(scope, &only.path).cloned().unwrap_or(Value::Null));
        }
        self.text(text, scope).map(Value::String)
    }

    /// `text` with every placeholder replaced by its value, formatted.
    fn text(&self, text: &str, scope: &Value) -> Result<String, String> {
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    out.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    out.push('}');
                }
                '{' => {
                    let mut inside = String::from("{");
                    for c in chars.by_ref() {
                        inside.push(c);
                        if c == '}' {
                            break;
                        }
                    }
                    let found = template::placeholders(&inside)?;
                    let Some(placeholder) = found.first() else {
                        return Err(format!("{inside} is not a placeholder"));
                    };
                    let value = lookup(scope, &placeholder.path).cloned().unwrap_or(Value::Null);
                    out.push_str(&shown(&value, placeholder.format));
                }
                _ => out.push(c),
            }
        }
        Ok(out)
    }
}

/// The handlers `on`, inside a component, with each path a `set` or
/// `toggle` writes through a param made the path the param was given:
/// a component holds no view state, and acts on what was passed in.
fn repath<T>(on: &mut Value, instances: &Instances<T>) {
    match on {
        Value::Object(object) => {
            for (key, value) in object.iter_mut() {
                match (key.as_str(), value) {
                    ("set" | "toggle", Value::String(path)) => *path = instances.through(path),
                    ("set", Value::Object(written)) => {
                        let mut renamed = Map::new();
                        for (path, value) in std::mem::take(written) {
                            renamed.insert(instances.through(&path), value);
                        }
                        *written = renamed;
                    }
                    (_, value) => repath(value, instances),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                repath(item, instances);
            }
        }
        _ => {}
    }
}

/// One element an `each` repeats over: the element, its index among every
/// row the repeat makes, how many levels of a tree in it is, and whether it
/// is drawn (none folded above it).
struct Row {
    item: Value,
    index: usize,
    level: usize,
    shown: bool,
}

/// `items` added to `rows` at `level`, each followed by the rows its `nest`
/// field holds, one level in, hidden under one whose `fold` field is true.
fn rows_of(
    items: &[&Value],
    nest: Option<&str>,
    fold: Option<&str>,
    level: usize,
    shown: bool,
    rows: &mut Vec<Row>,
) {
    for item in items {
        rows.push(Row { item: (*item).clone(), index: rows.len(), level, shown });
        let Some(Value::Array(inner)) = nest.and_then(|nest| item.get(nest)) else {
            continue;
        };
        let folded = fold.and_then(|fold| item.get(fold)) == Some(&Value::Bool(true));
        let inner: Vec<&Value> = inner.iter().collect();
        rows_of(&inner, nest, fold, level.saturating_add(1), shown && !folded, rows);
    }
}

fn is_zero(level: &usize) -> bool {
    *level == 0
}

/// `scope` with `item` and `index` added for one element of an `each`,
/// and the index under the name the repeat gives it, `named`, too.
fn with(scope: &Value, item: &Value, index: usize, named: Option<&str>) -> Value {
    let mut scope = scope.as_object().cloned().unwrap_or_default();
    scope.insert("item".to_string(), item.clone());
    scope.insert("index".to_string(), Value::from(index));
    if let Some(named) = named {
        scope.insert(named.to_string(), Value::from(index));
    }
    Value::Object(scope)
}

fn lookup<'a>(scope: &'a Value, path: &str) -> Option<&'a Value> {
    let mut at = Some(scope);
    for segment in path.split('.') {
        at = at.and_then(|v| v.get(segment));
    }
    at
}

/// `value` as text, with `format`. A wireframe's formatting: plain and
/// locale-free (`¥104,300`, `2026/11/12`), until descriptions carry a locale.
fn shown(value: &Value, format: Option<Format>) -> String {
    match (format, value) {
        (_, Value::Null) => String::new(),
        (Some(Format::Money), Value::Object(money)) => {
            let amount = money.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
            let currency = money.get("currency").and_then(Value::as_str).unwrap_or_default();
            money_text(amount, currency)
        }
        (Some(Format::Date), Value::String(date)) => date.replace('-', "/"),
        (Some(Format::DateShort), Value::String(date)) => {
            let mut parts = date.split('-');
            let (_, month, day) = (parts.next(), parts.next(), parts.next());
            format!("{}/{}", month.unwrap_or_default(), day.unwrap_or_default())
        }
        (Some(Format::Count), Value::Array(items)) => items.len().to_string(),
        (Some(Format::Number(decimals)), Value::Number(n)) => {
            number_text(n.as_f64().unwrap_or(0.0), decimals)
        }
        (_, Value::String(s)) => s.clone(),
        (_, other) => other.to_string(),
    }
}

fn money_text(amount: f64, currency: &str) -> String {
    let (symbol, cents) = match currency {
        "JPY" => ("¥", false),
        "CNY" => ("¥", true),
        "USD" => ("$", true),
        "EUR" => ("€", true),
        "GBP" => ("£", true),
        other => (other, true),
    };
    let fixed = if cents { format!("{amount:.2}") } else { format!("{amount:.0}") };
    let (whole, fraction) = match fixed.split_once('.') {
        Some((whole, fraction)) => (whole.to_string(), format!(".{fraction}")),
        None => (fixed, String::new()),
    };
    format!("{symbol}{}{fraction}", grouped(&whole))
}

/// `digits` with a comma between each group of three.
fn grouped(digits: &str) -> String {
    let (sign, digits) = match digits.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", digits),
    };
    let count = digits.chars().count();
    let mut out = String::from(sign);
    let mut left = count;
    for c in digits.chars() {
        out.push(c);
        left = left.saturating_sub(1);
        if left > 0 && left % 3 == 0 {
            out.push(',');
        }
    }
    out
}

/// `n` with at least `min` and at most `max` decimals: trailing zeros past
/// `min` are dropped.
fn number_text(n: f64, decimals: Decimals) -> String {
    let (min, max) = (usize::from(decimals.min), usize::from(decimals.max));
    let mut text = format!("{n:.max$}");
    if let Some(point) = text.find('.') {
        let mut kept = max;
        while kept > min && text.ends_with('0') {
            text.pop();
            kept -= 1;
        }
        if kept == 0 {
            text.truncate(point);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimals(min: u8, max: u8) -> Option<Format> {
        Decimals::new(min, max).map(Format::Number)
    }

    #[test]
    fn formats_like_a_wireframe() {
        let cases = [
            (serde_json::json!({"amount": 104300, "currency": "JPY"}), Some(Format::Money), "¥104,300"),
            (serde_json::json!({"amount": 1234.5, "currency": "USD"}), Some(Format::Money), "$1,234.50"),
            (serde_json::json!("2026-11-12"), Some(Format::Date), "2026/11/12"),
            (serde_json::json!("2026-11-12"), Some(Format::DateShort), "11/12"),
            (serde_json::json!(1), decimals(0, 1), "1"),
            (serde_json::json!(1.5), decimals(0, 1), "1.5"),
            (serde_json::json!(5), decimals(1, 1), "5.0"),
            (serde_json::json!(4.92), decimals(2, 2), "4.92"),
            (serde_json::json!(["a", "b"]), Some(Format::Count), "2"),
            (serde_json::json!(null), Some(Format::Money), ""),
        ];
        for (value, format, expected) in cases {
            assert_eq!(shown(&value, format), expected, "{value} {format:?}");
        }
    }

    #[test]
    fn groups_thousands() {
        for (digits, expected) in [("0", "0"), ("999", "999"), ("1000", "1,000"), ("-1234567", "-1,234,567")]
        {
            assert_eq!(grouped(digits), expected);
        }
    }
}
