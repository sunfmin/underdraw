//! Journeys walked by `check <root>`, with the real binary: the shop's
//! journey across its two screens walks, and a change to one of them that
//! breaks it names the journey and the step.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

const SHOP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/shop");

fn check(root: &Path) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).arg("check").arg(root).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// A copy of the shop draft in a directory of its own, named `name`.
fn shop(name: &str) -> std::path::PathBuf {
    let copy = std::env::temp_dir().join(format!("underdraw-journeys-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    for file in [
        "domain.json",
        "tokens.json",
        "orders/screen.json",
        "order/screen.json",
        "journeys/cancel-an-order.json",
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

#[test]
fn the_shops_journey_by_a_navigate_and_a_transition_walks() {
    let (answer, code) = check(Path::new(SHOP));
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["journeys"],
        json!([{ "journey": "journeys/cancel-an-order.json", "name": "cancel-an-order", "ok": true, "findings": [] }])
    );
}

#[test]
fn a_screen_changed_so_the_journey_breaks_names_it() {
    let root = shop("transition");
    edit(&root.join("order/screen.json"), r#""to": "cancelled""#, r#""to": "shipped""#);
    let (answer, code) = check(&root);
    assert_eq!(code, 1, "{answer}");
    let journey = &answer["journeys"][0];
    assert_eq!((&journey["name"], &journey["ok"]), (&json!("cancel-an-order"), &json!(false)));
    assert_eq!(journey["findings"][0]["at"], "steps.1.on", "{journey}");

    let root = shop("navigate");
    edit(&root.join("orders/screen.json"), r#""navigate": "ShopOrder""#, r#""back": true"#);
    let (answer, _) = check(&root);
    let problem = answer["journeys"][0]["findings"][0]["problem"].as_str().unwrap().to_string();
    assert!(
        problem.contains("does not reach the next step, ShopOrder open: navigate it to ShopOrder"),
        "{problem}"
    );

    let root = shop("shown");
    edit(&root.join("journeys/cancel-an-order.json"), r#""state": "open""#, r#""state": "shipped""#);
    let (answer, _) = check(&root);
    let problem = answer["journeys"][0]["findings"][0]["problem"].as_str().unwrap().to_string();
    assert!(problem.starts_with("summary-cancel is not shown in ShopOrder shipped"), "{problem}");

    let root = shop("params");
    edit(&root.join("order/screen.json"), r#""orderId": "A12""#, r#""orderId": "A13""#);
    let (answer, code) = check(&root);
    assert_eq!(code, 1, "{answer}");
    let finding = &answer["journeys"][0]["findings"][0];
    assert_eq!(finding["at"], "steps.0.on", "{answer}");
    assert_eq!(
        finding["problem"],
        r#"order-{index}.tap in ShopOrders default opens ShopOrder with orderId "A12", but the next step, ShopOrder open, holds orderId "A13": give the same value on both sides"#,
        "{answer}"
    );

    let root = shop("unknown");
    edit(
        &root.join("journeys/cancel-an-order.json"),
        r#""screen": "ShopOrder", "state": "cancelled""#,
        r#""screen": "ShopOrderz", "state": "cancelled""#,
    );
    let (answer, _) = check(&root);
    let finding = &answer["journeys"][0]["findings"][0];
    assert_eq!(finding["at"], "steps.2.screen", "{answer}");
    assert!(
        finding["problem"]
            .as_str()
            .unwrap()
            .starts_with("no screen description under the root declares ShopOrderz"),
        "{answer}"
    );

    let root = shop("directory");
    edit(&root.join("journeys/cancel-an-order.json"), r#""screen": "ShopOrders""#, r#""screen": "orders""#);
    let (answer, code) = check(&root);
    assert_eq!((code, &answer["journeys"][0]["findings"]), (0, &json!([])), "{answer}");
    for root in ["transition", "navigate", "shown", "params", "unknown", "directory"] {
        std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("underdraw-journeys-{root}-{}", std::process::id())),
        )
        .unwrap();
    }
}
