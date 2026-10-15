//! The components a description may use and what each is: underdraw's
//! own, with the project's shared ones and the screen's own
//! `"components"` over them by name, each a look or nodes, in one table
//! read once with the screen. The look itself is drawn by
//! underdraw-layout; here it is read and checked. Why:
//! [ADR 0021](../../../docs/adr/0021-kinds-become-components-one-screens-or-the-projects.md),
//! and [ADR 0007](../../../docs/adr/0007-kinds-are-data.md) for the look
//! vocabulary.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::{Map, Value};

use crate::check::Findings;
use crate::icons;
use crate::look::{
    Cases, Condition, Filter, Flow, Items, Justify, Look, NOTES, Overflow, SelfAlign, TextAlign,
};
use crate::overlay::Edge;
use crate::schema::object;
use crate::subtrees::{Subtree, Subtrees};

/// The components underdraw ships, in the form a screen's own take.
const BUILTINS: &str = include_str!("../components.json");

/// The first step of where a node of a component the project shares is
/// written ([`Node::written_at`](crate::Node::written_at)):
/// `/$components/OrderRow/nodes/…`.
const SHARED: &str = "$components";

/// Where the nodes of the project's component `name` are written:
/// `/$components/OrderRow`, and then its `nodes/…`.
pub(crate) fn shared_at(name: &str) -> String {
    format!("/{SHARED}/{name}")
}

/// Where the node written at `at` (a [`Node::written_at`](crate::Node::written_at))
/// is, when it is one of a component the project shares: the component's
/// name, and the pointer to it inside its `components/<Name>.json`
/// (`nodes/children/1`, without the leading `/`).
pub fn in_shared(at: &str) -> Option<(&str, &str)> {
    let inside = at.strip_prefix('/')?.strip_prefix(SHARED)?.strip_prefix('/')?;
    inside.split_once('/')
}

/// Components whose body is a look, by name: each entry (`look`,
/// `params`, `scrolls`…) with its look parsed.
#[derive(Default)]
struct Looks {
    entries: Map<String, Value>,
    looks: BTreeMap<String, Look>,
}

impl Looks {
    /// `component`, named `name`, when it has a look.
    fn add(&mut self, name: &str, component: &Value) {
        if let Some(look) = component.get("look") {
            self.entries.insert(name.to_string(), component.clone());
            self.looks.insert(name.to_string(), Look::parse(look));
        }
    }
}

/// underdraw's own components, parsed once for the process.
fn builtins() -> &'static Looks {
    static PARSED: OnceLock<Looks> = OnceLock::new();
    PARSED.get_or_init(parse_builtins)
}

fn parse_builtins() -> Looks {
    #[cfg(test)]
    tests::BUILTINS_PARSED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut builtins = Looks::default();
    if let Ok(parsed) = serde_json::from_str::<Value>(BUILTINS) {
        for (name, component) in object(&parsed, "components") {
            builtins.add(name, component);
        }
    }
    builtins
}

/// One component a screen may use besides underdraw's: one the project
/// shares, or the screen's own.
pub(crate) struct Declared {
    pub name: String,
    pub body: Value,
    /// Whether the project shares it, in its `components/<Name>.json`.
    pub shared: bool,
    /// Where findings in it are named: `components/OrderRow.json` for the
    /// project's, `components.OrderRow` for the screen's own.
    pub at: String,
}

/// Every component the description `screen` declares, with the ones its
/// project `shared`: the project's first, then the screen's own. One of
/// the screen's own of a name the project shares is left out and the
/// project's is used; [`Components::check`] names it.
fn declared(screen: &Value, shared: &Map<String, Value>) -> Vec<Declared> {
    let mut all = Vec::new();
    for (name, body) in shared {
        let at = format!("components/{name}.json");
        all.push(Declared { name: name.clone(), body: body.clone(), shared: true, at });
    }
    for (name, body) in object(screen, "components") {
        if !shared.contains_key(name) {
            let at = format!("components.{name}");
            all.push(Declared { name: name.clone(), body: body.clone(), shared: false, at });
        }
    }
    all
}

/// What a look key takes, as far as `check` can tell before the look is
/// drawn. A placeholder, `cases` or null is filled in later and passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Takes {
    /// Anything: a length, an ink, a template, a look.
    Any,
    /// true or false.
    Bool,
    /// One of these words: those the key's type in [`crate::look`] reads,
    /// which underdraw-layout draws.
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
    ("align", Takes::Word(TextAlign::WORDS)),
    ("underline", Takes::Bool),
    ("strike", Takes::Bool),
    ("box", Takes::Word(Flow::WORDS)),
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
    ("self", Takes::Word(SelfAlign::WORDS)),
    ("items", Takes::Word(Items::WORDS)),
    ("justify", Takes::Word(Justify::WORDS)),
    ("columns", Takes::Tracks),
    ("rows", Takes::Tracks),
    ("span", Takes::Any),
    ("overflowX", Takes::Word(Overflow::WORDS)),
    ("pin", Takes::Pin),
    ("backdrop", Takes::Bool),
    ("itemWidth", Takes::Any),
];

/// What `key` takes in a look, when a look reads it.
pub fn takes(key: &str) -> Option<Takes> {
    LOOK.into_iter().find(|(name, _)| *name == key).map(|(_, takes)| takes)
}

/// What a component may say of itself beside its look, each true or false: how a
/// platform treats it, which `compare` reads.
const FLAGS: [&str; 2] = ["scrolls", "pinned"];

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

/// What a component's body is.
pub enum Body<'c> {
    /// A look: drawn as one element.
    Look(&'c Look),
    /// Nodes: a subtree, each node in it named by the instance's id joined
    /// to its own.
    Nodes(&'c Subtree),
}

/// Every component a screen may use, in one table read once with the
/// screen: underdraw's own, with those the screen [`declared`] (the
/// project's and its own) over them by name, each a look or nodes.
pub struct Components {
    /// Each declared component, the project's first, as [`declared`] lists
    /// them; what [`Components::check`] reads.
    declared: Vec<Declared>,
    /// The declared components whose body is a look: over underdraw's own
    /// of the same name.
    looks: Looks,
    /// The declared components whose body is nodes.
    subtrees: Subtrees,
}

impl Components {
    /// underdraw's components, with those the description `screen`
    /// declares and those its project `shared` over them by name. Each
    /// fault in them is added to `findings`, as [`Components::check`] says.
    pub(crate) fn read(screen: &Value, shared: &Map<String, Value>, findings: &mut Findings) -> Components {
        #[cfg(test)]
        crate::screen::tests::count("components");
        let declared = declared(screen, shared);
        let mut looks = Looks::default();
        for Declared { name, body, .. } in &declared {
            looks.add(name, body);
        }
        let subtrees = Subtrees::read(&declared);
        let components = Components { declared, looks, subtrees };
        components.check(screen, shared, findings);
        components
    }

    /// Which table holds `component`, when its body is a look: the
    /// screen's, or underdraw's own.
    fn with_look(&self, component: &str) -> Option<&Looks> {
        [&self.looks, builtins()].into_iter().find(|looks| looks.looks.contains_key(component))
    }

    /// What `component`'s body is, when it is one the screen may use. One
    /// declared with nodes under a name underdraw's own has is nodes.
    pub fn body(&self, component: &str) -> Option<Body<'_>> {
        match self.subtrees.get(component) {
            Some(subtree) => Some(Body::Nodes(subtree)),
            None => self.look(component).map(Body::Look),
        }
    }

    /// Whether `component` is one whose body is a look.
    pub fn has(&self, component: &str) -> bool {
        self.with_look(component).is_some()
    }

    /// The components whose body is nodes.
    pub fn subtrees(&self) -> &Subtrees {
        &self.subtrees
    }

    /// Each component the screen declares, the project's first.
    pub(crate) fn declared(&self) -> &[Declared] {
        &self.declared
    }

    /// What `component` looks like, when it is one.
    pub fn look(&self, component: &str) -> Option<&Look> {
        self.with_look(component).and_then(|looks| looks.looks.get(component))
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
        let entry = self.with_look(component).and_then(|looks| looks.entries.get(component));
        entry.and_then(|entry| entry.get(key)) == Some(&Value::Bool(true))
    }

    /// Whether `component`'s whole look has a backdrop, which draws only when its
    /// node is in an `overlay`.
    pub fn backdrop(&self, component: &str) -> bool {
        matches!(self.look(component), Some(Look::Object(look)) if look.backdrop_without_pin())
    }

    /// Every fault in the components the description `screen` declares,
    /// with those its project `shared`, each at where [`declared`] says:
    /// one of the screen's own of a name the project shares, or with
    /// fixtures, which only a shared one is shown in; one with both a look
    /// and nodes or neither; in a look, a key no look reads, a filter or
    /// component that does not exist, and a name it reads that is not one
    /// of its declared params.
    fn check(&self, screen: &Value, shared: &Map<String, Value>, findings: &mut Findings) {
        if screen.get("icons").is_some() {
            findings.add(
                "icons",
                "icons are Material Symbols names (fonts.google.com/icons) and need no entry; take this out",
            );
        }
        if screen.get("components").is_some_and(|own| !own.is_object()) {
            findings.add(
                "components",
                "\"components\" maps each component's name to {\"params\": […], \"look\": …} or {\"nodes\": …}",
            );
        }
        for (name, _) in object(screen, "components") {
            if shared.contains_key(name) {
                findings.add(
                    &format!("components.{name}"),
                    &format!(
                        "shadows the project's {name}, shared in components/{name}.json: use the project's by name, or give the screen's own another name"
                    ),
                );
            }
        }
        for Declared { name, body: component, at, shared } in &self.declared {
            if !shared && component.get("fixtures").is_some() {
                findings.add(
                    &format!("{at}.fixtures"),
                    "only a component the project shares is shown on its own, in its fixtures: move it to components/<Name>.json at the project's root, or take fixtures out",
                );
            }
            match (component.get("look"), component.get("nodes")) {
                (Some(_), Some(_)) => findings.add(
                    at,
                    "a component's body is a look or nodes, never both: draw it as one element with \"look\", or as a subtree with \"nodes\"",
                ),
                (None, None) => findings.add(
                    at,
                    "a component needs a body: \"look\" (what it is drawn as) or \"nodes\" (the subtree it is)",
                ),
                (Some(_), None) => {
                    if let Some(look) = self.look(name) {
                        self.look_component(component, look, at, findings)
                    }
                }
                (None, Some(_)) => {}
            }
        }
        for Declared { name, body, at, .. } in &self.declared {
            if body.get("nodes").is_none()
                && let Some(cycle) = self.cycle(name, &mut vec![name.to_string()])
            {
                findings.add(
                    &format!("{at}.look"),
                    &format!("component {name} uses itself: {}", cycle.join(" → ")),
                );
            }
        }
    }

    /// The components `through` ends with, when its last one's look nests
    /// its first, at any depth: that path, back to the first; or none.
    fn cycle(&self, first: &str, through: &mut Vec<String>) -> Option<Vec<String>> {
        let mut nested = Vec::new();
        if let Some(look) = through.last().and_then(|last| self.look(last)) {
            look.nests(&mut nested);
        }
        for name in nested {
            if name == first {
                let mut cycle = through.clone();
                cycle.push(name);
                return Some(cycle);
            }
            if through.contains(&name) {
                continue;
            }
            through.push(name);
            let found = self.cycle(first, through);
            through.pop();
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// Every fault in the look component `component`, whose look is `look`.
    fn look_component(&self, component: &Value, look: &Look, at: &str, findings: &mut Findings) {
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
        look.reads(&[], &mut read);
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
    fn look_faults(&self, look: &Look, at: &str, top: bool, findings: &mut Findings) {
        match look {
            Look::Text { written, .. } if written.starts_with('$') => {
                findings
                    .add(at, &format!("{written} is not a look; \"$children\" places the node's children"));
            }
            Look::Text { template, .. } => {
                for placeholder in template.placeholders() {
                    filter_faults(&placeholder.filters, at, findings);
                }
            }
            Look::List(looks) => {
                for (index, look) in looks.iter().enumerate() {
                    self.look_faults(look, &format!("{at}[{index}]"), false, findings);
                }
            }
            Look::Object(object) => {
                let nested = object.component();
                if !top && object.backdrop_without_pin() {
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
                            "component {name} is not defined in the screen's components, the project's components/ or underdraw's own"
                        ),
                    );
                }
                if let Some(condition) = &object.when {
                    condition_faults(condition, &format!("{at}.if"), findings);
                }
                for key in &object.keys {
                    let (name, value) = (key.name.as_str(), &key.written);
                    if nested.is_none() && !NOTES.contains(&name) {
                        match takes(name) {
                            Some(takes) => {
                                value_faults(takes, name, value, &format!("{at}.{name}"), findings)
                            }
                            None => findings.add(
                                at,
                                &format!(
                                    "{name} is not a key a look reads; see `underdraw check --help`, LOOKS"
                                ),
                            ),
                        }
                    }
                    if nested.is_some()
                        && icons::is_icon_key(name)
                        && let Value::String(icon) = value
                        && !icon.contains('{')
                        && !icons::known(icon.as_str())
                    {
                        findings.add(&format!("{at}.{name}"), &icons::unknown(icon.as_str()));
                    }
                    if name != "pin" && !NOTES.contains(&name) {
                        self.look_faults(&key.look, &format!("{at}.{name}"), false, findings);
                    }
                }
                if let Some(cases) = &object.cases {
                    self.branch_faults(cases, &format!("{at}.cases"), top, findings);
                }
            }
            Look::Children | Look::Plain(_) => {}
        }
    }

    /// `cases`: a list of `{"if", "then"}`, ending in `{"else"}`.
    fn branch_faults(&self, cases: &Cases, at: &str, top: bool, findings: &mut Findings) {
        if !cases.listed {
            return findings.add(at, "cases is a list of {\"if\", \"then\"} ending in {\"else\"}");
        }
        if !cases.branches.last().is_some_and(|branch| branch.otherwise.is_some()) {
            findings.add(at, "the last branch of cases must be {\"else\": …}");
        }
        for (index, branch) in cases.branches.iter().enumerate() {
            let at = format!("{at}[{index}]");
            if let Some(condition) = &branch.when {
                condition_faults(condition, &format!("{at}.if"), findings);
            }
            for (key, side) in [("then", &branch.then), ("else", &branch.otherwise)] {
                if let Some(look) = side {
                    self.look_faults(look, &format!("{at}.{key}"), top, findings);
                }
            }
        }
    }
}

/// Every fault in a look's condition, at `at`: what is not the grammar of
/// a screen's condition, and each filter that does not exist.
fn condition_faults(condition: &Condition, at: &str, findings: &mut Findings) {
    if let Err(why) = &condition.parsed {
        findings.add(
            at,
            &format!(
                "{} is not a condition: {why}; a look's condition is written as a screen's, its names placeholders such as value|first",
                condition.written
            ),
        );
    }
    for placeholder in condition.placeholders() {
        filter_faults(&placeholder.filters, at, findings);
    }
}

/// Each filter in `filters`, at `at`, that does not exist.
fn filter_faults(filters: &[Filter], at: &str, findings: &mut Findings) {
    for filter in filters {
        if let Filter::Unknown(name) = filter
            && !name.is_empty()
            && !name.contains(['&', '=', '!'])
        {
            findings.add(at, &format!("{name} is not a filter; there are {}", Filter::names().join(", ")));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// How many times underdraw's own components were parsed in this
    /// process.
    pub(crate) static BUILTINS_PARSED: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn underdraws_own_are_parsed_once_for_the_process() {
        for _ in 0..3 {
            let mut findings = Findings::default();
            let components = Components::read(&serde_json::json!({}), &Map::new(), &mut findings);
            assert!(components.has("Button"));
        }
        assert!(builtins().looks.contains_key("Button"));
        assert_eq!(BUILTINS_PARSED.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_node_of_a_shared_component_is_written_in_its_file() {
        let at = format!("{}/nodes/children/1", shared_at("OrderRow"));
        assert_eq!(in_shared(&at), Some(("OrderRow", "nodes/children/1")));
        assert_eq!(in_shared("/root/children/1"), None);
    }
}
