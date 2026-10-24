//! The look language, parsed once: what a component is drawn as, read from
//! its `"look"` into a tree. `check` walks the tree for what it reads, the
//! filters it applies and the conditions it cannot parse; underdraw-layout
//! evaluates it and parses nothing. A look's condition is read by the one
//! parser every condition is ([`crate::expr`]), its names placeholders. Why:
//! [ADR 0007](../../../docs/adr/0007-kinds-are-data.md).

use serde_json::{Map, Value};

use crate::expr::{self, Atom, Expr};
use crate::length::{self, Computed};

/// A look, or any value inside one, parsed.
#[derive(Clone, Debug, PartialEq)]
pub enum Look {
    /// `"$children"`: the node's children, in its place.
    Children,
    /// Words with placeholders, as written.
    Text {
        written: String,
        template: Template,
    },
    List(Vec<Look>),
    Object(Object),
    /// A length key's arithmetic, `{depth} * space.4`: the length it comes
    /// to ([`crate::length`]).
    Length(Computed),
    /// A number, true or false, or null: itself.
    Plain(Value),
}

impl Look {
    pub fn parse(value: &Value) -> Look {
        Look::parse_as(value, false)
    }

    /// `value` parsed; as a `length`, a string that asks for arithmetic
    /// is a computed length, in any side or branch of it.
    fn parse_as(value: &Value, length: bool) -> Look {
        match value {
            Value::String(text) if text == "$children" => Look::Children,
            Value::String(text) => match Computed::read(text).filter(|_| length) {
                Some(computed) => Look::Length(computed),
                None => Look::Text { written: text.clone(), template: Template::parse(text) },
            },
            Value::Array(items) => {
                let mut looks = Vec::new();
                for item in items {
                    looks.push(Look::parse_as(item, length));
                }
                Look::List(looks)
            }
            Value::Object(object) => Look::Object(Object::parse(object, length)),
            other => Look::Plain(other.clone()),
        }
    }

    /// Each name this reads from its component's params, in the order
    /// read, leaving out those `bound` (what a `repeat` around it binds):
    /// each placeholder's, each a condition's, and the props a `frame`
    /// takes. A component's name and the notes for people read nothing.
    pub fn reads(&self, bound: &[&str], read: &mut Vec<String>) {
        self.collect(bound, Reading::All, read);
    }

    /// The params this reads only as a yes/no: each named bare in an `if`
    /// or a branch's `if` (`disabled`, `!selected`), never compared, shown
    /// or given on. A listed param of these is a bool in all but its type.
    pub fn only_asks(&self) -> Vec<String> {
        let (mut read, mut asked) = (Vec::new(), Vec::new());
        self.collect(&[], Reading::All, &mut read);
        self.collect(&[], Reading::Asked, &mut asked);
        let mut only = Vec::new();
        for name in &asked {
            let times = |names: &[String]| names.iter().filter(|other| *other == name).count();
            if times(&read) == times(&asked) && !only.contains(name) {
                only.push(name.clone());
            }
        }
        only
    }

    fn collect(&self, bound: &[&str], reading: Reading, read: &mut Vec<String>) {
        match self {
            Look::Text { template, .. } if reading == Reading::All => {
                for placeholder in template.placeholders() {
                    placeholder.read(bound, read);
                }
            }
            Look::List(looks) => {
                for look in looks {
                    look.collect(bound, reading, read);
                }
            }
            Look::Object(object) => object.collect(bound, reading, read),
            Look::Length(computed) if reading == Reading::All => {
                for name in computed.params() {
                    read_name(name, bound, read);
                }
            }
            Look::Text { .. } | Look::Length(_) | Look::Children | Look::Plain(_) => {}
        }
    }

    /// Each length it computes, at any depth, with where it is: `at`
    /// joined to the keys, indexes and branches that reach it, as `check`
    /// names them.
    pub fn lengths<'l>(&'l self, at: &str, found: &mut Vec<(String, &'l Computed)>) {
        match self {
            Look::Length(computed) => found.push((at.to_string(), computed)),
            Look::List(looks) => {
                for (index, look) in looks.iter().enumerate() {
                    look.lengths(&format!("{at}[{index}]"), found);
                }
            }
            Look::Object(object) => {
                for key in &object.keys {
                    key.look.lengths(&format!("{at}.{}", key.name), found);
                }
                if let Some(cases) = &object.cases {
                    for (index, branch) in cases.branches.iter().enumerate() {
                        for (side, look) in [("then", &branch.then), ("else", &branch.otherwise)] {
                            if let Some(look) = look {
                                look.lengths(&format!("{at}.cases[{index}].{side}"), found);
                            }
                        }
                    }
                }
            }
            Look::Children | Look::Text { .. } | Look::Plain(_) => {}
        }
    }

    /// Whether it places `"$children"` anywhere in it, so its node's slots
    /// are drawn. A component it nests is drawn without them.
    pub fn places_children(&self) -> bool {
        match self {
            Look::Children => true,
            Look::List(looks) => looks.iter().any(Look::places_children),
            Look::Object(object) => {
                object.component().is_none() && object.keys.iter().any(|key| key.look.places_children())
                    || object.cases.as_ref().is_some_and(|cases| {
                        cases.branches.iter().any(|branch| {
                            [&branch.then, &branch.otherwise].into_iter().flatten().any(Look::places_children)
                        })
                    })
            }
            Look::Text { .. } | Look::Length(_) | Look::Plain(_) => false,
        }
    }

    /// Each component this nests (`{"component": "Chip", …}`), at any
    /// depth of it, added to `nested`.
    pub(crate) fn nests(&self, nested: &mut Vec<String>) {
        match self {
            Look::List(looks) => {
                for look in looks {
                    look.nests(nested);
                }
            }
            Look::Object(object) => {
                if let Some(name) = object.component() {
                    nested.push(name.to_string());
                }
                for key in &object.keys {
                    key.look.nests(nested);
                }
                if let Some(cases) = &object.cases {
                    for branch in &cases.branches {
                        for side in [&branch.then, &branch.otherwise].into_iter().flatten() {
                            side.nests(nested);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// The names a `repeat` binds for its `each`: `n` from 1, `index` from 0,
/// `item`. No component declares them.
pub const REPEATED: [&str; 3] = ["item", "index", "n"];

/// The props a `frame` takes from its node, as its component's params.
pub const FRAMED: [&str; 5] = ["padding", "gap", "radius", "border", "elevation"];

/// Keys a look may also hold, read by people and not drawn.
pub(crate) const NOTES: [&str; 2] = ["$comment", "doc"];

/// An object in a look: an element (a box, text, a `repeat`, another
/// component) or a value such as a `pin`, or `cases` choosing one.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// Its `if`, when that is words: none of it is made unless this holds.
    pub when: Option<Condition>,
    /// Its `cases`, which stand for the whole object.
    pub cases: Option<Cases>,
    /// Every other key, in the order written.
    pub keys: Vec<Key>,
}

/// One key of an [`Object`], as written and parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub name: String,
    pub written: Value,
    pub look: Look,
}

/// What an [`Object`] makes, once its `if` holds.
pub enum Element<'l> {
    /// The look its chosen branch is.
    Cases(&'l Cases),
    /// `each` once per element of `over`: a count, or a list.
    Repeat {
        over: &'l Look,
        each: Option<&'l Look>,
    },
    /// Another component's look, with the other keys as its props.
    Component(&'l str),
    Text,
    Box,
}

impl Object {
    /// `object` parsed; `length` when it stands for a length key's value,
    /// so each branch of its `cases` is one.
    fn parse(object: &Map<String, Value>, length: bool) -> Object {
        let mut parsed = Object { when: None, cases: None, keys: Vec::new() };
        for (name, written) in object {
            match (name.as_str(), written) {
                ("if", Value::String(condition)) => parsed.when = Some(Condition::parse(condition)),
                ("cases", branches) => parsed.cases = Some(Cases::parse(branches, length)),
                _ => parsed.keys.push(Key {
                    name: name.clone(),
                    written: written.clone(),
                    look: Look::parse_as(written, length::KEYS.contains(&name.as_str())),
                }),
            }
        }
        parsed
    }

    /// What key `name` holds, parsed.
    pub fn get(&self, name: &str) -> Option<&Look> {
        self.key(name).map(|key| &key.look)
    }

    /// What key `name` holds, as written.
    pub fn written(&self, name: &str) -> Option<&Value> {
        self.key(name).map(|key| &key.written)
    }

    fn key(&self, name: &str) -> Option<&Key> {
        self.keys.iter().find(|key| key.name == name)
    }

    /// The component it nests, when it names one.
    pub fn component(&self) -> Option<&str> {
        self.written("component").and_then(Value::as_str)
    }

    /// What it makes: its `cases`, a `repeat`, a nested component, text,
    /// or else a box.
    pub fn element(&self) -> Element<'_> {
        if let Some(cases) = &self.cases {
            return Element::Cases(cases);
        }
        if let Some(over) = self.get("repeat") {
            return Element::Repeat { over, each: self.get("each") };
        }
        if let Some(name) = self.component() {
            return Element::Component(name);
        }
        if self.key("text").is_some() { Element::Text } else { Element::Box }
    }

    /// Whether it asks for a backdrop and does not float by its own `pin`:
    /// only a component's whole look, put in an `overlay`, may.
    pub(crate) fn backdrop_without_pin(&self) -> bool {
        self.written("backdrop") == Some(&Value::Bool(true)) && self.key("pin").is_none()
    }

    fn collect(&self, bound: &[&str], reading: Reading, read: &mut Vec<String>) {
        if reading == Reading::All
            && self.written("frame") == Some(&Value::Bool(true))
            && self.component().is_none()
        {
            for name in FRAMED {
                read_name(name, bound, read);
            }
        }
        if let Some(condition) = &self.when {
            condition.collect(bound, reading, read);
        }
        let repeats = self.key("repeat").is_some();
        for key in &self.keys {
            match key.name.as_str() {
                "each" if repeats => {
                    let mut inner = bound.to_vec();
                    inner.extend(REPEATED);
                    key.look.collect(&inner, reading, read);
                }
                "component" => {}
                name if NOTES.contains(&name) => {}
                _ => key.look.collect(bound, reading, read),
            }
        }
        if let Some(cases) = &self.cases {
            for branch in &cases.branches {
                if let Some(condition) = &branch.when {
                    condition.collect(bound, reading, read);
                }
                for side in [&branch.then, &branch.otherwise].into_iter().flatten() {
                    side.collect(bound, reading, read);
                }
            }
        }
    }
}

/// `cases`: a list of `{"if", "then"}`, ending in `{"else"}`.
#[derive(Clone, Debug, PartialEq)]
pub struct Cases {
    pub branches: Vec<Branch>,
    /// Whether it was written as a list; when not, it has no branches.
    pub listed: bool,
}

impl Cases {
    fn parse(written: &Value, length: bool) -> Cases {
        let Value::Array(branches) = written else {
            return Cases { branches: Vec::new(), listed: false };
        };
        let mut parsed = Vec::new();
        for branch in branches {
            parsed.push(Branch {
                when: branch.get("if").map(Condition::written),
                then: branch.get("then").map(|then| Look::parse_as(then, length)),
                otherwise: branch.get("else").map(|otherwise| Look::parse_as(otherwise, length)),
            });
        }
        Cases { branches: parsed, listed: true }
    }
}

/// One branch of [`Cases`]: `then` when its `if` holds; with no `if`, its
/// `else`.
#[derive(Clone, Debug, PartialEq)]
pub struct Branch {
    pub when: Option<Condition>,
    pub then: Option<Look>,
    pub otherwise: Option<Look>,
}

/// A look's condition (`value|first && size == "small"`): a screen
/// condition's grammar, each name a [`Placeholder`].
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub written: String,
    /// The condition, or why it is not one.
    pub parsed: Result<Expr<Placeholder>, String>,
}

impl Condition {
    pub fn parse(written: &str) -> Condition {
        let parsed = expr::parse(written).map(|expr| expr.named(&|name: String| Placeholder::parse(&name)));
        Condition { written: written.to_string(), parsed }
    }

    /// A branch's `"if"` as written: a string parsed; anything else a
    /// condition that never holds, saying an `"if"` takes a condition.
    fn written(written: &Value) -> Condition {
        match written {
            Value::String(condition) => Condition::parse(condition),
            other => Condition {
                written: crate::conditions::what_if_is(other).to_string(),
                parsed: Err(crate::conditions::IF_TAKES.to_string()),
            },
        }
    }

    /// Every placeholder it reads, in the order written.
    pub fn placeholders(&self) -> Vec<&Placeholder> {
        let mut all = Vec::new();
        if let Ok(expr) = &self.parsed {
            expr_placeholders(expr, &mut all);
        }
        all
    }

    fn collect(&self, bound: &[&str], reading: Reading, read: &mut Vec<String>) {
        match (reading, &self.parsed) {
            (Reading::All, _) => {
                for placeholder in self.placeholders() {
                    placeholder.read(bound, read);
                }
            }
            (Reading::Asked, Ok(expr)) => expr_asks(expr, bound, read),
            (Reading::Asked, Err(_)) => {}
        }
    }
}

/// Which names [`Look::collect`] gathers: every one read, or only those
/// asked as a bare yes/no.
#[derive(Clone, Copy, PartialEq)]
enum Reading {
    All,
    Asked,
}

/// Each name `expr` asks bare, as a yes/no, outside any comparison.
fn expr_asks(expr: &Expr<Placeholder>, bound: &[&str], asked: &mut Vec<String>) {
    match expr {
        Expr::Or(items) | Expr::And(items) => {
            for item in items {
                expr_asks(item, bound, asked);
            }
        }
        Expr::Not(inner) => expr_asks(inner, bound, asked),
        Expr::Atom(Atom::Group(inner)) => expr_asks(inner, bound, asked),
        Expr::Atom(Atom::Name(placeholder)) if placeholder.filters.is_empty() => {
            placeholder.read(bound, asked)
        }
        Expr::Atom(_) | Expr::Compare(..) => {}
    }
}

fn expr_placeholders<'e>(expr: &'e Expr<Placeholder>, all: &mut Vec<&'e Placeholder>) {
    match expr {
        Expr::Or(items) | Expr::And(items) => {
            for item in items {
                expr_placeholders(item, all);
            }
        }
        Expr::Not(inner) => expr_placeholders(inner, all),
        Expr::Compare(_, left, right) => {
            atom_placeholders(left, all);
            atom_placeholders(right, all);
        }
        Expr::Atom(atom) => atom_placeholders(atom, all),
    }
}

fn atom_placeholders<'e>(atom: &'e Atom<Placeholder>, all: &mut Vec<&'e Placeholder>) {
    match atom {
        Atom::Name(placeholder) => all.push(placeholder),
        Atom::Group(inner) => expr_placeholders(inner, all),
        _ => {}
    }
}

/// Words with `{placeholder}`s in them.
#[derive(Clone, Debug, PartialEq)]
pub enum Template {
    /// The whole of it is one placeholder: it gives that value itself, not
    /// words.
    Whole(Placeholder),
    /// Words, each placeholder filled in as words.
    Pieces(Vec<Piece>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Words(String),
    Hole(Placeholder),
}

impl Template {
    pub fn parse(text: &str) -> Template {
        if let Some(inner) = text.strip_prefix('{').and_then(|rest| rest.strip_suffix('}'))
            && !inner.contains('{')
            && !inner.contains('}')
        {
            return Template::Whole(Placeholder::parse(inner));
        }
        let mut pieces = Vec::new();
        let mut words = String::new();
        let mut rest = text;
        while let Some((before, after)) = rest.split_once('{') {
            words.push_str(before);
            match after.split_once('}') {
                Some((inner, next)) => {
                    if !words.is_empty() {
                        pieces.push(Piece::Words(std::mem::take(&mut words)));
                    }
                    pieces.push(Piece::Hole(Placeholder::parse(inner)));
                    rest = next;
                }
                None => {
                    words.push('{');
                    rest = after;
                    break;
                }
            }
        }
        words.push_str(rest);
        if !words.is_empty() {
            pieces.push(Piece::Words(words));
        }
        Template::Pieces(pieces)
    }

    /// Every placeholder in it, in order.
    pub fn placeholders(&self) -> Vec<&Placeholder> {
        match self {
            Template::Whole(placeholder) => vec![placeholder],
            Template::Pieces(pieces) => {
                let mut all = Vec::new();
                for piece in pieces {
                    if let Piece::Hole(placeholder) = piece {
                        all.push(placeholder);
                    }
                }
                all
            }
        }
    }
}

/// A path into the props, then each filter in turn: `items|first`,
/// `price|or:0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Placeholder {
    pub path: String,
    pub filters: Vec<Filter>,
}

impl Placeholder {
    /// `inner`, what stands between the braces.
    pub fn parse(inner: &str) -> Placeholder {
        let mut parts = inner.split('|');
        let path = parts.next().unwrap_or_default().trim().to_string();
        let mut filters = Vec::new();
        for filter in parts {
            filters.push(Filter::parse(filter));
        }
        Placeholder { path, filters }
    }

    /// The name it reads, the first step of its path; none when the path
    /// is empty or a `$` name, which read nothing.
    pub fn name(&self) -> Option<&str> {
        let name = self.path.split('.').next().unwrap_or_default();
        if name.is_empty() || name.starts_with('$') { None } else { Some(name) }
    }

    /// The steps of its path after its name, into what the name holds.
    pub fn steps(&self) -> impl Iterator<Item = &str> {
        self.path.split('.').skip(1)
    }

    fn read(&self, bound: &[&str], read: &mut Vec<String>) {
        if let Some(name) = self.name() {
            read_name(name, bound, read);
        }
    }
}

/// `name` added to `read`, unless `bound` holds it.
fn read_name(name: &str, bound: &[&str], read: &mut Vec<String>) {
    if !bound.contains(&name) {
        read.push(name.to_string());
    }
}

/// What a placeholder does to its value, after its path.
#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    /// `or:value`: this value, JSON or else words, when there is none.
    Or(Value),
    /// How many elements a list has; 0 for anything else.
    Count,
    First,
    Last,
    /// The first word.
    Head,
    /// The words after the first.
    Tail,
    /// 2026-11-12 → 2026/11/12.
    Slashes,
    /// An icon's name as its glyph.
    Glyph,
    /// `atLeast:n`: a number no less than n; none when n is no number.
    AtLeast(Option<f64>),
    /// `atMost:n`: a number no more than n.
    AtMost(Option<f64>),
    /// A name no filter has, as written: `check` names it, and it does
    /// nothing.
    Unknown(String),
}

/// How a filter reads its argument, the words after its `:`.
type ReadArgument = fn(&str) -> Filter;

/// Every filter's name, and how it reads its argument.
const FILTERS: [(&str, ReadArgument); 10] = [
    ("or", |argument| {
        Filter::Or(serde_json::from_str(argument).unwrap_or(Value::String(argument.to_string())))
    }),
    ("count", |_| Filter::Count),
    ("first", |_| Filter::First),
    ("last", |_| Filter::Last),
    ("head", |_| Filter::Head),
    ("tail", |_| Filter::Tail),
    ("slashes", |_| Filter::Slashes),
    ("glyph", |_| Filter::Glyph),
    ("atLeast", |argument| Filter::AtLeast(argument.trim().parse().ok())),
    ("atMost", |argument| Filter::AtMost(argument.trim().parse().ok())),
];

impl Filter {
    /// `written`, a name and, after a `:`, its argument.
    fn parse(written: &str) -> Filter {
        let (name, argument) = written.split_once(':').unwrap_or((written, ""));
        let name = name.trim();
        match FILTERS.iter().find(|(known, _)| *known == name) {
            Some((_, read)) => read(argument),
            None => Filter::Unknown(name.to_string()),
        }
    }

    /// Every filter's name, for a person choosing one.
    pub fn names() -> Vec<&'static str> {
        FILTERS.map(|(name, _)| name).to_vec()
    }
}

/// A look key that takes one of a few words: the words, and the type each
/// is read as. `check` lets through exactly the words that read.
macro_rules! words {
    ($(#[$doc:meta])* $name:ident { $($variant:ident = $word:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const WORDS: &'static [&'static str] = &[$($word),+];

            pub fn read(word: &str) -> Option<$name> {
                match word {
                    $($word => Some($name::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

words!(
    /// A box's `box`: how it lays out its children.
    Flow { Row = "row", Column = "column", Grid = "grid" }
);
words!(
    /// Text's `align`.
    TextAlign { Start = "start", Center = "center", End = "end" }
);
words!(
    /// A box's `self`: where it sits across its parent.
    SelfAlign { Start = "start" }
);
words!(
    /// A box's `items`: where its children sit across it.
    Items { Start = "start", Center = "center" }
);
words!(
    /// A box's `justify`: where its children sit along it.
    Justify { Center = "center", Between = "between", End = "end" }
);
words!(
    /// A box's `overflowX`.
    Overflow { Clip = "clip" }
);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_template_is_one_placeholder_or_words_and_holes() {
        let whole = Template::parse("{items|first|or:\"none\"}");
        let expected = Placeholder {
            path: "items".to_string(),
            filters: vec![Filter::First, Filter::Or(json!("none"))],
        };
        assert_eq!(whole, Template::Whole(expected));
        let pieces = Template::parse("{n} of {total|atMost:9} {open");
        assert_eq!(
            pieces,
            Template::Pieces(vec![
                Piece::Hole(Placeholder::parse("n")),
                Piece::Words(" of ".to_string()),
                Piece::Hole(Placeholder {
                    path: "total".to_string(),
                    filters: vec![Filter::AtMost(Some(9.0))]
                }),
                Piece::Words(" {open".to_string()),
            ])
        );
    }

    #[test]
    fn every_filter_name_reads_as_a_filter() {
        for name in Filter::names() {
            assert!(!matches!(Filter::parse(name), Filter::Unknown(_)), "{name}");
        }
        assert_eq!(Filter::parse(" upper "), Filter::Unknown("upper".to_string()));
    }

    #[test]
    fn a_condition_takes_placeholders_in_the_screen_grammar() {
        let condition = Condition::parse("value|first && value|last || size >= 2");
        let names: Vec<_> = condition.placeholders().iter().map(|p| p.path.as_str()).collect();
        assert_eq!(names, ["value", "value", "size"]);
        assert!(Condition::parse("value | first").parsed.is_err());
        assert!(Condition::parse("a = b").parsed.is_err());
    }

    #[test]
    fn a_repeat_binds_its_names_for_each_and_a_frame_reads_its_props() {
        let look = Look::parse(&json!({
            "frame": true,
            "if": "shown",
            "children": [{ "repeat": "{rows}", "each": { "text": "{n}. {item.title} {label}" } }],
            "cases": [{ "if": "wide", "then": "{left}" }, { "else": "$children" }],
            "doc": "{ignored}"
        }));
        let mut read = Vec::new();
        look.reads(&[], &mut read);
        assert_eq!(
            read,
            ["padding", "gap", "radius", "border", "elevation", "shown", "rows", "label", "wide", "left"]
        );
    }
}
