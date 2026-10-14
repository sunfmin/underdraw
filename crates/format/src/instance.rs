//! A screen in one state: the description with one fixture's data (or one
//! phase) filled in, at one breakpoint. Conditions are decided, `cases`
//! chosen, templates filled, `each` expanded, `ref`s inlined and `only`
//! applied, so what remains is a plain tree for layout.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::check::Findings;
use crate::conditions::Conditions;
use crate::fixture;
use crate::schema::{Schema, object};
use crate::template::{self, Decimals, Format};

/// Which state of the screen to show.
#[derive(Clone, Copy, Debug)]
pub enum State<'a> {
    /// The screen once data has arrived, with this fixture's data.
    Fixture(&'a str),
    /// A phase before data arrives, such as `loading`.
    Phase(&'a str),
}

/// The width class a responsive value is chosen by (`only`, and objects such
/// as `{"compact": 1, "regular": 2}`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Breakpoint {
    Compact,
    Regular,
}

impl Breakpoint {
    fn key(self) -> &'static str {
        match self {
            Breakpoint::Compact => "compact",
            Breakpoint::Regular => "regular",
        }
    }

    /// The breakpoint `tokens` gives `width`: the first of `breakpoint.*`
    /// whose `[min, max]` holds it (a null max is open). Compact when tokens
    /// say nothing.
    pub fn for_width(tokens: &Value, width: u32) -> Breakpoint {
        let regular = tokens.pointer("/breakpoint/regular").and_then(Value::as_array);
        let min = regular.and_then(|range| range.first()).and_then(Value::as_u64);
        match min {
            Some(min) if u64::from(width) >= min => Breakpoint::Regular,
            _ => Breakpoint::Compact,
        }
    }
}

/// One node of a screen in one state.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Node {
    pub id: String,
    pub kind: String,
    /// Every property other than the node's structure, with templates filled
    /// and cases and responsive values chosen.
    pub props: Map<String, Value>,
    /// The nodes it holds, each with the key it came from in `slot`
    /// (`children`, `leading`, `trailing`, `item`, `action`, `overlay`).
    pub children: Vec<Node>,
    pub slot: String,
    /// What the agent guessed here, for the person to confirm.
    pub assumed: Option<String>,
    /// Whether a person can act on it: it has an `on` handler or binds data.
    pub interactive: bool,
}

/// Keys that hold child nodes.
pub(crate) const SLOTS: [&str; 6] = ["children", "leading", "trailing", "item", "action", "overlay"];

/// Where an overlay floats over its node: a corner, the middle, or along
/// the bottom edge from side to side (a sheet). The first is the default.
pub const ANCHORS: [&str; 6] =
    ["bottom-trailing", "top-leading", "top-trailing", "bottom-leading", "center", "bottom"];
/// Keys that only steer how the tree is built.
const STRUCTURE: [&str; 11] =
    ["id", "kind", "ref", "if", "only", "each", "limit", "assumed", "platform", "$note", "$why"];

/// The screen `screen` in `state`, at `breakpoint`. The description is
/// expected to have passed `check`; what it cannot make sense of is an error.
pub fn instantiate(screen: &Value, state: State, breakpoint: Breakpoint) -> Result<Node, String> {
    let mut ignored = Findings::default();
    let schema = Schema::read(screen, &mut ignored);
    let conditions = Conditions::read(screen, &schema, &mut ignored);
    let (tree, data) = match state {
        State::Fixture(name) => {
            let fixtures = fixture::read(screen, &schema, &mut ignored);
            let mut found = None;
            for (fixture, data) in fixtures {
                if fixture == name {
                    found = Some(data);
                }
            }
            let data = found.ok_or_else(|| format!("there is no fixture {name}"))?;
            (screen.get("root").ok_or("the screen has no root")?, data)
        }
        State::Phase(name) => {
            let mut found = None;
            for (phase, tree) in object(screen, "phases") {
                if phase == name {
                    found = Some(tree);
                }
            }
            (found.ok_or_else(|| format!("there is no phase {name}"))?, Value::Object(Map::new()))
        }
    };
    let components = screen.get("components").and_then(Value::as_object).cloned().unwrap_or_default();
    let fill = Fill { conditions: &conditions, components: &components, breakpoint };
    let mut built = fill.node(tree, &data, "root")?;
    built.pop().ok_or_else(|| "the root is hidden in this state".to_string())
}

struct Fill<'a> {
    conditions: &'a Conditions,
    components: &'a Map<String, Value>,
    breakpoint: Breakpoint,
}

impl Fill<'_> {
    /// The nodes `written` becomes in `scope`: none when it is hidden, one
    /// otherwise.
    fn node(&self, written: &Value, scope: &Value, slot: &str) -> Result<Vec<Node>, String> {
        let Some(object) = written.as_object() else {
            return Err(format!("expected a node, found {written}"));
        };
        if let Some(Value::String(name)) = object.get("ref") {
            let component = self.components.get(name).ok_or_else(|| format!("no component {name}"))?;
            return self.node(component, scope, slot);
        }
        if let Some(Value::String(only)) = object.get("only")
            && only != self.breakpoint.key()
        {
            return Ok(Vec::new());
        }
        if let Some(Value::String(used)) = object.get("if")
            && !self.holds(used, scope)
        {
            return Ok(Vec::new());
        }
        let id = match object.get("id") {
            Some(Value::String(id)) => self.text(id, scope)?,
            _ => return Err(format!("a node has no id: {written}")),
        };
        let kind = object.get("kind").and_then(Value::as_str).unwrap_or_default().to_string();
        let mut node = Node {
            id,
            kind,
            props: Map::new(),
            children: Vec::new(),
            slot: slot.to_string(),
            assumed: object.get("assumed").and_then(Value::as_str).map(str::to_string),
            interactive: object.contains_key("on") || object.contains_key("bind"),
        };
        let each = match object.get("each") {
            Some(Value::String(path)) => Some(self.items(path, object.get("limit"), scope)?),
            _ => None,
        };
        for (key, value) in object {
            if STRUCTURE.contains(&key.as_str()) {
                continue;
            }
            if key == "item" {
                for (index, item) in each.iter().flatten().enumerate() {
                    let scope = with(scope, item, index);
                    node.children.append(&mut self.node(value, &scope, "item")?);
                }
            } else if SLOTS.contains(&key.as_str()) {
                let written = match value {
                    Value::Array(items) => items.clone(),
                    one => vec![one.clone()],
                };
                for child in &written {
                    node.children.append(&mut self.node(child, scope, key)?);
                }
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
        if let Some(items) = each
            && !object.contains_key("item")
        {
            node.props.insert("items".to_string(), Value::Array(items));
        }
        Ok(vec![node])
    }

    /// The elements `path` holds, at most `limit` of them: a count, or one
    /// per breakpoint.
    fn items(&self, path: &str, limit: Option<&Value>, scope: &Value) -> Result<Vec<Value>, String> {
        let Some(Value::Array(all)) = lookup(scope, path) else {
            return Err(format!("each: {path} is not an array here"));
        };
        let limit = match limit {
            Some(Value::Object(object)) if is_responsive(object) => object.get(self.breakpoint.key()),
            other => other,
        };
        let limit = limit.and_then(Value::as_u64).map_or(all.len(), |n| n as usize);
        let mut items = Vec::new();
        for item in all {
            if items.len() < limit {
                items.push(item.clone());
            }
        }
        Ok(items)
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
                if is_responsive(object) {
                    let chosen = object.get(self.breakpoint.key()).cloned().unwrap_or(Value::Null);
                    return self.value(&chosen, scope);
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

    /// Whether the use site `used` (`!`, then a condition or a Bool path) holds.
    fn holds(&self, used: &str, scope: &Value) -> bool {
        let (negated, name) = match used.strip_prefix('!') {
            Some(name) => (true, name),
            None => (false, used),
        };
        let holds = if self.conditions.contains(name) {
            self.conditions.holds(name, scope)
        } else {
            lookup(scope, name) == Some(&Value::Bool(true))
        };
        holds != negated
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

/// An object such as `{"compact": 1, "regular": 2}`: one value per breakpoint.
/// Whether `object` is a responsive value: keyed by breakpoints only.
pub(crate) fn is_responsive(object: &Map<String, Value>) -> bool {
    !object.is_empty() && object.keys().all(|k| k == "compact" || k == "regular")
}

/// `scope` with `item` and `index` added for one element of an `each`.
fn with(scope: &Value, item: &Value, index: usize) -> Value {
    let mut scope = scope.as_object().cloned().unwrap_or_default();
    scope.insert("item".to_string(), item.clone());
    scope.insert("index".to_string(), Value::from(index));
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
