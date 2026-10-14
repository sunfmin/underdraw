//! `check` with the layout in it: what underdraw-format finds, then every
//! overlay a picture would cut, which only laying each state out can show.

use serde_json::Value;
use underdraw_format::{Breakpoint, Checked, Finding, State, instantiate};

use crate::{Measure, layout};

/// The widths every state is laid out at, to find what the picture would
/// cut: a phone and a laptop browser, as `render --help` advises.
pub const CHECKED_WIDTHS: [u32; 2] = [390, 1120];

/// Every fault in `screen`: what underdraw_format finds, then, when there
/// is none, every overlay that runs off the page in some state at one of
/// `CHECKED_WIDTHS`, with text measured by `measure`. `tokens` and `kinds`
/// are the files beside it, as `layout` takes them.
pub fn check(
    screen: &Value,
    tokens: &Value,
    kinds: Option<&Value>,
    measure: &mut dyn Measure,
) -> Result<Checked, String> {
    let mut checked = underdraw_format::check(screen, kinds);
    if !checked.ok {
        return Ok(checked);
    }
    let states = checked
        .fixtures
        .iter()
        .map(|name| ("fixture", State::Fixture(name)))
        .chain(checked.phases.iter().map(|name| ("phase", State::Phase(name))));
    let mut cut = Vec::new();
    for (what, state) in states {
        let name = match state {
            State::Fixture(name) | State::Phase(name) => name,
        };
        for width in CHECKED_WIDTHS {
            let root = instantiate(screen, state, Breakpoint::for_width(tokens, width))?;
            let page = layout(&root, tokens, kinds, width as f32, measure)?;
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
    if !cut.is_empty() {
        checked.ok = false;
        checked.conditions.clear();
        checked.findings.append(&mut cut);
    }
    Ok(checked)
}
