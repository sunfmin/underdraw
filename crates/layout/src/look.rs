// A style is built one property per line, as CSS reads.
#![allow(clippy::field_reassign_with_default)]

//! Kinds as data: what each kind of node looks like, read from kinds.json
//! (underdraw's defaults, under a project's own) and turned into boxes for
//! `place` to lay out. A look is a box, text, a `repeat`, `cases`, another
//! kind, or `"$children"`; it reads the node's props through `{prop|filter}`
//! templates. Why: [ADR 0007](../../../docs/adr/0007-kinds-are-data.md).

use serde_json::{Map, Value};
use taffy::prelude::*;
use underdraw_format::{Anchor, Edge, Kinds, Node, Track};

use crate::place::{Elem, Look, Pin, Words};
use crate::tokens::{self, Tokens};
use crate::{Align, Ink};

/// The most a `repeat` makes, so a wrong count cannot run away.
const MOST_REPEATED: u64 = 1000;

/// How far in from its node's edges an overlay that is not flush stands.
const OVERLAY_INSET: f32 = 16.0;

/// What a look reads: the props of the kind it draws, the names a `repeat`
/// binds (`n` from 1, `index` from 0, `item`), and the node whose children
/// `"$children"` stands for.
struct Scope<'p, 'a> {
    props: &'p Map<String, Value>,
    vars: Map<String, Value>,
    node: Option<&'a Node>,
}

impl Scope<'_, '_> {
    fn get(&self, name: &str) -> Value {
        let mut segments = name.split('.');
        let first = segments.next().unwrap_or("");
        let mut value =
            self.vars.get(first).or_else(|| self.props.get(first)).cloned().unwrap_or(Value::Null);
        for segment in segments {
            value = match (&value, segment.parse::<usize>()) {
                (Value::Array(items), Ok(at)) => items.get(at).cloned().unwrap_or(Value::Null),
                (Value::Object(fields), _) => fields.get(segment).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
        }
        value
    }
}

pub(crate) struct Catalog<'t> {
    pub(crate) tokens: &'t Tokens<'t>,
    pub(crate) kinds: &'t Kinds,
}

impl Catalog<'_> {
    /// `node` drawn as its kind's look says, tagged as the node; a box with
    /// the kind's name, marked as a guess, when no kind has that name.
    pub(crate) fn node<'a>(&self, node: &'a Node) -> Elem<'a> {
        let mut elem = match self.kinds.look(&node.kind) {
            Some(look) => {
                let scope = Scope { props: &node.props, vars: Map::new(), node: Some(node) };
                self.one(look, &scope)
            }
            None => {
                let mut elem = self.unknown(&node.kind, Some(node));
                elem.assumed = Some(format!(
                    "{} is not a kind underdraw knows; drawn as a box with its name",
                    node.kind
                ));
                elem
            }
        };
        elem.source = Some(node);
        elem
    }

    /// The one box `look` makes: the first, or an empty one when it makes
    /// none.
    fn one<'a>(&self, look: &Value, scope: &Scope<'_, 'a>) -> Elem<'a> {
        let mut made = self.build(look, scope).into_iter();
        made.next().unwrap_or_else(|| Elem::new(Style::default()))
    }

    /// The boxes `look` makes in `scope`: none when its `if` fails, one per
    /// element for a `repeat`, the node's children for `"$children"`.
    fn build<'a>(&self, look: &Value, scope: &Scope<'_, 'a>) -> Vec<Elem<'a>> {
        let object = match look {
            Value::String(name) if name == "$children" => {
                return match scope.node {
                    Some(node) => self.children(node),
                    None => Vec::new(),
                };
            }
            Value::Array(looks) => {
                let mut made = Vec::new();
                for look in looks {
                    made.append(&mut self.build(look, scope));
                }
                return made;
            }
            Value::Object(object) => object,
            _ => return Vec::new(),
        };
        if let Some(Value::String(condition)) = object.get("if")
            && !self.holds(condition, scope)
        {
            return Vec::new();
        }
        if let Some(branches) = object.get("cases") {
            return match self.branch(branches, scope) {
                Some(chosen) => self.build(chosen, scope),
                None => Vec::new(),
            };
        }
        if let Some(over) = object.get("repeat") {
            return self.repeat(over, object.get("each").unwrap_or(&Value::Null), scope);
        }
        if let Some(Value::String(name)) = object.get("kind") {
            return vec![self.nested(name, object, scope)];
        }
        if object.contains_key("text") {
            return vec![self.text(object, scope)];
        }
        vec![self.boxed(object, scope)]
    }

    /// `each` once per element of `over`: a count, or a list whose elements
    /// are each `item`.
    fn repeat<'a>(&self, over: &Value, each: &Value, scope: &Scope<'_, 'a>) -> Vec<Elem<'a>> {
        let items = match self.resolve(over, scope) {
            Value::Array(items) => items,
            count => {
                let count = count.as_u64().unwrap_or(0).min(MOST_REPEATED);
                let mut items = Vec::new();
                for n in 1..=count {
                    items.push(Value::from(n));
                }
                items
            }
        };
        let mut made = Vec::new();
        let mut index: u64 = 0;
        for item in items.into_iter().take(MOST_REPEATED as usize) {
            let mut vars = scope.vars.clone();
            vars.insert("index".to_string(), Value::from(index));
            vars.insert("n".to_string(), Value::from(index.saturating_add(1)));
            vars.insert("item".to_string(), item);
            let inner = Scope { props: scope.props, vars, node: scope.node };
            made.append(&mut self.build(each, &inner));
            index = index.saturating_add(1);
        }
        made
    }

    /// Another kind's look, inside this one, with the props written beside
    /// `kind`. It is part of this node, not a node of its own.
    fn nested<'a>(&self, name: &str, object: &Map<String, Value>, scope: &Scope<'_, 'a>) -> Elem<'a> {
        let mut props = Map::new();
        for (key, value) in object {
            if key != "kind" && key != "if" {
                props.insert(key.clone(), self.resolve(value, scope));
            }
        }
        match self.kinds.look(name) {
            Some(look) => {
                let inner = Scope { props: &props, vars: Map::new(), node: None };
                self.one(look, &inner)
            }
            None => self.unknown(name, None),
        }
    }

    /// A box: its layout (`box`, sizes, `gap`, `padding`, …), how it looks
    /// (`fill`, `stroke`, `radius`, `cross`, `topLine`) and its `children`.
    /// A `frame` also takes the container props a description may set on
    /// the node: `padding`, `gap`, `radius`, `border`, `elevation`.
    fn boxed<'a>(&self, object: &Map<String, Value>, scope: &Scope<'_, 'a>) -> Elem<'a> {
        let get = |key: &str| self.get(object, key, scope);
        let mut style = get("box").as_str().and_then(box_style).unwrap_or_default();
        self.style(object, scope, &mut style);
        let mut gap = self.tokens.find_length(&get("gap"));
        let mut radius = self.tokens.find_length(&get("radius")).unwrap_or(0.0);
        let mut stroke = ink(&get("stroke"));
        let mut clip = None;
        if get("frame") == Value::Bool(true) {
            let props = scope.props;
            if let Some([top, right, bottom, left]) =
                props.get("padding").and_then(|padding| self.four(padding))
            {
                style.padding = sides(top, right, bottom, left);
            }
            gap = Some(self.tokens.length(props.get("gap"), gap.unwrap_or(0.0)));
            let framed = self.tokens.length(props.get("radius"), 0.0);
            if framed > 0.0 {
                radius = framed;
                clip = Some(framed);
            }
            if stroke.is_none() {
                stroke = match (props.get("border"), props.get("elevation")) {
                    (Some(border), _) => Some(tokens::ink(Some(border))),
                    (None, Some(_)) => Some(Ink::Line),
                    (None, None) => None,
                };
            }
        }
        if let Some(gap) = gap {
            style.gap = taffy::Size { width: length(gap), height: length(gap) };
        }
        let mut elem = Elem::new(style);
        let fill = ink(&get("fill"));
        if fill.is_some() || stroke.is_some() {
            elem = elem.with(Look::Box { radius, fill, stroke, dashed: get("dashed") == Value::Bool(true) });
        }
        if let Some(cross) = ink(&get("cross")) {
            elem = elem.with(Look::Cross(cross));
        }
        if let Some(line) = ink(&get("topLine")) {
            elem = elem.with(Look::TopLine(line));
        }
        let mut children = Vec::new();
        if let Some(looks) = object.get("children") {
            children = self.build(looks, scope);
        }
        if let Some(width) = self.tokens.find_length(&get("itemWidth")) {
            for child in &mut children {
                child.style.size.width = length(width);
                child.style.flex_shrink = 0.0;
            }
        }
        elem = elem.holding(children);
        elem.backdrop = get("backdrop") == Value::Bool(true);
        elem.clip = match get("clip") {
            Value::Bool(false) => None,
            Value::Null => clip,
            value => self.tokens.find_length(&value).or(clip),
        };
        self.pin(elem, &get("pin"))
    }

    /// Text: `text` in `font`, `ink`, `weight`, `maxLines`, `align`,
    /// `underline`, `strike`; it sits among its siblings as `self` says.
    fn text<'a>(&self, object: &Map<String, Value>, scope: &Scope<'_, 'a>) -> Elem<'a> {
        let get = |key: &str| self.get(object, key, scope);
        let mut style = Style::default();
        self.style(object, scope, &mut style);
        let mut font = self.tokens.font(Some(&get("font")), "font.body");
        if let Some(weight) = get("weight").as_u64() {
            font.weight = weight.min(1000) as u16;
        }
        let mut words = Words::new(said(&get("text")), font, ink(&get("ink")).unwrap_or(Ink::Text));
        words.max_lines = get("maxLines").as_u64().map(|n| n.min(100) as u32);
        words.align = get("align").as_str().and_then(text_align).unwrap_or(Align::Start);
        words.underline = get("underline") == Value::Bool(true);
        words.strike = get("strike") == Value::Bool(true);
        self.pin(Elem::text(style, words), &get("pin"))
    }

    /// How a box sits among its siblings and holds its own: sizes, `grow`,
    /// `shrink`, `basis`, `self`, `items`, `justify`, `padding`, `margin`,
    /// grid `columns`, `rows` and `span`, and `overflowX`.
    fn style(&self, object: &Map<String, Value>, scope: &Scope, style: &mut Style) {
        let get = |key: &str| self.get(object, key, scope);
        if let Some(size) = self.dimension(&get("size")) {
            style.size = taffy::Size { width: size, height: size };
        }
        if let Some(width) = self.dimension(&get("width")) {
            style.size.width = width;
        }
        if let Some(height) = self.dimension(&get("height")) {
            style.size.height = height;
        }
        style.aspect_ratio = aspect(&get("aspect")).or(style.aspect_ratio);
        if let Some(grow) = get("grow").as_f64() {
            style.flex_grow = grow as f32;
        }
        if let Some(shrink) = get("shrink").as_f64() {
            style.flex_shrink = shrink as f32;
        }
        if let Some(basis) = self.tokens.find_length(&get("basis")) {
            style.flex_basis = length(basis);
        }
        if let Some(align) = get("self").as_str().and_then(align_self) {
            style.align_self = Some(align);
        }
        if let Some(items) = get("items").as_str().and_then(align_items) {
            style.align_items = items;
        }
        if let Some(justify) = get("justify").as_str().and_then(justify_content) {
            style.justify_content = justify;
        }
        if let Some([top, right, bottom, left]) = self.four(&get("padding")) {
            style.padding = sides(top, right, bottom, left);
        }
        if let Some([top, right, bottom, left]) = self.four(&get("margin")) {
            style.margin = sides(top, right, bottom, left);
        }
        match get("columns") {
            Value::Array(widths) => style.grid_template_columns = tracks(&widths),
            Value::Null => {}
            count => {
                let count = count.as_u64().unwrap_or(1).clamp(1, 12);
                let mut tracks = Vec::new();
                for _ in 0..count {
                    tracks.push(fr(1.0));
                }
                style.grid_template_columns = tracks;
            }
        }
        if let Value::Array(heights) = get("rows") {
            style.grid_template_rows = tracks(&heights);
        }
        if let Some(cells) = get("span").as_u64() {
            let cells = cells.clamp(1, 12) as u16;
            style.grid_column = span(cells);
            style.grid_row = span(cells);
        }
        if let Some(x) = get("overflowX").as_str().and_then(overflow) {
            style.overflow = taffy::Point { x, y: taffy::Overflow::Visible };
        }
    }

    /// `elem` floated over its parent when `pin` says where: `x` and `y` each
    /// "start", "center" or "end", `inset` in from the edge.
    fn pin<'a>(&self, elem: Elem<'a>, pin: &Value) -> Elem<'a> {
        let Value::Object(pin) = pin else {
            return elem;
        };
        let edge =
            |key: &str| pin.get(key).and_then(Value::as_str).and_then(Edge::parse).unwrap_or(Edge::Start);
        let inset = self.tokens.length(pin.get("inset"), 0.0);
        elem.pinned(Pin { x: edge("x"), y: edge("y"), inset })
    }

    /// A length, or a share of the parent written "70%".
    fn dimension(&self, value: &Value) -> Option<Dimension> {
        if let Some(share) = value.as_str().and_then(|text| text.strip_suffix('%')) {
            return share.trim().parse::<f32>().ok().map(|n| percent(n / 100.0));
        }
        self.tokens.find_length(value).map(length)
    }

    /// Four sides from one length, `[vertical, horizontal]`, or
    /// `[top, right, bottom, left]`, as CSS writes them.
    fn four(&self, value: &Value) -> Option<[f32; 4]> {
        let one = |value: &Value| self.tokens.length(Some(value), 0.0);
        match value {
            Value::Null => None,
            Value::Array(sides) => match sides.as_slice() {
                [vertical, horizontal] => {
                    let (v, h) = (one(vertical), one(horizontal));
                    Some([v, h, v, h])
                }
                [top, right, bottom, left] => Some([one(top), one(right), one(bottom), one(left)]),
                _ => None,
            },
            value => {
                let all = one(value);
                Some([all, all, all, all])
            }
        }
    }

    /// `node`'s children in order: `leading`, the body (`children`, `item`,
    /// `action`), then `trailing` pushed to the far end; `overlay` floats
    /// over the box at its `anchor`: a corner, the center, or the bottom edge
    /// from side to side, flush.
    fn children<'a>(&self, node: &'a Node) -> Vec<Elem<'a>> {
        let mut built = Vec::new();
        let mut trailing = Vec::new();
        for child in &node.children {
            match child.slot.as_str() {
                "trailing" => trailing.push(self.node(child)),
                "overlay" => {}
                _ => built.push(self.node(child)),
            }
        }
        if !trailing.is_empty() {
            let mut spacer = Style::default();
            spacer.flex_grow = 1.0;
            built.push(Elem::new(spacer));
            built.append(&mut trailing);
        }
        for child in &node.children {
            if child.slot == "overlay" {
                let placement = Anchor::of(&child.props).placement();
                let mut elem = self.node(child);
                let inset = if placement.flush {
                    elem.style.size.width = percent(1.0);
                    0.0
                } else {
                    OVERLAY_INSET
                };
                elem.on_top = true;
                built.push(elem.pinned(Pin { x: placement.x, y: placement.y, inset }));
            }
        }
        built
    }

    /// A dashed box that names `kind`, holding `node`'s children, for a kind
    /// with no look: nothing is silently dropped.
    fn unknown<'a>(&self, kind: &str, node: Option<&'a Node>) -> Elem<'a> {
        let mut style = Style { flex_direction: FlexDirection::Column, ..Style::default() };
        style.gap = taffy::Size { width: length(8.0), height: length(8.0) };
        style.padding = sides(8.0, 8.0, 8.0, 8.0);
        let font = self.tokens.font(None, "font.caption");
        let mut parts = vec![Elem::text(Style::default(), Words::new(kind.to_string(), font, Ink::Muted))];
        if let Some(node) = node {
            parts.append(&mut self.children(node));
        }
        Elem::new(style)
            .with(Look::Box { radius: 4.0, fill: None, stroke: Some(Ink::Muted), dashed: true })
            .holding(parts)
    }

    /// The value `key` holds in `object`, its templates and `cases` read in
    /// `scope`; null when it holds none.
    fn get(&self, object: &Map<String, Value>, key: &str, scope: &Scope) -> Value {
        match object.get(key) {
            Some(value) => self.resolve(value, scope),
            None => Value::Null,
        }
    }

    /// `value` read in `scope`: a template gives text, or the value itself
    /// when it is one placeholder; `cases` gives its chosen branch; lists
    /// and objects are read through.
    fn resolve(&self, value: &Value, scope: &Scope) -> Value {
        match value {
            Value::String(text) => self.template(text, scope),
            Value::Object(object) if object.contains_key("cases") => {
                match object.get("cases").and_then(|branches| self.branch(branches, scope)) {
                    Some(chosen) => self.resolve(chosen, scope),
                    None => Value::Null,
                }
            }
            Value::Object(object) => {
                let mut read = Map::new();
                for (key, value) in object {
                    read.insert(key.clone(), self.resolve(value, scope));
                }
                Value::Object(read)
            }
            Value::Array(items) => {
                let mut read = Vec::new();
                for item in items {
                    read.push(self.resolve(item, scope));
                }
                Value::Array(read)
            }
            other => other.clone(),
        }
    }

    /// The first branch of `cases` whose `if` holds, or its `else`.
    fn branch<'v>(&self, branches: &'v Value, scope: &Scope) -> Option<&'v Value> {
        let Value::Array(branches) = branches else {
            return None;
        };
        for branch in branches {
            if let Some(Value::String(condition)) = branch.get("if") {
                if self.holds(condition, scope) {
                    return branch.get("then");
                }
            } else if let Some(otherwise) = branch.get("else") {
                return Some(otherwise);
            }
        }
        None
    }

    /// `text` with each `{placeholder}` filled; the placeholder's own value,
    /// not text, when the whole of `text` is one placeholder.
    fn template(&self, text: &str, scope: &Scope) -> Value {
        if let Some(inner) = text.strip_prefix('{').and_then(|rest| rest.strip_suffix('}'))
            && !inner.contains('{')
            && !inner.contains('}')
        {
            return self.placeholder(inner, scope);
        }
        let mut filled = String::new();
        let mut rest = text;
        while let Some((before, after)) = rest.split_once('{') {
            filled.push_str(before);
            match after.split_once('}') {
                Some((inner, next)) => {
                    filled.push_str(&said(&self.placeholder(inner, scope)));
                    rest = next;
                }
                None => {
                    filled.push('{');
                    rest = after;
                    break;
                }
            }
        }
        filled.push_str(rest);
        Value::String(filled)
    }

    /// A path, then each `|filter` in turn: `or:value`, `count`, `first`,
    /// `last`, `head` and `tail` (the first word and the rest), `slashes`
    /// (2026-11-12 → 2026/11/12), `glyph`, `atLeast:n` and `atMost:n`.
    fn placeholder(&self, inner: &str, scope: &Scope) -> Value {
        let mut parts = inner.split('|');
        let mut value = scope.get(parts.next().unwrap_or("").trim());
        for filter in parts {
            let (name, argument) = filter.split_once(':').unwrap_or((filter, ""));
            value = match (name.trim(), value) {
                ("or", Value::Null) => {
                    serde_json::from_str(argument).unwrap_or(Value::String(argument.to_string()))
                }
                ("count", Value::Array(items)) => Value::from(items.len()),
                ("count", _) => Value::from(0),
                ("first", Value::Array(items)) => items.into_iter().next().unwrap_or(Value::Null),
                ("last", Value::Array(items)) => items.into_iter().last().unwrap_or(Value::Null),
                ("first" | "last", _) => Value::Null,
                ("head", value) => {
                    let text = said(&value);
                    Value::String(text.split_once(' ').map_or(text.as_str(), |(head, _)| head).to_string())
                }
                ("tail", value) => match said(&value).split_once(' ') {
                    Some((_, tail)) => Value::String(tail.trim().to_string()),
                    None => Value::Null,
                },
                ("slashes", value) => Value::String(said(&value).replace('-', "/")),
                ("glyph", value) => {
                    // A name that is no icon is drawn as a "?"; no name, as nothing.
                    let name = said(&value);
                    let glyph = underdraw_format::icons::glyph(&name).map(String::from);
                    Value::String(
                        glyph
                            .unwrap_or_else(|| if name.is_empty() { String::new() } else { "?".to_string() }),
                    )
                }
                ("atLeast", Value::Number(n)) => bound(&n, argument, f64::max),
                ("atMost", Value::Number(n)) => bound(&n, argument, f64::min),
                (_, value) => value,
            };
        }
        value
    }

    /// Whether `condition` holds in `scope`. Its grammar is a screen
    /// condition's (`&&`, `||`, `!`, `==`, `!=`, parentheses, literals), over
    /// placeholders such as `value|first`; one alone holds when it is given
    /// and not false.
    fn holds(&self, condition: &str, scope: &Scope) -> bool {
        let mut cursor = Cursor { tokens: lex(condition).into_iter() };
        truthy(&self.or(&mut cursor, scope))
    }

    fn or(&self, cursor: &mut Cursor, scope: &Scope) -> Value {
        let mut value = self.and(cursor, scope);
        while cursor.peek() == Some(Token::Or) {
            cursor.next();
            let right = self.and(cursor, scope);
            value = Value::Bool(truthy(&value) || truthy(&right));
        }
        value
    }

    fn and(&self, cursor: &mut Cursor, scope: &Scope) -> Value {
        let mut value = self.not(cursor, scope);
        while cursor.peek() == Some(Token::And) {
            cursor.next();
            let right = self.not(cursor, scope);
            value = Value::Bool(truthy(&value) && truthy(&right));
        }
        value
    }

    fn not(&self, cursor: &mut Cursor, scope: &Scope) -> Value {
        if cursor.peek() == Some(Token::Not) {
            cursor.next();
            return Value::Bool(!truthy(&self.not(cursor, scope)));
        }
        let left = self.atom(cursor, scope);
        match cursor.peek() {
            Some(Token::Eq) => {
                cursor.next();
                Value::Bool(left == self.atom(cursor, scope))
            }
            Some(Token::Ne) => {
                cursor.next();
                Value::Bool(left != self.atom(cursor, scope))
            }
            _ => left,
        }
    }

    fn atom(&self, cursor: &mut Cursor, scope: &Scope) -> Value {
        match cursor.next() {
            Some(Token::Open) => {
                let inner = self.or(cursor, scope);
                cursor.next();
                inner
            }
            Some(Token::Literal(value)) => value,
            Some(Token::Name(name)) => match name.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                "null" => Value::Null,
                _ => self.placeholder(&name, scope),
            },
            _ => Value::Null,
        }
    }
}

#[derive(Clone, PartialEq)]
enum Token {
    Or,
    And,
    Not,
    Eq,
    Ne,
    Open,
    Close,
    Literal(Value),
    /// A placeholder: a path and its filters, `value|first`.
    Name(String),
}

/// The tokens of a condition, read from the front.
struct Cursor {
    tokens: std::vec::IntoIter<Token>,
}

impl Cursor {
    fn peek(&self) -> Option<Token> {
        self.tokens.clone().next()
    }

    fn next(&mut self) -> Option<Token> {
        self.tokens.next()
    }
}

/// The tokens of `condition`. A `|` inside a name begins a filter; `||`
/// is "or". What is not the grammar is skipped: `check` names it.
fn lex(condition: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = condition.chars().peekable();
    while let Some(c) = chars.next() {
        let token = match c {
            _ if c.is_whitespace() => continue,
            '|' if chars.peek() == Some(&'|') => {
                chars.next();
                Token::Or
            }
            '&' if chars.peek() == Some(&'&') => {
                chars.next();
                Token::And
            }
            '=' if chars.peek() == Some(&'=') => {
                chars.next();
                Token::Eq
            }
            '!' if chars.peek() == Some(&'=') => {
                chars.next();
                Token::Ne
            }
            '!' => Token::Not,
            '(' => Token::Open,
            ')' => Token::Close,
            '"' => {
                let mut text = String::new();
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    text.push(c);
                }
                Token::Literal(Value::String(text))
            }
            _ if c.is_ascii_digit() || c == '-' => {
                let mut text = c.to_string();
                while let Some(&d) = chars.peek() {
                    if !(d.is_ascii_digit() || d == '.') {
                        break;
                    }
                    text.push(d);
                    chars.next();
                }
                Token::Literal(serde_json::from_str(&text).unwrap_or(Value::Null))
            }
            _ if c.is_alphanumeric() || c == '_' || c == '$' => {
                let mut name = c.to_string();
                while let Some(&d) = chars.peek() {
                    let filter = d == '|' && !matches!(chars.clone().nth(1), Some('|'));
                    if !(d.is_alphanumeric() || "_.:%$".contains(d) || filter) {
                        break;
                    }
                    name.push(d);
                    chars.next();
                }
                Token::Name(name)
            }
            _ => continue,
        };
        tokens.push(token);
    }
    tokens
}

/// Whether a value counts as given: anything but null and false.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// `n` held to `argument` by `keep` (max for at least, min for at most);
/// a whole number stays one, so it can still count a `repeat`.
fn bound(n: &serde_json::Number, argument: &str, keep: fn(f64, f64) -> f64) -> Value {
    let Ok(limit) = argument.trim().parse::<f64>() else {
        return Value::Number(n.clone());
    };
    let kept = keep(n.as_f64().unwrap_or(0.0), limit);
    if kept >= 0.0 && kept.fract() == 0.0 && kept <= u32::MAX as f64 {
        Value::from(kept as u64)
    } else {
        Value::from(kept)
    }
}

/// The ink `value` names: one of the wireframe's own (`muted`, `fill`, …),
/// or a colour token (`color.textSecondary`); none when it is null.
fn ink(value: &Value) -> Option<Ink> {
    Some(match value.as_str()? {
        "text" => Ink::Text,
        "muted" => Ink::Muted,
        "line" => Ink::Line,
        "fill" => Ink::Fill,
        "strong" => Ink::Strong,
        "paper" => Ink::Paper,
        "assumed" => Ink::Assumed,
        _ => tokens::ink(Some(value)),
    })
}

fn sides<T: FromLength>(top: f32, right: f32, bottom: f32, left: f32) -> taffy::Rect<T> {
    taffy::Rect { left: length(left), right: length(right), top: length(top), bottom: length(bottom) }
}

/// Grid tracks written "1fr", "58%" or 48 points; `check` has refused
/// anything else, which is drawn as "1fr".
fn tracks(widths: &[Value]) -> Vec<GridTemplateComponent<String>> {
    let mut tracks = Vec::new();
    for width in widths {
        tracks.push(match Track::read(width) {
            Some(Track::Part(part)) => percent(part),
            Some(Track::Points(points)) => length(points),
            Some(Track::Fr(share)) => fr(share),
            None => fr(1.0),
        });
    }
    tracks
}

/// An aspect ratio: a number, or "4:3" as a width over a height.
fn aspect(value: &Value) -> Option<f32> {
    if let Some(ratio) = value.as_f64() {
        return Some(ratio as f32);
    }
    let (width, height) = value.as_str()?.split_once(':')?;
    let (width, height) = (width.trim().parse::<f32>().ok()?, height.trim().parse::<f32>().ok()?);
    if height > 0.0 { Some(width / height) } else { None }
}

/// A value as the words a wireframe shows.
fn said(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                parts.push(said(item));
            }
            parts.join(", ")
        }
        _ => String::new(),
    }
}

// What each word a look key takes draws as: one function per key of
// underdraw_format::LOOK that takes words, and a test that they agree.

/// A box's `box`.
fn box_style(word: &str) -> Option<Style> {
    match word {
        "row" => Some(Style {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::CENTER,
            ..Style::default()
        }),
        "column" => Some(Style { flex_direction: FlexDirection::Column, ..Style::default() }),
        "grid" => Some(Style { display: Display::Grid, ..Style::default() }),
        _ => None,
    }
}

/// Text's `align`.
fn text_align(word: &str) -> Option<Align> {
    match word {
        "start" => Some(Align::Start),
        "center" => Some(Align::Center),
        "end" => Some(Align::End),
        _ => None,
    }
}

/// A box's `self`.
fn align_self(word: &str) -> Option<AlignSelf> {
    match word {
        "start" => Some(AlignSelf::FLEX_START),
        _ => None,
    }
}

/// A box's `items`.
fn align_items(word: &str) -> Option<AlignItems> {
    match word {
        "start" => Some(AlignItems::START),
        "center" => Some(AlignItems::CENTER),
        _ => None,
    }
}

/// A box's `justify`.
fn justify_content(word: &str) -> Option<JustifyContent> {
    match word {
        "center" => Some(JustifyContent::CENTER),
        "between" => Some(JustifyContent::SPACE_BETWEEN),
        "end" => Some(JustifyContent::END),
        _ => None,
    }
}

/// A box's `overflowX`.
fn overflow(word: &str) -> Option<taffy::Overflow> {
    match word {
        "clip" => Some(taffy::Overflow::Clip),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use underdraw_format::{LOOK, Takes};

    use super::*;

    /// Whether layout draws `word` for look key `key`.
    fn drawn(key: &str, word: &str) -> bool {
        match key {
            "box" => box_style(word).is_some(),
            "align" => text_align(word).is_some(),
            "self" => align_self(word).is_some(),
            "items" => align_items(word).is_some(),
            "justify" => justify_content(word).is_some(),
            "overflowX" => overflow(word).is_some(),
            _ => panic!("{key} takes words, and layout has no reader for it"),
        }
    }

    #[test]
    fn every_word_check_lets_through_is_drawn() {
        for (key, takes) in LOOK {
            if let Takes::Word(words) = takes {
                for word in words {
                    assert!(drawn(key, word), "{key}: {word} passes check but is not drawn");
                }
                assert!(!drawn(key, "nonsense"), "{key}: a word check refuses is drawn");
            }
        }
    }
}
