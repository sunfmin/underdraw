//! Breakpoints, and the values chosen by them: what a breakpoint is called,
//! which one a width falls in, and how a value written once per breakpoint
//! (`{"compact": 1, "regular": 2}`) or a node's `only` is read. `check` and
//! `instantiate` both read them here, so neither can take a value the other
//! does not.

use serde::Serialize;
use serde_json::{Map, Value};

/// The width class a responsive value is chosen by (`only`, and objects such
/// as `{"compact": 1, "regular": 2}`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Breakpoint {
    Compact,
    Regular,
}

impl Breakpoint {
    /// Every breakpoint, narrowest first.
    pub const ALL: [Breakpoint; 2] = [Breakpoint::Compact, Breakpoint::Regular];

    /// Its name, as a description writes it.
    pub fn key(self) -> &'static str {
        match self {
            Breakpoint::Compact => "compact",
            Breakpoint::Regular => "regular",
        }
    }

    /// The breakpoint a description names `name`, if it names one.
    pub fn parse(name: &str) -> Option<Breakpoint> {
        Breakpoint::ALL.into_iter().find(|breakpoint| breakpoint.key() == name)
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

    /// The breakpoints' names, as a fix lists them: `compact, regular`.
    pub(crate) fn names() -> String {
        Breakpoint::ALL.map(Breakpoint::key).join(", ")
    }
}

/// Whether `object` is a value written once per breakpoint: keyed by
/// breakpoints only.
pub(crate) fn is_responsive(object: &Map<String, Value>) -> bool {
    !object.is_empty() && object.keys().all(|key| Breakpoint::parse(key).is_some())
}

/// What `value` is at `breakpoint`: its entry there when it is written once
/// per breakpoint (null when that one is missing), `value` itself otherwise.
pub(crate) fn at(value: &Value, breakpoint: Breakpoint) -> &Value {
    match value {
        Value::Object(object) if is_responsive(object) => {
            object.get(breakpoint.key()).unwrap_or(&Value::Null)
        }
        value => value,
    }
}

/// Every value `value` takes, each with where it is written below `at`:
/// one per breakpoint (`at.compact`, …) when it is written per breakpoint,
/// `value` itself at `at` otherwise.
pub(crate) fn each<'v>(value: &'v Value, at: &str) -> Vec<(String, &'v Value)> {
    match value {
        Value::Object(object) if is_responsive(object) => {
            object.iter().map(|(key, value)| (format!("{at}.{key}"), value)).collect()
        }
        value => vec![(at.to_string(), value)],
    }
}
