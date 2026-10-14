//! Compares the Airbnb draft against captures made from its own wireframe:
//! one that matches it, then one way of getting it wrong at a time. Text is
//! measured by a stand-in (every character half the font size wide), the
//! same for the wireframe and the capture.

use serde_json::{Map, Value, json};
use underdraw_compare::{Comparison, compare};
use underdraw_format::{Breakpoint, State, instantiate};
use underdraw_handoff::handoff;
use underdraw_layout::{Font, Measure, Size, layout};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/airbnb-listing");

struct Fixed;

impl Measure for Fixed {
    fn text(&mut self, text: &str, font: Font, max_width: Option<f32>) -> Size {
        let advance = font.size / 2.0;
        let one_line = text.chars().count() as f32 * advance;
        let width = match max_width {
            Some(max) => one_line.min(max.max(advance)),
            None => one_line,
        };
        let lines = if width > 0.0 { (one_line / width).ceil().max(1.0) } else { 1.0 };
        Size { width, height: lines * font.line_height }
    }
}

fn read(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{DRAFT}/{name}")).unwrap()).unwrap()
}

/// What a UI built exactly as described would capture, on iOS.
fn faithful(state: State, width: f32) -> Value {
    let (screen, tokens, mapping) = (read("screen.json"), read("tokens.json"), read("platforms/ios.json"));
    let breakpoint = Breakpoint::for_width(&tokens, width as u32);
    let named = match state {
        State::Fixture(name) => json!({ "fixture": name }),
        State::Phase(name) => json!({ "phase": name }),
    };
    let handed = handoff(&screen, &mapping, &tokens, None).unwrap();
    let expected = handed.states.iter().find(|s| s.state == named && s.breakpoint == breakpoint).unwrap();
    let page =
        layout(&instantiate(&screen, state, breakpoint).unwrap(), &tokens, None, width, &mut Fixed).unwrap();
    let mut elements = Vec::new();
    for id in &expected.present {
        let rect = page.nodes.iter().find(|n| &n.id == id).unwrap().rect;
        let mut element = Map::new();
        element.insert("id".into(), json!(id));
        for (key, value) in [("x", rect.x), ("y", rect.y), ("width", rect.width), ("height", rect.height)] {
            element.insert(key.into(), json!(value));
        }
        if let Some(Value::Object(shown)) = expected.shows.get(id) {
            let mut words = Vec::new();
            for (key, value) in shown {
                match (key.as_str(), value) {
                    ("disabled", Value::Bool(disabled)) => {
                        element.insert("enabled".into(), json!(!disabled));
                    }
                    (_, Value::String(s)) => words.push(s.clone()),
                    (_, Value::Number(n)) => words.push(n.to_string()),
                    _ => {}
                }
            }
            if !words.is_empty() {
                element.insert("text".into(), json!(words.join(" ")));
            }
        }
        elements.push(Value::Object(element));
    }
    json!({ "width": width, "elements": elements })
}

fn run(state: State, capture: &Value) -> Comparison {
    let (screen, tokens, mapping) = (read("screen.json"), read("tokens.json"), read("platforms/ios.json"));
    compare(&screen, &mapping, &tokens, None, state, capture, &mut Fixed).unwrap()
}

fn element<'a>(capture: &'a mut Value, id: &str) -> &'a mut Map<String, Value> {
    let elements = capture["elements"].as_array_mut().unwrap();
    elements.iter_mut().find(|e| e["id"] == id).unwrap().as_object_mut().unwrap()
}

fn checks(comparison: &Comparison) -> Vec<(&str, &str)> {
    comparison.findings.iter().map(|f| (f.at.as_str(), f.check)).collect()
}

#[test]
fn a_faithful_capture_has_no_findings() {
    for (state, width) in [
        (State::Fixture("default"), 390.0),
        (State::Fixture("unavailable"), 1120.0),
        (State::Fixture("no-dates"), 390.0),
        (State::Phase("loading"), 390.0),
    ] {
        let comparison = run(state, &faithful(state, width));
        assert!(comparison.ok, "{state:?} at {width}: {:#?}", comparison.findings);
        let checked = &comparison.checked;
        assert!(checked.present > 3 && checked.order > 0 && checked.inside > 0, "{state:?}: {checked:?}");
        assert!(comparison.unknown.is_empty());
    }
}

#[test]
fn a_missing_node_and_one_shown_in_the_wrong_state() {
    let state = State::Fixture("unavailable");
    let mut capture = faithful(state, 1120.0);
    let elements = capture["elements"].as_array_mut().unwrap();
    elements.retain(|e| e["id"] != "booking-card");
    elements.push(json!({ "id": "booking-bar" }));
    elements.push(json!({ "id": "debug-overlay" }));
    let comparison = run(state, &capture);
    let found = checks(&comparison);
    assert!(found.contains(&("booking-card", "present")), "{found:?}");
    assert!(found.contains(&("booking-bar", "absent")), "{found:?}");
    assert_eq!(comparison.unknown, ["debug-overlay"]);
}

#[test]
fn wrong_words_and_a_button_that_should_be_disabled() {
    let state = State::Fixture("unavailable");
    let mut capture = faithful(state, 1120.0);
    let button = element(&mut capture, "reserve-button");
    button.insert("text".into(), json!("预订"));
    button.insert("enabled".into(), json!(true));
    let comparison = run(state, &capture);
    let found = checks(&comparison);
    assert_eq!(
        found,
        [("reserve-button", "enabled"), ("reserve-button", "text")],
        "{:#?}",
        comparison.findings
    );
    assert!(comparison.findings[1].problem.contains("日期不可订"));
}

#[test]
fn siblings_out_of_order_and_a_child_outside() {
    let state = State::Fixture("default");
    let mut capture = faithful(state, 390.0);
    // The capacity line drawn above the kind line it follows, still inside
    // the summary.
    let kind = element(&mut capture, "summary-kind").clone();
    let capacity = element(&mut capture, "summary-capacity");
    capacity.insert("y".into(), kind["y"].clone());
    let comparison = run(state, &capture);
    assert_eq!(checks(&comparison), [("summary-capacity", "order")], "{:#?}", comparison.findings);

    // The host row pushed off the right edge of the page.
    let mut capture = faithful(state, 390.0);
    element(&mut capture, "host-row").insert("x".into(), json!(300));
    let comparison = run(state, &capture);
    assert!(checks(&comparison).contains(&("host-row", "inside")), "{:#?}", comparison.findings);
}

#[test]
fn a_capture_that_is_not_one_is_refused() {
    let (screen, tokens, mapping) = (read("screen.json"), read("tokens.json"), read("platforms/ios.json"));
    let capture = json!({ "elements": [ { "x": 1 }, { "id": "a", "x": 1, "y": 2 } ] });
    let refused = compare(&screen, &mapping, &tokens, None, State::Fixture("default"), &capture, &mut Fixed)
        .unwrap_err();
    assert!(refused.contains("\"width\""), "{refused}");
    assert!(refused.contains("elements[0]: needs \"id\""), "{refused}");
    assert!(refused.contains("elements[1]: a: a frame is"), "{refused}");
}
