//! Boxes in the window, in points: made from `f64`s, as the window works
//! in them, and asked whether a point is on one. The one place a press is
//! tested against a box.

use underdraw_layout::Rect;

/// The box `width` × `height` at `x`, `y`; a negative size is none.
pub fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x: x as f32, y: y as f32, width: width.max(0.0) as f32, height: height.max(0.0) as f32 }
}

/// Whether `at` is on `r`, edges included.
pub fn over(r: Rect, at: (f64, f64)) -> bool {
    let (x, y) = (f64::from(r.x), f64::from(r.y));
    (x..=x + f64::from(r.width)).contains(&at.0) && (y..=y + f64::from(r.height)).contains(&at.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_over_a_box_edges_included() {
        let r = rect(10.0, 20.0, 30.0, 40.0);
        for at in [(10.0, 20.0), (40.0, 60.0), (25.0, 30.0)] {
            assert!(over(r, at), "{at:?}");
        }
        for at in [(9.9, 30.0), (40.1, 30.0), (25.0, 19.9), (25.0, 60.1)] {
            assert!(!over(r, at), "{at:?}");
        }
        assert_eq!(rect(0.0, 0.0, -5.0, 3.0).width, 0.0, "a negative size is none");
    }
}
