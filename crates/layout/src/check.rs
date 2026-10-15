//! `check` with the layout in it: what underdraw-format finds, then every
//! overlay a picture would cut, which only laying each state out can show.

use serde_json::Value;
use underdraw_format::{Breakpoint, Checked, Finding, Lang, Model, ScreenParams, State};

use crate::{Measure, layout};

/// The widths every state is laid out at, to find what the picture would
/// cut: a phone and a laptop browser, as `render --help` advises.
pub const CHECKED_WIDTHS: [u32; 2] = [390, 1120];

/// Every fault in `screen`: what underdraw_format finds, then, when there
/// is none, every overlay that runs off the page in some state at one of
/// `CHECKED_WIDTHS`, with text measured by `measure`. `tokens` is the file
/// it takes its tokens from, as `layout` takes it; `others` the params of
/// the screens under its root, as `underdraw_format::check_among` takes them.
pub fn check(
    screen: &Value,
    tokens: &Value,
    others: &ScreenParams,
    measure: &mut dyn Measure,
) -> Result<Checked, String> {
    let model = match Model::read(screen, underdraw_format::check_among(screen, others)) {
        Ok(model) => model,
        Err(checked) => return Ok(*checked),
    };
    let mut cut = Vec::new();
    for state in model.states() {
        let (what, name) = match state {
            State::Fixture(name) => ("fixture", name),
            State::Phase(name) => ("phase", name),
        };
        for width in CHECKED_WIDTHS {
            let root = model.instantiate(state, Breakpoint::for_width(tokens, width))?;
            let page = layout(&root, tokens, screen, width as f32, Lang::default(), measure)?;
            for id in page.cut {
                cut.push(Finding {
                    at: id,
                    problem: format!(
                        "in {what} {name} at width {width} it runs off the page, which is only as tall as its content, so the picture cuts it: give {name} more content behind it, or make it shorter"
                    ),
                });
            }
        }
    }
    let mut checked = model.checked;
    if !cut.is_empty() {
        checked.ok = false;
        checked.conditions.clear();
        checked.findings.append(&mut cut);
    }
    Ok(checked)
}
