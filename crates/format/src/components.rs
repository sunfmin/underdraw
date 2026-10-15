//! The components a description may use and what each looks like:
//! underdraw's own, with the screen's own `"components"` over them by name.
//! The look itself is drawn by underdraw-layout; here it is read and
//! checked. Why: [ADR 0021](../../../docs/adr/0021-kinds-become-components-one-screens-or-the-projects.md),
//! and [ADR 0007](../../../docs/adr/0007-kinds-are-data.md) for the look
//! vocabulary.

use serde_json::{Map, Value};

use crate::check::Findings;
use crate::icons;
use crate::overlay::Edge;

/// The components underdraw ships, in the form a screen's own take.
const BUILTINS: &str = include_str!("../components.json");

/// What a look key takes, as far as `check` can tell before the look is
/// drawn. A placeholder, `cases` or null is filled in later and passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Takes {
    /// Anything: a length, an ink, a template, a look.
    Any,
    /// true or false.
    Bool,
    /// One of these words, each of which underdraw-layout draws.
    Word(&'static [&'static str]),
    /// Grid tracks: a count, or a list of [`Track`]s.
    Tracks,
    /// `{"x": edge, "y": edge, "inset": length}`, each edge an [`Edge`].
    Pin,
}

/// Every key a look may hold, and what it takes: what underdraw-layout
/// reads. A look with `component` holds that component's props instead.
pub const LOOK: [(&str, Takes); 43] = [
    ("if", Takes::Any),
    ("cases", Takes::Any),
    ("repeat", Takes::Any),
    ("each", Takes::Any),
    ("component", Takes::Any),
    ("children", Takes::Any),
    ("text", Takes::Any),
    ("font", Takes::Any),
    ("ink", Takes::Any),
    ("weight", Takes::Any),
    ("maxLines", Takes::Any),
    ("align", Takes::Word(&["start", "center", "end"])),
    ("underline", Takes::Bool),
    ("strike", Takes::Bool),
    ("box", Takes::Word(&["row", "column", "grid"])),
    ("frame", Takes::Bool),
    ("fill", Takes::Any),
    ("stroke", Takes::Any),
    ("radius", Takes::Any),
    ("dashed", Takes::Bool),
    ("cross", Takes::Any),
    ("topLine", Takes::Any),
    ("clip", Takes::Any),
    ("gap", Takes::Any),
    ("padding", Takes::Any),
    ("margin", Takes::Any),
    ("size", Takes::Any),
    ("width", Takes::Any),
    ("height", Takes::Any),
    ("aspect", Takes::Any),
    ("grow", Takes::Any),
    ("shrink", Takes::Any),
    ("basis", Takes::Any),
    ("self", Takes::Word(&["start"])),
    ("items", Takes::Word(&["start", "center"])),
    ("justify", Takes::Word(&["center", "between", "end"])),
    ("columns", Takes::Tracks),
    ("rows", Takes::Tracks),
    ("span", Takes::Any),
    ("overflowX", Takes::Word(&["clip"])),
    ("pin", Takes::Pin),
    ("backdrop", Takes::Bool),
    ("itemWidth", Takes::Any),
];

/// What `key` takes in a look, when a look reads it.
pub fn takes(key: &str) -> Option<Takes> {
    LOOK.into_iter().find(|(name, _)| *name == key).map(|(_, takes)| takes)
}

/// Keys a look may also hold, read by people and not drawn.
const NOTES: [&str; 2] = ["$comment", "doc"];

/// What a component may say of itself beside its look, each true or false: how a
/// platform treats it, which `compare` reads.
const FLAGS: [&str; 2] = ["scrolls", "pinned"];

/// The filters a placeholder in a look may apply.
pub const FILTERS: [&str; 10] =
    ["or", "count", "first", "last", "head", "tail", "slashes", "glyph", "atLeast", "atMost"];

/// One grid track, in `columns` or `rows`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Track {
    /// "2fr": a share of the width left over.
    Fr(f32),
    /// "40%": a part of the parent, 0 to 1.
    Part(f32),
    /// 48: a width in points.
    Points(f32),
}

impl Track {
    /// `value` as a track, or None when it is not one ("48", "wide").
    pub fn read(value: &Value) -> Option<Track> {
        if let Some(points) = value.as_f64() {
            return (points >= 0.0).then_some(Track::Points(points as f32));
        }
        let text = value.as_str()?;
        let (number, track): (&str, fn(f32) -> Track) =
            match (text.strip_suffix("fr"), text.strip_suffix('%')) {
                (Some(share), _) => (share, Track::Fr),
                (_, Some(percent)) => (percent, |percent| Track::Part(percent / 100.0)),
                _ => return None,
            };
        let number = number.parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.0)?;
        Some(track(number))
    }
}

/// Every entry of `tracks`, at `at`, that is not a track. A count is not a
/// list and passes; a placeholder is filled later and passes.
pub(crate) fn track_faults(tracks: &Value, at: &str, findings: &mut Findings) {
    let Value::Array(tracks) = tracks else {
        return;
    };
    for (index, track) in tracks.iter().enumerate() {
        let template = track.as_str().is_some_and(|text| text.contains('{'));
        if !template && Track::read(track).is_none() {
            findings.add(
                &format!("{at}[{index}]"),
                &format!("{track} is not a grid track: write \"2fr\", \"40%\" or a number of points, 48"),
            );
        }
    }
}

/// Whether a box in a look asks for a backdrop and does not float by its
/// own `pin`: only a component's whole look, put in an `overlay`, may.
fn backdrop_without_pin(look: &Map<String, Value>) -> bool {
    look.get("backdrop") == Some(&Value::Bool(true)) && !look.contains_key("pin")
}

/// Every fault in `value`, at `at`, as what look key `key` takes. A
/// placeholder, `cases` (each branch in turn) or null is decided later and
/// passes.
fn value_faults(takes: Takes, key: &str, value: &Value, at: &str, findings: &mut Findings) {
    if let Value::Object(object) = value
        && let Some(Value::Array(branches)) = object.get("cases")
    {
        for (index, branch) in branches.iter().enumerate() {
            for side in ["then", "else"] {
                if let Some(value) = branch.get(side) {
                    value_faults(takes, key, value, &format!("{at}.cases[{index}].{side}"), findings);
                }
            }
        }
        return;
    }
    if value.is_null() || value.as_str().is_some_and(|text| text.contains('{')) {
        return;
    }
    match takes {
        Takes::Any => {}
        Takes::Bool if value.is_boolean() => {}
        Takes::Bool => findings.add(at, &format!("{key} is true or false")),
        Takes::Word(words) if value.as_str().is_some_and(|word| words.contains(&word)) => {}
        Takes::Word(words) => {
            findings.add(at, &format!("{value} is not a {key}: write {}", words.join(", ")))
        }
        Takes::Tracks => track_faults(value, at, findings),
        Takes::Pin => pin_faults(value, at, findings),
    }
}

/// Every fault in a box's `pin`, at `at`.
fn pin_faults(pin: &Value, at: &str, findings: &mut Findings) {
    let Value::Object(pin) = pin else {
        return findings.add(at, "pin is {\"x\": …, \"y\": …, \"inset\": …}");
    };
    let edges = Edge::ALL.map(Edge::key).join(", ");
    for (key, value) in pin {
        match key.as_str() {
            "x" | "y" => {
                if !value.as_str().is_some_and(|edge| edge.contains('{') || Edge::parse(edge).is_some()) {
                    findings.add(&format!("{at}.{key}"), &format!("{value} is not an edge: write {edges}"));
                }
            }
            "inset" => {}
            _ => findings.add(&format!("{at}.{key}"), "a pin holds x, y and inset"),
        }
    }
}

/// The names a `repeat` binds for its `each`, which no component declares.
const REPEATED: [&str; 3] = ["item", "index", "n"];

/// The props a `frame` takes from its node, as its component's params.
const FRAMED: [&str; 5] = ["padding", "gap", "radius", "border", "elevation"];

/// Every component a screen may use: underdraw's own, with the screen's
/// own look components (its `"components"` that have a `"look"`) over them
/// by name. A component whose body is nodes is not here: a `ref` inlines it.
pub struct Components {
    components: Map<String, Value>,
}

impl Components {
    /// underdraw's components, with `screen`'s own look components over
    /// them by name.
    pub fn read(screen: &Value) -> Components {
        let mut components = Components { components: Map::new() };
        if let Ok(builtins) = serde_json::from_str::<Value>(BUILTINS) {
            components.add(&builtins);
        }
        components.add(screen);
        components
    }

    /// Each entry of `file`'s `"components"` that has a look.
    fn add(&mut self, file: &Value) {
        if let Some(Value::Object(components)) = file.get("components") {
            for (name, component) in components {
                if component.get("look").is_some() {
                    self.components.insert(name.clone(), component.clone());
                }
            }
        }
    }

    pub fn has(&self, component: &str) -> bool {
        self.components.contains_key(component)
    }

    /// What `component` looks like, when it is one.
    pub fn look(&self, component: &str) -> Option<&Value> {
        self.components.get(component).and_then(|entry| entry.get("look"))
    }

    /// Whether `component` says `"scrolls": true`: its content scrolls or pages
    /// on a platform, so what is in it may lie outside its frame.
    pub fn scrolls(&self, component: &str) -> bool {
        self.flag(component, "scrolls")
    }

    /// Whether `component` says `"pinned": true`: a platform pins it to the
    /// screen, out of the flow of its siblings.
    pub fn pinned(&self, component: &str) -> bool {
        self.flag(component, "pinned")
    }

    fn flag(&self, component: &str, key: &str) -> bool {
        self.components.get(component).and_then(|entry| entry.get(key)) == Some(&Value::Bool(true))
    }

    /// Whether `component`'s whole look has a backdrop, which draws only when its
    /// node is in an `overlay`.
    pub fn backdrop(&self, component: &str) -> bool {
        self.look(component).and_then(Value::as_object).is_some_and(backdrop_without_pin)
    }

    /// Every fault in `screen`'s own components, at `components.<Name>…`:
    /// one with both a look and nodes or neither; in a look, a key no look
    /// reads, a filter or component that does not exist, and a name it
    /// reads that is not one of its declared params.
    pub(crate) fn check(&self, screen: &Value, findings: &mut Findings) {
        if screen.get("icons").is_some() {
            findings.add(
                "icons",
                "icons are Material Symbols names (fonts.google.com/icons) and need no entry; take this out",
            );
        }
        let Some(Value::Object(components)) = screen.get("components") else {
            if screen.get("components").is_some() {
                findings.add(
                    "components",
                    "\"components\" maps each component's name to {\"params\": […], \"look\": …} or {\"nodes\": …}",
                );
            }
            return;
        };
        for (name, component) in components {
            let at = format!("components.{name}");
            match (component.get("look"), component.get("nodes")) {
                (Some(_), Some(_)) => findings.add(
                    &at,
                    "a component's body is a look or nodes, never both: draw it as one element with \"look\", or as a subtree with \"nodes\"",
                ),
                (None, None) => findings.add(
                    &at,
                    "a component needs a body: \"look\" (what it is drawn as) or \"nodes\" (the subtree it is)",
                ),
                (Some(look), None) => self.look_component(component, look, &at, findings),
                (None, Some(_)) => {}
            }
        }
    }

    /// Every fault in the look component `component`, whose look is `look`.
    fn look_component(&self, component: &Value, look: &Value, at: &str, findings: &mut Findings) {
        if component.get("assumed").is_some() {
            findings.add(&format!("{at}.assumed"), "underdraw draws no guesses: finish this component's look and take assumed out; a note for people goes in \"doc\"");
        }
        for flag in FLAGS {
            if let Some(value) = component.get(flag)
                && !value.is_boolean()
            {
                findings.add(&format!("{at}.{flag}"), &format!("{flag} is true or false"));
            }
        }
        self.look_faults(look, &format!("{at}.look"), true, findings);
        let params = match component.get("params") {
            None => Vec::new(),
            Some(Value::Array(names)) if names.iter().all(Value::is_string) => {
                names.iter().filter_map(Value::as_str).collect()
            }
            Some(_) => {
                findings.add(
                    &format!("{at}.params"),
                    "params is a list of the names the look reads: [\"text\", \"icon\"]",
                );
                return;
            }
        };
        let mut read = Vec::new();
        reads(look, &[], &mut read);
        let mut named: Vec<&str> = Vec::new();
        for name in &read {
            if !params.contains(&name.as_str()) && !named.contains(&name.as_str()) {
                named.push(name.as_str());
            }
        }
        for name in named {
            findings.add(
                &format!("{at}.look"),
                &format!(
                    "reads {name}, which is not one of its params: add \"{name}\" to \"params\", or read one it declares"
                ),
            );
        }
    }

    /// `top`: `look` is what the component's node is drawn as, which an overlay
    /// pins; anything inside it floats only by its own `pin`.
    fn look_faults(&self, look: &Value, at: &str, top: bool, findings: &mut Findings) {
        match look {
            Value::String(text) if text.starts_with('$') => {
                if text != "$children" {
                    findings
                        .add(at, &format!("{text} is not a look; \"$children\" places the node's children"));
                }
            }
            Value::String(text) => self.template_faults(text, at, findings),
            Value::Array(looks) => {
                for (index, look) in looks.iter().enumerate() {
                    self.look_faults(look, &format!("{at}[{index}]"), false, findings);
                }
            }
            Value::Object(object) => {
                let nested = object.get("component").and_then(Value::as_str);
                if !top && backdrop_without_pin(object) {
                    findings.add(
                        at,
                        "backdrop is drawn only by a box that floats: the whole look of a component put in \"overlay\", or a box with \"pin\"",
                    );
                }
                if let Some(name) = nested
                    && !self.has(name)
                {
                    findings.add(
                        at,
                        &format!(
                            "component {name} is not defined in the screen's components or underdraw's own"
                        ),
                    );
                }
                for (key, value) in object {
                    if key == "cases" {
                        self.branch_faults(value, &format!("{at}.cases"), top, findings);
                        continue;
                    }
                    if nested.is_none() && !NOTES.contains(&key.as_str()) {
                        match takes(key) {
                            Some(takes) => value_faults(takes, key, value, &format!("{at}.{key}"), findings),
                            None => findings.add(
                                at,
                                &format!(
                                    "{key} is not a key a look reads; see `underdraw check --help`, LOOKS"
                                ),
                            ),
                        }
                    }
                    if nested.is_some()
                        && icons::is_icon_key(key)
                        && let Value::String(icon) = value
                        && !icon.contains('{')
                        && !icons::known(icon)
                    {
                        findings.add(&format!("{at}.{key}"), &icons::unknown(icon));
                    }
                    if key != "pin" && !NOTES.contains(&key.as_str()) {
                        self.look_faults(value, &format!("{at}.{key}"), false, findings);
                    }
                }
            }
            _ => {}
        }
    }

    /// `cases`: a list of `{"if", "then"}`, ending in `{"else"}`.
    fn branch_faults(&self, branches: &Value, at: &str, top: bool, findings: &mut Findings) {
        let Value::Array(branches) = branches else {
            return findings.add(at, "cases is a list of {\"if\", \"then\"} ending in {\"else\"}");
        };
        if !branches.last().is_some_and(|branch| branch.get("else").is_some()) {
            findings.add(at, "the last branch of cases must be {\"else\": …}");
        }
        for (index, branch) in branches.iter().enumerate() {
            let at = format!("{at}[{index}]");
            for key in ["then", "else"] {
                if let Some(value) = branch.get(key) {
                    self.look_faults(value, &format!("{at}.{key}"), top, findings);
                }
            }
        }
    }

    fn template_faults(&self, text: &str, at: &str, findings: &mut Findings) {
        let mut rest = text;
        while let Some((_, after)) = rest.split_once('{') {
            let Some((inner, next)) = after.split_once('}') else {
                break;
            };
            for filter in inner.split('|').skip(1) {
                let name = filter.split_once(':').map_or(filter, |(name, _)| name).trim();
                if !name.is_empty() && !name.contains(['&', '=', '!']) && !FILTERS.contains(&name) {
                    findings.add(at, &format!("{name} is not a filter; there are {}", FILTERS.join(", ")));
                }
            }
            rest = next;
        }
    }
}

/// Each name `look` reads from its component's params, in the order read,
/// leaving out those `bound` (what a `repeat` around it binds): the first
/// step of each placeholder's path, each name a condition reads, and the
/// props a `frame` takes.
fn reads(look: &Value, bound: &[&str], read: &mut Vec<String>) {
    match look {
        Value::String(text) if text.starts_with('$') => {}
        Value::String(text) => {
            let mut rest = text.as_str();
            while let Some((_, after)) = rest.split_once('{') {
                let Some((inner, next)) = after.split_once('}') else {
                    break;
                };
                let path = inner.split('|').next().unwrap_or_default();
                read_name(path, bound, read);
                rest = next;
            }
        }
        Value::Array(looks) => {
            for look in looks {
                reads(look, bound, read);
            }
        }
        Value::Object(object) => {
            if object.get("frame") == Some(&Value::Bool(true)) && !object.contains_key("component") {
                for name in FRAMED {
                    read_name(name, bound, read);
                }
            }
            for (key, value) in object {
                match (key.as_str(), value) {
                    ("if", Value::String(condition)) => condition_reads(condition, bound, read),
                    ("cases", Value::Array(branches)) => {
                        for branch in branches {
                            if let Some(Value::String(condition)) = branch.get("if") {
                                condition_reads(condition, bound, read);
                            }
                            for side in ["then", "else"] {
                                if let Some(value) = branch.get(side) {
                                    reads(value, bound, read);
                                }
                            }
                        }
                    }
                    ("each", each) if object.contains_key("repeat") => {
                        let mut inner = bound.to_vec();
                        inner.extend(REPEATED);
                        reads(each, &inner, read);
                    }
                    ("component", _) => {}
                    (key, _) if NOTES.contains(&key) => {}
                    (_, value) => reads(value, bound, read),
                }
            }
        }
        _ => {}
    }
}

/// Each name the look condition `condition` reads (`value && value !=
/// ""`): every word that is not a literal, as a placeholder's path.
fn condition_reads(condition: &str, bound: &[&str], read: &mut Vec<String>) {
    let mut chars = condition.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
            }
        } else if c.is_ascii_digit() || c == '-' {
            while chars.next_if(|d| d.is_ascii_digit() || *d == '.').is_some() {}
        } else if c.is_alphanumeric() || c == '_' || c == '$' {
            let mut word = c.to_string();
            while let Some(d) = chars.next_if(|d| d.is_alphanumeric() || "_.:%$".contains(*d)) {
                word.push(d);
            }
            // A filter (`value|first`) is part of its placeholder; `||` is "or".
            while chars.peek() == Some(&'|') && chars.clone().nth(1) != Some('|') {
                chars.next();
                while chars.next_if(|d| d.is_alphanumeric() || "_.:%$".contains(*d)).is_some() {}
            }
            if !["true", "false", "null"].contains(&word.as_str()) {
                read_name(&word, bound, read);
            }
        }
    }
}

/// The first step of `path` added to `read`, unless `bound` holds it or it
/// is a `$` name.
fn read_name(path: &str, bound: &[&str], read: &mut Vec<String>) {
    let name = path.trim().split('.').next().unwrap_or_default();
    if !name.is_empty() && !name.starts_with('$') && !bound.contains(&name) {
        read.push(name.to_string());
    }
}
