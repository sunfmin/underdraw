//! Runs the real binary on the Airbnb draft, whole and with one fault planted
//! at a time, and reads its JSON and exit code.

use std::path::{Path, PathBuf};
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

/// A copy of the draft's screen.json with `from` replaced by `to`, where
/// `from` appears exactly once.
fn planted(name: &str, from: &str, to: &str) -> PathBuf {
    let text = std::fs::read_to_string(Path::new(DRAFT).join("screen.json")).unwrap();
    assert_eq!(text.matches(from).count(), 1, "{name}: {from:?} must appear exactly once in the draft");
    let dir = std::env::temp_dir().join(format!("underdraw-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("screen.json"), text.replace(from, to)).unwrap();
    dir
}

#[test]
fn the_draft_is_sound_and_every_condition_varies() {
    let (answer, code) = underdraw(&["check", DRAFT]);
    assert_eq!(code, 0);
    assert_eq!(answer["ok"], true, "{answer:#}");
    assert_eq!(answer["fixtures"], json!(["default", "no-dates", "unavailable", "new-listing"]));
    assert_eq!(answer["phases"], json!(["loading", "error"]));
    let holds: Vec<(&str, &Value)> = answer["conditions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["name"].as_str().unwrap(), &c["holds_in"]))
        .collect();
    assert_eq!(
        holds,
        [
            ("hasReviews", &json!(["default", "no-dates", "unavailable"])),
            ("hasDates", &json!(["default", "unavailable", "new-listing"])),
            ("canReserve", &json!(["default", "new-listing"])),
            ("datesUnavailable", &json!(["unavailable"])),
        ]
    );
}

#[test]
fn each_planted_fault_is_found_where_it_was_planted() {
    // (name, what the draft says, the fault planted in its place, where it is found, what is said)
    let cases = [
        (
            "truthiness",
            r#""when": "booking.checkIn != null && booking.checkOut != null""#,
            r#""when": "booking.checkIn && booking.checkOut != null""#,
            "conditions.hasDates",
            "no truthiness",
        ),
        (
            "expression-at-use-site",
            r#""text": "清除日期", "if": "hasDates""#,
            r#""text": "清除日期", "if": "booking.nights > 0""#,
            "calendar-clear.if",
            "name anything more in conditions",
        ),
        (
            "cases-without-else",
            r#"{ "else": "favorite" }"#,
            r#"{ "if": "listing.isSaved", "then": "favorite" }"#,
            "nav-save.icon",
            "must be {\"else\"",
        ),
        (
            "money-without-format",
            r#""{booking.total|money}""#,
            r#""{booking.total}""#,
            "bar-price-text.text",
            "|money",
        ),
        ("missing-field", r#""kind": "整套公寓", "#, "", "fixtures.default.listing", "missing kind"),
        (
            "too-long",
            r#""title": "涩谷站步行 6 分钟 · 安静的设计师公寓""#,
            &format!(r#""title": "{}""#, "长".repeat(51)),
            "fixtures.default.listing.title",
            "length 51 is outside 1..50",
        ),
        (
            "unknown-field",
            r#""isSaved": false,"#,
            r#""isSaved": false, "isFavorite": true,"#,
            "fixtures.default.listing",
            "has no field isFavorite",
        ),
        (
            "unguarded-null",
            r#""kind": "Link", "if": "hasDates", "font": "font.caption""#,
            r#""kind": "Link", "font": "font.caption""#,
            "bar-dates.text",
            "may be null",
        ),
        (
            "phase-reads-data",
            r#""title": "房源加载失败""#,
            r#""title": "{listing.title} 加载失败""#,
            "error-state.title",
            "not a path here",
        ),
        (
            "never-null",
            r#""when": "listing.reviewCount > 0 && listing.rating != null""#,
            r#""when": "listing.reviewCount != null && listing.rating != null""#,
            "conditions.hasReviews",
            "cannot be null",
        ),
        ("backwards-decimals", "number:0-1", "number:2-1", "summary-capacity.text", "unknown format"),
        (
            "logic-in-a-string",
            r#""text": "{listing.title}""#,
            r#""text": "{listing.isSaved ? a : b}""#,
            "summary-title.text",
            "not a placeholder",
        ),
        (
            "one-breakpoint",
            r#""columns": { "compact": ["1fr"], "regular": ["58%", "1fr"] }"#,
            r#""columns": { "regular": ["58%", "1fr"] }"#,
            "body.columns",
            "names both breakpoints",
        ),
        (
            "always-true",
            r#""when": "hasDates && !booking.available""#,
            r#""when": "hasDates && !booking.available || true""#,
            "conditions.datesUnavailable",
            "true in every fixture",
        ),
        (
            "each-with-children",
            r#""item": { "id": "highlight-{index}""#,
            r#""children": { "id": "highlight-{index}""#,
            "highlights",
            "put the template for one element in \"item\"",
        ),
        (
            "kind-not-defined",
            r#""kind": "Badge", "if": "listing.isGuestFavorite""#,
            r#""kind": "Bagde", "if": "listing.isGuestFavorite""#,
            "guest-favorite",
            "kind Bagde is not defined",
        ),
        (
            "limit-not-a-count",
            r#""limit": { "compact": 10, "regular": 9 }"#,
            r#""limit": "ten""#,
            "amenities-list",
            "limit is a whole number",
        ),
        (
            "icon-not-material",
            r#""icon": "ios_share""#,
            r#""icon": "share-arrow""#,
            "nav-share.icon",
            "share-arrow is not a Material Symbols icon",
        ),
        (
            "fixture-icon-not-material",
            r#"{ "icon": "wifi", "label""#,
            r#"{ "icon": "wi-fi", "label""#,
            "fixtures.default.listing.amenities[0].icon",
            "wi-fi is not a Material Symbols icon",
        ),
        (
            "item-without-each",
            r#""each": "listing.ratingBreakdown", "item""#,
            r#""item""#,
            "reviews-breakdown",
            "has no \"each\"",
        ),
    ];
    for (name, from, to, at, said) in cases {
        let dir = planted(name, from, to);
        let (answer, code) = underdraw(&["check", dir.to_str().unwrap()]);
        assert_eq!(code, 1, "{name}: findings exit 1, so a gate stops on them");
        assert_eq!(answer["ok"], false, "{name}: {answer:#}");
        let found = answer["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["at"] == at && f["problem"].as_str().unwrap().contains(said));
        assert!(found, "{name}: expected a finding at {at} saying {said:?}, got {:#}", answer["findings"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// The draft with `kinds` written as its kinds.json.
fn with_kinds(name: &str, kinds: &Value) -> PathBuf {
    let dir = planted(name, r#""screen": "ListingDetail""#, r#""screen": "ListingDetail""#);
    std::fs::write(dir.join("kinds.json"), kinds.to_string()).unwrap();
    dir
}

#[test]
fn underdraws_own_kinds_pass_as_a_projects() {
    let defaults: Value =
        serde_json::from_str(include_str!("../../format/kinds.json")).expect("the defaults are JSON");
    let dir = with_kinds("own-kinds", &defaults);
    let (answer, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!((code, &answer["findings"]), (0, &json!([])), "{answer:#}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn each_fault_in_a_projects_kinds_is_found() {
    let kinds = json!({ "kinds": {
        "NoLook": { "doc": "forgot its look" },
        "Wrong": { "look": { "box": "row", "colour": "line", "children": [
            { "text": "{title|upper}" },
            { "kind": "Nowhere" },
            { "cases": [ { "if": "title", "then": "$kids" } ] },
            { "kind": "Glyph", "icon": "chevron.right" }
        ] } }
    }, "icons": { "checklist": "☑" } });
    let dir = with_kinds("bad-kinds", &kinds);
    let (answer, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(code, 1, "{answer:#}");
    let expected = [
        ("kinds.NoLook", "needs \"look\""),
        ("kinds.Wrong.look", "colour is not a key"),
        ("kinds.Wrong.look.children[0].text", "upper is not a filter"),
        ("kinds.Wrong.look.children[1]", "kind Nowhere is not defined"),
        ("kinds.Wrong.look.children[2].cases", "must be {\"else\""),
        ("kinds.Wrong.look.children[2].cases[0].then", "$kids is not a look"),
        ("kinds.Wrong.look.children[3].icon", "chevron.right is not a Material Symbols icon"),
        ("icons", "icons are Material Symbols"),
    ];
    for (at, said) in expected {
        let found = answer["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["at"] == at && f["problem"].as_str().unwrap().contains(said));
        assert!(found, "expected a finding at {at} saying {said:?}, got {:#}", answer["findings"]);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_cannot_be_read_is_an_error_with_its_fix() {
    let (answer, code) = underdraw(&["check", "/nonexistent/screen"]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("pass a screen's directory"), "{answer}");

    let dir = std::env::temp_dir().join(format!("underdraw-{}-not-json", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("screen.json"), "{ nope").unwrap();
    let (answer, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("is not JSON"), "{answer}");
    assert!(answer["error"].as_str().unwrap().contains("line 1"), "{answer}");
    std::fs::remove_dir_all(dir).unwrap();

    let (answer, code) = underdraw(&["chek"]);
    assert_eq!(code, 2);
    assert_eq!(answer["fix"], "underdraw --help");
}

#[test]
fn doctor_and_version() {
    let (answer, code) = underdraw(&["doctor"]);
    assert_eq!((code, &answer["ok"], &answer["findings"]), (0, &json!(true), &json!([])), "{answer}");
    assert!(answer["gpu"]["name"].is_string(), "doctor names the GPU render uses: {answer}");
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).arg("--version").output().unwrap();
    let version = String::from_utf8(out.stdout).unwrap();
    assert!(version.starts_with(&format!("underdraw {}", env!("CARGO_PKG_VERSION"))), "{version}");
}

#[test]
fn help_names_exactly_the_commands_that_exist() {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).arg("--help").output().unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    // A command is named where a line starts with `underdraw <command>`.
    let mut named: Vec<String> = help
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("underdraw "))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|word| word.chars().all(|c| c.is_ascii_lowercase()))
        .map(str::to_string)
        .collect();
    named.sort();
    named.dedup();
    let mut exist = underdraw::commands();
    exist.sort();
    assert_eq!(named, exist);
}

#[test]
fn the_todo_draft_defines_its_own_text_field() {
    let todo = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/todo");
    let (answer, code) = underdraw(&["check", todo]);
    assert_eq!((code, &answer["findings"]), (0, &json!([])), "{answer:#}");
}
