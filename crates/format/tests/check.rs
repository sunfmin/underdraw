//! `check` on the Airbnb draft, with one fault planted at a time: each is
//! found where it was planted, and says what is wrong.

use serde_json::{Value, json};
use underdraw_format::{Checked, check};

const DRAFT: &str = include_str!("../../../drafts/airbnb-listing/screen.json");

fn draft() -> Value {
    serde_json::from_str(DRAFT).unwrap()
}

/// The draft with `from` replaced by `to`, where `from` appears exactly once.
fn planted(name: &str, from: &str, to: &str) -> Value {
    assert_eq!(DRAFT.matches(from).count(), 1, "{name}: {from:?} must appear exactly once in the draft");
    serde_json::from_str(&DRAFT.replace(from, to)).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"))
}

/// Whether `checked` has a finding at `at` whose problem says `said`.
fn found(checked: &Checked, at: &str, said: &str) -> bool {
    checked.findings.iter().any(|f| f.at == at && f.problem.contains(said))
}

#[test]
fn the_draft_is_sound() {
    let checked = check(&draft(), None);
    assert!(checked.ok, "{:#?}", checked.findings);
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
            "names every breakpoint",
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
        (
            "cases-in-a-slot",
            r#""leading":  [ { "ref": "nav-back" } ]"#,
            r#""leading": { "cases": [ { "if": "hasDates", "then": { "ref": "nav-back" } }, { "else": { "ref": "nav-back" } } ] }"#,
            "nav.leading",
            "a slot holds nodes, not cases",
        ),
        (
            "anchor-not-a-place",
            r#""kind": "Badge", "if": "listing.isGuestFavorite""#,
            r#""kind": "Badge", "anchor": "middle", "if": "listing.isGuestFavorite""#,
            "guest-favorite.anchor",
            "middle is not an anchor",
        ),
        (
            "anchor-per-breakpoint",
            r#""kind": "Badge", "if": "listing.isGuestFavorite""#,
            r#""kind": "Badge", "anchor": { "compact": "bottom", "regular": "middle" }, "if": "listing.isGuestFavorite""#,
            "guest-favorite.anchor.regular",
            "middle is not an anchor",
        ),
        (
            "quoted-track",
            r#""regular": ["58%", "1fr"]"#,
            r#""regular": ["58%", "48"]"#,
            "body.columns.regular[1]",
            "\"48\" is not a grid track",
        ),
        (
            "only-not-a-breakpoint",
            r#""kind": "Badge", "if": "listing.isGuestFavorite""#,
            r#""kind": "Badge", "only": "compcat", "if": "listing.isGuestFavorite""#,
            "guest-favorite.only",
            "compcat\" is not a breakpoint",
        ),
    ];
    for (name, from, to, at, said) in cases {
        let checked = check(&planted(name, from, to), None);
        assert!(!checked.ok, "{name}");
        assert!(
            found(&checked, at, said),
            "{name}: expected a finding at {at} saying {said:?}, got {:#?}",
            checked.findings
        );
    }
}

#[test]
fn underdraws_own_kinds_pass_as_a_projects() {
    let defaults: Value =
        serde_json::from_str(include_str!("../../format/kinds.json")).expect("the defaults are JSON");
    let checked = check(&draft(), Some(&defaults));
    assert!(checked.findings.is_empty(), "{:#?}", checked.findings);
}

#[test]
fn each_fault_in_a_projects_kinds_is_found() {
    let kinds = json!({ "kinds": {
        "NoLook": { "doc": "forgot its look" },
        "Wrong": { "assumed": "looks like a Badge for now", "look": { "box": "row", "colour": "line", "children": [
            { "text": "{title|upper}" },
            { "kind": "Nowhere" },
            { "cases": [ { "if": "title", "then": "$kids" } ] },
            { "kind": "Glyph", "icon": "chevron.right" },
            { "box": "grid", "columns": ["2fr", "{width}", 48, "48"], "rows": ["1.5fr", "wide"] },
            { "box": "column", "backdrop": true },
            { "box": "column", "backdrop": true, "pin": { "x": "center", "y": "center" } }
        ] } }
    }, "icons": { "checklist": "☑" } });
    let checked = check(&draft(), Some(&kinds));
    let expected = [
        ("kinds.NoLook", "needs \"look\""),
        ("kinds.Wrong.look", "colour is not a key"),
        ("kinds.Wrong.look.children[0].text", "upper is not a filter"),
        ("kinds.Wrong.look.children[1]", "kind Nowhere is not defined"),
        ("kinds.Wrong.look.children[2].cases", "must be {\"else\""),
        ("kinds.Wrong.look.children[2].cases[0].then", "$kids is not a look"),
        ("kinds.Wrong.look.children[3].icon", "chevron.right is not a Material Symbols icon"),
        ("kinds.Wrong.look.children[4].columns[3]", "\"48\" is not a grid track"),
        ("kinds.Wrong.look.children[4].rows[1]", "\"wide\" is not a grid track"),
        ("kinds.Wrong.look.children[5]", "backdrop is drawn only by a box that floats"),
        ("kinds.Wrong.assumed", "underdraw draws no guesses"),
        ("icons", "icons are Material Symbols"),
    ];
    for (at, said) in expected {
        assert!(
            found(&checked, at, said),
            "expected a finding at {at} saying {said:?}, got {:#?}",
            checked.findings
        );
    }
    let pinned = checked.findings.iter().any(|f| f.at == "kinds.Wrong.look.children[6]");
    assert!(!pinned, "a pinned box may have a backdrop: {:#?}", checked.findings);
}

#[test]
fn a_kind_with_a_backdrop_must_be_an_overlay() {
    let kinds = json!({ "kinds": { "Badge": { "look": {
        "box": "row", "backdrop": true, "children": [ { "text": "{text}" } ] } } } });
    let checked = check(&draft(), Some(&kinds));
    assert!(found(&checked, "guest-favorite", "put it in \"overlay\""), "{:#?}", checked.findings);
}
