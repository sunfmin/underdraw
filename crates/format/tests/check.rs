//! `check` on the Airbnb draft, with one fault planted at a time: each is
//! found where it was planted, and says what is wrong.

use serde_json::{Value, json};
use underdraw_format::{Checked, Screen, Shared};

/// A frozen copy of `drafts/airbnb-listing`, kept beside the tests so that
/// designing in the live draft never moves a planted fault. The other crates'
/// tests on the Airbnb draft read this copy too.
const DRAFT: &str = include_str!("airbnb-listing/screen.json");

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
    let checked = check(&draft());
    assert!(checked.ok, "{:#?}", checked.findings);
}

#[test]
fn each_planted_fault_is_found_where_it_was_planted() {
    // (name, what the draft says, the fault planted in its place, where it is found, what is said)
    let cases = [
        (
            "assumed-too-short",
            r#""text": "房客推荐","#,
            r#""text": "房客推荐", "assumed": "待定","#,
            "guest-favorite.assumed",
            "what was guessed and what else it could be",
        ),
        (
            "assumed-a-word-or-two",
            r#""text": "房客推荐","#,
            r#""text": "房客推荐", "assumed": "maybe server","#,
            "guest-favorite.assumed",
            "too short for the person to answer",
        ),
        (
            "assumed-not-text",
            r#""text": "房客推荐","#,
            r#""text": "房客推荐", "assumed": true,"#,
            "guest-favorite.assumed",
            "is text",
        ),
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
            "comparisons and anything more in conditions",
        ),
        (
            "or-at-use-site",
            r#""text": "清除日期", "if": "hasDates""#,
            r#""text": "清除日期", "if": "hasDates || listing.isSaved""#,
            "calendar-clear.if",
            "name ||, comparisons and anything more in conditions",
        ),
        (
            "comparison-in-a-chain",
            r#""text": "清除日期", "if": "hasDates""#,
            r#""text": "清除日期", "if": "hasDates && booking.nights > 0""#,
            "calendar-clear.if",
            "name ||, comparisons and anything more in conditions",
        ),
        (
            "negated-guard-in-a-chain",
            r#""component": "Link", "if": "hasDates", "font": "font.caption""#,
            r#""component": "Link", "if": "booking.available && !hasDates", "font": "font.caption""#,
            "bar-dates.text",
            "may be null",
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
            r#""component": "Link", "if": "hasDates", "font": "font.caption""#,
            r#""component": "Link", "font": "font.caption""#,
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
            "kind-for-component",
            r#""component": "Badge", "if": "listing.isGuestFavorite""#,
            r#""kind": "Badge", "if": "listing.isGuestFavorite""#,
            "guest-favorite.kind",
            "write \"component\" in place of \"kind\"",
        ),
        (
            "component-not-defined",
            r#""component": "Badge", "if": "listing.isGuestFavorite""#,
            r#""component": "Bagde", "if": "listing.isGuestFavorite""#,
            "guest-favorite",
            "component Bagde is not defined",
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
            r#""leading":  [ { "id": "nav-back", "component": "NavBack" } ]"#,
            r#""leading": { "cases": [ { "if": "hasDates", "then": { "id": "nav-back", "component": "NavBack" } }, { "else": { "id": "nav-back", "component": "NavBack" } } ] }"#,
            "nav.leading",
            "a slot holds nodes, not cases",
        ),
        (
            "anchor-not-a-place",
            r#""component": "Badge", "if": "listing.isGuestFavorite""#,
            r#""component": "Badge", "anchor": "middle", "if": "listing.isGuestFavorite""#,
            "guest-favorite.anchor",
            "middle is not an anchor",
        ),
        (
            "anchor-per-breakpoint",
            r#""component": "Badge", "if": "listing.isGuestFavorite""#,
            r#""component": "Badge", "anchor": { "compact": "bottom", "regular": "middle" }, "if": "listing.isGuestFavorite""#,
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
            r#""component": "Badge", "if": "listing.isGuestFavorite""#,
            r#""component": "Badge", "only": "compcat", "if": "listing.isGuestFavorite""#,
            "guest-favorite.only",
            "compcat\" is not a breakpoint",
        ),
    ];
    for (name, from, to, at, said) in cases {
        let checked = check(&planted(name, from, to));
        assert!(!checked.ok, "{name}");
        assert!(
            found(&checked, at, said),
            "{name}: expected a finding at {at} saying {said:?}, got {:#?}",
            checked.findings
        );
    }
}

/// The draft with `own` among its own components, over any of the same name.
#[test]
fn a_use_site_takes_conditions_joined_by_and() {
    // hasDates in the chain makes the dates it tests != null readable under it.
    for chain in ["booking.available && hasDates", "!listing.isSaved && hasDates && booking.available"] {
        let screen = planted(
            chain,
            r#""component": "Link", "if": "hasDates", "font": "font.caption""#,
            &format!(r#""component": "Link", "if": "{chain}", "font": "font.caption""#),
        );
        let checked = check(&screen);
        assert!(checked.ok, "{chain}: {:#?}", checked.findings);
    }
}

fn with_own(own: Value) -> Value {
    let mut draft = draft();
    for (name, component) in own.as_object().unwrap() {
        draft["components"][name] = component.clone();
    }
    draft
}

#[test]
fn underdraws_own_components_pass_as_a_screens() {
    let builtins: Value =
        serde_json::from_str(include_str!("../../format/components.json")).expect("the built-ins are JSON");
    let checked = check(&with_own(builtins["components"].clone()));
    assert!(checked.findings.is_empty(), "{:#?}", checked.findings);
}

#[test]
fn a_component_has_a_look_or_nodes_never_both() {
    let checked = check(&with_own(json!({
        "Both": { "params": [], "look": { "box": "row" }, "nodes": { "component": "Row" } },
        "Neither": { "params": [], "doc": "forgot its body" },
        "Look": { "params": [], "look": { "box": "row" } },
        "Nodes": { "nodes": { "component": "Row" } }
    })));
    assert!(found(&checked, "components.Both", "a look or nodes, never both"), "{:#?}", checked.findings);
    assert!(found(&checked, "components.Neither", "needs a body"), "{:#?}", checked.findings);
    for fine in ["components.Look", "components.Nodes"] {
        assert!(!checked.findings.iter().any(|f| f.at.starts_with(fine)), "{fine}: {:#?}", checked.findings);
    }
}

#[test]
fn a_look_reads_only_the_params_its_component_declares() {
    let checked = check(&with_own(json!({
        "Tag": { "params": ["text"], "look": { "box": "row", "children": [
            { "text": "{text} {count|or:0}", "ink": "{tone}" },
            { "if": "selected && kind != \"x\"", "text": "{label.short}" },
            { "repeat": "{text|count}", "each": { "text": "{n}. {item} {index}" } },
            { "component": "Badge", "text": "{badge}" }
        ] } },
        "Framed": { "params": ["gap"], "look": { "box": "column", "frame": true, "children": ["$children"] } },
        "Unlisted": { "params": "text", "look": { "text": "{text}" } }
    })));
    for read in ["count", "tone", "selected", "kind", "label", "badge"] {
        let said = format!("reads {read}, which is not one of its params");
        assert!(found(&checked, "components.Tag.look", &said), "{read}: {:#?}", checked.findings);
    }
    for bound in ["text", "item", "index", "n", "x", "Badge"] {
        let said = format!("reads {bound}, which");
        assert!(!found(&checked, "components.Tag.look", &said), "{bound} passes: {:#?}", checked.findings);
    }
    assert!(found(&checked, "components.Framed.look", "reads padding, which"), "{:#?}", checked.findings);
    assert!(!found(&checked, "components.Framed.look", "reads gap, which"), "{:#?}", checked.findings);
    assert!(found(&checked, "components.Unlisted.params", "params is a list"), "{:#?}", checked.findings);
}

#[test]
fn each_fault_in_a_screens_components_is_found() {
    let own = with_own(json!({
        "Wrong": { "params": ["title", "width", "edge"], "assumed": "looks like a Badge for now", "look": { "box": "row", "colour": "line", "children": [
            { "text": "{title|upper}" },
            { "component": "Nowhere" },
            { "cases": [ { "if": "title", "then": "$kids" } ] },
            { "component": "Glyph", "icon": "chevron.right" },
            { "box": "grid", "columns": ["2fr", "{width}", 48, "48"], "rows": ["1.5fr", "wide"] },
            { "box": "column", "backdrop": true },
            { "box": "column", "backdrop": true, "pin": { "x": "center", "y": "center" } },
            { "box": "stack", "justify": { "cases": [ { "if": "title", "then": "end" }, { "else": "left" } ] } },
            { "text": "{title}", "underline": "yes", "pin": { "x": "right", "y": "{edge}", "z": 1 } },
            { "text": "{title}", "if": "title = \"\"" },
            { "cases": [ { "if": "title|loud", "then": "{title}" }, { "else": null } ] }
        ] }, "pinned": "yes" }
    }));
    let mut own = own;
    own["icons"] = json!({ "checklist": "☑" });
    let checked = check(&own);
    let expected = [
        ("components.Wrong.look", "colour is not a key"),
        ("components.Wrong.look.children[0].text", "upper is not a filter"),
        ("components.Wrong.look.children[1]", "component Nowhere is not defined"),
        ("components.Wrong.look.children[2].cases", "must be {\"else\""),
        ("components.Wrong.look.children[2].cases[0].then", "$kids is not a look"),
        ("components.Wrong.look.children[3].icon", "chevron.right is not a Material Symbols icon"),
        ("components.Wrong.look.children[4].columns[3]", "\"48\" is not a grid track"),
        ("components.Wrong.look.children[4].rows[1]", "\"wide\" is not a grid track"),
        ("components.Wrong.look.children[5]", "backdrop is drawn only by a box that floats"),
        ("components.Wrong.assumed", "underdraw draws no guesses"),
        ("components.Wrong.pinned", "pinned is true or false"),
        ("components.Wrong.look.children[7].box", "\"stack\" is not a box: write row, column, grid"),
        ("components.Wrong.look.children[7].justify.cases[1].else", "\"left\" is not a justify"),
        ("components.Wrong.look.children[8].underline", "underline is true or false"),
        ("components.Wrong.look.children[8].pin.x", "\"right\" is not an edge: write start, center, end"),
        ("components.Wrong.look.children[8].pin.z", "a pin holds x, y and inset"),
        ("components.Wrong.look.children[9].if", "title = \"\" is not a condition: equality is written =="),
        ("components.Wrong.look.children[10].cases[0].if", "loud is not a filter"),
        ("icons", "icons are Material Symbols"),
    ];
    for (at, said) in expected {
        assert!(
            found(&checked, at, said),
            "expected a finding at {at} saying {said:?}, got {:#?}",
            checked.findings
        );
    }
    let pinned = checked.findings.iter().any(|f| f.at == "components.Wrong.look.children[6]");
    assert!(!pinned, "a pinned box may have a backdrop: {:#?}", checked.findings);
    let decided_later = [
        "components.Wrong.look.children[7].justify.cases[0].then",
        "components.Wrong.look.children[8].pin.y",
    ];
    for at in decided_later {
        assert!(!checked.findings.iter().any(|f| f.at == at), "{at} passes: {:#?}", checked.findings);
    }
}

#[test]
fn a_component_with_a_backdrop_must_be_an_overlay() {
    let checked = check(&with_own(json!({ "Badge": { "params": ["text"], "look": {
        "box": "row", "backdrop": true, "children": [ { "text": "{text}" } ] } } })));
    assert!(found(&checked, "guest-favorite", "put it in \"overlay\""), "{:#?}", checked.findings);
}

#[test]
fn a_node_in_a_slot_a_look_never_draws_is_found() {
    let slotted = r#""text": "房客推荐", "overlay": { "id": "dot", "component": "Badge", "text": "1" },
        "trailing": [ { "id": "a", "component": "Icon", "icon": "star" }, { "id": "b", "component": "Icon", "icon": "star" } ],"#;
    let checked = check(&planted("slotted", r#""text": "房客推荐","#, slotted));
    assert!(
        found(&checked, "guest-favorite.overlay", "Badge's look places no \"$children\", so dot in overlay"),
        "{:#?}",
        checked.findings
    );
    assert!(found(&checked, "guest-favorite.trailing", "so a, b in trailing"), "{:#?}", checked.findings);

    // A look that places "$children", even in one branch of its cases, draws them.
    let mut placed: Value = serde_json::from_str(&DRAFT.replace(r#""text": "房客推荐","#, slotted)).unwrap();
    placed["components"]["Badge"] = json!({ "params": ["text"], "look": { "box": "row", "children": [
        { "text": "{text}" }, { "cases": [ { "if": "text", "then": "$children" }, { "else": null } ] } ] } });
    let checked = check(&placed);
    assert!(!checked.findings.iter().any(|f| f.at.starts_with("guest-favorite.")), "{:#?}", checked.findings);
}

#[test]
fn a_component_says_whether_it_scrolls_or_is_pinned() {
    let defaults = read(&json!({})).components;
    assert!(defaults.scrolls("Carousel") && defaults.pinned("StickyBar"));
    assert!(!defaults.scrolls("Row") && !defaults.pinned("Row"));
    let screen = json!({ "components": {
        "Feed": { "scrolls": true, "look": "$children" },
        "StickyBar": { "look": "$children" } } });
    let components = read(&screen).components;
    assert!(components.scrolls("Feed"), "a screen's own component scrolls when it says so");
    assert!(!components.pinned("StickyBar"), "a screen's own component replaces underdraw's whole");
}

/// `description` read as a screen of its own, with nothing shared.
fn read(description: &Value) -> Screen {
    Screen::read(description, Shared::default())
}

fn check(description: &Value) -> Checked {
    underdraw_format::check(&read(description))
}

#[test]
fn a_cases_branch_whose_if_is_not_a_condition_is_found_at_that_branch() {
    // (name, what the draft says, the "if" planted in its place, where it is found, what it is called)
    let planted_ifs = [
        (
            "nested-cases-in-a-prop",
            r#"{ "if": "listing.isSaved", "then": "favorite.fill" }"#,
            r#"{ "if": { "cases": [ { "if": "listing.isSaved", "then": true }, { "else": false } ] }, "then": "favorite.fill" }"#,
            "nav-save.icon.cases[0].if",
            "a nested cases",
        ),
        (
            "object-in-a-prop",
            r#"{ "if": "listing.location.approx", "then": "circle" }"#,
            r#"{ "if": { "listing.location.approx": true }, "then": "circle" }"#,
            "map-view.marker.cases[0].if",
            "an object",
        ),
        (
            "number-in-a-handler",
            r#"{ "if": "!hasDates", "then": { "scrollTo": "calendar" } }"#,
            r#"{ "if": 1, "then": { "scrollTo": "calendar" } }"#,
            "reserve-button.on.tap.cases[0].if",
            "a number",
        ),
    ];
    for (name, from, to, at, what) in planted_ifs {
        let checked = check(&planted(name, from, to));
        assert!(!checked.ok, "{name}");
        let said = format!("{what} is not a condition: an \"if\" takes a condition");
        assert!(
            found(&checked, at, &said),
            "{name}: expected a finding at {at} saying {said:?}, got {:#?}",
            checked.findings
        );
    }
}

#[test]
fn a_looks_cases_branch_whose_if_is_not_a_condition_is_found_at_that_branch() {
    let checked = check(&with_own(json!({
        "Toolbar": { "params": ["title", "wide"], "look": { "box": "row", "children": [
            { "text": "{title}", "underline": { "cases": [ { "if": { "cases": [ { "if": "wide", "then": true }, { "else": false } ] }, "then": false }, { "else": true } ] } },
            { "component": "Glyph", "icon": "star", "filled": { "cases": [ { "if": 3, "then": true }, { "else": false } ] } },
            { "cases": [ { "if": { "wide": true }, "then": "{title}" }, { "else": null } ] },
            { "text": "{title}", "underline": { "cases": [ { "if": "wide && !title", "then": true }, { "else": false } ] } }
        ] } }
    })));
    assert!(!checked.ok);
    let expected = [
        ("components.Toolbar.look.children[0].underline.cases[0].if", "a nested cases"),
        ("components.Toolbar.look.children[1].filled.cases[0].if", "a number"),
        ("components.Toolbar.look.children[2].cases[0].if", "an object"),
    ];
    for (at, what) in expected {
        let said = format!("{what} is not a condition: an \"if\" takes a condition");
        assert!(found(&checked, at, &said), "expected {said:?} at {at}, got {:#?}", checked.findings);
    }
    let well_formed =
        checked.findings.iter().any(|f| f.at.starts_with("components.Toolbar.look.children[3]"));
    assert!(!well_formed, "a name, ! and && pass: {:#?}", checked.findings);
}
