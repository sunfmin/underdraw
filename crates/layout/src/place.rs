//! Geometry: the catalog's boxes laid out by taffy, then turned into draws
//! in page coordinates, with the tags a person refers to them by.

use taffy::{AvailableSpace, LayoutOutput, NodeId, Style, TaffyError, TaffyTree};
use underdraw_format::Node;

use crate::{Align, Draw, Font, Ink, Measure, Page, Placed, Rect};

/// One box to lay out: how it sits among its siblings (`style`), how it
/// looks, the text it holds when it is a leaf, and the node it shows.
pub(crate) struct Elem<'a> {
    pub(crate) style: Style,
    pub(crate) looks: Vec<Look>,
    pub(crate) words: Option<Words>,
    pub(crate) source: Option<&'a Node>,
    /// Its children are cut to it, with corners of this radius.
    pub(crate) clip: Option<f32>,
    pub(crate) children: Vec<Elem<'a>>,
}

/// What a box draws over its own area, before its children.
pub(crate) enum Look {
    Box { radius: f32, fill: Option<Ink>, stroke: Option<Ink>, dashed: bool },
    Cross(Ink),
    TopLine(Ink),
}

pub(crate) struct Words {
    pub(crate) text: String,
    pub(crate) font: Font,
    pub(crate) ink: Ink,
    pub(crate) max_lines: Option<u32>,
    pub(crate) align: Align,
    pub(crate) underline: bool,
    pub(crate) strike: bool,
}

impl<'a> Elem<'a> {
    pub(crate) fn new(style: Style) -> Elem<'a> {
        Elem { style, looks: Vec::new(), words: None, source: None, clip: None, children: Vec::new() }
    }

    pub(crate) fn text(style: Style, words: Words) -> Elem<'a> {
        let mut elem = Elem::new(style);
        elem.words = Some(words);
        elem
    }

    pub(crate) fn with(mut self, look: Look) -> Elem<'a> {
        self.looks.push(look);
        self
    }

    pub(crate) fn holding(mut self, children: Vec<Elem<'a>>) -> Elem<'a> {
        self.children = children;
        self
    }
}

impl Words {
    pub(crate) fn new(text: String, font: Font, ink: Ink) -> Words {
        Words { text, font, ink, max_lines: None, align: Align::Start, underline: false, strike: false }
    }
}

/// The tree as taffy holds it, beside the `Elem` tree it was made from.
struct Laid {
    id: NodeId,
    children: Vec<Laid>,
}

/// Where draws go while the tree is walked.
struct Out {
    draws: Vec<Draw>,
    /// Tags go over everything else, and are never cut by a clip.
    tags: Vec<Draw>,
    nodes: Vec<Placed>,
    next_label: u32,
}

pub(crate) fn place(root: &Elem, width: f32, measure: &mut dyn Measure) -> Result<Page, TaffyError> {
    let mut tree: TaffyTree<usize> = TaffyTree::new();
    let mut texts = Vec::new();
    let laid = add(&mut tree, root, &mut texts)?;
    let mut style = root.style.clone();
    style.size.width = taffy::Dimension::length(width);
    tree.set_style(laid.id, style)?;
    let space = taffy::Size { width: AvailableSpace::Definite(width), height: AvailableSpace::MaxContent };
    tree.compute_layout_with_measure(laid.id, space, |input, _, context, _| {
        let known = input.known_dimensions;
        let words = match context {
            Some(index) => texts.get(*index),
            None => None,
        };
        let Some(words) = words else {
            return LayoutOutput::from_outer_size(taffy::Size {
                width: known.width.unwrap_or(0.0),
                height: known.height.unwrap_or(0.0),
            });
        };
        let max_width = match (known.width, input.available_space.width) {
            (Some(width), _) | (None, AvailableSpace::Definite(width)) => Some(width),
            (None, AvailableSpace::MinContent) => Some(0.0),
            (None, AvailableSpace::MaxContent) => None,
        };
        let set = measure.text(&words.text, words.font, max_width);
        let height = match words.max_lines {
            Some(lines) => set.height.min(lines as f32 * words.font.line_height),
            None => set.height,
        };
        LayoutOutput::from_outer_size(taffy::Size {
            width: known.width.unwrap_or(set.width),
            height: known.height.unwrap_or(height),
        })
    })?;
    let mut out = Out { draws: Vec::new(), tags: Vec::new(), nodes: Vec::new(), next_label: 1 };
    emit(&tree, root, &laid, (0.0, 0.0), None, &mut out)?;
    let size = tree.layout(laid.id)?.size;
    let mut draws = out.draws;
    draws.append(&mut out.tags);
    Ok(Page { width: size.width, height: size.height, draws, nodes: out.nodes })
}

fn add<'e>(
    tree: &mut TaffyTree<usize>,
    elem: &'e Elem,
    texts: &mut Vec<&'e Words>,
) -> Result<Laid, TaffyError> {
    if let Some(words) = &elem.words {
        texts.push(words);
        let id = tree.new_leaf_with_context(elem.style.clone(), texts.len() - 1)?;
        return Ok(Laid { id, children: Vec::new() });
    }
    let mut children = Vec::new();
    let mut ids = Vec::new();
    for child in &elem.children {
        let laid = add(tree, child, texts)?;
        ids.push(laid.id);
        children.push(laid);
    }
    let id = tree.new_with_children(elem.style.clone(), &ids)?;
    Ok(Laid { id, children })
}

/// Draws `elem`, whose parent's corner is at `origin`, and everything in it.
/// `clip` is the area it may show in, when an ancestor cuts it.
fn emit(
    tree: &TaffyTree<usize>,
    elem: &Elem,
    laid: &Laid,
    origin: (f32, f32),
    clip: Option<Rect>,
    out: &mut Out,
) -> Result<(), TaffyError> {
    let layout = tree.layout(laid.id)?;
    let rect = Rect {
        x: origin.0 + layout.location.x,
        y: origin.1 + layout.location.y,
        width: layout.size.width,
        height: layout.size.height,
    };
    for look in &elem.looks {
        out.draws.push(match look {
            Look::Box { radius, fill, stroke, dashed } => Draw::Box {
                rect,
                radius: radius.min(rect.width / 2.0).min(rect.height / 2.0),
                fill: *fill,
                stroke: *stroke,
                dashed: *dashed,
            },
            Look::Cross(ink) => Draw::Cross { rect, ink: *ink },
            Look::TopLine(ink) => {
                Draw::Line { from: [rect.x, rect.y], to: [rect.x + rect.width, rect.y], ink: *ink }
            }
        });
    }
    if let Some(words) = &elem.words {
        out.draws.push(Draw::Text {
            rect,
            text: words.text.clone(),
            font: words.font,
            ink: words.ink,
            max_lines: words.max_lines,
            align: words.align,
            underline: words.underline,
            strike: words.strike,
        });
    }
    if let Some(node) = elem.source {
        record(node, rect, clip, out);
    }
    let corner = (rect.x, rect.y);
    let (inner, saved) = match elem.clip {
        Some(_) => (Some(cut(rect, clip)), Some(std::mem::take(&mut out.draws))),
        None => (clip, None),
    };
    for index in 0..elem.children.len() {
        if let (Some(child), Some(laid)) = (elem.children.get(index), laid.children.get(index)) {
            emit(tree, child, laid, corner, inner, out)?;
        }
    }
    if let (Some(radius), Some(saved)) = (elem.clip, saved) {
        let draws = std::mem::replace(&mut out.draws, saved);
        out.draws.push(Draw::Clip { rect, radius, draws });
    }
    Ok(())
}

/// Notes where `node` went, and tags it: a number when a person can act on
/// it, a dashed outline and "?" when it is assumed. A node cut away entirely
/// gets no tag.
fn record(node: &Node, rect: Rect, clip: Option<Rect>, out: &mut Out) {
    let seen = match clip {
        Some(area) => overlaps(rect, area),
        None => true,
    };
    let mut label = None;
    if seen && node.interactive {
        label = Some(out.next_label);
        let text = out.next_label.to_string();
        out.next_label = out.next_label.saturating_add(1);
        out.tags.push(Draw::Tag { rect: tag(rect.x - 6.0, rect.y - 6.0), text, ink: Ink::Strong });
    }
    if seen && node.assumed.is_some() {
        let around =
            Rect { x: rect.x - 2.0, y: rect.y - 2.0, width: rect.width + 4.0, height: rect.height + 4.0 };
        out.tags.push(Draw::Box {
            rect: around,
            radius: 4.0,
            fill: None,
            stroke: Some(Ink::Assumed),
            dashed: true,
        });
        let corner = tag(rect.x + rect.width - 12.0, rect.y - 6.0);
        out.tags.push(Draw::Tag { rect: corner, text: "?".to_string(), ink: Ink::Assumed });
    }
    if !node.id.is_empty() {
        out.nodes.push(Placed {
            id: node.id.clone(),
            kind: node.kind.clone(),
            rect,
            label,
            assumed: node.assumed.clone(),
        });
    }
}

fn tag(x: f32, y: f32) -> Rect {
    Rect { x, y, width: 18.0, height: 18.0 }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// `rect` cut to `clip`, when there is one.
fn cut(rect: Rect, clip: Option<Rect>) -> Rect {
    let Some(clip) = clip else {
        return rect;
    };
    let x = rect.x.max(clip.x);
    let y = rect.y.max(clip.y);
    let right = (rect.x + rect.width).min(clip.x + clip.width);
    let bottom = (rect.y + rect.height).min(clip.y + clip.height);
    Rect { x, y, width: (right - x).max(0.0), height: (bottom - y).max(0.0) }
}
