//! The built-in TextField is handed off as every component is: each platform
//! of the Airbnb listing draft names how it builds one.

use serde_json::{Value, json};
use underdraw_format::{Screen, Shared};

fn platform(name: &str) -> Value {
    let path = format!("{}/../format/tests/airbnb-listing/platforms/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A message box: a TextField several lines high, bound to view state.
fn screen() -> Screen {
    Screen::read(
        &json!({
            "screen": "Message",
            "data": { "draft": { "type": "string", "view": true } },
            "fixtures": { "empty": { "draft": "" } },
            "root": { "id": "screen", "component": "Screen", "children": [
                { "id": "message", "component": "TextField", "bind": "draft",
                  "placeholder": "Write a message", "lines": 4 } ] }
        }),
        Shared::default(),
    )
}

#[test]
fn each_platform_names_how_it_builds_a_text_field() {
    for name in ["web", "ios", "android"] {
        let mapping = platform(name);
        let handoff = underdraw_handoff::handoff(&screen(), &mapping, &json!({}))
            .unwrap_or_else(|c| panic!("{name}: {:?}", c.findings));
        let field = handoff.nodes.iter().find(|n| n.id == "message").expect("the field is handed off");
        assert_eq!(field.component, "TextField");
        assert_eq!(Some(field.implement.as_str()), mapping["components"]["TextField"].as_str(), "{name}");
        assert!(
            !handoff.findings.iter().any(|f| f.problem.contains("TextField")),
            "{name}: {:?}",
            handoff.findings
        );
    }
}
