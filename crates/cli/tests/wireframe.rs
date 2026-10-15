//! A description's state drawn inside another as a picture (the built-in
//! Wireframe), run with the real binary on a project written here: check
//! holds that what it names is there and draws no description inside
//! itself, and render draws it.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// A project under a directory of its own, named `name`: domain.json at
/// its root, and a description `List` with a fixture `empty`.
fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("underdraw-wireframe-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("domain.json"), "{}").unwrap();
    describe(&root, "List", json!([{ "id": "head", "component": "Text", "text": "{title}" }]));
    root
}

/// The description of the screen `screen` at `<root>/<screen>`, its root
/// holding `children`.
fn describe(root: &Path, screen: &str, children: Value) -> String {
    let dir = root.join(screen);
    std::fs::create_dir_all(&dir).unwrap();
    let description = json!({
        "screen": screen, "data": { "title": "string" }, "types": {}, "conditions": {},
        "fixtures": { "default": { "title": "Orders" }, "empty": { "title": "Nothing yet" } },
        "root": { "id": "root", "component": "Screen", "children": children }
    });
    std::fs::write(dir.join("screen.json"), description.to_string()).unwrap();
    dir.to_str().unwrap().to_string()
}

/// The findings `check` answers of the description at `dir`, by node.
fn findings(dir: &str) -> Vec<(String, String)> {
    let (checked, code) = underdraw(&["check", dir]);
    assert_eq!(code, 1, "{checked}");
    let findings = checked["findings"].as_array().unwrap();
    findings
        .iter()
        .map(|f| (f["at"].as_str().unwrap().to_string(), f["problem"].as_str().unwrap().to_string()))
        .collect()
}

#[test]
fn a_wireframe_is_drawn_at_the_nodes_size() {
    let root = project("drawn");
    let tool = describe(
        &root,
        "Tool",
        json!([{ "id": "preview", "component": "Wireframe", "screen": "List", "state": "empty" }]),
    );
    let (checked, code) = underdraw(&["check", &tool]);
    assert_eq!((code, &checked["ok"]), (0, &json!(true)), "{checked}");
    let png = root.join("tool.png");
    let (rendered, code) = underdraw(&["render", &tool, "--out", png.to_str().unwrap()]);
    assert_eq!(code, 0, "{rendered}");
    let nodes = rendered["nodes"].as_array().unwrap();
    let preview = nodes.iter().find(|n| n["id"] == "preview").unwrap();
    assert_eq!(preview["box"]["width"], 390.0);
    assert!(preview["box"]["height"].as_f64().unwrap() > 0.0, "{preview}");
    assert!(nodes.iter().all(|n| n["id"] != "head"), "the pictured screen's nodes are not this one's");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_description_or_state_not_there_is_a_finding_at_the_node() {
    let root = project("missing");
    let tool = describe(
        &root,
        "Tool",
        json!([
            { "id": "no-state", "component": "Wireframe", "screen": "List", "state": "gone" },
            { "id": "no-screen", "component": "Wireframe", "screen": "Nowhere" }
        ]),
    );
    let found = findings(&tool);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].0, "no-state");
    assert!(found[0].1.contains("List has no state \"gone\""), "{found:?}");
    assert_eq!(found[1].0, "no-screen");
    assert!(found[1].1.contains("declares the screen \"Nowhere\""), "{found:?}");
    let (rendered, code) = underdraw(&["render", &tool]);
    assert_eq!(code, 2, "a picture of it would mislead: {rendered}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_description_that_draws_itself_at_any_depth_is_refused() {
    let root = project("itself");
    let mirror =
        describe(&root, "Mirror", json!([{ "id": "me", "component": "Wireframe", "screen": "Mirror" }]));
    let found = findings(&mirror);
    assert_eq!(found[0].0, "me");
    assert!(found[0].1.contains("a description cannot draw itself"), "{found:?}");

    let a = describe(&root, "A", json!([{ "id": "b", "component": "Wireframe", "screen": "B" }]));
    describe(&root, "B", json!([{ "id": "a", "component": "Wireframe", "screen": "A" }]));
    let found = findings(&a);
    assert_eq!(found[0].0, "b");
    assert!(found[0].1.contains("(A → B → A)"), "{found:?}");
    let (checked, code) = underdraw(&["check", root.to_str().unwrap()]);
    assert_eq!((code, &checked["ok"]), (1, &json!(false)), "{checked}");
    std::fs::remove_dir_all(root).unwrap();
}
