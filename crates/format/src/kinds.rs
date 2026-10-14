//! The kinds a description may use and what each looks like: kinds.json,
//! underdraw's defaults with a project's file over them by name. The look
//! itself is drawn by underdraw-layout; here it is read and checked. Why:
//! [ADR 0007](../../../docs/adr/0007-kinds-are-data.md).

use serde_json::{Map, Value};

use crate::check::Findings;
use crate::icons;

/// The kinds underdraw ships, in the form a project's kinds.json takes.
const DEFAULTS: &str = include_str!("../kinds.json");

/// Every key a look may hold: what underdraw-layout reads. A look with
/// `kind` holds that kind's props instead.
pub const LOOK_KEYS: [&str; 43] = [
    "if",
    "cases",
    "repeat",
    "each",
    "kind",
    "children",
    "text",
    "font",
    "ink",
    "weight",
    "maxLines",
    "align",
    "underline",
    "strike",
    "box",
    "frame",
    "fill",
    "stroke",
    "radius",
    "dashed",
    "cross",
    "topLine",
    "clip",
    "gap",
    "padding",
    "margin",
    "size",
    "width",
    "height",
    "aspect",
    "grow",
    "shrink",
    "basis",
    "self",
    "items",
    "justify",
    "columns",
    "rows",
    "span",
    "overflowX",
    "pin",
    "backdrop",
    "itemWidth",
];

/// Keys a look may also hold, read by people and not drawn.
const NOTES: [&str; 2] = ["$comment", "doc"];

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

/// Every kind there is.
pub struct Kinds {
    kinds: Map<String, Value>,
}

impl Kinds {
    /// underdraw's kinds, with `project`'s (its kinds.json) over them by
    /// name.
    pub fn read(project: Option<&Value>) -> Kinds {
        let mut kinds = Kinds { kinds: Map::new() };
        if let Ok(defaults) = serde_json::from_str::<Value>(DEFAULTS) {
            kinds.add(&defaults);
        }
        if let Some(project) = project {
            kinds.add(project);
        }
        kinds
    }

    fn add(&mut self, file: &Value) {
        if let Some(Value::Object(kinds)) = file.get("kinds") {
            for (name, kind) in kinds {
                self.kinds.insert(name.clone(), kind.clone());
            }
        }
    }

    pub fn has(&self, kind: &str) -> bool {
        self.kinds.contains_key(kind)
    }

    /// What `kind` looks like, when it is a kind.
    pub fn look(&self, kind: &str) -> Option<&Value> {
        self.kinds.get(kind).and_then(|entry| entry.get("look"))
    }

    /// Whether `kind`'s whole look has a backdrop, which draws only when its
    /// node is in an `overlay`.
    pub fn backdrop(&self, kind: &str) -> bool {
        self.look(kind)
            .is_some_and(|look| look.get("backdrop") == Some(&Value::Bool(true)) && look.get("pin").is_none())
    }

    /// Why `kind`'s look is a guess the person has not confirmed, when it is.
    pub fn assumed(&self, kind: &str) -> Option<&str> {
        self.kinds.get(kind).and_then(|entry| entry.get("assumed")).and_then(Value::as_str)
    }

    /// Every fault in the looks of `project`'s kinds, at `kinds.<Name>…`:
    /// a kind with no look, a key no look reads, a filter or kind that does
    /// not exist.
    pub(crate) fn check(&self, project: Option<&Value>, findings: &mut Findings) {
        let Some(project) = project else {
            return;
        };
        if project.get("icons").is_some() {
            findings.add(
                "icons",
                "icons are Material Symbols names (fonts.google.com/icons) and need no entry; take this out",
            );
        }
        let Some(Value::Object(kinds)) = project.get("kinds") else {
            if project.get("kinds").is_some() {
                findings.add("kinds", "kinds.json's \"kinds\" maps each kind's name to {\"look\": …}");
            }
            return;
        };
        for (name, kind) in kinds {
            let at = format!("kinds.{name}");
            match kind.get("look") {
                Some(look) => self.look_faults(look, &format!("{at}.look"), true, findings),
                None => findings.add(&at, "a kind needs \"look\": what it is drawn as"),
            }
        }
    }

    /// `top`: `look` is what the kind's node is drawn as, which an overlay
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
                let nested = object.get("kind").and_then(Value::as_str);
                if !top && object.get("backdrop") == Some(&Value::Bool(true)) && !object.contains_key("pin") {
                    findings.add(
                        at,
                        "backdrop is drawn only by a box that floats: the whole look of a kind put in \"overlay\", or a box with \"pin\"",
                    );
                }
                if let Some(kind) = nested
                    && !self.has(kind)
                {
                    findings.add(at, &format!("kind {kind} is not defined in kinds.json or underdraw's own"));
                }
                for (key, value) in object {
                    if key == "cases" {
                        self.branch_faults(value, &format!("{at}.cases"), top, findings);
                        continue;
                    }
                    if nested.is_none()
                        && !LOOK_KEYS.contains(&key.as_str())
                        && !NOTES.contains(&key.as_str())
                    {
                        findings.add(
                            at,
                            &format!(
                                "{key} is not a key a look reads; see `underdraw check --help`, KINDS.JSON"
                            ),
                        );
                    }
                    if nested.is_some()
                        && icons::is_icon_key(key)
                        && let Value::String(icon) = value
                        && !icon.contains('{')
                        && !icons::known(icon)
                    {
                        findings.add(&format!("{at}.{key}"), &icons::unknown(icon));
                    }
                    if nested.is_none() && (key == "columns" || key == "rows") {
                        track_faults(value, &format!("{at}.{key}"), findings);
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
