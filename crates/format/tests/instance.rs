//! The Airbnb draft, instantiated in each state, says what the person would see.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Node, State, instantiate};

fn draft() -> Value {
    serde_json::from_str(include_str!("../../../drafts/airbnb-listing/screen.json")).unwrap()
}

fn find<'a>(node: &'a Node, id: &str) -> Option<&'a Node> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, id))
}

fn text(root: &Node, id: &str) -> Value {
    find(root, id).unwrap_or_else(|| panic!("no node {id}")).props["text"].clone()
}

#[test]
fn each_state_shows_what_it_should() {
    // (state, breakpoint, node, its text)
    let cases = [
        ("default", Breakpoint::Compact, "reserve-button", json!("预订")),
        ("default", Breakpoint::Compact, "bar-price-text", json!("¥104,300")),
        ("default", Breakpoint::Compact, "bar-dates", json!("11/12 – 11/17")),
        (
            "default",
            Breakpoint::Compact,
            "summary-capacity",
            json!("3 位房客 · 1 间卧室 · 2 张床 · 1 间卫生间"),
        ),
        ("no-dates", Breakpoint::Compact, "reserve-button", json!("查看可订日期")),
        ("no-dates", Breakpoint::Compact, "bar-price-text", json!("¥18,500 / 晚")),
        ("unavailable", Breakpoint::Compact, "reserve-button", json!("日期不可订")),
        ("unavailable", Breakpoint::Regular, "card-price", json!("所选日期不可订")),
        ("default", Breakpoint::Regular, "card-price", json!("¥104,300 共 5 晚")),
    ];
    let screen = draft();
    for (fixture, breakpoint, id, expected) in cases {
        let root = instantiate(&screen, State::Fixture(fixture), breakpoint).unwrap();
        assert_eq!(text(&root, id), expected, "{fixture} {breakpoint:?} {id}");
    }
}

#[test]
fn conditions_breakpoints_and_each_shape_the_tree() {
    let screen = draft();
    let compact = instantiate(&screen, State::Fixture("default"), Breakpoint::Compact).unwrap();
    let regular = instantiate(&screen, State::Fixture("default"), Breakpoint::Regular).unwrap();
    assert!(find(&compact, "booking-bar").is_some() && find(&compact, "booking-card").is_none());
    assert!(find(&regular, "booking-card").is_some() && find(&regular, "booking-bar").is_none());
    // limit 10 of 12 amenities, each an item with its index in its id
    let amenities = find(&compact, "amenities-list").unwrap();
    assert_eq!(amenities.children.len(), 10);
    assert_eq!(amenities.children[9].id, "amenity-9");
    // the photo carousel holds its photos as items
    assert_eq!(find(&compact, "photos-carousel").unwrap().props["items"].as_array().unwrap().len(), 7);
    // no reviews: the rating bar and the review section are gone, the new-listing line is there
    let new = instantiate(&screen, State::Fixture("new-listing"), Breakpoint::Compact).unwrap();
    assert!(find(&new, "rating-bar").is_none() && find(&new, "reviews").is_none());
    assert!(find(&new, "rating-new").is_some());
    // what the agent guessed travels with the node
    assert!(find(&compact, "calendar-picker").unwrap().assumed.is_some());
    assert!(find(&compact, "nav-save").unwrap().interactive);
}

#[test]
fn a_phase_is_its_own_tree() {
    let root = instantiate(&draft(), State::Phase("loading"), Breakpoint::Compact).unwrap();
    assert_eq!(root.id, "listing-detail-loading");
    assert!(find(&root, "loading-skeleton").is_some() && find(&root, "nav-back").is_some());
    assert!(instantiate(&draft(), State::Phase("nope"), Breakpoint::Compact).is_err());
    assert!(instantiate(&draft(), State::Fixture("nope"), Breakpoint::Compact).is_err());
}
