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
fn an_icon_is_as_big_as_its_square() {
    let page = tiny(serde_json::json!([
        { "id": "row", "kind": "ListItem", "leadingIcon": "circle", "title": "a" },
        { "id": "big", "kind": "Glyph", "icon": "flight", "size": 48 }
    ]));
    let size = |glyph: &str| {
        page.draws.iter().find_map(|d| match d {
            Draw::Text { text, font, .. } if text == glyph => Some(font.size),
            _ => None,
        })
    };
    // Material Symbols' circle (U+EF4A) in ListItem's 24 square, and flight
    // (U+E539) in a 48 one.
    assert_eq!((size("\u{ef4a}"), size("\u{e539}")), (Some(24.0), Some(48.0)));
}

#[test]
fn a_nodes_padding_takes_sides_as_a_looks_does() {
    let page = tiny(serde_json::json!([
        { "id": "body", "kind": "Stack", "padding": [0, "space.4"], "children": [
            { "id": "line", "kind": "Text", "text": "a" } ] }
    ]));
    let (body, line) = (placed(&page, "body").rect, placed(&page, "line").rect);
    assert_eq!((line.x, line.width, line.y), (16.0, 390.0 - 32.0, body.y), "16 either side, none above");
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
fn a_grid_track_is_a_share_a_part_or_points() {
    let page = tiny(serde_json::json!([
        { "id": "grid", "kind": "Grid", "gap": 0, "columns": ["1fr", 48, "50%"], "children": [
            { "id": "share", "kind": "Text", "text": "a" },
            { "id": "points", "kind": "Text", "text": "b" },
            { "id": "part", "kind": "Text", "text": "c" } ] }
    ]));
    let width = |id| placed(&page, id).rect.width;
    assert_eq!((width("share"), width("points"), width("part")), (390.0 - 48.0 - 195.0, 48.0, 195.0));
}

#[test]
fn an_overlay_anchored_at_the_center_sits_in_the_middle() {
    let page = tiny(serde_json::json!([
        { "id": "body", "kind": "Stack", "children": [
            { "id": "a", "kind": "Text", "text": "a" }, { "id": "b", "kind": "Text", "text": "b" },
            { "id": "c", "kind": "Text", "text": "c" }, { "id": "d", "kind": "Text", "text": "d" } ],
          "overlay": { "id": "dialog", "kind": "Card", "anchor": "center", "children": [
            { "id": "dialog-text", "kind": "Text", "text": "?" } ] } }
    ]));
    let (body, dialog) = (placed(&page, "body").rect, placed(&page, "dialog").rect);
    assert_eq!(dialog.x - body.x, body.x + body.width - (dialog.x + dialog.width), "as far from either side");
    assert_eq!(
        dialog.y - body.y,
        body.y + body.height - (dialog.y + dialog.height),
        "as far from top and bottom"
    );
}

#[test]
fn a_dialog_hides_the_tags_of_what_it_covers() {
    let kinds = serde_json::json!({ "kinds": { "Dialog": { "look": {
        "box": "column", "width": "80%", "padding": 16, "fill": "paper", "stroke": "line",
        "children": ["$children"] } } } });
    let button = |id: &str| serde_json::json!({ "id": id, "kind": "Button", "text": id, "on": { "tap": { "go": id } } });
    let buttons: Vec<Value> = ["a", "b", "c", "d", "e"].map(button).into();
    let page = tiny_with(
        serde_json::json!([{ "id": "body", "kind": "Stack", "gap": 8,
            "children": buttons,
            "overlay": { "id": "dialog", "kind": "Dialog", "anchor": "center", "children": [button("ok")] } }]),
        Some(&kinds),
    );
    let dialog = placed(&page, "dialog").rect;
    let under = |id| {
        let rect = placed(&page, id).rect;
        let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        x >= dialog.x && x <= dialog.x + dialog.width && y >= dialog.y && y <= dialog.y + dialog.height
    };
    let labels: Vec<_> = ["a", "b", "c", "d", "e", "ok"].map(|id| (id, placed(&page, id).label)).into();
    assert!(under("c") && !under("a") && !under("e"), "the dialog covers the middle: {labels:?}");
    let mut next = 1;
    for (id, label) in &labels {
        if *id != "ok" && under(id) {
            assert_eq!(*label, None, "{id} is under the dialog: {labels:?}");
        } else {
            assert_eq!(*label, Some(next), "{id} is numbered in order, skipping what is covered: {labels:?}");
            next += 1;
        }
    }
    let tags = page.draws.iter().filter(|d| matches!(d, Draw::Tag { .. })).count();
    assert_eq!(tags, next as usize - 1, "a tag for each number, none for what is covered");
}

#[test]
fn a_backdrop_dims_what_the_dialog_floats_over_and_hides_its_tags() {
    let kinds = serde_json::json!({ "kinds": { "Dialog": { "look": {
        "box": "column", "backdrop": true, "width": "80%", "padding": 16, "fill": "paper",
        "children": ["$children"] } } } });
    let button = |id: &str| serde_json::json!({ "id": id, "kind": "Button", "text": id, "on": { "tap": { "go": id } } });
    let buttons: Vec<Value> = ["a", "b", "c", "d", "e"].map(button).into();
    let page = tiny_with(
        serde_json::json!([{ "id": "body", "kind": "Stack", "gap": 8, "children": buttons,
            "overlay": { "id": "dialog", "kind": "Dialog", "anchor": "center", "children": [button("ok")] } }]),
        Some(&kinds),
    );
    let body = placed(&page, "body").rect;
    let scrim = page.draws.iter().position(
        |d| matches!(d, Draw::Box { rect, fill: Some(underdraw_layout::Ink::Scrim), .. } if *rect == body),
    );
    let dialog = page.draws.iter().position(
        |d| matches!(d, Draw::Box { rect, fill: Some(underdraw_layout::Ink::Paper), .. } if *rect == placed(&page, "dialog").rect),
    );
    assert!(
        scrim.is_some() && scrim < dialog,
        "the scrim covers body, under the dialog: {scrim:?} {dialog:?}"
    );
    let labels: Vec<_> = ["a", "b", "c", "d", "e", "ok"].map(|id| placed(&page, id).label).into();
    assert_eq!(labels, [None, None, None, None, None, Some(1)], "behind a backdrop nothing can be acted on");
}

#[test]
fn a_backdrop_hides_the_marks_of_guesses_behind_it() {
    let kinds = serde_json::json!({ "kinds": { "Dialog": { "look": {
        "box": "column", "backdrop": true, "width": "80%", "padding": 16, "fill": "paper",
        "children": ["$children"] } } } });
    let page = tiny_with(
        serde_json::json!([{ "id": "body", "kind": "Stack", "children": [
            { "id": "behind", "kind": "Text", "text": "behind", "assumed": "a guess behind" },
            { "id": "filler", "kind": "Text", "text": "filler" } ],
          "overlay": { "id": "dialog", "kind": "Dialog", "anchor": "center", "children": [
            { "id": "inside", "kind": "Text", "text": "inside", "assumed": "a guess on the dialog" } ] } }]),
        Some(&kinds),
    );
    let marks: Vec<_> = page
        .draws
        .iter()
        .filter_map(|d| match d {
            Draw::Tag { rect, text, .. } if text == "?" => Some(*rect),
            _ => None,
        })
        .collect();
    let inside = placed(&page, "inside").rect;
    assert_eq!(marks.len(), 1, "one \"?\", on the dialog: {marks:?}");
    assert!((marks[0].y - (inside.y - 6.0)).abs() < 1.0, "the \"?\" marks the guess on the dialog");
    let outlines = page
        .draws
        .iter()
        .filter(|d| matches!(d, Draw::Box { dashed: true, stroke: Some(underdraw_layout::Ink::Assumed), .. }))
        .count();
    assert_eq!(outlines, 1, "one orange outline");
    assert_eq!(
        placed(&page, "behind").assumed.as_deref(),
        Some("a guess behind"),
        "still a guess in the answer"
    );
}

#[test]
fn an_overlay_anchored_at_the_bottom_spans_it_edge_to_edge() {
    let page = tiny(serde_json::json!([
        { "id": "body", "kind": "Stack", "padding": [0, 16], "children": [
            { "id": "a", "kind": "Text", "text": "a" }, { "id": "b", "kind": "Text", "text": "b" },
            { "id": "c", "kind": "Text", "text": "c" } ],
          "overlay": { "id": "sheet", "kind": "Card", "anchor": "bottom", "children": [
            { "id": "sheet-text", "kind": "Text", "text": "?" } ] } }
    ]));
    let (body, sheet) = (placed(&page, "body").rect, placed(&page, "sheet").rect);
    assert_eq!((sheet.x, sheet.width), (body.x, body.width), "as wide as the node, padding and all");
    assert_eq!(sheet.y + sheet.height, body.y + body.height, "on its bottom edge");
}

#[test]
fn an_overlay_taller_than_its_node_is_named_as_cut() {
    let overlay = |lines: usize| {
        let texts: Vec<Value> = (0..lines)
            .map(|n| serde_json::json!({ "id": format!("t{n}"), "kind": "Text", "text": "line" }))
            .collect();
        tiny(serde_json::json!([{ "id": "body", "kind": "Stack", "children": [
            { "id": "a", "kind": "Text", "text": "a" }, { "id": "b", "kind": "Text", "text": "b" },
            { "id": "c", "kind": "Text", "text": "c" } ],
          "overlay": { "id": "sheet", "kind": "Stack", "anchor": "bottom", "children": texts } }]))
    };
    assert_eq!(overlay(1).cut, Vec::<String>::new(), "a short sheet fits");
    assert_eq!(overlay(12).cut, ["sheet"], "a tall one runs off the top");
}

#[test]
fn an_overlay_is_drawn_over_what_comes_after_its_node() {
    let kinds = serde_json::json!({ "kinds": { "Menu": { "look": {
        "box": "column", "width": 200, "padding": 8, "fill": "paper", "stroke": "line",
        "children": ["$children"] } } } });
    let row = |id: &str, menu: bool| {
        let mut row = serde_json::json!({ "id": id, "kind": "Row", "padding": 8, "on": { "tap": { "go": id } },
            "children": [{ "id": format!("{id}-text"), "kind": "Text", "text": id }] });
        if menu {
            row["overlay"] = serde_json::json!({ "id": "menu", "kind": "Menu", "anchor": "top-trailing", "children": [
                { "id": "m1", "kind": "Text", "text": "one" }, { "id": "m2", "kind": "Text", "text": "two" },
                { "id": "m3", "kind": "Text", "text": "three" }, { "id": "m4", "kind": "Text", "text": "four" } ] });
        }
        row
    };
    let page = tiny_with(
        serde_json::json!([{ "id": "list", "kind": "Stack", "children": [row("a", true), row("b", false), row("c", false)] }]),
        Some(&kinds),
    );
    let menu = placed(&page, "menu").rect;
    let at = |wanted: &dyn Fn(&Draw) -> bool| page.draws.iter().position(wanted).unwrap();
    let menu_box = at(&|d| matches!(d, Draw::Box { rect, fill: Some(_), .. } if *rect == menu));
    let later_text = at(&|d| matches!(d, Draw::Text { text, .. } if text == "b"));
    assert!(menu_box > later_text, "the menu is drawn after the row below its own: {menu_box} {later_text}");
    let b = placed(&page, "b").rect;
    assert!(b.y < menu.y + menu.height, "the menu reaches over row b");
    assert_eq!(placed(&page, "b").label, None, "row b, under the menu, has no number");
}
