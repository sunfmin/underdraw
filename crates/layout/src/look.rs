// A style is built one property per line, as CSS reads.
#![allow(clippy::field_reassign_with_default)]

//! Components as data: what each component of a node looks like, parsed by
//! underdraw-format from the screen's own components (over underdraw's) and
//! evaluated here into boxes for `place` to lay out. A look is a box, text, a
//! `repeat`, `cases`, another component, or `"$children"`; it reads the
//! node's props through `{prop|filter}` templates. Nothing is parsed here. Why: [ADR 0007](../../../docs/adr/0007-kinds-are-data.md),
//! [ADR 0021](../../../docs/adr/0021-kinds-become-components-one-screens-or-the-projects.md).

use serde_json::{Map, Value};
use taffy::prelude::*;
use underdraw_format::look::{
    self, Condition, Element, Filter, Look as Parsed, Object, Piece, Placeholder, Template,
};
use underdraw_format::{Anchor, Atom, Components, Edge, Expr, Lang, Node, Track};

use crate::place::{Elem, Look, Pin, Words};
use crate::tokens::Tokens;
use crate::{Align, Ink};

/// The most a `repeat` makes, so a wrong count cannot run away.
const MOST_REPEATED: u64 = 1000;

/// How far in from its node's edges an overlay that is not flush stands.
const OVERLAY_INSET: f32 = 16.0;

/// What a look reads: the props of the component it draws, the names a `repeat`
/// binds (`n` from 1, `index` from 0, `item`), and the node whose children
/// `"$children"` stands for.
struct Scope<'p, 'a> {
    props: &'p Map<String, Value>,
    vars: Map<String, Value>,
    node: Option<&'a Node>,
}

impl Scope<'_, '_> {
    /// What `placeholder`'s path holds, before its filters.
    fn get(&self, placeholder: &Placeholder) -> Value {
        let Some(name) = placeholder.name() else {
            return Value::Null;
        };
        let mut value = match self.vars.get(name) {
            Some(value) => value.clone(),
            None => self.prop(name).cloned().unwrap_or(Value::Null),
        };
        for segment in placeholder.steps() {
            value = match (&value, segment.parse::<usize>()) {
                (Value::Array(items), Ok(at)) => items.get(at).cloned().unwrap_or(Value::Null),
                (Value::Object(fields), _) => fields.get(segment).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
        }
        value
    }

    /// The prop `name` of the component drawn.
    fn prop(&self, name: &str) -> Option<&Value> {
        #[cfg(test)]
        tests::looked_up(name);
        self.props.get(name)
    }
}

pub(crate) struct Catalog<'t> {
    pub(crate) tokens: &'t Tokens<'t>,
    pub(crate) components: &'t Components,
    /// The language a guess underdraw makes itself is said in.
    pub(crate) lang: Lang,
}

impl Catalog<'_> {
    /// `node` drawn as its component's look says, tagged as the node; a box with
    /// the component's name, marked as a guess, when no component has that name.
    pub(crate) fn node<'a>(&self, node: &'a Node) -> Elem<'a> {
        let mut elem = match self.components.look(&node.component) {
            Some(look) => {
                let scope = Scope { props: &node.props, vars: Map::new(), node: Some(node) };
                self.one(look, &scope)
            }
            None => {
                let mut elem = self.unknown(&node.component, Some(node));
                elem.assumed =
                    Some(underdraw_format::assumption::unknown_component(&node.component, self.lang));
                elem
            }
        };
        elem.source = Some(node);
        elem
    }

    /// The one box `look` makes: the first, or an empty one when it makes
    /// none.
    fn one<'a>(&self, look: &Parsed, scope: &Scope<'_, 'a>) -> Elem<'a> {
        let mut made = self.build(look, scope).into_iter();
        made.next().unwrap_or_else(|| Elem::new(Style::default()))
    }

    /// The boxes `look` makes in `scope`: none when its `if` fails, one per
    /// element for a `repeat`, the node's children for `"$children"`.
    fn build<'a>(&self, look: &Parsed, scope: &Scope<'_, 'a>) -> Vec<Elem<'a>> {
        let object = match look {
            Parsed::Children => {
                return match scope.node {
                    Some(node) => self.children(node),
                    None => Vec::new(),
                };
            }
            Parsed::List(looks) => {
                let mut made = Vec::new();
                for look in looks {
                    made.append(&mut self.build(look, scope));
                }
                return made;
            }
            Parsed::Object(object) => object,
            Parsed::Text { .. } | Parsed::Plain(_) => return Vec::new(),
        };
        if let Some(condition) = &object.when
            && !self.holds(condition, scope)
        {
            return Vec::new();
        }
        match object.element() {
            Element::Cases(cases) => match self.branch(cases, scope) {
                Some(chosen) => self.build(chosen, scope),
                None => Vec::new(),
            },
            Element::Repeat { over, each } => self.repeat(over, each, scope),
            Element::Component(name) => vec![self.nested(name, object, scope)],
            Element::Text => vec![self.text(object, scope)],
            Element::Box => vec![self.boxed(object, scope)],
        }
    }

    /// `each` once per element of `over`: a count, or a list whose elements
    /// are each `item`.
    fn repeat<'a>(&self, over: &Parsed, each: Option<&Parsed>, scope: &Scope<'_, 'a>) -> Vec<Elem<'a>> {
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
            if let Some(each) = each {
                made.append(&mut self.build(each, &inner));
            }
            index = index.saturating_add(1);
        }
        made
    }

    /// Another component's look, inside this one, with the props written beside
    /// `component`. It is part of this node, not a node of its own.
    fn nested<'a>(&self, name: &str, object: &Object, scope: &Scope<'_, 'a>) -> Elem<'a> {
        let mut props = Map::new();
        for key in &object.keys {
            if key.name != "component" && key.name != "if" {
                props.insert(key.name.clone(), self.resolve(&key.look, scope));
            }
        }
        match self.components.look(name) {
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
    fn boxed<'a>(&self, object: &Object, scope: &Scope<'_, 'a>) -> Elem<'a> {
        let get = |key: &str| self.get(object, key, scope);
        let mut style = get("box").as_str().and_then(look::Flow::read).map(box_style).unwrap_or_default();
        self.style(object, scope, &mut style);
        let mut gap = self.tokens.find_length(&get("gap"));
        let mut radius = self.tokens.find_length(&get("radius")).unwrap_or(0.0);
        let mut stroke = ink(self.tokens, &get("stroke"));
        let mut clip = None;
        if get("frame") == Value::Bool(true) {
            let [padding, framed_gap, framed_radius, border, elevation] =
                look::FRAMED.map(|name| scope.prop(name));
            if let Some([top, right, bottom, left]) = padding.and_then(|padding| self.four(padding)) {
                style.padding = sides(top, right, bottom, left);
            }
            gap = Some(self.tokens.length(framed_gap, gap.unwrap_or(0.0)));
            let framed = self.tokens.length(framed_radius, 0.0);
            if framed > 0.0 {
                radius = framed;
                clip = Some(framed);
            }
            if stroke.is_none() {
                stroke = match (border, elevation) {
                    (Some(border), _) => Some(self.tokens.ink(Some(border))),
                    (None, Some(_)) => Some(Ink::Line),
                    (None, None) => None,
                };
            }
        }
        if let Some(gap) = gap {
            style.gap = taffy::Size { width: length(gap), height: length(gap) };
        }
        let mut elem = Elem::new(style);
        let fill = ink(self.tokens, &get("fill"));
        if fill.is_some() || stroke.is_some() {
            elem = elem.with(Look::Box { radius, fill, stroke, dashed: get("dashed") == Value::Bool(true) });
        }
        if let Some(cross) = ink(self.tokens, &get("cross")) {
            elem = elem.with(Look::Cross(cross));
        }
        if let Some(line) = ink(self.tokens, &get("topLine")) {
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
    fn text<'a>(&self, object: &Object, scope: &Scope<'_, 'a>) -> Elem<'a> {
        let get = |key: &str| self.get(object, key, scope);
        let mut style = Style::default();
        self.style(object, scope, &mut style);
        let mut font = self.tokens.font(Some(&get("font")), "font.body");
        if let Some(weight) = get("weight").as_u64() {
            font.weight = weight.min(1000) as u16;
        }
        let mut words =
            Words::new(said(&get("text")), font, ink(self.tokens, &get("ink")).unwrap_or(Ink::Text));
        words.max_lines = get("maxLines").as_u64().map(|n| n.min(100) as u32);
        words.align =
            get("align").as_str().and_then(look::TextAlign::read).map(text_align).unwrap_or(Align::Start);
        words.underline = get("underline") == Value::Bool(true);
        words.strike = get("strike") == Value::Bool(true);
        self.pin(Elem::text(style, words), &get("pin"))
    }

    /// How a box sits among its siblings and holds its own: sizes, `grow`,
    /// `shrink`, `basis`, `self`, `items`, `justify`, `padding`, `margin`,
    /// grid `columns`, `rows` and `span`, and `overflowX`.
    fn style(&self, object: &Object, scope: &Scope, style: &mut Style) {
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
        if let Some(align) = get("self").as_str().and_then(look::SelfAlign::read) {
            style.align_self = Some(align_self(align));
        }
        if let Some(items) = get("items").as_str().and_then(look::Items::read) {
            style.align_items = align_items(items);
        }
        if let Some(justify) = get("justify").as_str().and_then(look::Justify::read) {
            style.justify_content = justify_content(justify);
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
        if let Some(x) = get("overflowX").as_str().and_then(look::Overflow::read) {
            style.overflow = taffy::Point { x: overflow(x), y: taffy::Overflow::Visible };
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

    /// A dashed box that names `component`, holding `node`'s children, for a component
    /// with no look: nothing is silently dropped.
    fn unknown<'a>(&self, component: &str, node: Option<&'a Node>) -> Elem<'a> {
        let mut style = Style { flex_direction: FlexDirection::Column, ..Style::default() };
        style.gap = taffy::Size { width: length(8.0), height: length(8.0) };
        style.padding = sides(8.0, 8.0, 8.0, 8.0);
        let font = self.tokens.font(None, "font.caption");
        let mut parts =
            vec![Elem::text(Style::default(), Words::new(component.to_string(), font, Ink::Muted))];
        if let Some(node) = node {
            parts.append(&mut self.children(node));
        }
        Elem::new(style)
            .with(Look::Box { radius: 4.0, fill: None, stroke: Some(Ink::Muted), dashed: true })
            .holding(parts)
    }

    /// The value `key` holds in `object`, its templates and `cases` read in
    /// `scope`; null when it holds none.
    fn get(&self, object: &Object, key: &str, scope: &Scope) -> Value {
        match object.get(key) {
            Some(look) => self.resolve(look, scope),
            None => Value::Null,
        }
    }

    /// `look` read in `scope` as a value: a template gives text, or the
    /// value itself when it is one placeholder; `cases` gives its chosen
    /// branch; lists and objects are read through.
    fn resolve(&self, look: &Parsed, scope: &Scope) -> Value {
        match look {
            Parsed::Children => Value::String("$children".to_string()),
            Parsed::Text { template, .. } => self.template(template, scope),
            Parsed::Object(object) => match &object.cases {
                Some(cases) => match self.branch(cases, scope) {
                    Some(chosen) => self.resolve(chosen, scope),
                    None => Value::Null,
                },
                None => {
                    let mut read = Map::new();
                    for key in &object.keys {
                        read.insert(key.name.clone(), self.resolve(&key.look, scope));
                    }
                    Value::Object(read)
                }
            },
            Parsed::List(items) => {
                let mut read = Vec::new();
                for item in items {
                    read.push(self.resolve(item, scope));
                }
                Value::Array(read)
            }
            Parsed::Plain(value) => value.clone(),
        }
    }

    /// The first branch of `cases` whose `if` holds, or its `else`.
    fn branch<'l>(&self, cases: &'l look::Cases, scope: &Scope) -> Option<&'l Parsed> {
        for branch in &cases.branches {
            if let Some(condition) = &branch.when {
                if self.holds(condition, scope) {
                    return branch.then.as_ref();
                }
            } else if let Some(otherwise) = &branch.otherwise {
                return Some(otherwise);
            }
        }
        None
    }

    /// `template` with each placeholder filled; the placeholder's own
    /// value, not text, when the whole of it is one placeholder.
    fn template(&self, template: &Template, scope: &Scope) -> Value {
        match template {
            Template::Whole(placeholder) => self.placeholder(placeholder, scope),
            Template::Pieces(pieces) => {
                let mut filled = String::new();
                for piece in pieces {
                    match piece {
                        Piece::Words(words) => filled.push_str(words),
                        Piece::Hole(placeholder) => {
                            filled.push_str(&said(&self.placeholder(placeholder, scope)))
                        }
                    }
                }
                Value::String(filled)
            }
        }
    }

    /// What `placeholder`'s path holds, through each of its filters in turn.
    fn placeholder(&self, placeholder: &Placeholder, scope: &Scope) -> Value {
        let mut value = scope.get(placeholder);
        for filter in &placeholder.filters {
            value = match (filter, value) {
                (Filter::Or(otherwise), Value::Null) => otherwise.clone(),
                (Filter::Count, Value::Array(items)) => Value::from(items.len()),
                (Filter::Count, _) => Value::from(0),
                (Filter::First, Value::Array(items)) => items.into_iter().next().unwrap_or(Value::Null),
                (Filter::Last, Value::Array(items)) => items.into_iter().last().unwrap_or(Value::Null),
                (Filter::First | Filter::Last, _) => Value::Null,
                (Filter::Head, value) => {
                    let text = said(&value);
                    Value::String(text.split_once(' ').map_or(text.as_str(), |(head, _)| head).to_string())
                }
                (Filter::Tail, value) => match said(&value).split_once(' ') {
                    Some((_, tail)) => Value::String(tail.trim().to_string()),
                    None => Value::Null,
                },
                (Filter::Slashes, value) => Value::String(said(&value).replace('-', "/")),
                (Filter::Glyph, value) => {
                    // A name that is no icon is drawn as a "?"; no name, as nothing.
                    let name = said(&value);
                    let glyph = underdraw_format::icons::glyph(&name).map(String::from);
                    Value::String(
                        glyph
                            .unwrap_or_else(|| if name.is_empty() { String::new() } else { "?".to_string() }),
                    )
                }
                (Filter::AtLeast(Some(limit)), Value::Number(n)) => bound(&n, *limit, f64::max),
                (Filter::AtMost(Some(limit)), Value::Number(n)) => bound(&n, *limit, f64::min),
                (Filter::Or(_) | Filter::AtLeast(_) | Filter::AtMost(_) | Filter::Unknown(_), value) => value,
            };
        }
        value
    }

    /// Whether `condition` holds in `scope`: what it comes to is given and
    /// not false. One that does not parse never holds; `check` names it.
    fn holds(&self, condition: &Condition, scope: &Scope) -> bool {
        match &condition.parsed {
            Ok(expr) => truthy(&self.expr(expr, scope)),
            Err(_) => false,
        }
    }

    /// What `expr` comes to in `scope`. Every side is read, so each name
    /// it reads is looked up.
    fn expr(&self, expr: &Expr<Placeholder>, scope: &Scope) -> Value {
        match expr {
            Expr::Or(items) => {
                let mut any = false;
                for item in items {
                    any |= truthy(&self.expr(item, scope));
                }
                Value::Bool(any)
            }
            Expr::And(items) => {
                let mut all = true;
                for item in items {
                    all &= truthy(&self.expr(item, scope));
                }
                Value::Bool(all)
            }
            Expr::Not(inner) => Value::Bool(!truthy(&self.expr(inner, scope))),
            Expr::Compare(op, left, right) => {
                Value::Bool(op.holds(&self.atom(left, scope), &self.atom(right, scope)))
            }
            Expr::Atom(atom) => self.atom(atom, scope),
        }
    }

    fn atom(&self, atom: &Atom<Placeholder>, scope: &Scope) -> Value {
        match atom {
            Atom::Name(placeholder) => self.placeholder(placeholder, scope),
            Atom::Str(text) => Value::String(text.clone()),
            Atom::Num(n) => Value::from(*n),
            Atom::Bool(b) => Value::Bool(*b),
            Atom::Null => Value::Null,
            Atom::Group(inner) => self.expr(inner, scope),
        }
    }
}

/// Whether a value counts as given: anything but null and false.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// `n` held to `limit` by `keep` (max for at least, min for at most);
/// a whole number stays one, so it can still count a `repeat`.
fn bound(n: &serde_json::Number, limit: f64, keep: fn(f64, f64) -> f64) -> Value {
    let kept = keep(n.as_f64().unwrap_or(0.0), limit);
    if kept >= 0.0 && kept.fract() == 0.0 && kept <= u32::MAX as f64 {
        Value::from(kept as u64)
    } else {
        Value::from(kept)
    }
}

/// The ink `value` names: one of the wireframe's own (`muted`, `fill`, …),
/// or a colour token (`color.textSecondary`); none when it is null.
fn ink(tokens: &Tokens, value: &Value) -> Option<Ink> {
    Some(match value.as_str()? {
        "text" => Ink::Text,
        "muted" => Ink::Muted,
        "line" => Ink::Line,
        "fill" => Ink::Fill,
        "strong" => Ink::Strong,
        "paper" => Ink::Paper,
        "assumed" => Ink::Assumed,
        _ => tokens.ink(Some(value)),
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

// What each word a look key takes draws as: one function per key that
// takes words, over the type underdraw_format::look reads it as.

/// A box's `box`.
fn box_style(flow: look::Flow) -> Style {
    match flow {
        look::Flow::Row => {
            Style { flex_direction: FlexDirection::Row, align_items: AlignItems::CENTER, ..Style::default() }
        }
        look::Flow::Column => Style { flex_direction: FlexDirection::Column, ..Style::default() },
        look::Flow::Grid => Style { display: Display::Grid, ..Style::default() },
    }
}

/// Text's `align`.
fn text_align(align: look::TextAlign) -> Align {
    match align {
        look::TextAlign::Start => Align::Start,
        look::TextAlign::Center => Align::Center,
        look::TextAlign::End => Align::End,
    }
}

/// A box's `self`.
fn align_self(align: look::SelfAlign) -> AlignSelf {
    match align {
        look::SelfAlign::Start => AlignSelf::FLEX_START,
    }
}

/// A box's `items`.
fn align_items(items: look::Items) -> AlignItems {
    match items {
        look::Items::Start => AlignItems::START,
        look::Items::Center => AlignItems::CENTER,
    }
}

/// A box's `justify`.
fn justify_content(justify: look::Justify) -> JustifyContent {
    match justify {
        look::Justify::Center => JustifyContent::CENTER,
        look::Justify::Between => JustifyContent::SPACE_BETWEEN,
        look::Justify::End => JustifyContent::END,
    }
}

/// A box's `overflowX`.
fn overflow(overflow: look::Overflow) -> taffy::Overflow {
    match overflow {
        look::Overflow::Clip => taffy::Overflow::Clip,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    use proptest::prelude::*;
    use serde_json::json;
    use underdraw_format::look::Look as Parsed;

    use super::*;

    thread_local! {
        static LOOKED_UP: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// Notes that evaluation looked up the prop `name`.
    pub(super) fn looked_up(name: &str) {
        LOOKED_UP.with(|all| all.borrow_mut().push(name.to_string()));
    }

    /// The props evaluating `look` with `props` looks up, and the names the
    /// parsed look says it reads, each as a set.
    fn both(look: &Value, props: &Map<String, Value>) -> (BTreeSet<String>, BTreeSet<String>) {
        let parsed = Parsed::parse(look);
        let mut read = Vec::new();
        parsed.reads(&[], &mut read);
        let tokens_json = json!({});
        let tokens = Tokens(&tokens_json);
        let components = underdraw_format::Screen::read(&json!({}), Default::default()).components;
        let catalog = Catalog { tokens: &tokens, components: &components, lang: Lang::default() };
        let scope = Scope { props, vars: Map::new(), node: None };
        LOOKED_UP.with(|all| all.borrow_mut().clear());
        match &parsed {
            Parsed::Object(object) if object.keys.is_empty() => {
                // A condition alone: `{"if": …}`.
                if let Some(condition) = &object.when {
                    catalog.holds(condition, &scope);
                }
            }
            Parsed::Text { template, .. } => {
                catalog.template(template, &scope);
            }
            _ => {
                catalog.build(&parsed, &scope);
            }
        }
        let looked_up = LOOKED_UP.with(|all| all.borrow_mut().drain(..).collect());
        (looked_up, read.into_iter().collect())
    }

    /// The names a look reads, among them those a `repeat` binds.
    const NAMES: [&str; 6] = ["a", "b", "c.d", "item", "n", "index"];

    fn placeholder() -> impl Strategy<Value = String> {
        let filter =
            prop::sample::select(vec!["", "|first", "|count", "|or:0", "|head", "|atMost:3", "|upper"]);
        (prop::sample::select(NAMES.to_vec()), filter).prop_map(|(name, filter)| format!("{name}{filter}"))
    }

    fn template() -> impl Strategy<Value = String> {
        let piece = prop_oneof![
            placeholder().prop_map(|p| format!("{{{p}}}")),
            prop::sample::select(vec!["", "x ", " of ", "-"]).prop_map(String::from),
        ];
        prop::collection::vec(piece, 0..4).prop_map(|pieces| pieces.concat())
    }

    /// A condition in the screen grammar, each name a placeholder.
    fn condition() -> impl Strategy<Value = String> {
        let atom = prop_oneof![
            placeholder(),
            Just("\"x\"".to_string()),
            Just("2".to_string()),
            Just("true".to_string()),
            Just("null".to_string()),
        ];
        atom.prop_recursive(3, 12, 2, |inner| {
            let op = prop::sample::select(vec!["&&", "||", "==", "!=", "<", ">="]);
            prop_oneof![
                (inner.clone(), op, inner.clone()).prop_map(|(l, op, r)| format!("{l} {op} {r}")),
                inner.clone().prop_map(|e| format!("!({e})")),
                inner.prop_map(|e| format!("({e})")),
            ]
        })
    }

    /// A value a look's key holds: a template, or a number.
    fn value() -> impl Strategy<Value = Value> {
        prop_oneof![template().prop_map(Value::String), Just(json!(8))]
    }

    /// A look every part of which is drawn, whatever the props: text and
    /// boxes holding only keys they always read, `repeat`s of a count.
    fn drawn() -> impl Strategy<Value = Value> {
        let text_keys =
            prop::sample::subsequence(vec!["font", "ink", "maxLines", "align", "width", "grow"], 0..3);
        let text = (template(), text_keys, value()).prop_map(|(text, keys, value)| {
            let mut object = Map::new();
            object.insert("text".to_string(), Value::String(text));
            for key in keys {
                object.insert(key.to_string(), value.clone());
            }
            Value::Object(object)
        });
        text.prop_recursive(3, 16, 3, |inner| {
            let box_keys =
                prop::sample::subsequence(vec!["box", "gap", "fill", "radius", "padding", "clip"], 0..3);
            prop_oneof![
                (prop::collection::vec(inner.clone(), 0..3), box_keys, value(), any::<bool>()).prop_map(
                    |(children, keys, value, frame)| {
                        let mut object = Map::new();
                        for key in keys {
                            object.insert(key.to_string(), value.clone());
                        }
                        if frame {
                            object.insert("frame".to_string(), json!(true));
                        }
                        object.insert("children".to_string(), Value::Array(children));
                        Value::Object(object)
                    }
                ),
                (1..3u64, inner).prop_map(|(count, each)| json!({ "repeat": count, "each": each })),
            ]
        })
    }

    /// Any look: also `if`, `cases` and a `repeat` over a placeholder,
    /// which leave parts of it undrawn.
    fn any_look() -> impl Strategy<Value = Value> {
        drawn().prop_recursive(2, 16, 3, |inner| {
            prop_oneof![
                (condition(), inner.clone()).prop_map(|(when, look)| {
                    let mut look = look;
                    if let Value::Object(object) = &mut look {
                        object.insert("if".to_string(), Value::String(when));
                    }
                    look
                }),
                (condition(), inner.clone(), inner.clone()).prop_map(|(when, then, otherwise)| {
                    json!({ "cases": [{ "if": when, "then": then }, { "else": otherwise }] })
                }),
                (placeholder(), inner)
                    .prop_map(|(over, each)| json!({ "repeat": format!("{{{over}}}"), "each": each })),
            ]
        })
    }

    fn props() -> impl Strategy<Value = Map<String, Value>> {
        let value = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            (0..4u64).prop_map(Value::from),
            Just(json!("x y")),
            Just(json!([1, 2])),
            Just(json!({ "d": true })),
        ];
        prop::collection::vec(value, 6).prop_map(|values| {
            let mut props = Map::new();
            for (name, value) in ["a", "b", "c", "item", "n", "index"].into_iter().zip(values) {
                props.insert(name.to_string(), value);
            }
            props
        })
    }

    proptest! {
        /// A condition or a template looks up exactly the names it says it
        /// reads.
        #[test]
        fn a_condition_or_template_looks_up_exactly_what_it_reads(
            condition in condition(),
            template in template(),
            props in props(),
        ) {
            let (looked_up, read) = both(&json!({ "if": condition }), &props);
            prop_assert_eq!(looked_up, read, "{}", condition);
            let (looked_up, read) = both(&Value::String(template.clone()), &props);
            prop_assert_eq!(looked_up, read, "{}", template);
        }

        /// A look every part of which is drawn looks up exactly the names it
        /// says it reads.
        #[test]
        fn a_drawn_look_looks_up_exactly_what_it_reads(look in drawn(), props in props()) {
            let (looked_up, read) = both(&look, &props);
            prop_assert_eq!(looked_up, read, "{}", look);
        }

        /// Any look looks up only names it says it reads: what `if`,
        /// `cases` or an empty `repeat` leaves undrawn it does not look up.
        #[test]
        fn any_look_looks_up_only_what_it_reads(look in any_look(), props in props()) {
            let (looked_up, read) = both(&look, &props);
            prop_assert!(looked_up.is_subset(&read), "{} looked up {:?}, reads {:?}", look, looked_up, read);
        }
    }
}
