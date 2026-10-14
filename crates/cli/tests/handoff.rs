//! Hands the Airbnb draft off to each platform with the real binary, and
//! reads what it answers.

use std::process::Command;

use serde_json::{Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/airbnb-listing");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

fn node<'a>(answer: &'a Value, id: &str) -> &'a Value {
    answer["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

#[test]
fn every_platform_is_covered() {
    for (platform, file, test_id) in [
        ("ios", "ListingDetailModel.swift", ".accessibilityIdentifier(\"reserve-button\")"),
        ("android", "ListingDetailModel.kt", "Modifier.testTag(\"reserve-button\")"),
        ("web", "ListingDetailModel.ts", "data-testid=\"reserve-button\""),
    ] {
        let (answer, code) = underdraw(&["handoff", DRAFT, "--platform", platform]);
        assert_eq!((code, &answer["ok"]), (0, &json!(true)), "{platform}: {}", answer["findings"]);
        assert_eq!(answer["code"]["file"], file);
        assert!(
            node(&answer, "reserve-button")["test_id"].as_str().unwrap().starts_with(test_id),
            "{platform}"
        );
        // 4 fixtures and 2 phases, each at both breakpoints
        assert_eq!(answer["states"].as_array().unwrap().len(), 12, "{platform}");
        assert_eq!(answer["unconfirmed"].as_array().unwrap().len(), 3, "{platform}");
    }
}

#[test]
fn says_what_each_state_must_show() {
    let (answer, _) = underdraw(&["handoff", DRAFT, "--platform", "ios"]);
    let state = answer["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["state"] == json!({ "fixture": "unavailable" }) && s["breakpoint"] == "regular")
        .unwrap();
    assert_eq!(state["shows"]["reserve-button"], json!({ "text": "日期不可订", "disabled": true }));
    let present = state["present"].as_array().unwrap();
    assert!(present.contains(&json!("booking-card")) && !present.contains(&json!("booking-bar")));
    let conditions = &answer["conditions"];
    assert!(
        conditions
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "datesUnavailable" && c["holds_in"] == json!(["unavailable"]))
    );
}

#[test]
fn events_and_hidden_nodes_follow_the_platform() {
    let (answer, _) = underdraw(&["handoff", DRAFT, "--platform", "web"]);
    let hidden = answer["hidden"].as_array().unwrap();
    assert!(hidden.iter().any(|h| h["id"] == "nav" && h["why"].is_string()), "{hidden:?}");
    assert!(
        answer["nodes"].as_array().unwrap().iter().all(|n| n["id"] != "nav-share"),
        "inside a hidden node"
    );
    let events = node(&answer, "reserve-button")["events"].as_array().unwrap().clone();
    assert_eq!(events.len(), 2);
    assert_eq!(
        (events[0]["action"].as_str(), events[0]["when"].as_str()),
        (Some("scrollTo"), Some("!hasDates"))
    );
    assert_eq!((events[1]["action"].as_str(), events[1]["when"].as_str()), (Some("navigate"), Some("else")));
    assert!(events[1]["implement"].as_str().unwrap().contains("router.push"));
}

#[test]
fn refuses_a_platform_that_is_not_there() {
    let (answer, code) = underdraw(&["handoff", DRAFT, "--platform", "watchos"]);
    assert_eq!(code, 2);
    let error = answer["error"].as_str().unwrap();
    assert!(error.contains("[\"android\", \"ios\", \"web\"]"), "{error}");
}

#[test]
fn a_gap_in_the_mapping_is_a_finding() {
    let dir = std::env::temp_dir().join(format!("underdraw-handoff-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("platforms")).unwrap();
    std::fs::copy(format!("{DRAFT}/screen.json"), dir.join("screen.json")).unwrap();
    let mut mapping: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{DRAFT}/platforms/ios.json")).unwrap())
            .unwrap();
    mapping["kinds"].as_object_mut().unwrap().remove("Stepper");
    std::fs::write(dir.join("platforms/ios.json"), mapping.to_string()).unwrap();
    let (answer, code) = underdraw(&["handoff", dir.to_str().unwrap(), "--platform", "ios"]);
    assert_eq!((code, &answer["ok"]), (1, &json!(false)));
    assert!(
        answer["findings"][0]["problem"].as_str().unwrap().contains("kind Stepper"),
        "{}",
        answer["findings"]
    );
    std::fs::remove_dir_all(dir).unwrap();
}
