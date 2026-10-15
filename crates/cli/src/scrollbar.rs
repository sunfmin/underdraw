//! A pane's scrollbar: the one calculation every pane of the gallery that
//! scrolls draws its scrollbar by, and takes presses on it by — the tree,
//! the pictures down and across, and the sidebar — from three values: how
//! large what it holds is, how large the pane is, and how far it is
//! scrolled. Its look is the chrome's ([`crate::chrome`]).

use vello::kurbo::Rect as Area;

use crate::chrome::{SCROLLBAR_INSET, SCROLLBAR_LEAST, SCROLLBAR_REACH, SCROLLBAR_WIDE};

/// Which way a pane scrolls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Axis {
    Down,
    Across,
}

/// The thumb along a pane `pane` long holding `content`, scrolled
/// `scrolled` along it, its track kept `corner` short of the pane's end
/// (for the other bar where a pane scrolls both ways): where it starts,
/// from the pane's start, and how long it is. Its length is the share of the content in view, never under
/// [`SCROLLBAR_LEAST`]; its place how far the pane is scrolled, from the
/// track's start at the top to its end at the bottom. None while the
/// content fits: no scrollbar is drawn.
pub fn thumb(content: f64, pane: f64, scrolled: f64, corner: f64) -> Option<(f64, f64)> {
    if content <= pane {
        return None;
    }
    let track = track(pane, corner);
    let length = (track * pane / content).max(SCROLLBAR_LEAST.min(track));
    let share = (scrolled / (content - pane)).clamp(0.0, 1.0);
    Some((SCROLLBAR_INSET + (track - length) * share, length))
}

/// How long the track is along a pane `pane` long, `corner` short of its end.
fn track(pane: f64, corner: f64) -> f64 {
    (pane - 2.0 * SCROLLBAR_INSET - corner).max(0.0)
}

/// Where on its scrollbar a press is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    /// On the track before the thumb: above it, or left of it.
    Before,
    Thumb,
    /// On the track after the thumb: below it, or right of it.
    After,
}

/// The scrollbar of the pane at `pane`, in window points, scrolling
/// `axis`: along its right edge scrolling down, along its foot scrolling
/// across, `corner` short of the pane's end, over `content` of which it
/// is scrolled `scrolled`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub pane: Area,
    pub axis: Axis,
    pub content: f64,
    pub scrolled: f64,
    pub corner: f64,
}

impl Bar {
    /// How long the pane is along the way it scrolls: one page.
    pub fn page(&self) -> f64 {
        match self.axis {
            Axis::Down => self.pane.height(),
            Axis::Across => self.pane.width(),
        }
    }

    /// How far along the way it scrolls `at`, a point in the window, is.
    pub fn along(&self, at: (f64, f64)) -> f64 {
        match self.axis {
            Axis::Down => at.1,
            Axis::Across => at.0,
        }
    }

    /// The thumb, in window points; none while the content fits.
    pub fn thumb(&self) -> Option<Area> {
        let (start, length) = thumb(self.content, self.page(), self.scrolled, self.corner)?;
        let edge = SCROLLBAR_INSET + SCROLLBAR_WIDE;
        Some(match self.axis {
            Axis::Down => {
                let (x, y) = (self.pane.x1 - edge, self.pane.y0 + start);
                Area::new(x, y, x + SCROLLBAR_WIDE, y + length)
            }
            Axis::Across => {
                let (x, y) = (self.pane.x0 + start, self.pane.y1 - edge);
                Area::new(x, y, x + length, y + SCROLLBAR_WIDE)
            }
        })
    }

    /// Where a press takes hold of the scrollbar, in window points: its
    /// whole track, as wide as the thumb and [`SCROLLBAR_REACH`] more on
    /// the pane's inner side. It keeps [`SCROLLBAR_INSET`] off every edge
    /// of the pane, clear of where a line between panes is taken hold of.
    /// None while the content fits.
    pub fn lane(&self) -> Option<Area> {
        self.thumb()?;
        let (p, inset, wide) = (self.pane, SCROLLBAR_INSET, SCROLLBAR_WIDE + SCROLLBAR_REACH);
        Some(match self.axis {
            Axis::Down => {
                Area::new(p.x1 - inset - wide, p.y0 + inset, p.x1 - inset, p.y1 - inset - self.corner)
            }
            Axis::Across => {
                Area::new(p.x0 + inset, p.y1 - inset - wide, p.x1 - inset - self.corner, p.y1 - inset)
            }
        })
    }

    /// The part of the scrollbar under `at`, a point in the window; none
    /// off its lane, or while the content fits.
    pub fn at(&self, at: (f64, f64)) -> Option<Part> {
        let lane = self.lane()?;
        let inside = (lane.x0..lane.x1).contains(&at.0) && (lane.y0..lane.y1).contains(&at.1);
        if !inside {
            return None;
        }
        let thumb = self.thumb()?;
        let (start, end) = match self.axis {
            Axis::Down => (thumb.y0, thumb.y1),
            Axis::Across => (thumb.x0, thumb.x1),
        };
        let along = self.along(at);
        Some(if along < start {
            Part::Before
        } else if along < end {
            Part::Thumb
        } else {
            Part::After
        })
    }

    /// How far the pane is scrolled with its thumb, taken hold of while
    /// it was scrolled `from`, moved `moved` points along its track: the
    /// content moves as much more than the thumb as the track it can
    /// travel is shorter than what is out of view. Not yet held within
    /// what there is to scroll.
    pub fn dragged(&self, from: f64, moved: f64) -> f64 {
        let page = self.page();
        let Some((_, length)) = thumb(self.content, page, self.scrolled, self.corner) else {
            return from;
        };
        let travel = track(page, self.corner) - length;
        if travel <= 0.0 {
            return from;
        }
        from + moved * (self.content - page) / travel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(pane: Area, axis: Axis, content: f64, scrolled: f64, corner: f64) -> Bar {
        Bar { pane, axis, content, scrolled, corner }
    }

    #[test]
    fn content_that_fits_draws_no_scrollbar() {
        assert_eq!(thumb(500.0, 900.0, 0.0, 0.0), None);
        assert_eq!(thumb(900.0, 900.0, 0.0, 0.0), None, "exactly the pane's size fits");
        let fits = bar(Area::new(0.0, 0.0, 300.0, 900.0), Axis::Across, 300.0, 0.0, 0.0);
        assert_eq!((fits.thumb(), fits.lane(), fits.at((150.0, 890.0))), (None, None, None));
    }

    #[test]
    fn the_thumb_is_the_share_in_view_placed_as_far_as_it_is_scrolled() {
        let (pane, content) = (400.0, 1600.0);
        let track = pane - 2.0 * SCROLLBAR_INSET;
        let (top, length) = thumb(content, pane, 0.0, 0.0).unwrap();
        assert_eq!(length, track * 0.25, "a quarter of the content in view, a quarter of the track");
        assert_eq!(top, SCROLLBAR_INSET, "at the very top, the track's start");
        let (bottom, _) = thumb(content, pane, content - pane, 0.0).unwrap();
        assert_eq!(bottom + length, SCROLLBAR_INSET + track, "at the very bottom, the track's end");
        let (middle, _) = thumb(content, pane, (content - pane) / 2.0, 0.0).unwrap();
        assert_eq!(middle, SCROLLBAR_INSET + (track - length) / 2.0, "halfway, halfway");
    }

    #[test]
    fn a_thumb_too_short_to_see_is_drawn_at_its_least() {
        let (top, length) = thumb(1e6, 400.0, 1e6, 0.0).unwrap();
        assert_eq!(length, SCROLLBAR_LEAST);
        assert_eq!(top + length, 400.0 - SCROLLBAR_INSET, "still ending at the track's end");
    }

    #[test]
    fn a_pane_scrolls_down_on_its_right_edge_and_across_at_its_foot() {
        let pane = Area::new(100.0, 0.0, 500.0, 800.0);
        let down = bar(pane, Axis::Down, 1600.0, 0.0, 0.0).thumb().unwrap();
        assert_eq!((down.x1, down.width()), (500.0 - SCROLLBAR_INSET, SCROLLBAR_WIDE));
        let across = bar(pane, Axis::Across, 800.0, 400.0, 0.0).thumb().unwrap();
        assert_eq!((across.y1, across.height()), (800.0 - SCROLLBAR_INSET, SCROLLBAR_WIDE));
        assert_eq!(across.x1, 500.0 - SCROLLBAR_INSET, "scrolled to the end");
        let short = bar(pane, Axis::Across, 800.0, 400.0, 10.0).thumb().unwrap();
        assert_eq!(short.x1, 500.0 - SCROLLBAR_INSET - 10.0, "short of the corner");
    }

    #[test]
    fn a_press_is_on_the_thumb_or_on_the_track_either_side_of_it() {
        let pane = Area::new(100.0, 0.0, 500.0, 800.0);
        let down = bar(pane, Axis::Down, 3200.0, 1200.0, 0.0);
        let thumb = down.thumb().unwrap();
        let x = thumb.center().x;
        assert_eq!(down.at((x, thumb.center().y)), Some(Part::Thumb));
        assert_eq!(down.at((x, thumb.y0 - 1.0)), Some(Part::Before));
        assert_eq!(down.at((x, thumb.y1 + 1.0)), Some(Part::After));
        assert_eq!(down.at((thumb.x0 - SCROLLBAR_REACH, thumb.center().y)), Some(Part::Thumb), "beside it");
        assert_eq!(down.at((thumb.x0 - SCROLLBAR_REACH - 1.0, thumb.center().y)), None, "the pane's");
        assert_eq!(down.at((500.0 - SCROLLBAR_INSET, thumb.center().y)), None, "the edge's");
    }

    #[test]
    fn dragging_the_thumb_its_whole_travel_scrolls_the_whole_content() {
        let pane = Area::new(0.0, 0.0, 300.0, 400.0);
        let down = bar(pane, Axis::Down, 1600.0, 0.0, 0.0);
        let thumb = down.thumb().unwrap();
        let travel = 400.0 - 2.0 * SCROLLBAR_INSET - thumb.height();
        assert_eq!(down.dragged(0.0, travel), 1200.0, "top to bottom, the whole of what is out of view");
        assert_eq!(down.dragged(600.0, -travel / 4.0), 300.0, "a quarter back, a quarter of it back");
        let fits = bar(pane, Axis::Down, 300.0, 0.0, 0.0);
        assert_eq!(fits.dragged(0.0, 50.0), 0.0, "nothing to scroll");
    }
}
