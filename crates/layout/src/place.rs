//! Geometry: the catalog's boxes laid out by taffy, then turned into draws
//! in page coordinates, with the tags a person refers to them by.

use taffy::{AvailableSpace, LayoutOutput, NodeId, Style, TaffyError, TaffyTree};
use underdraw_format::{Edge, Finding, Node};

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
    /// Why the other description's state it draws cannot be drawn, when
    /// it draws one: it is crossed out instead.
    pub(crate) unpictured: Option<String>,
    /// A line between its cells, in this ink, drawn over them: panes
    /// split without a border round each.
    pub(crate) dividers: Option<Ink>,
    pub(crate) children: Vec<Elem<'a>>,
}

/// What a box draws over its own area, before its children.
pub(crate) enum Look {
    Box {
        radius: f32,
        fill: Option<Ink>,
        stroke: Option<Ink>,
        dashed: bool,
    },
    Cross(Ink),
    TopLine(Ink),
    /// Another description's state, scaled to fit the box, at its top.
    Picture(Box<Page>),
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
            unpictured: None,
            dividers: None,
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
    /// What the person sees of it, which its "?" sits by when it was
    /// guessed.
    shown: Rect,
    covered: bool,
}

/// Where the "?" of a guess goes: over the top-right corner of `shown`,
/// what the person sees of it, kept on `page`.
fn guess_tag(shown: Rect, page: Rect) -> Rect {
    tag(shown.x + shown.width - 12.0, shown.y - 6.0, page)
}

/// Where else the "?" of a guess may go, after [`guess_tag`], each still
/// by the top-right corner of `shown`: off it, above it, right of it,
/// then below it.
fn guess_tags(shown: Rect, page: Rect) -> [Rect; 5] {
    let right = shown.x + shown.width;
    [
        guess_tag(shown, page),
        tag(right - 2.0, shown.y - 16.0, page),
        tag(right - 18.0, shown.y - 19.0, page),
        tag(right + 1.0, shown.y, page),
        tag(right - 18.0, shown.y + shown.height + 1.0, page),
    ]
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
    let marks = out.tags.len();
    draws.append(&mut out.tags);
    let mut cut = Vec::new();
    for (id, rect) in out.floated {
        if rect.y < -0.5 || rect.y + rect.height > size.height + 0.5 {
            cut.push(id);
        }
    }
    let mut unpictured = Vec::new();
    unpictured_in(root, "", &mut unpictured);
    Ok(Page { width: size.width, height: size.height, draws, nodes: out.nodes, cut, marks, unpictured })
}

/// Each box in `elem` that cannot draw the state it draws, as a finding
/// at the node it is part of: its own, or else `owner`'s.
fn unpictured_in(elem: &Elem, owner: &str, found: &mut Vec<Finding>) {
    let owner = match elem.source {
        Some(node) if !node.id.is_empty() => node.id.as_str(),
        _ => owner,
    };
    if let Some(problem) = &elem.unpictured {
        let finding = Finding { at: owner.to_string(), problem: problem.clone() };
        if !found.contains(&finding) {
            found.push(finding);
        }
    }
    for child in &elem.children {
        unpictured_in(child, owner, found);
    }
}

/// `page`'s draws scaled to fit `rect` and set at its top, in its middle
/// across, cut to it. Its marks are left out: its labels number the nodes of
/// another screen, which no one acts on here, as are its guesses' marks.
fn pictured(page: &Page, rect: Rect) -> Draw {
    let scale = if page.width > 0.0 && page.height > 0.0 {
        (rect.width / page.width).min(rect.height / page.height)
    } else {
        0.0
    };
    let x = rect.x + (rect.width - page.width * scale) / 2.0;
    let mut draws = Vec::new();
    for draw in page.wireframe() {
        draws.push(moved(draw, scale, x, rect.y));
    }
    Draw::Clip { rect, radius: 0.0, draws }
}

/// `draw` scaled by `scale` and moved by `x`, `y`.
fn moved(draw: &Draw, scale: f32, x: f32, y: f32) -> Draw {
    let at = |rect: &Rect| Rect {
        x: x + rect.x * scale,
        y: y + rect.y * scale,
        width: rect.width * scale,
        height: rect.height * scale,
    };
    match draw {
        Draw::Box { rect, radius, fill, stroke, dashed } => Draw::Box {
            rect: at(rect),
            radius: radius * scale,
            fill: *fill,
            stroke: *stroke,
            dashed: *dashed,
        },
        Draw::Text { rect, text, font, ink, max_lines, align, underline, strike } => Draw::Text {
            rect: at(rect),
            text: text.clone(),
            font: Font {
                size: font.size * scale,
                weight: font.weight,
                line_height: font.line_height * scale,
            },
            ink: *ink,
            max_lines: *max_lines,
            align: *align,
            underline: *underline,
            strike: *strike,
        },
        Draw::Cross { rect, ink } => Draw::Cross { rect: at(rect), ink: *ink },
        Draw::Line { from, to, ink } => Draw::Line {
            from: [x + from[0] * scale, y + from[1] * scale],
            to: [x + to[0] * scale, y + to[1] * scale],
            ink: *ink,
        },
        Draw::Clip { rect, radius, draws } => {
            let mut inside = Vec::new();
            for draw in draws {
                inside.push(moved(draw, scale, x, y));
            }
            Draw::Clip { rect: at(rect), radius: radius * scale, draws: inside }
        }
        Draw::Tag { rect, text, ink } => Draw::Tag { rect: at(rect), text: text.clone(), ink: *ink },
    }
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
            Look::Picture(page) => pictured(page, rect),
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
    let mut cells = Vec::new();
    for index in 0..elem.children.len() {
        if let (Some(child), Some(laid)) = (elem.children.get(index), laid.children.get(index)) {
            if child.on_top {
                later.push((child, laid, rect));
            } else {
                emit(tree, child, laid, rect, inner, out, later)?;
            }
            if child.pin.is_none() {
                let at = tree.layout(laid.id)?;
                let (x, y) = (rect.x + at.location.x, rect.y + at.location.y);
                cells.push(Rect { x, y, width: at.size.width, height: at.size.height });
            }
        }
    }
    if let Some(ink) = elem.dividers {
        out.draws.append(&mut dividers(&cells, ink));
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
            marked.shown = shown;
        }
    }
    if let (Some(radius), Some(saved)) = (elem.clip, saved) {
        let draws = std::mem::replace(&mut out.draws, saved);
        out.draws.push(Draw::Clip { rect, radius, draws });
    }
    Ok(())
}

/// The lines between `cells`, laid out in rows and columns: down the
/// middle of the gap between a cell and the one before it in its row, and
/// across the middle of the gap between a cell and the one above it, each
/// as long as the cell's share of its row or column, so lines meet where
/// they cross.
fn dividers(cells: &[Rect], ink: Ink) -> Vec<Draw> {
    let mut lines = Vec::new();
    for cell in cells {
        let (right, bottom) = (cell.x + cell.width, cell.y + cell.height);
        let (mut before, mut after, mut above, mut below) = (None, None, None, None);
        for other in cells {
            let (other_right, other_bottom) = (other.x + other.width, other.y + other.height);
            if other.y < bottom - 0.5 && cell.y < other_bottom - 0.5 {
                if other_right <= cell.x + 0.5 {
                    before = Some(before.map_or(other_right, |at: f32| at.max(other_right)));
                }
                if other.x >= right - 0.5 {
                    after = Some(after.map_or(other.x, |at: f32| at.min(other.x)));
                }
            }
            if other.x < right - 0.5 && cell.x < other_right - 0.5 {
                if other_bottom <= cell.y + 0.5 {
                    above = Some(above.map_or(other_bottom, |at: f32| at.max(other_bottom)));
                }
                if other.y >= bottom - 0.5 {
                    below = Some(below.map_or(other.y, |at: f32| at.min(other.y)));
                }
            }
        }
        let left = before.map_or(cell.x, |at| (at + cell.x) / 2.0);
        let right = after.map_or(right, |at| (right + at) / 2.0);
        let top = above.map_or(cell.y, |at| (at + cell.y) / 2.0);
        let bottom = below.map_or(bottom, |at| (bottom + at) / 2.0);
        if before.is_some() {
            lines.push(Draw::Line { from: [left, top], to: [left, bottom], ink });
        }
        if above.is_some() {
            lines.push(Draw::Line { from: [left, top], to: [right, top], ink });
        }
    }
    lines
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
        held = guessed.then_some(out.marked.len());
        out.marked.push(Marked { node: listed, rect, acts, guessed, shown: rect, covered: false });
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
            || (marked.guessed && overlaps(guess_tag(marked.shown, page), over))
        {
            marked.covered = true;
        }
    }
}

/// Numbers, in order, every node a person can act on and still see, and
/// marks every guess still seen; each tag where it covers no other node's
/// text, nor a tag drawn before it.
fn mark(out: &mut Out) {
    let mut words = Vec::new();
    texts(&out.draws, None, &mut words);
    let mut next: u32 = 1;
    for marked in &out.marked {
        if marked.covered {
            continue;
        }
        if marked.acts {
            if let Some(placed) = marked.node.and_then(|index| out.nodes.get_mut(index)) {
                placed.label = Some(next);
            }
            let rect = clear(&number_tags(marked.rect, out.page), marked.rect, &words, &out.tags);
            out.tags.push(Draw::Tag { rect, text: next.to_string(), ink: Ink::Label });
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
            let guess = clear(&guess_tags(marked.shown, out.page), rect, &words, &out.tags);
            if let Some(placed) = marked.node.and_then(|index| out.nodes.get_mut(index)) {
                placed.guess_tag = Some(guess);
            }
            out.tags.push(Draw::Tag { rect: guess, text: "?".to_string(), ink: Ink::Assumed });
        }
    }
}

/// The first of `spots` that covers no text in `words` but what lies
/// within `own`, the node the tag names, and no tag among `marks` (a
/// guess's outline is no tag); the first of them when each covers some.
fn clear(spots: &[Rect], own: Rect, words: &[Rect], marks: &[Draw]) -> Rect {
    let foreign = |spot: Rect| {
        words.iter().any(|word| !within(*word, own) && overlaps(spot, *word))
            || marks.iter().any(|mark| matches!(mark, Draw::Tag { rect, .. } if overlaps(spot, *rect)))
    };
    match spots.iter().find(|spot| !foreign(**spot)) {
        Some(spot) => *spot,
        None => spots.first().copied().unwrap_or_default(),
    }
}

/// Where each text in `draws` is drawn, cut to `clip` when there is one,
/// added to `words`: what a tag must not cover.
fn texts(draws: &[Draw], clip: Option<Rect>, words: &mut Vec<Rect>) {
    for draw in draws {
        match draw {
            Draw::Text { rect, .. } => {
                let seen = cut(*rect, clip);
                if seen.width > 0.0 && seen.height > 0.0 {
                    words.push(seen);
                }
            }
            Draw::Clip { rect, draws, .. } => texts(draws, Some(cut(*rect, clip)), words),
            _ => {}
        }
    }
}

/// Whether `rect` lies within `area`, to within half a point.
fn within(rect: Rect, area: Rect) -> bool {
    rect.x >= area.x - 0.5
        && rect.y >= area.y - 0.5
        && rect.x + rect.width <= area.x + area.width + 0.5
        && rect.y + rect.height <= area.y + area.height + 0.5
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

/// Where the number of a node laid out at `rect` goes: left of it, level
/// with its top, so that whatever the node shows at its top-left stays
/// whole and the number sits by the node it names; where `page` has no
/// room there, off its top-left corner, the circle touching the corner from
/// outside; kept on `page`.
fn number_tag(rect: Rect, page: Rect) -> Rect {
    let (size, gap, margin) = (18.0, 1.0, 1.0);
    let left = rect.x - gap - size;
    if left >= page.x + margin { tag(left, rect.y, page) } else { corner_tag(rect, page) }
}

/// Off the top-left corner of `rect`, the circle touching it from outside,
/// kept on `page`.
fn corner_tag(rect: Rect, page: Rect) -> Rect {
    tag(rect.x - 16.0, rect.y - 16.0, page)
}

/// Where else the number of a node laid out at `rect` may go, after
/// [`number_tag`], each still by its top-left: off its corner, above it,
/// below it, then right of its top.
fn number_tags(rect: Rect, page: Rect) -> [Rect; 5] {
    [
        number_tag(rect, page),
        corner_tag(rect, page),
        tag(rect.x, rect.y - 19.0, page),
        tag(rect.x, rect.y + rect.height + 1.0, page),
        tag(rect.x + rect.width + 1.0, rect.y, page),
    ]
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
