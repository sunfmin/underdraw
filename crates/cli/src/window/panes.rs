//! The gallery's three panes side by side: the tree on the left, the
//! pictures in the middle and the sidebar on the right. How wide each is,
//! how far each is scrolled, and the lines between them and the scrollbars
//! ([`crate::scrollbar`]) taken hold of and dragged: geometry alone, from
//! the window's size and how large what each pane holds is, as the gallery
//! last told it with [`Panes::fit`]. The tree's rows are placed here too,
//! where it scrolls.

use vello::kurbo::Rect as Area;

use super::{SMALLEST, WIDTHS};
use crate::chrome::{
    GRIP, MARGIN, ROW, SCROLLBAR_INSET, SCROLLBAR_WIDE, SIDEBAR, SIDEBAR_LEAST, TREE, TREE_LEAST,
};
use crate::scrollbar::{Axis, Bar, Part};

/// One of the window's panes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Pane {
    Tree,
    Pictures,
    Side,
}

/// A line between two panes, dragged to widen or narrow the pane beside
/// the pictures it bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Edge {
    /// Between the tree and the pictures.
    Tree,
    /// Between the pictures and the sidebar.
    Side,
}

/// A pane's scrollbar, by the scroll it moves: the tree's, the pictures'
/// down and across, the sidebar's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Scrolls {
    Tree,
    Down,
    Across,
    Side,
}

/// How large what each pane holds is, in points, as the gallery lays it
/// out at the panes' widths.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Content {
    /// How many rows the tree lists.
    pub rows: usize,
    /// How wide and how tall the pictures are, the space around them
    /// included.
    pub pictures: (f64, f64),
    /// How tall the sidebar's cards are.
    pub side: f64,
}

/// What a press took hold of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Took {
    /// A line between two panes.
    Line,
    /// A scrollbar: its thumb, or its track, scrolled a page.
    Bar,
}

/// What is held, being dragged: a line between two panes, or a
/// scrollbar's thumb with how far along its track the pointer was when
/// it was taken hold of and how far its pane was scrolled then.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Held {
    Line(Edge),
    Thumb { scrolls: Scrolls, from: f64, scrolled: f64 },
}

/// The panes' geometry. Every width it gives is within its bounds and
/// every scroll within what there is to scroll, whatever it was told:
/// what it keeps is held there once the gallery [fits](Panes::fit) it,
/// and read through the same bounds until then.
#[derive(Clone, Debug)]
pub(super) struct Panes {
    /// The window's width and height, in points.
    window: (f64, f64),
    /// The tree's width, and the sidebar's, kept while it is hidden.
    tree: f64,
    side: f64,
    /// Whether the sidebar is shown: ⌘\ hides and shows it.
    shown: bool,
    /// How far each pane is scrolled, by [`Scrolls`].
    scrolled: [f64; 4],
    content: Content,
    held: Option<Held>,
    /// The scrollbar whose thumb the pointer is over, drawn darker.
    hot: Option<Scrolls>,
}

impl Panes {
    /// The panes as a window opens: the tree and the sidebar at their
    /// widths, nothing scrolled, in a window of no size yet.
    pub fn new() -> Panes {
        Panes {
            window: (0.0, 0.0),
            tree: TREE,
            side: SIDEBAR,
            shown: true,
            scrolled: [0.0; 4],
            content: Content::default(),
            held: None,
            hot: None,
        }
    }

    /// The window's width and height, in points.
    pub fn window(&self) -> (f64, f64) {
        self.window
    }

    /// The window resized to `window`.
    pub fn resize(&mut self, window: (f64, f64)) {
        self.window = window;
    }

    /// The sidebar shown, or hidden, its width given to the pictures.
    pub fn toggle_side(&mut self) {
        self.shown = !self.shown;
    }

    /// How large what each pane holds is, now: the widths and the scrolls
    /// held within their bounds by it.
    pub fn fit(&mut self, content: Content) {
        self.content = content;
        (self.tree, self.side) = self.widths();
        for scrolls in [Scrolls::Tree, Scrolls::Down, Scrolls::Across, Scrolls::Side] {
            self.scrolled[scrolls as usize] = self.scrolled(scrolls);
        }
    }

    /// The tree's width, then the sidebar's, each within its bounds: a
    /// window narrowed, or the sidebar shown, narrows the tree first.
    fn widths(&self) -> (f64, f64) {
        let (least, most) = self.bounds(Edge::Tree, if self.shown { self.side } else { 0.0 });
        let tree = self.tree.clamp(least, most);
        let (least, most) = self.bounds(Edge::Side, tree);
        (tree, self.side.clamp(least, most))
    }

    /// The least and the most the width of the pane `edge` bounds can be,
    /// the other pane `beside` wide: [`TREE_LEAST`] or [`SIDEBAR_LEAST`],
    /// and what leaves the pictures' pane room for the phone's picture at
    /// [`SMALLEST`], with the space around it. In a window too narrow for
    /// both, the least.
    fn bounds(&self, edge: Edge, beside: f64) -> (f64, f64) {
        let least = match edge {
            Edge::Tree => TREE_LEAST,
            Edge::Side => SIDEBAR_LEAST,
        };
        let picture = f64::from(WIDTHS[0]) * SMALLEST + 2.0 * MARGIN;
        (least, (self.window.0 - beside - picture).max(least))
    }

    /// The tree's pane, in window points.
    pub fn tree(&self) -> Area {
        Area::new(0.0, 0.0, self.widths().0, self.window.1)
    }

    /// The pictures' pane, in window points: between the tree and the
    /// sidebar, or the window's right edge while it is hidden.
    pub fn pictures(&self) -> Area {
        let (tree, side) = self.widths();
        let right = if self.shown { self.window.0 - side } else { self.window.0 };
        Area::new(tree, 0.0, right, self.window.1)
    }

    /// The sidebar's pane, in window points; none while it is hidden.
    pub fn side(&self) -> Option<Area> {
        let (width, height) = self.window;
        self.shown.then(|| Area::new(self.pictures().x1, 0.0, width, height))
    }

    /// The sidebar's width, kept while it is hidden.
    pub fn side_width(&self) -> f64 {
        self.widths().1
    }

    /// The pane `at`, a point in the window, is in.
    pub fn pane_at(&self, at: (f64, f64)) -> Pane {
        let pictures = self.pictures();
        if at.0 < pictures.x0 {
            Pane::Tree
        } else if self.shown && at.0 >= pictures.x1 {
            Pane::Side
        } else {
            Pane::Pictures
        }
    }

    /// Where the top of the tree's row `index` is, in window points, as
    /// far as the tree is scrolled.
    pub fn row_top(&self, index: usize) -> f64 {
        MARGIN / 2.0 + index as f64 * ROW - self.scrolled(Scrolls::Tree)
    }

    /// The tree's row at `at`, a point in the window; none off the tree,
    /// or off its rows.
    pub fn row_at(&self, at: (f64, f64)) -> Option<usize> {
        if self.pane_at(at) != Pane::Tree {
            return None;
        }
        (0..self.content.rows).find(|&index| (self.row_top(index)..self.row_top(index) + ROW).contains(&at.1))
    }

    /// How far the pane `scrolls` scrolls is scrolled, within what there
    /// is to scroll.
    pub fn scrolled(&self, scrolls: Scrolls) -> f64 {
        self.scrolled[scrolls as usize].clamp(0.0, self.beyond(scrolls))
    }

    /// The pane `scrolls` scrolls scrolled `to`, held within what there is
    /// to scroll.
    pub fn scroll_to(&mut self, scrolls: Scrolls, to: f64) {
        self.scrolled[scrolls as usize] = to.clamp(0.0, self.beyond(scrolls));
    }

    /// How far the pane `scrolls` scrolls can be scrolled: how much larger
    /// what it holds is than the pane, along the way it scrolls.
    fn beyond(&self, scrolls: Scrolls) -> f64 {
        let (content, page) = match scrolls {
            Scrolls::Tree => (self.content.rows as f64 * ROW + MARGIN, self.window.1),
            Scrolls::Down => (self.content.pictures.1, self.window.1),
            Scrolls::Across => (self.content.pictures.0, self.pictures().width()),
            Scrolls::Side if self.shown => (self.content.side, self.window.1),
            Scrolls::Side => (0.0, self.window.1),
        };
        (content - page).max(0.0)
    }

    /// The wheel turned `right` and `down` points at `at`, a point in the
    /// window: the pane under it scrolled, the pictures alone across.
    pub fn wheel(&mut self, at: (f64, f64), right: f64, down: f64) {
        match self.pane_at(at) {
            Pane::Tree => self.scroll_by(Scrolls::Tree, down),
            Pane::Side => self.scroll_by(Scrolls::Side, down),
            Pane::Pictures => {
                self.scroll_by(Scrolls::Down, down);
                self.scroll_by(Scrolls::Across, right);
            }
        }
    }

    /// The pane `scrolls` scrolls scrolled `by` points more, held within
    /// what there is to scroll.
    fn scroll_by(&mut self, scrolls: Scrolls, by: f64) {
        self.scroll_to(scrolls, self.scrolled(scrolls) + by);
    }

    /// Where `edge`'s line is, in window points; none for the sidebar's
    /// while it is hidden.
    fn line_x(&self, edge: Edge) -> Option<f64> {
        match edge {
            Edge::Tree => Some(self.pictures().x0),
            Edge::Side => self.side().map(|side| side.x0),
        }
    }

    /// The line between two panes at `at`, a point in the window, near
    /// enough to take hold of it.
    pub fn line_at(&self, at: (f64, f64)) -> Option<Edge> {
        [Edge::Tree, Edge::Side]
            .into_iter()
            .find(|&edge| self.line_x(edge).is_some_and(|x| (at.0 - x).abs() <= GRIP))
    }

    /// Whether the pointer at `at` grips: a line is held, or it is on one.
    pub fn grip(&self, at: (f64, f64)) -> bool {
        matches!(self.held, Some(Held::Line(_))) || self.line_at(at).is_some()
    }

    /// The left button pressed at `at`, a point in the window: on a line
    /// between two panes, taking hold of it; on a scrollbar, its thumb
    /// taken hold of, or a page scrolled toward the press on its track.
    /// What it took hold of; none off every line and scrollbar.
    pub fn press(&mut self, at: (f64, f64)) -> Option<Took> {
        if let Some(edge) = self.line_at(at) {
            self.held = Some(Held::Line(edge));
            return Some(Took::Line);
        }
        let (scrolls, bar, part) = self.bar_at(at)?;
        match part {
            Part::Thumb => {
                self.held = Some(Held::Thumb { scrolls, from: bar.along(at), scrolled: bar.scrolled });
            }
            Part::Before => self.scroll_by(scrolls, -bar.page()),
            Part::After => self.scroll_by(scrolls, bar.page()),
        }
        Some(Took::Bar)
    }

    /// The pointer moved to `at`, a point in the window. A line held: the
    /// pane it bounds as wide as reaches the pointer, within its bounds;
    /// the other pane keeps its width. A thumb held: its pane scrolled as
    /// far as the thumb has moved with the pointer along its track, within
    /// what there is to scroll. Whether anything is held.
    pub fn drag(&mut self, at: (f64, f64)) -> bool {
        match self.held {
            None => false,
            Some(Held::Line(edge)) => {
                let (tree, side) = self.widths();
                let (beside, reach) = match edge {
                    Edge::Tree => (if self.shown { side } else { 0.0 }, at.0),
                    Edge::Side => (tree, self.window.0 - at.0),
                };
                let (least, most) = self.bounds(edge, beside);
                let width = reach.clamp(least, most);
                (self.tree, self.side) = match edge {
                    Edge::Tree => (width, side),
                    Edge::Side => (tree, width),
                };
                true
            }
            Some(Held::Thumb { scrolls, from, scrolled }) => {
                if let Some(bar) = self.bar(scrolls) {
                    self.scroll_to(scrolls, bar.dragged(scrolled, bar.along(at) - from));
                }
                true
            }
        }
    }

    /// The left button let go: nothing held. Whether a thumb was.
    pub fn release(&mut self) -> bool {
        matches!(self.held.take(), Some(Held::Thumb { .. }))
    }

    /// The pointer moved to `at`: the scrollbar whose thumb it is over
    /// warmed. Whether that changed which.
    pub fn hover(&mut self, at: (f64, f64)) -> bool {
        let hot = self.bar_at(at).and_then(|(scrolls, _, part)| (part == Part::Thumb).then_some(scrolls));
        let warmed = hot != self.hot;
        self.hot = hot;
        warmed
    }

    /// The scrollbar drawn darker: the one whose thumb is held, else the
    /// one the pointer is over.
    pub fn hot(&self) -> Option<Scrolls> {
        match self.held {
            Some(Held::Thumb { scrolls, .. }) => Some(scrolls),
            _ => self.hot,
        }
    }

    /// Each pane's scrollbar: the pictures', down then across, and the
    /// tree's, then the sidebar's while it is shown. Where the pictures
    /// scroll both ways, the bar across stops short of the one down.
    pub fn bars(&self) -> Vec<(Scrolls, Bar)> {
        let pictures = self.pictures();
        let (content, scrolled) = (self.content.pictures.1, self.scrolled(Scrolls::Down));
        let down = Bar { pane: pictures, axis: Axis::Down, content, scrolled, corner: 0.0 };
        let corner = if down.thumb().is_some() { SCROLLBAR_WIDE + SCROLLBAR_INSET } else { 0.0 };
        let (content, scrolled) = (self.content.pictures.0, self.scrolled(Scrolls::Across));
        let across = Bar { pane: pictures, axis: Axis::Across, content, scrolled, corner };
        let content = self.content.rows as f64 * ROW + MARGIN;
        let scrolled = self.scrolled(Scrolls::Tree);
        let tree = Bar { pane: self.tree(), axis: Axis::Down, content, scrolled, corner: 0.0 };
        let mut bars = vec![(Scrolls::Down, down), (Scrolls::Across, across), (Scrolls::Tree, tree)];
        if let Some(pane) = self.side() {
            let (content, scrolled) = (self.content.side, self.scrolled(Scrolls::Side));
            bars.push((Scrolls::Side, Bar { pane, axis: Axis::Down, content, scrolled, corner: 0.0 }));
        }
        bars
    }

    /// The scrollbar `scrolls`; none for the sidebar's while it is hidden.
    pub fn bar(&self, scrolls: Scrolls) -> Option<Bar> {
        self.bars().into_iter().find_map(|(of, bar)| (of == scrolls).then_some(bar))
    }

    /// The scrollbar under `at`, a point in the window, and the part of it
    /// there; none off every scrollbar.
    fn bar_at(&self, at: (f64, f64)) -> Option<(Scrolls, Bar, Part)> {
        self.bars().into_iter().find_map(|(scrolls, bar)| Some((scrolls, bar, bar.at(at)?)))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::chrome::{SCROLLBAR_INSET, SCROLLBAR_REACH};

    /// The room the pictures keep beside the tree and the sidebar: a phone
    /// at half size, with the space around it.
    const ROOM: f64 = 390.0 * 0.5 + 2.0 * MARGIN;

    /// Panes in a window `window`, fitted to `content`.
    fn panes(window: (f64, f64), content: Content) -> Panes {
        let mut panes = Panes::new();
        panes.resize(window);
        panes.fit(content);
        panes
    }

    /// What overflows every pane of a window 900 points tall: 100 rows,
    /// pictures 3000 wide and 2000 tall, cards 1500 tall.
    const TALL: Content = Content { rows: 100, pictures: (3000.0, 2000.0), side: 1500.0 };

    /// The thumb of the scrollbar `scrolls`, which overflows.
    fn thumb_of(panes: &Panes, scrolls: Scrolls) -> Area {
        panes.bar(scrolls).and_then(|bar| bar.thumb()).unwrap()
    }

    fn centre(area: Area) -> (f64, f64) {
        (area.center().x, area.center().y)
    }

    #[test]
    fn the_wheel_scrolls_the_pane_under_it_as_far_as_its_end() {
        let mut panes = panes((1440.0, 900.0), TALL);
        let on_pictures = (TREE + 100.0, 400.0);
        panes.wheel(on_pictures, 0.0, 120.0);
        assert_eq!(panes.scrolled(Scrolls::Down), 120.0);
        panes.wheel(on_pictures, 0.0, 1e6);
        assert_eq!(panes.scrolled(Scrolls::Down), 2000.0 - 900.0, "no further than the end");
        panes.wheel(on_pictures, 0.0, -1e6);
        assert_eq!(panes.scrolled(Scrolls::Down), 0.0, "no further than the top");
        panes.wheel(on_pictures, 1e6, 0.0);
        assert_eq!(
            panes.scrolled(Scrolls::Across),
            3000.0 - (1440.0 - TREE - SIDEBAR),
            "the pictures across"
        );

        panes.wheel((40.0, 400.0), 1e6, 500.0);
        assert_eq!(panes.scrolled(Scrolls::Down), 0.0, "over the tree, the tree scrolls");
        assert_eq!(panes.scrolled(Scrolls::Tree), 500.0, "and nothing across");
        panes.wheel((40.0, 400.0), 0.0, 1e6);
        assert_eq!(panes.scrolled(Scrolls::Tree), 100.0 * ROW + MARGIN - 900.0, "as far as its last row");
        panes.wheel((1400.0, 400.0), 0.0, 1e6);
        assert_eq!(panes.scrolled(Scrolls::Side), 1500.0 - 900.0, "over the sidebar, the sidebar");
    }

    #[test]
    fn a_row_of_the_tree_is_found_where_it_is_drawn_however_far_it_is_scrolled() {
        let mut panes = panes((1440.0, 900.0), TALL);
        for scrolled in [0.0, 13.0, 500.0] {
            panes.scroll_to(Scrolls::Tree, scrolled);
            for index in [0, 1, 20, 40] {
                let top = panes.row_top(index);
                assert_eq!(panes.row_at((40.0, top)), Some(index), "its top");
                assert_eq!(panes.row_at((40.0, top + ROW - 0.01)), Some(index), "its foot");
            }
        }
        panes.scroll_to(Scrolls::Tree, 0.0);
        assert_eq!(panes.row_at((40.0, MARGIN / 2.0 - 1.0)), None, "above the first");
        assert_eq!(panes.row_at((40.0, panes.row_top(100))), None, "below the last");
        assert_eq!(panes.row_at((TREE + 40.0, panes.row_top(1))), None, "off the tree");
    }

    #[test]
    fn a_pane_shows_a_scrollbar_while_it_overflows_and_follows_its_scroll() {
        let mut panes = panes((1440.0, 900.0), TALL);
        let (down, across) = (thumb_of(&panes, Scrolls::Down), thumb_of(&panes, Scrolls::Across));
        assert!(across.x1 < down.x0, "the bar across stops short of the one down");
        assert_eq!(across.x0, TREE + SCROLLBAR_INSET, "unscrolled across, at its start");
        assert_eq!(down.y0, SCROLLBAR_INSET, "unscrolled, at the very top");
        let share = 900.0 / 2000.0;
        assert!((down.height() - (900.0 - 2.0 * SCROLLBAR_INSET) * share).abs() < 1e-9);
        panes.wheel((TREE + 100.0, 400.0), 0.0, 1e6);
        let end = thumb_of(&panes, Scrolls::Down);
        assert_eq!(end.y1, 900.0 - SCROLLBAR_INSET, "scrolled to the end, at the very bottom");
        assert_eq!(end.height(), down.height());
        assert!(thumb_of(&panes, Scrolls::Tree).x1 <= TREE, "the tree's in the tree");
        assert!(thumb_of(&panes, Scrolls::Side).x0 >= 1440.0 - SIDEBAR, "the sidebar's in the sidebar");

        let fits = Content { rows: 3, pictures: (400.0, 400.0), side: 300.0 };
        panes.fit(fits);
        let thumbs: Vec<_> = panes.bars().iter().filter_map(|(_, bar)| bar.thumb()).collect();
        assert!(thumbs.is_empty(), "panes large enough for all of it have none: {thumbs:?}");
        assert_eq!(panes.scrolled(Scrolls::Down), 0.0, "and are scrolled back to their start");
        panes.fit(TALL);
        panes.toggle_side();
        assert_eq!(panes.bar(Scrolls::Side), None, "a hidden sidebar has none");
    }

    #[test]
    fn dragging_a_thumb_scrolls_its_pane_in_step_held_within_its_bounds() {
        let mut panes = panes((1440.0, 900.0), TALL);
        let bar = panes.bar(Scrolls::Down).unwrap();
        let thumb = bar.thumb().unwrap();
        let at = centre(thumb);
        assert!(panes.hover(at), "the pointer over a thumb warms it");
        assert_eq!(panes.hot(), Some(Scrolls::Down));
        assert_eq!(panes.press(at), Some(Took::Bar), "the thumb taken hold of");
        assert_eq!(panes.scrolled(Scrolls::Down), 0.0, "a press on the thumb scrolls nothing yet");
        assert!(panes.drag((at.0, at.1 + 30.0)), "the drag moves the thumb");
        assert!((panes.scrolled(Scrolls::Down) - bar.dragged(0.0, 30.0)).abs() < 1e-9, "in step");
        let moved = thumb_of(&panes, Scrolls::Down);
        assert!((moved.y0 - (thumb.y0 + 30.0)).abs() < 1e-9, "the thumb under the pointer still: {moved:?}");
        panes.drag((at.0, 1e5));
        assert_eq!(panes.scrolled(Scrolls::Down), 2000.0 - 900.0, "no further than the end");
        panes.drag((at.0, -1e5));
        assert_eq!(panes.scrolled(Scrolls::Down), 0.0, "no further than the start");
        assert!(panes.release(), "letting go cools the thumb");
        assert!(!panes.drag((at.0, 500.0)), "nothing held");
        assert_eq!(panes.scrolled(Scrolls::Down), 0.0);

        let across = centre(thumb_of(&panes, Scrolls::Across));
        panes.press(across);
        panes.drag((across.0 + 40.0, across.1));
        assert!(panes.scrolled(Scrolls::Across) > 0.0, "the thumb across scrolls across");
        assert_eq!(panes.scrolled(Scrolls::Down), 0.0, "and not down");
    }

    #[test]
    fn a_press_on_the_track_scrolls_a_page_toward_it() {
        let mut panes = panes((1440.0, 300.0), TALL);
        let down = thumb_of(&panes, Scrolls::Down);
        assert_eq!(panes.press((down.center().x, down.y1 + 10.0)), Some(Took::Bar), "below the thumb");
        assert_eq!(panes.scrolled(Scrolls::Down), 300.0, "one page down: as tall as the pane");
        assert!(!panes.release(), "takes hold of nothing");
        let down = thumb_of(&panes, Scrolls::Down);
        panes.press((down.center().x, down.y0 - 1.0));
        assert_eq!(panes.scrolled(Scrolls::Down), 0.0, "one page back up");
        for _ in 0..20 {
            panes.press((down.center().x, 300.0 - SCROLLBAR_INSET - 1.0));
            panes.release();
        }
        assert_eq!(panes.scrolled(Scrolls::Down), 2000.0 - 300.0, "never past the end");
        let across = thumb_of(&panes, Scrolls::Across);
        panes.press((across.x1 + 10.0, across.center().y));
        assert_eq!(panes.scrolled(Scrolls::Across), 1440.0 - TREE - SIDEBAR, "a page across: as wide");
    }

    #[test]
    fn a_line_between_panes_and_the_scrollbar_beside_it_never_share_a_point() {
        let panes = panes((1440.0, 300.0), TALL);
        let pictures = panes.pictures();
        let lanes: Vec<_> = panes.bars().into_iter().filter_map(|(s, bar)| Some((s, bar.lane()?))).collect();
        assert_eq!(lanes.len(), 4, "every pane overflows: {lanes:?}");
        for (scrolls, lane) in lanes {
            for line in [pictures.x0, pictures.x1] {
                let clear = lane.x1 <= line - GRIP || lane.x0 >= line + GRIP;
                assert!(clear, "{scrolls:?}'s scrollbar {lane:?} clear of the line at {line}");
            }
            let at = (lane.x0 + SCROLLBAR_REACH, lane.center().y);
            assert_eq!(panes.line_at(at), None, "{scrolls:?}: a scrollbar is never a line's");
        }
    }

    #[test]
    fn dragging_the_line_between_the_tree_and_the_pictures_resizes_the_tree_within_its_bounds() {
        let mut panes = panes((1440.0, 900.0), TALL);
        assert_eq!(panes.tree().width(), TREE, "the window opens with the tree at 260pt");
        assert!(panes.grip((TREE + 2.0, 300.0)), "the pointer grips the line");
        assert_eq!(panes.press((TREE + 2.0, 300.0)), Some(Took::Line));
        assert!(panes.drag((400.0, 320.0)), "the drag moves the line");
        assert_eq!(panes.pictures().x0, 400.0, "to where the pointer is, the pictures after it");
        assert_eq!(panes.side_width(), SIDEBAR, "the sidebar keeps its width");
        assert!(panes.grip((600.0, 300.0)), "held, off the line still");
        assert!(!panes.release(), "a line is no thumb");
        assert!(!panes.drag((500.0, 320.0)), "let go, the line stays");
        assert_eq!(panes.tree().width(), 400.0);

        panes.press((402.0, 300.0));
        panes.drag((20.0, 300.0));
        assert_eq!(panes.tree().width(), TREE_LEAST, "no narrower than 160pt");
        panes.drag((1400.0, 300.0));
        assert_eq!(panes.pictures().width(), ROOM, "the pictures keep room for a half-size phone");
        panes.release();

        panes.resize((900.0, 900.0));
        assert_eq!(panes.tree().width(), 900.0 - SIDEBAR - ROOM, "a narrower window narrows the tree too");
        panes.resize((500.0, 900.0));
        assert_eq!(panes.tree().width(), TREE_LEAST, "never below its least");
    }

    #[test]
    fn dragging_the_line_between_the_pictures_and_the_sidebar_resizes_the_sidebar_within_its_bounds() {
        let width = 2400.0;
        let mut panes = panes((width, 900.0), TALL);
        assert_eq!(panes.side_width(), SIDEBAR, "the window opens with the sidebar at 360pt");
        let line = width - SIDEBAR;
        assert_eq!(panes.line_at((line - 3.0, 300.0)), Some(Edge::Side), "the pointer takes hold of it");
        panes.press((line - 3.0, 300.0));
        assert!(panes.drag((line - 400.0, 320.0)), "the drag moves the line");
        assert_eq!(panes.side_width(), SIDEBAR + 400.0, "to where the pointer is");
        assert_eq!(panes.tree().width(), TREE, "the tree keeps its width");
        panes.drag((width - 40.0, 300.0));
        panes.release();
        assert_eq!(panes.side_width(), SIDEBAR_LEAST, "no narrower than 280pt");

        panes.toggle_side();
        assert_eq!(panes.line_at((width - SIDEBAR_LEAST, 300.0)), None, "hidden, there is no line");
        assert_eq!(panes.pictures().x1, width, "the pictures take its width");
        panes.toggle_side();
        assert_eq!(panes.side_width(), SIDEBAR_LEAST, "shown again, at the width it was dragged to");
        assert_eq!(panes.pictures().x1, width - SIDEBAR_LEAST);

        panes.press((panes.pictures().x1, 300.0));
        panes.drag((0.0, 300.0));
        panes.release();
        assert_eq!(panes.side_width(), width - TREE - ROOM, "the pictures keep room for a half-size phone");
        panes.press((TREE, 300.0));
        panes.drag((600.0, 300.0));
        panes.release();
        assert_eq!(panes.tree().width(), TREE, "the tree widens no further than leaves that room");
        panes.resize((1200.0, 900.0));
        assert_eq!(panes.side_width() + panes.tree().width(), 1200.0 - ROOM, "a narrower window, both");
        panes.resize((600.0, 900.0));
        assert_eq!((panes.tree().width(), panes.side_width()), (TREE_LEAST, SIDEBAR_LEAST), "their least");
    }

    /// One thing done to the panes.
    #[derive(Clone, Debug)]
    enum Input {
        Resize(f64, f64),
        Toggle,
        Fit(Content),
        Wheel((f64, f64), f64, f64),
        Press((f64, f64)),
        Drag((f64, f64)),
        Release,
        ScrollTo(usize, f64),
    }

    fn point() -> impl Strategy<Value = (f64, f64)> {
        (-100.0..3000.0, -100.0..2000.0)
    }

    fn input() -> impl Strategy<Value = Input> {
        let content = (0usize..200, 0.0..6000.0, 0.0..6000.0, 0.0..4000.0)
            .prop_map(|(rows, across, down, side)| Content { rows, pictures: (across, down), side });
        prop_oneof![
            (0.0..3000.0, 0.0..2000.0).prop_map(|(w, h)| Input::Resize(w, h)),
            Just(Input::Toggle),
            content.prop_map(Input::Fit),
            (point(), -5000.0..5000.0, -5000.0..5000.0).prop_map(|(at, x, y)| Input::Wheel(at, x, y)),
            point().prop_map(Input::Press),
            point().prop_map(Input::Drag),
            Just(Input::Release),
            (0usize..4, -5000.0..5000.0).prop_map(|(s, to)| Input::ScrollTo(s, to)),
        ]
    }

    proptest! {
        #[test]
        fn every_scroll_and_width_stays_within_its_bounds(inputs in prop::collection::vec(input(), 0..60)) {
            let mut panes = Panes::new();
            for input in inputs {
                match input {
                    Input::Resize(w, h) => panes.resize((w, h)),
                    Input::Toggle => panes.toggle_side(),
                    Input::Fit(content) => panes.fit(content),
                    Input::Wheel(at, right, down) => panes.wheel(at, right, down),
                    Input::Press(at) => {
                        panes.press(at);
                    }
                    Input::Drag(at) => {
                        panes.drag(at);
                    }
                    Input::Release => {
                        panes.release();
                    }
                    Input::ScrollTo(s, to) => {
                        let scrolls = [Scrolls::Tree, Scrolls::Down, Scrolls::Across, Scrolls::Side][s];
                        panes.scroll_to(scrolls, to);
                    }
                }
                let (window, content) = (panes.window(), panes.content);
                let (tree, side) = (panes.tree().width(), panes.side_width());
                let room = |beside: f64, least: f64| (window.0 - beside - ROOM).max(least);
                let beside = if panes.side().is_some() { side } else { 0.0 };
                prop_assert!((TREE_LEAST..=room(beside, TREE_LEAST)).contains(&tree), "tree {tree}");
                prop_assert!((SIDEBAR_LEAST..=room(tree, SIDEBAR_LEAST)).contains(&side), "side {side}");
                let pictures = panes.pictures().width();
                let side_content = if panes.side().is_some() { content.side } else { 0.0 };
                for (scrolls, content, view) in [
                    (Scrolls::Tree, content.rows as f64 * ROW + MARGIN, window.1),
                    (Scrolls::Down, content.pictures.1, window.1),
                    (Scrolls::Across, content.pictures.0, pictures),
                    (Scrolls::Side, side_content, window.1),
                ] {
                    let scrolled = panes.scrolled(scrolls);
                    prop_assert!(
                        (0.0..=(content - view).max(0.0)).contains(&scrolled),
                        "{scrolls:?} scrolled {scrolled} of {content} in {view}"
                    );
                }
            }
        }
    }
}
