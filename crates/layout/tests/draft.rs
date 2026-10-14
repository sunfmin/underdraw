//! Lays the Airbnb listing draft out with text measured by a stand-in: every
//! character is half the font size wide, so the geometry is exact.

use serde_json::Value;
use underdraw_format::{Breakpoint, State, instantiate};
use underdraw_layout::{Draw, Font, Measure, Page, Placed, Size, layout};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/airbnb-listing");

struct Fixed;

impl Measure for Fixed {
    fn text(&mut self, text: &str, font: Font, max_width: Option<f32>) -> Size {
        let advance = font.size / 2.0;
        let one_line = text.chars().count() as f32 * advance;
        let width = match max_width {
            Some(max) => one_line.min(max.max(advance)),
            None => one_line,
        };
        let lines = if width > 0.0 { (one_line / width).ceil().max(1.0) } else { 1.0 };
        Size { width, height: lines * font.line_height }
    }
}

fn read(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{DRAFT}/{name}")).unwrap()).unwrap()
}

fn page(state: State, width: f32) -> Page {
    let (screen, tokens) = (read("screen.json"), read("tokens.json"));
    let breakpoint = Breakpoint::for_width(&tokens, width as u32);
    let root = instantiate(&screen, state, breakpoint).unwrap();
    layout(&root, &tokens, None, width, &mut Fixed).unwrap()
}

fn placed<'a>(page: &'a Page, id: &str) -> &'a Placed {
    page.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("{id} is not placed"))
}

#[test]
fn a_phone_is_one_column_with_the_bar_at_the_end() {
    let page = page(State::Fixture("default"), 390.0);
    assert_eq!(page.width, 390.0);
    let photos = placed(&page, "photos-carousel");
    assert_eq!((photos.rect.x, photos.rect.width), (0.0, 390.0));
    assert_eq!(photos.rect.height, (390.0f32 * 3.0 / 4.0).round(), "4:3");
    let bar = placed(&page, "booking-bar");
    assert_eq!(bar.rect.y + bar.rect.height, page.height, "the bar closes the page");
    assert!(page.nodes.iter().all(|n| n.id != "booking-card"), "the card is regular only");
    for node in &page.nodes {
        let r = node.rect;
        assert!(
            r.x >= -0.5 && r.x + r.width <= 390.5 || inside_carousel(&page, node),
            "{} spills: {r:?}",
            node.id
        );
    }
}

/// Carousel items run past the edge on purpose; they are cut there.
fn inside_carousel(page: &Page, node: &Placed) -> bool {
    page.nodes
        .iter()
        .any(|c| c.kind == "Carousel" && c.rect.y <= node.rect.y && node.rect.y < c.rect.y + c.rect.height)
}

#[test]
fn a_desktop_puts_the_card_beside_the_content() {
    let page = page(State::Fixture("default"), 1120.0);
    let card = placed(&page, "booking-card");
    let photos = placed(&page, "photos-grid");
    assert!(card.rect.x > 1120.0 / 2.0, "card sits in the right column: {:?}", card.rect);
    assert!(card.rect.y > photos.rect.y + photos.rect.height, "below the photos");
    assert!(page.nodes.iter().all(|n| n.id != "booking-bar"));
}

#[test]
fn what_a_person_can_act_on_is_numbered_in_order() {
    let page = page(State::Fixture("default"), 390.0);
    let labels: Vec<u32> = page.nodes.iter().filter_map(|n| n.label).collect();
    assert!(labels.len() > 5, "{labels:?}");
    assert_eq!(labels, (1..=labels.len() as u32).collect::<Vec<_>>());
    assert!(placed(&page, "reserve-button").label.is_some());
    let tags = page.draws.iter().filter(|d| matches!(d, Draw::Tag { .. })).count();
    assert!(tags >= labels.len());
}

#[test]
fn a_disabled_reserve_button_is_drawn_gray() {
    let page = page(State::Fixture("unavailable"), 390.0);
    let button = placed(&page, "reserve-button").rect;
    let filled = page.draws.iter().any(|d| match d {
        Draw::Box { rect, fill, .. } => *rect == button && *fill == Some(underdraw_layout::Ink::Fill),
        _ => false,
    });
    assert!(filled, "disabled reads as a gray fill");
}

#[test]
fn the_loading_phase_lays_out_its_skeleton() {
    let page = page(State::Phase("loading"), 390.0);
    assert!(page.height > 300.0, "{}", page.height);
    let fills = page
        .draws
        .iter()
        .filter(|d| matches!(d, Draw::Box { fill: Some(underdraw_layout::Ink::Fill), .. }))
        .count();
    assert!(fills >= 5, "{fills}");
}

#[test]
fn every_kind_in_the_draft_is_in_the_catalog() {
    let page = page(State::Fixture("default"), 1120.0);
    let unknown: Vec<_> = page
        .draws
        .iter()
        .filter_map(|d| match d {
            Draw::Box { dashed: true, stroke: Some(underdraw_layout::Ink::Muted), rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert!(unknown.is_empty(), "kinds drawn as unknown at {unknown:?}");
}

#[test]
fn overlays_sit_in_their_corner() {
    let page = page(State::Fixture("default"), 390.0);
    let photos = placed(&page, "photos-carousel").rect;
    let counter = page.draws.iter().find_map(|d| match d {
        Draw::Text { text, rect, .. } if text.contains(" / ") => Some(*rect),
        _ => None,
    });
    let counter = counter.expect("the photo counter is drawn");
    assert!(
        counter.y > photos.y + photos.height / 2.0 && counter.y + counter.height < photos.y + photos.height
    );
    assert!(counter.x > photos.x + photos.width / 2.0);
}

/// A screen of `children` with no data, laid out at a phone's width.
fn tiny(children: Value) -> Page {
    tiny_with(children, None)
}

/// The same, with `kinds` as the project's kinds.json.
fn tiny_with(children: Value, kinds: Option<&Value>) -> Page {
    let screen = serde_json::json!({
        "screen": "Tiny", "data": {}, "types": {}, "fixtures": { "default": {} }, "conditions": {},
        "root": { "id": "tiny", "kind": "Screen", "children": children }
    });
    let tokens = read("tokens.json");
    let root = instantiate(&screen, State::Fixture("default"), Breakpoint::for_width(&tokens, 390)).unwrap();
    layout(&root, &tokens, kinds, 390.0, &mut Fixed).unwrap()
}

fn texts(page: &Page) -> Vec<&str> {
    page.draws
        .iter()
        .filter_map(|d| match d {
            Draw::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_filled_icon_reads_apart_from_its_outline() {
    let page = tiny(serde_json::json!([
        { "id": "open", "kind": "ListItem", "leadingIcon": "circle", "title": "a" },
        { "id": "done", "kind": "ListItem", "leadingIcon": "check_circle.fill", "title": "b" }
    ]));
    let texts = texts(&page);
    // Material Symbols' circle (U+EF4A) outlined, and its check_circle
    // (U+F0BE) solid, moved to U+F0000 + it where the solid font has it.
    assert!(texts.contains(&"\u{ef4a}") && texts.contains(&"\u{ff0be}"), "{texts:?}");
}

#[test]
fn a_kind_not_in_the_catalog_is_drawn_as_a_guess() {
    let page = tiny(serde_json::json!([{ "id": "input", "kind": "TextField" }]));
    let assumed = placed(&page, "input").assumed.as_deref().unwrap_or_default();
    assert!(assumed.contains("TextField"), "{assumed:?}");
    let marked = page
        .draws
        .iter()
        .any(|d| matches!(d, Draw::Box { dashed: true, stroke: Some(underdraw_layout::Ink::Assumed), .. }));
    assert!(marked, "drawn in the colour of a guess");
}

#[test]
fn a_projects_kind_is_drawn_from_its_look() {
    let kinds = serde_json::json!({ "kinds": { "TextField": { "look": {
        "box": "row", "grow": 1, "padding": [10, 12], "stroke": "line",
        "children": [ { "text": "{placeholder}", "ink": "muted" } ] } } } });
    let row = serde_json::json!([{ "id": "composer", "kind": "Row", "gap": 12, "children": [
        { "id": "input", "kind": "TextField", "placeholder": "添加一项待办" },
        { "id": "add", "kind": "Button", "text": "添加" } ] }]);
    let page = tiny_with(row, Some(&kinds));
    let (input, add) = (placed(&page, "input"), placed(&page, "add"));
    assert_eq!(input.assumed, None, "a defined kind is no guess");
    assert_eq!(input.rect.x + input.rect.width + 12.0, add.rect.x, "the field fills up to the button");
    assert_eq!(add.rect.x + add.rect.width, 390.0, "the button ends the row");
    assert!(texts(&page).contains(&"添加一项待办"), "{:?}", texts(&page));
}

#[test]
fn a_kind_still_assumed_is_drawn_as_a_guess() {
    let kinds = serde_json::json!({ "kinds": { "Chip": {
        "assumed": "drawn like a Badge until the person says otherwise",
        "look": { "text": "{text}" } } } });
    let page = tiny_with(serde_json::json!([{ "id": "chip", "kind": "Chip", "text": "新" }]), Some(&kinds));
    let assumed = placed(&page, "chip").assumed.as_deref().unwrap_or_default();
    assert!(assumed.contains("until the person says otherwise"), "{assumed:?}");
}
