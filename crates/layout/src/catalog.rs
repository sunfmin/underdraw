// A style is built one property per line, as CSS reads.
#![allow(clippy::field_reassign_with_default)]

//! The kind catalog: what each kind of node looks like as a wireframe, as
//! boxes for `place` to lay out. A property means the same on every kind
//! (`padding`, `gap`, `border`, `radius`); a kind not in the catalog is a
//! dashed box that names it, so nothing is silently dropped.

use serde_json::Value;
use taffy::prelude::*;
use underdraw_format::Node;

use crate::place::{Edge, Elem, Look, Pin, Words};
use crate::tokens::{self, BODY, Tokens};
use crate::{Align, Font, Ink};

pub(crate) struct Catalog<'t> {
    pub(crate) tokens: &'t Tokens<'t>,
}

/// How wide one card of a carousel is.
const CAROUSEL_ITEM: f32 = 280.0;

impl Catalog<'_> {
    pub(crate) fn node<'a>(&self, node: &'a Node) -> Elem<'a> {
        let mut elem = match node.kind.as_str() {
            "Screen" | "Scroll" | "Stack" | "List" | "FieldGroup" => self.frame(node, column(0.0), 0.0),
            "Card" => self.frame(node, column(16.0), 16.0),
            "Row" => self.frame(node, row(0.0), 0.0),
            "Section" => self.section(node),
            "NavBar" => {
                let mut style = row(8.0);
                style.size.height = length(56.0);
                style.padding = sides(16.0, 0.0);
                self.frame(node, style, 8.0)
            }
            "StickyBar" => {
                let mut style = row(16.0);
                style.justify_content = JustifyContent::SPACE_BETWEEN;
                self.frame(node, style, 16.0).with(Look::TopLine(Ink::Line))
            }
            "Grid" => self.grid(node),
            "Columns" => self.columns(node),
            "Carousel" => self.carousel(node),
            "Text" => self.text(node),
            "Link" => self.link(node),
            "Rating" => {
                let value = said(node.props.get("value"));
                let font = self.tokens.font(None, "font.subhead");
                Elem::text(Style::default(), Words::new(format!("★ {value}"), font, Ink::Text))
            }
            "Badge" => self.badge(node),
            "Button" => self.button(node),
            "IconButton" => icon_button(node, self.tokens.font(None, "font.subhead")),
            "Avatar" => self.avatar(node),
            "Divider" => {
                let mut style = Style::default();
                style.size.height = length(1.0);
                Elem::new(style).with(fill(0.0, Ink::Line))
            }
            "ListItem" => self.list_item(node),
            "Metric" => self.metric(node),
            "EmptyState" => self.empty_state(node),
            "ImageCarousel" => self.image_carousel(node),
            "ImageGrid" => self.image_grid(node),
            "Map" => map(node),
            "DateRangePicker" => self.date_range(node),
            "DateField" => self.date_field(node),
            "Stepper" => self.stepper(node),
            "PriceBreakdown" => price_breakdown(),
            "Skeleton" => skeleton(node),
            _ => {
                let mut elem = self.unknown(node);
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

    /// A box holding `node`'s children, with the properties every container
    /// shares: `padding`, `gap` (or `gap_or`), `border`, `radius`, `elevation`.
    fn frame<'a>(&self, node: &'a Node, mut style: Style, gap_or: f32) -> Elem<'a> {
        let props = &node.props;
        if props.contains_key("padding") {
            let padding = self.tokens.length(props.get("padding"), 0.0);
            style.padding = sides(padding, padding);
        }
        let gap = self.tokens.length(props.get("gap"), gap_or);
        style.gap = taffy::Size { width: length(gap), height: length(gap) };
        let radius = self.tokens.length(props.get("radius"), 0.0);
        let stroke = match (props.get("border"), props.get("elevation")) {
            (Some(border), _) => Some(tokens::ink(Some(border))),
            (None, Some(_)) => Some(Ink::Line),
            (None, None) => None,
        };
        let mut elem = Elem::new(style);
        if stroke.is_some() || node.kind == "Screen" {
            let fill = if node.kind == "Screen" { Some(Ink::Paper) } else { None };
            elem = elem.with(Look::Box { radius, fill, stroke, dashed: false });
        }
        if radius > 0.0 && node.kind != "Card" {
            elem.clip = Some(radius);
        }
        elem.holding(self.children(node))
    }

    /// `node`'s children in order: `leading`, the body (`children`, `item`,
    /// `action`), then `trailing` pushed to the far end; `overlay` floats
    /// over the box at its `anchor`.
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
                built.push(self.overlay(child));
            }
        }
        built
    }

    /// `node` floated over its parent at its `anchor`, such as
    /// "bottom-trailing".
    fn overlay<'a>(&self, node: &'a Node) -> Elem<'a> {
        let anchor = node.props.get("anchor").and_then(Value::as_str).unwrap_or("bottom-trailing");
        let x = if anchor.contains("trailing") { Edge::End } else { Edge::Start };
        let y = if anchor.contains("bottom") { Edge::End } else { Edge::Start };
        self.node(node).pinned(Pin { x, y, inset: 16.0 })
    }

    fn section<'a>(&self, node: &'a Node) -> Elem<'a> {
        let mut style = column(16.0);
        let padding = self.tokens.length(node.props.get("padding"), 0.0);
        style.padding = sides(padding, padding);
        let mut parts = Vec::new();
        let mut heading = Vec::new();
        if let Some(title) = node.props.get("title") {
            let font = self.tokens.font(None, "font.heading");
            heading.push(Elem::text(Style::default(), Words::new(said(Some(title)), font, Ink::Text)));
        }
        if let Some(subtitle) = node.props.get("subtitle") {
            let font = self.tokens.font(None, "font.body");
            heading.push(Elem::text(Style::default(), Words::new(said(Some(subtitle)), font, Ink::Muted)));
        }
        if !heading.is_empty() {
            parts.push(Elem::new(column(4.0)).holding(heading));
        }
        parts.append(&mut self.children(node));
        let gap = self.tokens.length(node.props.get("gap"), 16.0);
        style.gap = taffy::Size { width: length(gap), height: length(gap) };
        Elem::new(style).holding(parts)
    }

    fn grid<'a>(&self, node: &'a Node) -> Elem<'a> {
        let count = node.props.get("columns").and_then(Value::as_u64).unwrap_or(2).clamp(1, 12);
        let mut style = Style { display: Display::Grid, ..Style::default() };
        let mut tracks = Vec::new();
        for _ in 0..count {
            tracks.push(fr(1.0));
        }
        style.grid_template_columns = tracks;
        self.frame(node, style, 16.0)
    }

    /// Side-by-side columns: `columns` is a count, or a list of widths
    /// written "58%" or "1fr".
    fn columns<'a>(&self, node: &'a Node) -> Elem<'a> {
        let mut style = Style { display: Display::Grid, ..Style::default() };
        let mut tracks = Vec::new();
        match node.props.get("columns") {
            Some(Value::Array(widths)) => {
                for width in widths {
                    tracks.push(track(width.as_str().unwrap_or("1fr")));
                }
            }
            Some(count) => {
                for _ in 0..count.as_u64().unwrap_or(1).clamp(1, 12) {
                    tracks.push(fr(1.0));
                }
            }
            None => tracks.push(fr(1.0)),
        }
        style.grid_template_columns = tracks;
        style.align_items = AlignItems::START;
        self.frame(node, style, 24.0)
    }

    fn carousel<'a>(&self, node: &'a Node) -> Elem<'a> {
        let mut style = row(16.0);
        style.align_items = AlignItems::START;
        style.overflow = taffy::Point { x: taffy::Overflow::Clip, y: taffy::Overflow::Visible };
        let mut elem = self.frame(node, style, 16.0);
        for child in &mut elem.children {
            child.style.size.width = length(CAROUSEL_ITEM);
            child.style.flex_shrink = 0.0;
        }
        elem.clip = Some(0.0);
        elem
    }

    fn text<'a>(&self, node: &'a Node) -> Elem<'a> {
        let props = &node.props;
        let mut words = Words::new(
            said(props.get("text")),
            self.tokens.font(props.get("font"), "font.body"),
            tokens::ink(props.get("color")),
        );
        words.max_lines = props.get("maxLines").and_then(Value::as_u64).map(|n| n.min(100) as u32);
        words.underline = props.get("underline") == Some(&Value::Bool(true));
        if props.get("align").and_then(Value::as_str) == Some("center") {
            words.align = Align::Center;
        }
        Elem::text(Style::default(), words)
    }

    fn link<'a>(&self, node: &'a Node) -> Elem<'a> {
        let props = &node.props;
        let mut text = said(props.get("text"));
        if props.contains_key("trailingIcon") {
            text.push_str(" ›");
        }
        let mut words = Words::new(text, self.tokens.font(props.get("font"), "font.subhead"), Ink::Text);
        words.underline = true;
        Elem::text(start(), words)
    }

    fn badge<'a>(&self, node: &'a Node) -> Elem<'a> {
        let font = self.tokens.font(None, "font.caption");
        let words = Elem::text(Style::default(), Words::new(said(node.props.get("text")), font, Ink::Text));
        let mut style = start();
        style.padding = sides(10.0, 4.0);
        Elem::new(style).with(outline(999.0, Ink::Text)).holding(vec![words])
    }

    fn button<'a>(&self, node: &'a Node) -> Elem<'a> {
        let props = &node.props;
        let small = props.get("size").and_then(Value::as_str) == Some("small");
        let primary = props.get("variant").and_then(Value::as_str) != Some("secondary");
        let disabled = props.get("disabled") == Some(&Value::Bool(true));
        let mut font = self.tokens.font(None, if small { "font.caption" } else { "font.subhead" });
        font.weight = 600;
        let (look, ink) = match (primary, disabled) {
            (_, true) => (fill(8.0, Ink::Fill), Ink::Muted),
            (true, false) => (fill(8.0, Ink::Strong), Ink::Paper),
            (false, false) => (outline(8.0, Ink::Text), Ink::Text),
        };
        let mut style = row(0.0);
        style.justify_content = JustifyContent::CENTER;
        style.size.height = length(if small { 32.0 } else { 48.0 });
        style.padding = sides(if small { 12.0 } else { 24.0 }, 0.0);
        let words = Elem::text(Style::default(), Words::new(said(props.get("text")), font, ink));
        Elem::new(style).with(look).holding(vec![words])
    }

    fn avatar<'a>(&self, node: &'a Node) -> Elem<'a> {
        let size = self.tokens.length(node.props.get("size"), 40.0);
        let mut style = start();
        style.size = taffy::Size { width: length(size), height: length(size) };
        Elem::new(style).with(fill(999.0, Ink::Fill))
    }

    fn list_item<'a>(&self, node: &'a Node) -> Elem<'a> {
        let props = &node.props;
        let mut style = row(12.0);
        style.padding = sides(0.0, 12.0);
        let mut parts = Vec::new();
        let icon_font = self.tokens.font(None, "font.subhead");
        if let Some(icon) = props.get("leadingIcon").and_then(Value::as_str) {
            parts.push(glyph_box(icon, 24.0, icon_font, None));
        }
        let mut title = Words::new(said(props.get("title")), self.tokens.font(None, "font.body"), Ink::Text);
        title.strike = props.get("strikethrough") == Some(&Value::Bool(true));
        let mut texts = vec![Elem::text(Style::default(), title)];
        if let Some(detail) = props.get("detail") {
            let font = self.tokens.font(None, "font.caption");
            texts.push(Elem::text(Style::default(), Words::new(said(Some(detail)), font, Ink::Muted)));
        }
        let mut body = column(2.0);
        body.flex_grow = 1.0;
        body.flex_shrink = 1.0;
        parts.push(Elem::new(body).holding(texts));
        if let Some(icon) = props.get("trailingIcon").and_then(Value::as_str) {
            parts.push(glyph_box(icon, 24.0, icon_font, None));
        }
        Elem::new(style).holding(parts)
    }

    fn metric<'a>(&self, node: &'a Node) -> Elem<'a> {
        let value =
            Words::new(said(node.props.get("value")), self.tokens.font(None, "font.price"), Ink::Text);
        let label =
            Words::new(said(node.props.get("label")), self.tokens.font(None, "font.caption"), Ink::Muted);
        Elem::new(column(2.0))
            .holding(vec![Elem::text(Style::default(), value), Elem::text(Style::default(), label)])
    }

    fn empty_state<'a>(&self, node: &'a Node) -> Elem<'a> {
        let props = &node.props;
        let mut style = column(8.0);
        style.align_items = AlignItems::CENTER;
        style.padding = sides(24.0, 24.0);
        let icon = props.get("icon").and_then(Value::as_str).unwrap_or("");
        let mut parts = vec![glyph_box(icon, 48.0, self.tokens.font(None, "font.heading"), Some(Ink::Fill))];
        let mut title =
            Words::new(said(props.get("title")), self.tokens.font(None, "font.subhead"), Ink::Text);
        title.align = Align::Center;
        parts.push(Elem::text(Style::default(), title));
        if let Some(detail) = props.get("detail") {
            let mut words = Words::new(said(Some(detail)), self.tokens.font(None, "font.body"), Ink::Muted);
            words.align = Align::Center;
            parts.push(Elem::text(Style::default(), words));
        }
        parts.append(&mut self.children(node));
        Elem::new(style).holding(parts)
    }

    /// One photo at a time, with "1 / N" over its corner.
    fn image_carousel<'a>(&self, node: &'a Node) -> Elem<'a> {
        let count = match node.props.get("items") {
            Some(Value::Array(items)) => items.len(),
            _ => 0,
        };
        let mut style = Style::default();
        style.aspect_ratio = Some(aspect(node.props.get("aspectRatio")).unwrap_or(4.0 / 3.0));
        let mut counter = Style::default();
        counter.padding = sides(10.0, 4.0);
        let font = self.tokens.font(None, "font.caption");
        let words = Elem::text(Style::default(), Words::new(format!("1 / {count}"), font, Ink::Paper));
        let counter = Elem::new(counter).with(fill(999.0, Ink::Strong)).holding(vec![words]).pinned(Pin {
            x: Edge::End,
            y: Edge::End,
            inset: 12.0,
        });
        Elem::new(style).with(fill(0.0, Ink::Fill)).with(Look::Cross(Ink::Line)).holding(vec![counter])
    }

    /// One large photo beside four small ones.
    fn image_grid<'a>(&self, node: &'a Node) -> Elem<'a> {
        let count = match node.props.get("items") {
            Some(Value::Array(items)) => items.len(),
            _ => 0,
        };
        let radius = self.tokens.length(node.props.get("radius"), 0.0);
        let mut style = Style { display: Display::Grid, ..Style::default() };
        style.grid_template_columns = vec![fr(1.0), fr(1.0), fr(1.0), fr(1.0)];
        style.grid_template_rows = vec![fr(1.0), fr(1.0)];
        style.gap = taffy::Size { width: length(8.0), height: length(8.0) };
        style.aspect_ratio = Some(2.0);
        let mut parts = Vec::new();
        for index in 0..count.min(5) {
            let mut cell = Style::default();
            if index == 0 {
                cell.grid_column = span(2);
                cell.grid_row = span(2);
            }
            parts.push(Elem::new(cell).with(fill(0.0, Ink::Fill)).with(Look::Cross(Ink::Line)));
        }
        for child in &node.children {
            if child.slot == "overlay" {
                parts.push(self.overlay(child));
            }
        }
        let mut elem = Elem::new(style).holding(parts);
        elem.clip = Some(radius);
        elem
    }

    /// The chosen dates over one month per `months`, each a grid of days.
    fn date_range<'a>(&self, node: &'a Node) -> Elem<'a> {
        let caption = self.tokens.font(None, "font.caption");
        let mut parts = Vec::new();
        let chosen = match node.props.get("value") {
            Some(Value::Array(dates)) if dates.iter().all(Value::is_string) => {
                let first = said(dates.first());
                let last = said(dates.last());
                format!("{} – {}", first.replace('-', "/"), last.replace('-', "/"))
            }
            _ => "选择日期".to_string(),
        };
        parts.push(Elem::text(
            Style::default(),
            Words::new(chosen, self.tokens.font(None, "font.subhead"), Ink::Text),
        ));
        let months = node.props.get("months").and_then(Value::as_u64).unwrap_or(1).clamp(1, 3);
        let mut row_of_months = Vec::new();
        for _ in 0..months {
            let mut style = Style { display: Display::Grid, ..Style::default() };
            style.grid_template_columns = vec![fr(1.0); 7];
            style.gap = taffy::Size { width: length(2.0), height: length(2.0) };
            style.flex_grow = 1.0;
            style.flex_basis = length(0.0);
            let mut days = Vec::new();
            for day in 1..=30u32 {
                let mut cell = row(0.0);
                cell.justify_content = JustifyContent::CENTER;
                cell.size.height = length(32.0);
                let mut words = Words::new(day.to_string(), caption, Ink::Muted);
                words.align = Align::Center;
                days.push(Elem::new(cell).holding(vec![Elem::text(Style::default(), words)]));
            }
            row_of_months.push(Elem::new(style).holding(days));
        }
        let mut style = row(24.0);
        style.align_items = AlignItems::START;
        parts.push(Elem::new(style).holding(row_of_months));
        Elem::new(column(12.0)).holding(parts)
    }

    fn date_field<'a>(&self, node: &'a Node) -> Elem<'a> {
        let mut label_font = self.tokens.font(None, "font.caption");
        label_font.weight = 600;
        let label = Words::new(said(node.props.get("label")), label_font, Ink::Text);
        let value = match node.props.get("value") {
            Some(Value::String(date)) => Words::new(date.replace('-', "/"), BODY, Ink::Text),
            _ => Words::new("添加日期".to_string(), BODY, Ink::Muted),
        };
        let mut style = column(2.0);
        style.padding = sides(12.0, 10.0);
        style.flex_grow = 1.0;
        style.flex_basis = length(0.0);
        Elem::new(style)
            .with(outline(0.0, Ink::Line))
            .holding(vec![Elem::text(Style::default(), label), Elem::text(Style::default(), value)])
    }

    fn stepper<'a>(&self, node: &'a Node) -> Elem<'a> {
        let font = self.tokens.font(None, "font.body");
        let label = Elem::text(Style::default(), Words::new(said(node.props.get("label")), font, Ink::Text));
        let value = Elem::text(Style::default(), Words::new(said(node.props.get("value")), font, Ink::Text));
        let controls = Elem::new(row(12.0)).holding(vec![
            glyph_box("minus", 32.0, font, None),
            value,
            glyph_box("plus", 32.0, font, None),
        ]);
        let mut style = row(0.0);
        style.justify_content = JustifyContent::SPACE_BETWEEN;
        style.padding = sides(12.0, 8.0);
        Elem::new(style).holding(vec![label, controls])
    }

    fn unknown<'a>(&self, node: &'a Node) -> Elem<'a> {
        let mut style = column(8.0);
        style.padding = sides(8.0, 8.0);
        let font = self.tokens.font(None, "font.caption");
        let mut parts = vec![Elem::text(Style::default(), Words::new(node.kind.clone(), font, Ink::Muted))];
        parts.append(&mut self.children(node));
        Elem::new(style)
            .with(Look::Box { radius: 4.0, fill: None, stroke: Some(Ink::Muted), dashed: true })
            .holding(parts)
    }
}

fn icon_button(node: &Node, font: Font) -> Elem<'_> {
    let icon = node.props.get("icon").and_then(Value::as_str).unwrap_or("");
    let mut elem = glyph_box(icon, 40.0, font, None);
    elem.looks =
        vec![Look::Box { radius: 20.0, fill: Some(Ink::Paper), stroke: Some(Ink::Line), dashed: false }];
    elem
}

/// A map: a gray area of `aspectRatio`, its marker in the middle.
fn map(node: &Node) -> Elem<'_> {
    let mut style = Style::default();
    style.aspect_ratio = Some(aspect(node.props.get("aspectRatio")).unwrap_or(16.0 / 9.0));
    let mut marker = Style::default();
    marker.size = taffy::Size { width: length(24.0), height: length(24.0) };
    let marker = Elem::new(marker).with(fill(999.0, Ink::Strong)).pinned(Pin {
        x: Edge::Center,
        y: Edge::Center,
        inset: 0.0,
    });
    Elem::new(style).with(fill(0.0, Ink::Fill)).holding(vec![marker])
}

/// Lines of a price, not yet designed: gray bars for each label and amount.
fn price_breakdown<'a>() -> Elem<'a> {
    let mut lines = Vec::new();
    for label in [0.5, 0.35, 0.4] {
        lines.push(Elem::new(spread()).holding(vec![bar(label), bar(0.2)]));
    }
    lines.push(divider());
    lines.push(Elem::new(spread()).holding(vec![bar(0.3), bar(0.25)]));
    Elem::new(column(12.0)).holding(lines)
}

/// What a phase shows while data is on its way: `shape` lists
/// "image 4:3" and "line 70%".
fn skeleton(node: &Node) -> Elem<'_> {
    let mut parts = Vec::new();
    let mut style = column(12.0);
    style.padding = sides(16.0, 16.0);
    if let Some(Value::Array(shapes)) = node.props.get("shape") {
        for shape in shapes {
            let shape = shape.as_str().unwrap_or("");
            if let Some(ratio) = shape.strip_prefix("image ") {
                let mut image = Style::default();
                image.aspect_ratio = Some(aspect(Some(&Value::from(ratio))).unwrap_or(4.0 / 3.0));
                image.margin = taffy::Rect {
                    left: length(-16.0),
                    right: length(-16.0),
                    top: length(-16.0),
                    bottom: length(0.0),
                };
                parts.push(Elem::new(image).with(fill(0.0, Ink::Fill)));
            } else if let Some(width) = shape.strip_prefix("line ") {
                let share = width.trim_end_matches('%').parse::<f32>().unwrap_or(100.0) / 100.0;
                parts.push(bar(share));
            }
        }
    }
    Elem::new(style).holding(parts)
}

fn bar<'a>(share: f32) -> Elem<'a> {
    let mut style = Style::default();
    style.size = taffy::Size { width: percent(share), height: length(14.0) };
    Elem::new(style).with(fill(4.0, Ink::Fill))
}

fn divider<'a>() -> Elem<'a> {
    let mut style = Style::default();
    style.size.height = length(1.0);
    Elem::new(style).with(fill(0.0, Ink::Line))
}

/// A square of `size` with an icon's stand-in glyph in its middle.
fn glyph_box<'a>(icon: &str, size: f32, font: Font, background: Option<Ink>) -> Elem<'a> {
    let mut style = row(0.0);
    style.justify_content = JustifyContent::CENTER;
    style.size = taffy::Size { width: length(size), height: length(size) };
    style.flex_shrink = 0.0;
    let mut words = Words::new(glyph(icon).to_string(), font, Ink::Text);
    words.align = Align::Center;
    let mut elem = Elem::new(style).holding(vec![Elem::text(Style::default(), words)]);
    if let Some(ink) = background {
        elem = elem.with(fill(999.0, ink));
    }
    elem
}

/// A character standing in for an icon; the icon vocabulary is not settled.
fn glyph(icon: &str) -> &'static str {
    match icon {
        "back" => "‹",
        "close" => "×",
        "share" => "↑",
        "heart" => "♡",
        "heart.fill" => "♥",
        "chevron.right" => "›",
        "warning" => "!",
        "minus" => "−",
        "plus" => "+",
        "star" => "★",
        "" => "",
        // SF Symbols' convention: `x.fill` is the solid form of `x`, so a
        // toggled state (done, saved) reads apart from its outline.
        _ if icon.ends_with(".fill") => "◆",
        _ => "◇",
    }
}

fn column(gap: f32) -> Style {
    Style {
        flex_direction: FlexDirection::Column,
        gap: taffy::Size { width: length(gap), height: length(gap) },
        ..Style::default()
    }
}

fn row(gap: f32) -> Style {
    Style {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::CENTER,
        gap: taffy::Size { width: length(gap), height: length(gap) },
        ..Style::default()
    }
}

/// A row whose two ends sit at the far sides.
fn spread() -> Style {
    let mut style = row(16.0);
    style.justify_content = JustifyContent::SPACE_BETWEEN;
    style
}

/// A box that takes the width of its content, not of its column.
fn start() -> Style {
    Style { align_self: Some(AlignSelf::FLEX_START), ..Style::default() }
}

fn sides(horizontal: f32, vertical: f32) -> taffy::Rect<LengthPercentage> {
    taffy::Rect {
        left: length(horizontal),
        right: length(horizontal),
        top: length(vertical),
        bottom: length(vertical),
    }
}

fn fill(radius: f32, ink: Ink) -> Look {
    Look::Box { radius, fill: Some(ink), stroke: None, dashed: false }
}

fn outline(radius: f32, ink: Ink) -> Look {
    Look::Box { radius, fill: None, stroke: Some(ink), dashed: false }
}

/// A column width written "58%" or "1fr".
fn track(width: &str) -> GridTemplateComponent<String> {
    if let Some(share) = width.strip_suffix('%') {
        return percent(share.parse::<f32>().unwrap_or(100.0) / 100.0);
    }
    fr(width.trim_end_matches("fr").parse::<f32>().unwrap_or(1.0))
}

/// "4:3" as a width over a height.
fn aspect(value: Option<&Value>) -> Option<f32> {
    let (width, height) = value?.as_str()?.split_once(':')?;
    let (width, height) = (width.trim().parse::<f32>().ok()?, height.trim().parse::<f32>().ok()?);
    if height > 0.0 { Some(width / height) } else { None }
}

/// A property's value as the words a wireframe shows.
fn said(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Array(items)) => {
            let mut parts = Vec::new();
            for item in items {
                parts.push(said(Some(item)));
            }
            parts.join(", ")
        }
        _ => String::new(),
    }
}
