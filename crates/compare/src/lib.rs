//! How a running UI differs from the description, in one state: the
//! elements a platform's UI test tools read off it (a capture) against what
//! the handoff says that state shows, and the order and nesting the
//! wireframe of that state has.

// Loops, not iterator adapters, which Flux does not check.
#![allow(clippy::manual_find)]

extern crate flux_alloc;
extern crate flux_core;

mod capture;
mod cli;

use serde::Serialize;
use serde_json::{Map, Value};
use underdraw_format::{Breakpoint, Components, Node, State, instantiate};
use underdraw_handoff::{Expected, handoff};
use underdraw_layout::{Measure, Page, Rect, layout};

pub use capture::{Acted, Call, Capture, Element};
pub use cli::{CliChecked, CliComparison, compare_cli};

#[derive(Debug, Serialize)]
pub struct Comparison {
    pub screen: String,
    pub platform: String,
    /// True when the capture shows what the description says, as far as
    /// the capture tells.
    pub ok: bool,
    pub state: Value,
    pub breakpoint: Breakpoint,
    pub width: f32,
    /// How many of each check were made: a check needs what it reads in the
    /// capture (a frame, text, `enabled`), and is skipped without it.
    pub checked: Checked,
    pub findings: Vec<Finding>,
    /// Ids in the capture that no node of the description has, in any state.
    pub unknown: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Checked {
    pub present: u32,
    pub absent: u32,
    pub text: u32,
    pub enabled: u32,
    pub order: u32,
    pub inside: u32,
    /// Interactions whose calls were checked.
    pub calls: u32,
}

/// One difference, at the node it is about.
#[derive(Debug, Serialize)]
pub struct Finding {
    pub at: String,
    /// Which check found it: `present`, `absent`, `duplicate`, `text`,
    /// `enabled`, `order`, `inside`, `call-missing`, `call-extra` or
    /// `call-with`; of a CLI's run, `exit` or `stdout`.
    pub check: &'static str,
    pub problem: String,
}

/// How far a real frame may stray, in points, before it counts: frames are
/// rounded differently on every platform.
const SLACK: f32 = 1.0;

/// How `capture` differs from `screen` in `state` on the platform `mapping`
/// describes. The wireframe is laid out at the capture's width, text
/// measured by `measure`, each component drawn as the screen's own components, over underdraw's, say. Refused, with why: a description with faults, a
/// capture that is not one, a state not shown at that width.
pub fn compare(
    screen: &Value,
    mapping: &Value,
    tokens: &Value,
    state: State,
    capture: &Value,
    measure: &mut dyn Measure,
) -> Result<Comparison, String> {
    let handed = match handoff(screen, mapping, tokens) {
        Ok(handed) => handed,
        Err(checked) => {
            return Err(format!(
                "the description has {} finding(s), so it cannot say what the UI should show: run `underdraw check` and fix them first",
                checked.findings.len()
            ));
        }
    };
    let capture = Capture::read(capture).map_err(|faults| faults.join("; "))?;
    let breakpoint = Breakpoint::for_width(tokens, capture.width as u32);
    let named = match state {
        State::Fixture(name) => object("fixture", name),
        State::Phase(name) => object("phase", name),
    };
    let mut expected = None;
    for candidate in &handed.states {
        if candidate.state == named && candidate.breakpoint == breakpoint {
            expected = Some(candidate);
        }
    }
    let Some(expected) = expected else {
        return Err(format!("the description does not show {named} at the {breakpoint:?} breakpoint"));
    };
    let root = instantiate(screen, state, breakpoint)?;
    let page = layout(&root, tokens, screen, capture.width, underdraw_format::Lang::default(), measure)?;

    let mut known = Vec::new();
    for state in &handed.states {
        for id in &state.present {
            known.push(id.clone());
        }
    }
    ids(&root, &mut known);

    let mut differ = Differ {
        capture: &capture,
        expected,
        page: &page,
        components: &Components::read(screen),
        platform: &handed.platform,
        checked: Checked::default(),
        findings: Vec::new(),
    };
    differ.presence(&known);
    differ.shows();
    differ.geometry(&root, None, false);
    differ.calls();
    let mut unknown = Vec::new();
    for element in &capture.elements {
        if !known.contains(&element.id) && !unknown.contains(&element.id) {
            unknown.push(element.id.clone());
        }
    }
    Ok(Comparison {
        screen: handed.screen,
        platform: handed.platform.clone(),
        ok: differ.findings.is_empty(),
        state: named,
        breakpoint,
        width: capture.width,
        checked: differ.checked,
        findings: differ.findings,
        unknown,
    })
}

struct Differ<'a> {
    capture: &'a Capture,
    expected: &'a Expected,
    page: &'a Page,
    /// Which components scroll, and which are pinned out of the flow.
    components: &'a Components,
    platform: &'a str,
    checked: Checked,
    findings: Vec<Finding>,
}

impl Differ<'_> {
    fn find(&mut self, at: &str, check: &'static str, problem: String) {
        self.findings.push(Finding { at: at.to_string(), check, problem });
    }

    /// Every node the state shows is in the capture, once; no node it does
    /// not show is.
    fn presence(&mut self, known: &[String]) {
        let present = &self.expected.present;
        for id in present {
            self.checked.present = self.checked.present.saturating_add(1);
            let mut count: u32 = 0;
            for element in &self.capture.elements {
                if &element.id == id {
                    count = count.saturating_add(1);
                }
            }
            if count == 0 {
                self.find(
                    id,
                    "present",
                    "the description shows it in this state; the capture has no element with this id"
                        .to_string(),
                );
            } else if count > 1 {
                self.find(
                    id,
                    "duplicate",
                    format!("{count} elements have this id; a test id must name one element"),
                );
            }
        }
        let mut reported: Vec<&str> = Vec::new();
        for element in &self.capture.elements {
            let id = element.id.as_str();
            if present.contains(&element.id) || !known.contains(&element.id) || reported.contains(&id) {
                continue;
            }
            reported.push(id);
            self.checked.absent = self.checked.absent.saturating_add(1);
            let problem = format!(
                "the capture shows it; in this state the description does not, or platforms/{}.json hides it",
                self.platform
            );
            self.find(id, "absent", problem);
        }
    }

    /// What each node shows: its words, and whether it can be used.
    fn shows(&mut self) {
        let capture = self.capture;
        for (id, shown) in &self.expected.shows {
            let Some(element) = capture.get(id) else {
                continue;
            };
            let Some(shown) = shown.as_object() else {
                continue;
            };
            if let (Some(Value::Bool(disabled)), Some(enabled)) = (shown.get("disabled"), element.enabled) {
                self.checked.enabled = self.checked.enabled.saturating_add(1);
                if enabled == *disabled {
                    let (says, is) =
                        if *disabled { ("disabled", "enabled") } else { ("enabled", "disabled") };
                    self.find(id, "enabled", format!("it is {is}; the description says it is {says} here"));
                }
            }
            if let Some(text) = &element.text {
                self.text(id, shown, text);
            }
        }
    }

    /// One shown value is the element's whole text; several (a title and a
    /// subtitle) are each somewhere in it.
    fn text(&mut self, id: &str, shown: &Map<String, Value>, text: &str) {
        let mut words = Vec::new();
        for (key, value) in shown {
            if key == "disabled" {
                continue;
            }
            match value {
                Value::String(s) => words.push(normal(s)),
                Value::Number(n) => words.push(n.to_string()),
                _ => {}
            }
        }
        if words.is_empty() {
            return;
        }
        self.checked.text = self.checked.text.saturating_add(1);
        let text = normal(text);
        let mut wrong = Vec::new();
        if words.len() == 1 {
            if words[0] != text {
                wrong.push(words[0].clone());
            }
        } else {
            for word in &words {
                if !text.contains(word.as_str()) {
                    wrong.push(word.clone());
                }
            }
        }
        if !wrong.is_empty() {
            self.find(id, "text", format!("it shows {text:?}; the description says it shows {wrong:?}"));
        }
    }

    /// For each node the test acted on, the calls the app made against the
    /// ones the description says that event makes: each expected one made,
    /// with the same arguments, and no other.
    fn calls(&mut self) {
        let capture = self.capture;
        let none = Vec::new();
        for acted in &capture.acted {
            if !self.expected.present.contains(&acted.id) {
                continue;
            }
            self.checked.calls = self.checked.calls.saturating_add(1);
            let listed = match self.expected.calls.get(&acted.id) {
                Some(Value::Array(listed)) => listed,
                _ => &none,
            };
            let mut matched: Vec<usize> = Vec::new();
            for wanted in listed {
                if wanted.get("on").and_then(Value::as_str) != Some(acted.on.as_str()) {
                    continue;
                }
                let operation = wanted.get("operation").and_then(Value::as_str).unwrap_or_default();
                let with = wanted.get("with").cloned().unwrap_or(Value::Null);
                let mut made = None;
                for (index, call) in acted.calls.iter().enumerate() {
                    if call.operation == operation && !matched.contains(&index) {
                        made = Some((index, call));
                    }
                }
                match made {
                    None => {
                        let problem = format!(
                            "acting on it ({}) should call {operation} with {with}; the app made no such call",
                            acted.on
                        );
                        self.find(&acted.id, "call-missing", problem);
                    }
                    Some((index, call)) => {
                        matched.push(index);
                        let given = &call.with;
                        if *given != with {
                            let problem = format!(
                                "acting on it ({}) called {operation} with {given}; the description says with {with}",
                                acted.on
                            );
                            self.find(&acted.id, "call-with", problem);
                        }
                    }
                }
            }
            for (index, call) in acted.calls.iter().enumerate() {
                if !matched.contains(&index) {
                    let problem = format!(
                        "acting on it ({}) called {} with {}; the description calls no such operation here",
                        acted.on, call.operation, call.with
                    );
                    self.find(&acted.id, "call-extra", problem);
                }
            }
        }
    }

    /// Each node's frame against the wireframe's: every node inside the
    /// nearest captured node holding it (unless something between scrolls),
    /// and every sibling below, or right of, the one before it when the
    /// wireframe puts it there.
    fn geometry(&mut self, node: &Node, holder: Option<(&str, Rect)>, scrolled: bool) {
        let frame = self.capture.get(&node.id).and_then(|e| e.frame);
        if let (Some(frame), Some((outer, around))) = (frame, holder)
            && !scrolled
        {
            self.checked.inside = self.checked.inside.saturating_add(1);
            if !inside(frame, around) {
                let problem = format!("its frame {} is not inside {outer}'s {}", show(frame), show(around));
                self.find(&node.id, "inside", problem);
            }
        }
        let scrolls = self.components.scrolls(&node.component);
        let (holder, scrolled) = match frame {
            Some(frame) => (Some((node.id.as_str(), frame)), scrolls),
            None => (holder, scrolled || scrolls),
        };
        let mut before: Option<&Node> = None;
        for child in &node.children {
            let flows = child.slot != "overlay" && !self.components.pinned(&child.component);
            if flows && self.capture.get(&child.id).is_some() {
                if let Some(previous) = before {
                    self.order(previous, child);
                }
                before = Some(child);
            }
            self.geometry(child, holder, scrolled);
        }
    }

    /// `b` follows `a` where the wireframe has it follow: below it, or else
    /// right of it.
    fn order(&mut self, a: &Node, b: &Node) {
        let (Some(wa), Some(wb)) = (self.placed(&a.id), self.placed(&b.id)) else {
            return;
        };
        let (Some(ra), Some(rb)) =
            (self.capture.get(&a.id).and_then(|e| e.frame), self.capture.get(&b.id).and_then(|e| e.frame))
        else {
            return;
        };
        let (wanted, holds) = if wb.y >= wa.y + wa.height - SLACK {
            ("below", rb.y + SLACK >= ra.y + ra.height)
        } else if wb.x >= wa.x + wa.width - SLACK {
            ("right of", rb.x + SLACK >= ra.x + ra.width)
        } else {
            return;
        };
        self.checked.order = self.checked.order.saturating_add(1);
        if !holds {
            let problem = format!(
                "the description puts it {wanted} {}; its frame {} is not, against {}",
                a.id,
                show(rb),
                show(ra)
            );
            self.find(&b.id, "order", problem);
        }
    }

    /// Where the wireframe put the node `id`.
    fn placed(&self, id: &str) -> Option<Rect> {
        for placed in &self.page.nodes {
            if placed.id == id {
                return Some(placed.rect);
            }
        }
        None
    }
}

fn inside(frame: Rect, around: Rect) -> bool {
    frame.x + SLACK >= around.x
        && frame.y + SLACK >= around.y
        && frame.x + frame.width <= around.x + around.width + SLACK
        && frame.y + frame.height <= around.y + around.height + SLACK
}

fn show(rect: Rect) -> String {
    format!("[{}, {}, {}×{}]", rect.x, rect.y, rect.width, rect.height)
}

/// `text` with every run of white space as one space, and none at the ends.
fn normal(text: &str) -> String {
    let mut out = String::new();
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// Every id in `node`'s tree.
fn ids(node: &Node, into: &mut Vec<String>) {
    if !node.id.is_empty() {
        into.push(node.id.clone());
    }
    for child in &node.children {
        ids(child, into);
    }
}

fn object(key: &str, name: &str) -> Value {
    let mut map = Map::new();
    map.insert(key.to_string(), Value::from(name));
    Value::Object(map)
}
