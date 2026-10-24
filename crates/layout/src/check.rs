//! `check` with the layout in it: what underdraw-format finds, then every
//! overlay a picture would cut, which only laying each state out can show.

use serde_json::Value;
use underdraw_format::{Breakpoint, Checked, Finding, Lang, Model, Screen, ScreenParams, State};

use crate::{Measure, Pictures, layout};

/// The widths a project's descriptions are drawn and checked at when its
/// tokens.json names none: a phone and a laptop browser.
pub const DEFAULT_WIDTHS: [u32; 2] = [390, 1120];

/// The least and the most a width may be, in points, as `render --width`
/// takes it.
pub const WIDTH_RANGE: std::ops::RangeInclusive<u32> = 200..=4000;

/// The widths `tokens` names (`"widths": [390, 1440]`), in the order it
/// names them, the first the one a single picture is drawn at; or
/// [`DEFAULT_WIDTHS`] when it names none. Why they cannot be used, when
/// they are not a list of whole numbers each in [`WIDTH_RANGE`].
pub fn named_widths(tokens: &Value) -> Result<Vec<u32>, String> {
    let Some(widths) = tokens.get("widths") else {
        return Ok(DEFAULT_WIDTHS.to_vec());
    };
    let (least, most) = (WIDTH_RANGE.start(), WIDTH_RANGE.end());
    let wrong = || {
        format!(
            "\"widths\" is a list of the widths in points the project's descriptions are drawn and checked at, each a whole number from {least} to {most}, such as [390, 1440]: it is {widths}"
        )
    };
    let list = widths.as_array().filter(|list| !list.is_empty()).ok_or_else(wrong)?;
    list.iter()
        .map(|width| width.as_u64().and_then(|w| u32::try_from(w).ok()).filter(|w| WIDTH_RANGE.contains(w)))
        .collect::<Option<Vec<u32>>>()
        .ok_or_else(wrong)
}

/// The widths `tokens` names, as [`named_widths`] reads them; the default
/// ones when they cannot be used, which `check` finds.
pub fn widths(tokens: &Value) -> Vec<u32> {
    named_widths(tokens).unwrap_or_else(|_| DEFAULT_WIDTHS.to_vec())
}

/// Every fault in `screen`: what underdraw_format finds, then, when there
/// is none, each length a look computes that cannot be proved from 0 to
/// `MOST` points, then widths in `tokens` that cannot be used, or else every overlay
/// that runs off the page in some state at one of the widths it names
/// ([`widths`]), with text measured by `measure`, and every node that
/// draws a state `pictures` cannot give. `tokens` is the file
/// it takes its tokens from, as `layout` takes it; `others` the params of
/// the screens under its root, as `underdraw_format::check_among` takes them.
pub fn check(
    screen: &Screen,
    tokens: &Value,
    others: &ScreenParams,
    measure: &mut dyn Measure,
    pictures: &dyn Pictures,
) -> Result<Checked, String> {
    let model = match Model::read(screen, underdraw_format::check_among(screen, others)) {
        Ok(model) => model,
        Err(checked) => return Ok(*checked),
    };
    // A length a look computes is proved in range before anything is drawn.
    let lengths = crate::length::faults(&screen.components, tokens);
    if !lengths.is_empty() {
        let mut checked = model.checked;
        for (at, problem) in lengths {
            checked.fault(Finding { at, problem });
        }
        return Ok(checked);
    }
    let widths = match named_widths(tokens) {
        Ok(widths) => widths,
        Err(problem) => {
            let mut checked = model.checked;
            checked.fault(Finding { at: "tokens.json widths".to_string(), problem });
            return Ok(checked);
        }
    };
    let mut cut: Vec<Finding> = Vec::new();
    for state in model.states() {
        let (what, name) = match state {
            State::Fixture(name) => ("fixture", name),
            State::Phase(name) => ("phase", name),
        };
        for &width in &widths {
            let root = model.instantiate(state, Breakpoint::for_width(tokens, width), None)?;
            let page = layout(&root, tokens, screen, width as f32, Lang::default(), measure, pictures)?;
            for finding in page.unpictured {
                if !cut.contains(&finding) {
                    cut.push(finding);
                }
            }
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
    for finding in cut {
        checked.fault(finding);
    }
    Ok(checked)
}
