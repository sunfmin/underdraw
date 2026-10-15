//! Components the project shares (ADR 0021), run with the real binary:
//! drafts/shop's OrderRow (nodes, calling cancelOrder) and OrderStatus (a
//! look) live in its components/, and both of its screens use them as
//! their own; one edit to the file changes both.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const SHOP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/shop");
const SCREENS: [&str; 2] = ["orders", "order"];

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// A copy of the shop in a directory of its own, named `name`.
fn shop(name: &str) -> PathBuf {
    let copy = std::env::temp_dir().join(format!("underdraw-components-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    for file in [
        "domain.json",
        "tokens.json",
        "orders/screen.json",
        "orders/platforms/web.json",
        "order/screen.json",
        "order/platforms/web.json",
        "components/OrderRow.json",
        "components/OrderStatus.json",
    ] {
        let to = copy.join(file);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(Path::new(SHOP).join(file), to).unwrap();
    }
    copy
}

fn edit(file: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains(from), "{from} is not in {}", file.display());
    std::fs::write(file, text.replacen(from, to, 1)).unwrap();
}

/// What `check` finds in the screen `name` under `root`, as (at, problem).
fn findings(root: &Path, name: &str) -> Vec<(String, String)> {
    let (checked, code) = underdraw(&["check", root.join(name).to_str().unwrap()]);
    assert_eq!(code, 1, "{checked}");
    let found = checked["findings"].as_array().unwrap();
    found.iter().map(|f| (f["at"].as_str().unwrap().into(), f["problem"].as_str().unwrap().into())).collect()
}

/// The id of each node the screen `name` under `root` draws first.
fn drawn(root: &Path, name: &str) -> Vec<String> {
    let png = root.join(format!("{name}.png"));
    let dir = root.join(name);
    let (rendered, code) = underdraw(&["render", dir.to_str().unwrap(), "--out", png.to_str().unwrap()]);
    assert_eq!(code, 0, "{rendered}");
    rendered["nodes"].as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn two_screens_use_the_projects_components_and_one_edit_changes_both() {
    for name in SCREENS {
        let written = std::fs::read_to_string(Path::new(SHOP).join(name).join("screen.json")).unwrap();
        assert!(written.contains(r#""component": "OrderRow""#), "{name} uses OrderRow");
        assert!(!written.contains(r#""components""#), "{name} declares no component of its own");
    }
    let root = shop("shared");
    // Each screen's instance, as drawn in its first state and as written.
    let instance = [("orders", "order-0-row", "order-{index}-row"), ("order", "summary", "summary")];
    for (name, id, written) in instance {
        let (checked, code) = underdraw(&["check", root.join(name).to_str().unwrap()]);
        assert_eq!((code, &checked["ok"]), (0, &json!(true)), "{checked}");
        let ids = drawn(&root, name);
        for inside in ["status", "cancel"] {
            assert!(ids.contains(&format!("{id}-{inside}")), "{name}: {ids:?}");
        }
        let (handed, code) = underdraw(&["handoff", root.join(name).to_str().unwrap(), "--platform", "web"]);
        assert_eq!((code, &handed["ok"]), (0, &json!(true)), "{handed}");
        let calls = handed["nodes"].as_array().unwrap().iter().any(|node| {
            node["id"] == format!("{written}-cancel")
                && node["events"].as_array().unwrap().iter().any(|e| e["operation"] == "cancelOrder")
        });
        assert!(calls, "{name}: the shared OrderRow's cancel calls cancelOrder: {}", handed["nodes"]);
    }

    edit(
        &root.join("components/OrderRow.json"),
        r#"{ "id": "status","#,
        r#"{ "id": "placed", "component": "Text", "text": "Placed today" },
      { "id": "status","#,
    );
    for (name, id, _) in instance {
        assert!(drawn(&root, name).contains(&format!("{id}-placed")), "{name} changed with the file");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_names_a_cycle_a_missing_shared_component_and_a_screens_that_shadows_one() {
    let root = shop("cycle");
    edit(
        &root.join("components/OrderRow.json"),
        r#"{ "id": "status","#,
        r#"{ "id": "again", "component": "OrderRow", "with": { "order": "{order}" } },
      { "id": "status","#,
    );
    let found = findings(&root, "order");
    assert!(
        found.iter().any(
            |(at, p)| at == "summary-again" && p == "component OrderRow uses itself: OrderRow → OrderRow"
        ),
        "{found:?}"
    );
    std::fs::remove_dir_all(root).unwrap();
    let root = shop("look-cycle");
    edit(
        &root.join("components/OrderStatus.json"),
        r#"{ "text": "{status}", "font": "font.caption" }"#,
        r#"{ "component": "OrderStatus", "status": "{status}" }"#,
    );
    let found = findings(&root, "order");
    let cycle = (
        "components/OrderStatus.json.look".to_string(),
        "component OrderStatus uses itself: OrderStatus → OrderStatus".to_string(),
    );
    assert!(found.contains(&cycle), "{found:?}");

    std::fs::remove_dir_all(root).unwrap();
    let root = shop("missing");
    std::fs::remove_file(root.join("components/OrderStatus.json")).unwrap();
    for (name, id) in [("orders", "order-{index}-row-status"), ("order", "summary-status")] {
        let found = findings(&root, name);
        assert!(
            found.iter().any(|(at, p)| at == id
                && p.contains("component OrderStatus is not defined")
                && p.contains("components/OrderStatus.json")),
            "{name}: {found:?}"
        );
    }

    std::fs::remove_dir_all(root).unwrap();
    let root = shop("shadows");
    let file = root.join("order/screen.json");
    let mut screen: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    screen["components"] = json!({ "OrderRow": { "params": { "order": "Order" }, "nodes": { "component": "Text", "text": "{order.number}" } } });
    std::fs::write(&file, screen.to_string()).unwrap();
    let found = findings(&root, "order");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "components.OrderRow");
    assert!(
        found[0].1.starts_with("shadows the project's OrderRow, shared in components/OrderRow.json"),
        "{found:?}"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn help_says_where_shared_components_live() {
    let help = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    assert!(help(&["--help"]).contains("components/<Name>.json"));
    let check = help(&["check", "--help"]);
    assert!(check.contains("components/<Name>.json") && check.contains("shadows the project's"), "{check}");
}
