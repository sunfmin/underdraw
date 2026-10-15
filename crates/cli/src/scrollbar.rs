//! A pane's scrollbar: the one calculation every pane of the gallery that
//! scrolls draws its scrollbar by — the tree, the pictures down and across,
//! and the sidebar — from three values: how large what it holds is, how
//! large the pane is, and how far it is scrolled. Its look is the chrome's
//! ([`crate::chrome`]).

use vello::kurbo::Rect as Area;

use crate::chrome::{SCROLLBAR_INSET, SCROLLBAR_LEAST, SCROLLBAR_WIDE};

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
    let track = (pane - 2.0 * SCROLLBAR_INSET - corner).max(0.0);
    let length = (track * pane / content).max(SCROLLBAR_LEAST.min(track));
    let share = (scrolled / (content - pane)).clamp(0.0, 1.0);
    Some((SCROLLBAR_INSET + (track - length) * share, length))
}

/// The thumb of the pane at `pane`, in window points, scrolling `axis`:
/// along its right edge scrolling down, along its foot scrolling across,
/// `corner` short of the pane's end.
pub fn bar(pane: Area, axis: Axis, content: f64, scrolled: f64, corner: f64) -> Option<Area> {
    let edge = SCROLLBAR_INSET + SCROLLBAR_WIDE;
    Some(match axis {
        Axis::Down => {
            let (start, length) = thumb(content, pane.height(), scrolled, corner)?;
            let (x, y) = (pane.x1 - edge, pane.y0 + start);
            Area::new(x, y, x + SCROLLBAR_WIDE, y + length)
        }
        Axis::Across => {
            let (start, length) = thumb(content, pane.width(), scrolled, corner)?;
            let (x, y) = (pane.x0 + start, pane.y1 - edge);
            Area::new(x, y, x + length, y + SCROLLBAR_WIDE)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_that_fits_draws_no_scrollbar() {
        assert_eq!(thumb(500.0, 900.0, 0.0, 0.0), None);
        assert_eq!(thumb(900.0, 900.0, 0.0, 0.0), None, "exactly the pane's size fits");
        assert_eq!(bar(Area::new(0.0, 0.0, 300.0, 900.0), Axis::Across, 300.0, 0.0, 0.0), None);
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
        let down = bar(pane, Axis::Down, 1600.0, 0.0, 0.0).unwrap();
        assert_eq!((down.x1, down.width()), (500.0 - SCROLLBAR_INSET, SCROLLBAR_WIDE));
        let across = bar(pane, Axis::Across, 800.0, 400.0, 0.0).unwrap();
        assert_eq!((across.y1, across.height()), (800.0 - SCROLLBAR_INSET, SCROLLBAR_WIDE));
        assert_eq!(across.x1, 500.0 - SCROLLBAR_INSET, "scrolled to the end");
        let short = bar(pane, Axis::Across, 800.0, 400.0, 10.0).unwrap();
        assert_eq!(short.x1, 500.0 - SCROLLBAR_INSET - 10.0, "short of the corner");
    }
}
