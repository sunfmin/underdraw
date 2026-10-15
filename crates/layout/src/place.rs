//! Geometry: the catalog's boxes laid out by taffy, then turned into draws
//! in page coordinates, with the tags a person refers to them by.

use taffy::{AvailableSpace, LayoutOutput, NodeId, Style, TaffyError, TaffyTree};
use underdraw_format::{Edge, Node};

use crate::{Align, Draw, Font, Ink, Measure, Page, Placed, Rect};

/// One box to lay out: how it sits among its siblings (`style`), how it
/// looks, the text it holds when it is a leaf, and the node it shows.
pub(crate) struct Elem<'a> {
    pub(crate) style: Style,
    pub(crate) looks: Vec<Look>,
    pub(crate) words: Option<Words>,
    pub(crate) source: Option<&'a Node>,
    /// Why it is drawn from a guess, when the catalog guessed it; the
    /// node's own `assumed` says why when the description did.
    pub(crate) assumed: Option<String>,
    /// Its children are cut to it, with corners of this radius.
    pub(crate) clip: Option<f32>,
    /// Where it floats over its parent, out of the flow. Taffy only sizes
    /// it: taffy 0.15 resolves `bottom` against the wrong box when the page
    /// height is open, so the position is set here instead.
    pub(crate) pin: Option<Pin>,
    /// When pinned: the parent is dimmed under it, and nothing on the
    /// parent can be acted on (a dialog).
    pub(crate) backdrop: bool,
    /// It is a popup (a node in an `overlay` slot): drawn after the whole
    /// page, so nothing that comes after its node covers it.
    pub(crate) on_top: bool,
    pub(crate) children: Vec<Elem<'a>>,
}

/// What a box draws over its own area, before its children.
pub(crate) enum Look {
    Box { radius: f32, fill: Option<Ink>, stroke: Option<Ink>, dashed: bool },
    Cross(Ink),
    TopLine(Ink),
}

/// A place on the parent: each axis at its start, centre or end, `inset`
/// in from the edge it sits on.
#[derive(Clone, Copy)]
pub(crate) struct Pin {
    pub(crate) x: Edge,
    pub(crate) y: Edge,
    pub(crate) inset: f32,
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
        Elem {
            style,
            looks: Vec::new(),
            words: None,
            source: None,
            assumed: None,
            clip: None,
            pin: None,
            backdrop: false,
            on_top: false,
            children: Vec::new(),
        }
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

    /// This box floated over its parent at `pin`.
    pub(crate) fn pinned(mut self, pin: Pin) -> Elem<'a> {
        self.style.position = taffy::Position::Absolute;
        self.pin = Some(pin);
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
    /// Nodes to number or mark as guessed, in order, waiting until all is
    /// drawn: an opaque box or a backdrop pinned over one later hides its
    /// marks.
    marked: Vec<Marked>,
    /// Each node floated over another, by id, and where it went.
    floated: Vec<(String, Rect)>,
    /// The whole page: no tag is drawn past its edge.
    page: Rect,
}

/// A node to number or mark as guessed, unless something drawn later
/// covers it.
struct Marked {
    /// Its place in `Out::nodes`, when it is listed there.
    node: Option<usize>,
    rect: Rect,
    /// A person can act on it: it gets a number.
    acts: bool,
    /// It was guessed: it gets an orange outline and "?".
    guessed: bool,
    /// Where its "?" goes, when it was guessed.
    guess: Rect,
    covered: bool,
}

/// Where the "?" of a guess goes: over the top-right corner of `shown`,
/// what the person sees of it, kept on `page`.
fn guess_tag(shown: Rect, page: Rect) -> Rect {
    tag(shown.x + shown.width - 12.0, shown.y - 6.0, page)
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
    let size = tree.layout(laid.id)?.size;
    let mut out = Out {
        draws: Vec::new(),
        tags: Vec::new(),
        nodes: Vec::new(),
        marked: Vec::new(),
        floated: Vec::new(),
        page: Rect { x: 0.0, y: 0.0, width: size.width, height: size.height },
    };
    let mut later = Vec::new();
    emit(&tree, root, &laid, Rect::default(), None, &mut out, &mut later)?;
    // Popups, in the order they were met; a popup inside one is met then.
    let mut next = 0;
    while let Some(&(elem, laid, parent)) = later.get(next) {
        emit(&tree, elem, laid, parent, None, &mut out, &mut later)?;
        next += 1;
    }
    mark(&mut out);
    let mut draws = out.draws;
    draws.append(&mut out.tags);
    let mut cut = Vec::new();
    for (id, rect) in out.floated {
        if rect.y < -0.5 || rect.y + rect.height > size.height + 0.5 {
            cut.push(id);
        }
    }
    Ok(Page { width: size.width, height: size.height, draws, nodes: out.nodes, cut })
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

/// Draws `elem`, inside `parent`, and everything in it but its popups,
/// which go to `later` with the box they float over. `clip` is the area it
/// may show in, when an ancestor cuts it.
fn emit<'t, 'n>(
    tree: &TaffyTree<usize>,
    elem: &'t Elem<'n>,
    laid: &'t Laid,
    parent: Rect,
    clip: Option<Rect>,
    out: &mut Out,
    later: &mut Vec<(&'t Elem<'n>, &'t Laid, Rect)>,
) -> Result<(), TaffyError> {
    let layout = tree.layout(laid.id)?;
    let (width, height) = (layout.size.width, layout.size.height);
    let (x, y) = match elem.pin {
        Some(pin) => (
            along(pin.x, parent.x, parent.width, width, pin.inset),
            along(pin.y, parent.y, parent.height, height, pin.inset),
        ),
        None => (parent.x + layout.location.x, parent.y + layout.location.y),
    };
    let rect = Rect { x, y, width, height };
    let opaque = elem.looks.iter().any(|look| matches!(look, Look::Box { fill: Some(_), .. }));
    if let (Some(_), Some(node)) = (elem.pin, elem.source)
        && !node.id.is_empty()
    {
        out.floated.push((node.id.clone(), rect));
    }
    if elem.pin.is_some() && elem.backdrop {
        out.draws.push(Draw::Box {
            rect: parent,
            radius: 0.0,
            fill: Some(Ink::Scrim),
            stroke: None,
            dashed: false,
        });
        cover(parent, out);
    }
    if elem.pin.is_some() && opaque {
        cover(rect, out);
    }
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
    let guessed = match elem.source {
        Some(node) => {
            let assumed = node.assumed.clone().or_else(|| elem.assumed.clone());
            record(node, assumed, rect, clip, out)
        }
        None => None,
    };
    let (inner, saved) = match elem.clip {
        Some(_) => (Some(cut(rect, clip)), Some(std::mem::take(&mut out.draws))),
        None => (clip, None),
    };
    let first = out.draws.len();
    for index in 0..elem.children.len() {
        if let (Some(child), Some(laid)) = (elem.children.get(index), laid.children.get(index)) {
            if child.on_top {
                later.push((child, laid, rect));
            } else {
                emit(tree, child, laid, rect, inner, out, later)?;
            }
        }
    }
    // A guess as big as the page would have its "?" in the page's corner,
    // where the person does not look: it goes on what the node shows.
    if let Some(index) = guessed
        && covers(rect, out.page)
    {
        let page = out.page;
        let mut shown: Option<Rect> = None;
        for draw in out.draws.iter().skip(first) {
            let drawn = extent(draw);
            if !covers(drawn, page) {
                shown = Some(match shown {
                    Some(so_far) => union(so_far, drawn),
                    None => drawn,
                });
            }
        }
        if let (Some(shown), Some(marked)) = (shown, out.marked.get_mut(index)) {
            marked.guess = guess_tag(shown, page);
        }
    }
    if let (Some(radius), Some(saved)) = (elem.clip, saved) {
        let draws = std::mem::replace(&mut out.draws, saved);
        out.draws.push(Draw::Clip { rect, radius, draws });
    }
    Ok(())
}

/// Notes where `node` went, and holds it for its marks, drawn once all is
/// drawn: a number when a person can act on it, a dashed outline and "?"
/// when `assumed` says why it was guessed. A node cut away entirely gets
/// none. Says where in `Out::marked` it is held, when it was guessed.
fn record(
    node: &Node,
    assumed: Option<String>,
    rect: Rect,
    clip: Option<Rect>,
    out: &mut Out,
) -> Option<usize> {
    let seen = match clip {
        Some(area) => overlaps(rect, area),
        None => true,
    };
    let (acts, guessed) = (seen && node.interactive, seen && assumed.is_some());
    let mut held = None;
    if acts || guessed {
        let listed = (!node.id.is_empty()).then_some(out.nodes.len());
        let guess = guess_tag(rect, out.page);
        held = guessed.then_some(out.marked.len());
        out.marked.push(Marked { node: listed, rect, acts, guessed, guess, covered: false });
    }
    if !node.id.is_empty() {
        out.nodes.push(Placed {
            id: node.id.clone(),
            component: node.component.clone(),
            rect,
            label: None,
            assumed,
            guess_tag: None,
        });
    }
    held
}

/// Marks what an opaque box at `over`, drawn on top, covers: a node whose
/// middle is under it, or whose tag would sit on it.
fn cover(over: Rect, out: &mut Out) {
    let page = out.page;
    for marked in &mut out.marked {
        let rect = marked.rect;
        let middle =
            Rect { x: rect.x + rect.width / 2.0, y: rect.y + rect.height / 2.0, width: 0.0, height: 0.0 };
        if inside(middle, over)
            || (marked.acts && overlaps(number_tag(rect, page), over))
            || (marked.guessed && overlaps(marked.guess, over))
        {
            marked.covered = true;
        }
    }
}

/// Numbers, in order, every node a person can act on and still see, and
/// marks every guess still seen.
fn mark(out: &mut Out) {
    let mut next: u32 = 1;
    for marked in &out.marked {
        if marked.covered {
            continue;
        }
        if marked.acts {
            if let Some(placed) = marked.node.and_then(|index| out.nodes.get_mut(index)) {
                placed.label = Some(next);
            }
            out.tags.push(Draw::Tag {
                rect: number_tag(marked.rect, out.page),
                text: next.to_string(),
                ink: Ink::Label,
            });
            next = next.saturating_add(1);
        }
        if marked.guessed {
            let rect = marked.rect;
            let around =
                Rect { x: rect.x - 2.0, y: rect.y - 2.0, width: rect.width + 4.0, height: rect.height + 4.0 };
            out.tags.push(Draw::Box {
                rect: around,
                radius: 4.0,
                fill: None,
                stroke: Some(Ink::Assumed),
                dashed: true,
            });
            if let Some(placed) = marked.node.and_then(|index| out.nodes.get_mut(index)) {
                placed.guess_tag = Some(marked.guess);
            }
            out.tags.push(Draw::Tag { rect: marked.guess, text: "?".to_string(), ink: Ink::Assumed });
        }
    }
}

/// Whether `point`, a rect with no size, lies within `area`.
fn inside(point: Rect, area: Rect) -> bool {
    point.x >= area.x
        && point.x <= area.x + area.width
        && point.y >= area.y
        && point.y <= area.y + area.height
}

/// Where a box `size` long starts on a side from `start`, `length` long,
/// when it sits at `edge`, `inset` in.
fn along(edge: Edge, start: f32, length: f32, size: f32, inset: f32) -> f32 {
    match edge {
        Edge::Start => start + inset,
        Edge::Center => start + (length - size) / 2.0,
        Edge::End => start + length - size - inset,
    }
}

/// Where the number of a node laid out at `rect` goes: over its top-left
/// corner, kept on `page`.
fn number_tag(rect: Rect, page: Rect) -> Rect {
    tag(rect.x - 6.0, rect.y - 6.0, page)
}

/// A tag's circle at `x`, `y`, moved in as far as it takes to be wholly on
/// `page`, a point clear of its edge.
fn tag(x: f32, y: f32, page: Rect) -> Rect {
    let (size, margin) = (18.0, 1.0);
    let x = x.min(page.x + page.width - size - margin).max(page.x + margin);
    let y = y.min(page.y + page.height - size - margin).max(page.y + margin);
    Rect { x, y, width: size, height: size }
}

/// Whether `rect` covers all of `page`, to within half a point.
fn covers(rect: Rect, page: Rect) -> bool {
    rect.x <= page.x + 0.5
        && rect.y <= page.y + 0.5
        && rect.x + rect.width >= page.x + page.width - 0.5
        && rect.y + rect.height >= page.y + page.height - 0.5
}

/// The area `draw` takes.
fn extent(draw: &Draw) -> Rect {
    match draw {
        Draw::Box { rect, .. }
        | Draw::Text { rect, .. }
        | Draw::Cross { rect, .. }
        | Draw::Clip { rect, .. }
        | Draw::Tag { rect, .. } => *rect,
        Draw::Line { from, to, .. } => {
            let (x, y) = (from[0].min(to[0]), from[1].min(to[1]));
            Rect { x, y, width: from[0].max(to[0]) - x, height: from[1].max(to[1]) - y }
        }
    }
}

/// The smallest rect holding both `a` and `b`.
fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect { x, y, width: right - x, height: bottom - y }
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
