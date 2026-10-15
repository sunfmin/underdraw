//! A length a look computes is handed off as the expression, with the
//! params and tokens it reads, for the platform to compute: never a
//! number per state.

use serde_json::{Value, json};
use underdraw_format::{Screen, Shared};

fn platform() -> Value {
    let path = format!("{}/../format/tests/airbnb-listing/platforms/web.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn a_computed_length_is_handed_off_as_written() {
    let screen = Screen::read(
        &json!({
            "screen": "Outline",
            "data": { "title": "string" },
            "fixtures": { "default": { "title": "a" } },
            "components": { "TreeLine": {
                "params": { "depth": "int 0..4", "title": "string" },
                "look": { "box": "row", "padding": [8, 12, 8, "{depth} * space.4 + 12"],
                          "children": [ { "text": "{title}" } ] } } },
            "root": { "id": "screen", "component": "Screen", "children": [
                { "id": "line", "component": "TreeLine", "depth": 2, "title": "{title}" } ] }
        }),
        Shared::default(),
    );
    let handoff = underdraw_handoff::handoff(&screen, &platform(), &json!({ "space": { "4": 16 } }))
        .unwrap_or_else(|c| panic!("{:?}", c.findings));
    let lengths = serde_json::to_value(&handoff.lengths).unwrap();
    assert_eq!(
        lengths,
        json!([{
            "component": "TreeLine",
            "at": "components.TreeLine.look.padding[3]",
            "length": "{depth} * space.4 + 12",
            "params": { "depth": "int 0..4" },
            "tokens": ["space.4"]
        }])
    );
}
