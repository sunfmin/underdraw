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
    // a limit may differ per breakpoint: three full rows of three on a desktop
    let regular = instantiate(&screen, State::Fixture("default"), Breakpoint::Regular).unwrap();
    assert_eq!(find(&regular, "amenities-list").unwrap().children.len(), 9);
    // the photo carousel holds its photos as items
    assert_eq!(find(&compact, "photos-carousel").unwrap().props["items"].as_array().unwrap().len(), 7);
    // no reviews: the rating bar and the review section are gone, the new-listing line is there
    let new = instantiate(&screen, State::Fixture("new-listing"), Breakpoint::Compact).unwrap();
    assert!(find(&new, "rating-bar").is_none() && find(&new, "reviews").is_none());
    assert!(find(&new, "rating-new").is_some());
    // what the person decided travels with the node, for the platforms to build
    let calendar = find(&compact, "calendar-picker").unwrap();
    assert_eq!(
        (calendar.props["bookedDates"].as_str(), &calendar.assumed),
        (Some("灰色且不可点，不画删除线"), &None)
    );
    assert!(find(&compact, "nav-save").unwrap().interactive);
    // a use site is decided, a binding shows its value
    let unavailable = instantiate(&screen, State::Fixture("unavailable"), Breakpoint::Regular).unwrap();
    assert_eq!(find(&unavailable, "reserve-button").unwrap().props["disabled"], json!(true));
    assert_eq!(find(&compact, "reserve-button").unwrap().props["disabled"], json!(false));
    assert_eq!(find(&regular, "card-guests").unwrap().props["value"], json!(2));
    assert_eq!(
        find(&compact, "calendar-picker").unwrap().props["value"],
        json!(["2026-11-12", "2026-11-17"])
    );
}

#[test]
fn a_phase_is_its_own_tree() {
    let root = instantiate(&draft(), State::Phase("loading"), Breakpoint::Compact).unwrap();
    assert_eq!(root.id, "listing-detail-loading");
    assert!(find(&root, "loading-skeleton").is_some() && find(&root, "nav-back").is_some());
    assert!(instantiate(&draft(), State::Phase("nope"), Breakpoint::Compact).is_err());
    assert!(instantiate(&draft(), State::Fixture("nope"), Breakpoint::Compact).is_err());
}

#[test]
fn a_model_shows_each_fixture_then_each_phase_as_instantiate_does() {
    let screen = draft();
    let model = underdraw_format::model(&screen, None).unwrap_or_else(|c| panic!("{:#?}", c.findings));
    let names: Vec<String> = model.states().iter().map(|state| format!("{state:?}")).collect();
    assert_eq!(
        names,
        [
            "Fixture(\"default\")",
            "Fixture(\"no-dates\")",
            "Fixture(\"unavailable\")",
            "Fixture(\"new-listing\")",
            "Phase(\"loading\")",
            "Phase(\"error\")"
        ]
    );
    for state in model.states() {
        for breakpoint in Breakpoint::ALL {
            assert_eq!(model.instantiate(state, breakpoint), instantiate(&screen, state, breakpoint));
        }
    }
}

/// Every node and each node in it, the node first.
fn every(node: &Node) -> Vec<&Node> {
    std::iter::once(node).chain(node.children.iter().flat_map(every)).collect()
}

#[test]
fn each_node_says_where_it_is_written_and_for_which_element() {
    let screen = draft();
    let mut states: Vec<State> = vec![State::Fixture("default"), State::Fixture("no-dates")];
    states.extend(
        screen["phases"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|name| !name.starts_with('$'))
            .map(|name| State::Phase(name)),
    );
    for state in states {
        let root = instantiate(&screen, state, Breakpoint::Compact).unwrap();
        for node in every(&root) {
            let written = screen.pointer(&node.written_at);
            let written =
                written.unwrap_or_else(|| panic!("{} names nothing, for {}", node.written_at, node.id));
            assert_eq!(written["id"], json!(node.written_id), "{state:?} {}", node.written_at);
        }
    }
    let root = instantiate(&screen, State::Fixture("default"), Breakpoint::Compact).unwrap();
    let amenity = find(&root, "amenity-9").unwrap();
    assert_eq!(amenity.written_id, "amenity-{index}", "the template, not the id it is given");
    assert_eq!(amenity.instance, [9], "the tenth amenity");
    assert!(amenity.written_at.ends_with("/item"), "{}", amenity.written_at);
    assert_eq!(find(&root, "amenities-list").unwrap().instance, Vec::<usize>::new(), "outside any each");
}
