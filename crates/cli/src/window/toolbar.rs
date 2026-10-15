//! The toolbar along the window's top, over the three panes: at its left,
//! back and forward through what the window has shown
//! ([`super::history`]); at its right, the button that hides and shows the
//! sidebar, as ⌘\ does. It lies above the gallery's points, which start at
//! its foot: its own are from `-TOOLBAR` to 0.

use underdraw_layout::{Align, Draw, Ink, Measure, Rect};

use crate::area::{over, rect};
use crate::chrome::{BUTTON, BUTTON_PAD, BUTTON_RADIUS, CARD_PADDING as PADDING, TOOLBAR, TOOLBAR_INSET};

/// What a button of the toolbar does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Tool {
    Back,
    Forward,
    Sidebar,
}

/// A button of the toolbar, laid out: where it is, in the gallery's
/// points, what it does and says, and whether there is anything to do:
/// grayed, and pressed in vain, where there is not.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Button {
    pub area: Rect,
    pub tool: Tool,
    pub said: String,
    pub enabled: bool,
}

/// The toolbar `width` wide: the buttons in `left` from its left end, in
/// order, and those in `right` ending at its right end, each as wide as
/// what it says, set by `measure`.
pub(super) fn lay_out(
    measure: &mut dyn Measure,
    width: f64,
    left: Vec<(Tool, String, bool)>,
    right: Vec<(Tool, String, bool)>,
) -> Vec<Button> {
    let tall = f64::from(BUTTON.line_height) + 2.0 * BUTTON_PAD;
    let top = -TOOLBAR + (TOOLBAR - tall) / 2.0;
    let mut wide = |said: &str| f64::from(measure.text(said, BUTTON, None).width).ceil() + 2.0 * PADDING;
    let mut buttons = Vec::new();
    let mut x = TOOLBAR_INSET;
    for (tool, said, enabled) in left {
        let w = wide(&said);
        buttons.push(Button { area: rect(x, top, w, tall), tool, said, enabled });
        x += w + TOOLBAR_INSET;
    }
    let mut x = width - TOOLBAR_INSET;
    for (tool, said, enabled) in right.into_iter().rev() {
        let w = wide(&said);
        x -= w;
        buttons.push(Button { area: rect(x, top, w, tall), tool, said, enabled });
        x -= TOOLBAR_INSET;
    }
    buttons
}

/// What the button at `at`, a point in the gallery's points, does; none
/// off every button, or on one grayed.
pub(super) fn tool_at(buttons: &[Button], at: (f64, f64)) -> Option<Tool> {
    buttons.iter().find(|button| button.enabled && over(button.area, at)).map(|button| button.tool)
}

/// The toolbar `width` wide holding `buttons`, drawn: the tree's surface
/// is painted under it by the window, a hairline along its foot, and each
/// button as the sidebar's are, a grayed one in the muted ink.
pub(super) fn draws(width: f64, buttons: &[Button]) -> Vec<Draw> {
    let mut draws = vec![Draw::Line { from: [0.0, -0.5], to: [width as f32, -0.5], ink: Ink::Line }];
    for button in buttons {
        let (stroke, ink) = if button.enabled { (Ink::Muted, Ink::Text) } else { (Ink::Line, Ink::Muted) };
        draws.push(Draw::Box {
            rect: button.area,
            radius: BUTTON_RADIUS,
            fill: Some(Ink::Paper),
            stroke: Some(stroke),
            dashed: false,
        });
        let Rect { x, y, width, height } = button.area;
        draws.push(Draw::Text {
            rect: rect(
                f64::from(x),
                f64::from(y) + BUTTON_PAD,
                f64::from(width),
                f64::from(height) - 2.0 * BUTTON_PAD,
            ),
            text: button.said.clone(),
            font: BUTTON,
            ink,
            max_lines: Some(1),
            align: Align::Center,
            underline: false,
            strike: false,
        });
    }
    draws
}
