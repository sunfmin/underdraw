//! Lays the Airbnb listing draft out with text measured by a stand-in: every
//! character is half the font size wide, so the geometry is exact.

use serde_json::Value;
use underdraw_format::{Breakpoint, Lang, Screen, Shared, State};
use underdraw_layout::{Draw, Ink, Page, Placed, layout};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../format/tests/airbnb-listing");

mod fixed;
use fixed::Fixed;

fn read(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{DRAFT}/{name}")).unwrap()).unwrap()
}

fn page(state: State, width: f32) -> Page {
    let (screen, tokens) = (Screen::read(&read("screen.json"), Shared::default()), read("tokens.json"));
    let breakpoint = Breakpoint::for_width(&tokens, width as u32);
    let root = screen.instantiate(state, breakpoint, None).unwrap();
    layout(&root, &tokens, &screen, width, Lang::default(), &mut Fixed, &underdraw_layout::NoPictures)
        .unwrap()
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
    page.nodes.iter().any(|c| {
        c.component == "Carousel" && c.rect.y <= node.rect.y && node.rect.y < c.rect.y + c.rect.height
    })
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
    // Each label in the ink only labels draw with, to stand out from the
    // wireframe it sits on.
    let numbers: Vec<Ink> = page
        .draws
        .iter()
        .filter_map(|d| match d {
            Draw::Tag { text, ink, .. } if text != "?" => Some(*ink),
            _ => None,
        })
        .collect();
    assert_eq!(numbers, vec![Ink::Label; labels.len()]);
    let inks = |d: &Draw| match d {
        Draw::Box { fill, stroke, .. } => vec![*fill, *stroke],
        Draw::Text { ink, .. } | Draw::Cross { ink, .. } | Draw::Line { ink, .. } => vec![Some(*ink)],
        Draw::Clip { .. } | Draw::Tag { .. } => Vec::new(),
    };
    assert!(!page.draws.iter().flat_map(inks).any(|ink| ink == Some(Ink::Label)), "no wireframe draws in it");
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
fn every_component_in_the_draft_is_in_the_catalog() {
    let page = page(State::Fixture("default"), 1120.0);
    let unknown: Vec<_> = page
        .draws
        .iter()
        .filter_map(|d| match d {
            Draw::Box { dashed: true, stroke: Some(underdraw_layout::Ink::Muted), rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert!(unknown.is_empty(), "components drawn as unknown at {unknown:?}");
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

/// The same, with the screen's own `"components"` those `own` holds.
fn tiny_with(children: Value, own: Option<&Value>) -> Page {
    let mut screen = serde_json::json!({
        "screen": "Tiny", "data": {}, "types": {}, "fixtures": { "default": {} }, "conditions": {},
        "root": { "id": "tiny", "component": "Screen", "children": children }
    });
    if let Some(own) = own {
        screen["components"] = own["components"].clone();
    }
    let tokens = read("tokens.json");
    let screen = Screen::read(&screen, Shared::default());
    let root =
        screen.instantiate(State::Fixture("default"), Breakpoint::for_width(&tokens, 390), None).unwrap();
    layout(&root, &tokens, &screen, 390.0, Lang::default(), &mut Fixed, &underdraw_layout::NoPictures)
        .unwrap()
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
        { "id": "open", "component": "ListItem", "leadingIcon": "circle", "title": "a" },
        { "id": "done", "component": "ListItem", "leadingIcon": "check_circle.fill", "title": "b" }
    ]));
    let texts = texts(&page);
    // Material Symbols' circle (U+EF4A) outlined, and its check_circle
    // (U+F0BE) solid, moved to U+F0000 + it where the solid font has it.
    assert!(texts.contains(&"\u{ef4a}") && texts.contains(&"\u{ff0be}"), "{texts:?}");
}

#[test]
fn an_icon_is_as_big_as_its_square() {
    let page = tiny(serde_json::json!([
        { "id": "row", "component": "ListItem", "leadingIcon": "circle", "title": "a" },
        { "id": "big", "component": "Glyph", "icon": "flight", "size": 48 }
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
        { "id": "body", "component": "Stack", "padding": [0, "space.4"], "children": [
            { "id": "line", "component": "Text", "text": "a" } ] }
    ]));
    let (body, line) = (placed(&page, "body").rect, placed(&page, "line").rect);
    assert_eq!((line.x, line.width, line.y), (16.0, 390.0 - 32.0, body.y), "16 either side, none above");
}

#[test]
fn a_component_not_in_the_catalog_is_drawn_as_a_guess() {
    let page = tiny(serde_json::json!([{ "id": "input", "component": "SignaturePad" }]));
    let assumed = placed(&page, "input").assumed.as_deref().unwrap_or_default();
    assert!(assumed.contains("SignaturePad"), "{assumed:?}");
    let marked = page
        .draws
        .iter()
        .any(|d| matches!(d, Draw::Box { dashed: true, stroke: Some(underdraw_layout::Ink::Assumed), .. }));
    assert!(marked, "drawn in the colour of a guess");
}

/// A built-in TextField bound to `draft`, one line high and four, with
/// `draft` as `value`.
fn text_fields(value: &str) -> Page {
    let screen = serde_json::json!({
        "screen": "Tiny", "data": { "draft": { "type": "string", "view": true } }, "types": {},
        "fixtures": { "default": { "draft": value } }, "conditions": {},
        "root": { "id": "tiny", "component": "Screen", "children": [
            { "id": "one", "component": "TextField", "bind": "draft", "placeholder": "Add a todo" },
            { "id": "four", "component": "TextField", "bind": "draft", "placeholder": "Write a message", "lines": 4 } ] }
    });
    let tokens = read("tokens.json");
    let screen = Screen::read(&screen, Shared::default());
    let root =
        screen.instantiate(State::Fixture("default"), Breakpoint::for_width(&tokens, 390), None).unwrap();
    layout(&root, &tokens, &screen, 390.0, Lang::default(), &mut Fixed, &underdraw_layout::NoPictures)
        .unwrap()
}

fn ink_of(page: &Page, shown: &str) -> Vec<Ink> {
    page.draws
        .iter()
        .filter_map(|d| match d {
            Draw::Text { text, ink, .. } if text == shown => Some(*ink),
            _ => None,
        })
        .collect()
}

#[test]
fn a_text_field_shows_its_placeholder_muted_while_its_value_is_empty() {
    let empty = text_fields("");
    assert_eq!(placed(&empty, "one").assumed, None, "TextField is built in");
    assert_eq!(ink_of(&empty, "Add a todo"), [Ink::Muted]);
    assert_eq!(ink_of(&empty, "Write a message"), [Ink::Muted]);
    let typed = text_fields("milk");
    assert_eq!(ink_of(&typed, "milk"), [Ink::Text, Ink::Text], "the value, in the ink of text");
    assert!(ink_of(&typed, "Add a todo").is_empty(), "no placeholder once typed: {:?}", texts(&typed));
}

#[test]
fn a_text_field_is_as_high_as_its_lines() {
    let page = text_fields("");
    let (one, four) = (placed(&page, "one").rect, placed(&page, "four").rect);
    // font.body's lines are 22 high, inside 10 above and below.
    assert_eq!((one.height, four.height), (22.0 + 20.0, 4.0 * 22.0 + 20.0));
    assert_eq!(one.width, four.width, "both as wide as the screen");
}

#[test]
fn a_projects_component_is_drawn_from_its_look() {
    let own = serde_json::json!({ "components": { "TextField": { "look": {
        "box": "row", "grow": 1, "padding": [10, 12], "stroke": "line",
        "children": [ { "text": "{placeholder}", "ink": "muted" } ] } } } });
    let row = serde_json::json!([{ "id": "composer", "component": "Row", "gap": 12, "children": [
        { "id": "input", "component": "TextField", "placeholder": "添加一项待办" },
        { "id": "add", "component": "Button", "text": "添加" } ] }]);
    let page = tiny_with(row, Some(&own));
    let (input, add) = (placed(&page, "input"), placed(&page, "add"));
    assert_eq!(input.assumed, None, "a defined component is no guess");
    assert_eq!(input.rect.x + input.rect.width + 12.0, add.rect.x, "the field fills up to the button");
    assert_eq!(add.rect.x + add.rect.width, 390.0, "the button ends the row");
    assert!(texts(&page).contains(&"添加一项待办"), "{:?}", texts(&page));
}

#[test]
fn a_grid_track_is_a_share_a_part_or_points() {
    let page = tiny(serde_json::json!([
        { "id": "grid", "component": "Grid", "gap": 0, "columns": ["1fr", 48, "50%"], "children": [
            { "id": "share", "component": "Text", "text": "a" },
            { "id": "points", "component": "Text", "text": "b" },
            { "id": "part", "component": "Text", "text": "c" } ] }
    ]));
    let width = |id| placed(&page, id).rect.width;
    assert_eq!((width("share"), width("points"), width("part")), (390.0 - 48.0 - 195.0, 48.0, 195.0));
}

/// The lines a page draws in `ink`, each from and to.
fn lines(page: &Page, ink: Ink) -> Vec<([f32; 2], [f32; 2])> {
    page.draws
        .iter()
        .filter_map(|d| match d {
            Draw::Line { from, to, ink: drawn } if *drawn == ink => Some((*from, *to)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_column_fills_its_row_and_panes_are_split_by_a_line() {
    let text = |id: &str| serde_json::json!({ "id": id, "component": "Text", "text": id });
    let page = tiny(serde_json::json!([
        { "id": "panes", "component": "Columns", "gap": 0, "columns": [100, "1fr"], "dividers": true,
          "children": [
            { "id": "tree", "component": "Stack", "border": "color.border", "children": [text("a")] },
            { "id": "pictures", "component": "Stack", "children": [text("b"), text("c"), text("d")] } ] }
    ]));
    let (tree, pictures) = (placed(&page, "tree").rect, placed(&page, "pictures").rect);
    assert!(pictures.height > placed(&page, "b").rect.height * 2.0, "the pictures pane is three lines tall");
    assert_eq!((tree.y, tree.height), (pictures.y, pictures.height), "the tree fills its row");
    let outline = page.draws.iter().find_map(|d| match d {
        Draw::Box { rect, stroke: Some(Ink::Line), .. } if rect.x == tree.x => Some(*rect),
        _ => None,
    });
    assert_eq!(outline, Some(tree), "its border reaches the row's foot");
    assert_eq!(
        lines(&page, Ink::Line),
        [([100.0, tree.y], [100.0, tree.y + tree.height])],
        "one line between the panes, as tall as the row"
    );
}

#[test]
fn a_grid_draws_lines_between_its_rows_and_columns_halfway_across_the_gap() {
    let text = |id: &str| serde_json::json!({ "id": id, "component": "Text", "text": id });
    let page = tiny(serde_json::json!([
        { "id": "grid", "component": "Grid", "gap": 10, "columns": 2, "dividers": true,
          "children": [text("a"), text("b"), text("c"), text("d")] }
    ]));
    let (a, d) = (placed(&page, "a").rect, placed(&page, "d").rect);
    let (middle_x, middle_y) = ((a.x + a.width + d.x) / 2.0, (a.y + a.height + d.y) / 2.0);
    let (top, bottom) = (a.y, d.y + d.height);
    let (left, right) = (a.x, d.x + d.width);
    assert_eq!(
        lines(&page, Ink::Line),
        [
            ([middle_x, top], [middle_x, middle_y]),
            ([left, middle_y], [middle_x, middle_y]),
            ([middle_x, middle_y], [middle_x, bottom]),
            ([middle_x, middle_y], [right, middle_y]),
        ],
        "b's line down, c's across, d's down and across: they meet in the middle"
    );
    let plain = tiny(serde_json::json!([
        { "id": "grid", "component": "Grid", "columns": 2, "children": [text("a"), text("b")] }
    ]));
    assert!(lines(&plain, Ink::Line).is_empty(), "no lines unless asked for");
}

#[test]
fn an_overlay_anchored_at_the_center_sits_in_the_middle() {
    let page = tiny(serde_json::json!([
        { "id": "body", "component": "Stack", "children": [
            { "id": "a", "component": "Text", "text": "a" }, { "id": "b", "component": "Text", "text": "b" },
            { "id": "c", "component": "Text", "text": "c" }, { "id": "d", "component": "Text", "text": "d" } ],
          "overlay": { "id": "dialog", "component": "Card", "anchor": "center", "children": [
            { "id": "dialog-text", "component": "Text", "text": "?" } ] } }
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
    let own = serde_json::json!({ "components": { "Dialog": { "look": {
        "box": "column", "width": "80%", "padding": 16, "fill": "paper", "stroke": "line",
        "children": ["$children"] } } } });
    let button = |id: &str| serde_json::json!({ "id": id, "component": "Button", "text": id, "on": { "tap": { "go": id } } });
    let buttons: Vec<Value> = ["a", "b", "c", "d", "e"].map(button).into();
    let page = tiny_with(
        serde_json::json!([{ "id": "body", "component": "Stack", "gap": 8,
            "children": buttons,
            "overlay": { "id": "dialog", "component": "Dialog", "anchor": "center", "children": [button("ok")] } }]),
        Some(&own),
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
    let own = serde_json::json!({ "components": { "Dialog": { "look": {
        "box": "column", "backdrop": true, "width": "80%", "padding": 16, "fill": "paper",
        "children": ["$children"] } } } });
    let button = |id: &str| serde_json::json!({ "id": id, "component": "Button", "text": id, "on": { "tap": { "go": id } } });
    let buttons: Vec<Value> = ["a", "b", "c", "d", "e"].map(button).into();
    let page = tiny_with(
        serde_json::json!([{ "id": "body", "component": "Stack", "gap": 8, "children": buttons,
            "overlay": { "id": "dialog", "component": "Dialog", "anchor": "center", "children": [button("ok")] } }]),
        Some(&own),
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
    let own = serde_json::json!({ "components": { "Dialog": { "look": {
        "box": "column", "backdrop": true, "width": "80%", "padding": 16, "fill": "paper",
        "children": ["$children"] } } } });
    let page = tiny_with(
        serde_json::json!([{ "id": "body", "component": "Stack", "children": [
            { "id": "behind", "component": "Text", "text": "behind", "assumed": "a guess behind" },
            { "id": "filler", "component": "Text", "text": "filler" } ],
          "overlay": { "id": "dialog", "component": "Dialog", "anchor": "center", "children": [
            { "id": "inside", "component": "Text", "text": "inside", "assumed": "a guess on the dialog" } ] } }]),
        Some(&own),
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

/// Every "?" drawn on `page`.
fn guess_marks(page: &Page) -> Vec<underdraw_layout::Rect> {
    page.draws
        .iter()
        .filter_map(|d| match d {
            Draw::Tag { rect, text, .. } if text == "?" => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn a_guess_as_big_as_the_page_has_its_mark_on_what_it_shows() {
    let own = serde_json::json!({ "components": { "Viewer": { "look": {
        "box": "column", "backdrop": true, "width": "100%", "height": "100%", "fill": "ink",
        "justify": "center", "padding": 16, "children": ["$children"] } } } });
    let page = tiny_with(
        serde_json::json!([{ "id": "body", "component": "Stack", "children": [
            { "id": "a", "component": "Text", "text": "a" }, { "id": "b", "component": "Text", "text": "b" },
            { "id": "c", "component": "Text", "text": "c" }, { "id": "d", "component": "Text", "text": "d" },
            { "id": "e", "component": "Text", "text": "e" }, { "id": "f", "component": "Text", "text": "f" } ],
          "overlay": { "id": "viewer", "component": "Viewer", "anchor": "center",
            "assumed": "a viewer over the whole page", "children": [
            { "id": "shown", "component": "Text", "text": "shown" } ] } }]),
        Some(&own),
    );
    let viewer = placed(&page, "viewer");
    assert_eq!((viewer.rect.width, viewer.rect.height), (page.width, page.height), "as big as the page");
    let shown = placed(&page, "shown").rect;
    let marks = guess_marks(&page);
    assert_eq!(marks.len(), 1, "{marks:?}");
    let mark = marks[0];
    assert!(
        mark.x >= 0.0
            && mark.y >= 0.0
            && mark.x + mark.width <= page.width
            && mark.y + mark.height <= page.height,
        "never cut by the page's edge: {mark:?}"
    );
    assert!(
        (mark.y - (shown.y - 6.0)).abs() < 1.0 && mark.x + mark.width > shown.x + shown.width - 1.0,
        "on the top-right of what it shows, {shown:?}: {mark:?}"
    );
    assert_eq!(viewer.guess_tag, Some(mark), "a click on the mark drawn is on its assumption");
}

#[test]
fn a_guess_at_the_page_edge_has_its_mark_wholly_on_the_page() {
    let page = tiny(serde_json::json!([
        { "id": "wide", "component": "Text", "text": "wide", "assumed": "a line as wide as the page" } ]));
    let wide = placed(&page, "wide");
    assert!(wide.rect.x + wide.rect.width >= page.width - 0.5, "it reaches the edge: {:?}", wide.rect);
    let marks = guess_marks(&page);
    assert_eq!(marks.len(), 1, "{marks:?}");
    assert!(marks[0].x + marks[0].width <= page.width && marks[0].x >= 0.0, "not cut: {:?}", marks[0]);
    assert_eq!(wide.guess_tag, Some(marks[0]));
}

#[test]
fn an_overlay_anchored_at_the_bottom_spans_it_edge_to_edge() {
    let page = tiny(serde_json::json!([
        { "id": "body", "component": "Stack", "padding": [0, 16], "children": [
            { "id": "a", "component": "Text", "text": "a" }, { "id": "b", "component": "Text", "text": "b" },
            { "id": "c", "component": "Text", "text": "c" } ],
          "overlay": { "id": "sheet", "component": "Card", "anchor": "bottom", "children": [
            { "id": "sheet-text", "component": "Text", "text": "?" } ] } }
    ]));
    let (body, sheet) = (placed(&page, "body").rect, placed(&page, "sheet").rect);
    assert_eq!((sheet.x, sheet.width), (body.x, body.width), "as wide as the node, padding and all");
    assert_eq!(sheet.y + sheet.height, body.y + body.height, "on its bottom edge");
}

#[test]
fn an_overlay_taller_than_its_node_is_named_as_cut() {
    let overlay = |lines: usize| {
        let texts: Vec<Value> = (0..lines)
            .map(|n| serde_json::json!({ "id": format!("t{n}"), "component": "Text", "text": "line" }))
            .collect();
        tiny(serde_json::json!([{ "id": "body", "component": "Stack", "children": [
            { "id": "a", "component": "Text", "text": "a" }, { "id": "b", "component": "Text", "text": "b" },
            { "id": "c", "component": "Text", "text": "c" } ],
          "overlay": { "id": "sheet", "component": "Stack", "anchor": "bottom", "children": texts } }]))
    };
    assert_eq!(overlay(1).cut, Vec::<String>::new(), "a short sheet fits");
    assert_eq!(overlay(12).cut, ["sheet"], "a tall one runs off the top");
}

#[test]
fn an_overlay_is_drawn_over_what_comes_after_its_node() {
    let own = serde_json::json!({ "components": { "Menu": { "look": {
        "box": "column", "width": 200, "padding": 8, "fill": "paper", "stroke": "line",
        "children": ["$children"] } } } });
    let row = |id: &str, menu: bool| {
        let mut row = serde_json::json!({ "id": id, "component": "Row", "padding": 8, "on": { "tap": { "go": id } },
            "children": [{ "id": format!("{id}-text"), "component": "Text", "text": id }] });
        if menu {
            row["overlay"] = serde_json::json!({ "id": "menu", "component": "Menu", "anchor": "top-trailing", "children": [
                { "id": "m1", "component": "Text", "text": "one" }, { "id": "m2", "component": "Text", "text": "two" },
                { "id": "m3", "component": "Text", "text": "three" }, { "id": "m4", "component": "Text", "text": "four" } ] });
        }
        row
    };
    let page = tiny_with(
        serde_json::json!([{ "id": "list", "component": "Stack", "children": [row("a", true), row("b", false), row("c", false)] }]),
        Some(&own),
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

#[test]
fn text_in_a_colour_token_is_drawn_in_the_ink_its_colour_reads_as() {
    let page = tiny(serde_json::json!([
        { "id": "light", "component": "Text", "text": "light", "color": "color.surface" },
        { "id": "secondary", "component": "Text", "text": "secondary", "color": "color.textSecondary" },
        { "id": "dark", "component": "Text", "text": "dark", "color": "color.text" },
        { "id": "missing", "component": "Text", "text": "missing", "color": "color.nowhere" }
    ]));
    let ink = |said: &str| {
        page.draws
            .iter()
            .find_map(|d| match d {
                Draw::Text { text, ink, .. } if text == said => Some(*ink),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{said} is not drawn"))
    };
    // color.surface is #FFFFFF: text in it is meant to show on a dark fill.
    assert_eq!(ink("light"), underdraw_layout::Ink::Paper);
    assert_eq!(ink("secondary"), underdraw_layout::Ink::Muted);
    assert_eq!(ink("dark"), underdraw_layout::Ink::Text);
    assert_eq!(ink("missing"), underdraw_layout::Ink::Text);
}

/// Another screen's state as a picture: `List` in `empty`, a 200 wide
/// page holding a button, numbered; or why it cannot be drawn.
struct Album;

impl Album {
    fn picture_of_list(&self) -> Page {
        underdraw_layout::Pictures::picture(self, "List", Some("empty")).unwrap()
    }
}

impl underdraw_layout::Pictures for Album {
    fn picture(&self, screen: &str, state: Option<&str>) -> Result<Page, String> {
        if (screen, state) != ("List", Some("empty")) {
            return Err(format!("no {screen} {state:?}"));
        }
        Ok(narrow(serde_json::json!([
            { "id": "go", "component": "Button", "text": "Go", "on": { "tap": { "navigate": "Other" } } }
        ])))
    }
}

/// A screen of `children` with no data, laid out 200 wide.
fn narrow(children: Value) -> Page {
    let screen = serde_json::json!({
        "screen": "List", "data": {}, "types": {}, "fixtures": { "empty": {} }, "conditions": {},
        "root": { "id": "list", "component": "Screen", "children": children }
    });
    let tokens = read("tokens.json");
    let screen = Screen::read(&screen, Shared::default());
    let root =
        screen.instantiate(State::Fixture("empty"), Breakpoint::for_width(&tokens, 200), None).unwrap();
    layout(&root, &tokens, &screen, 200.0, Lang::default(), &mut Fixed, &underdraw_layout::NoPictures)
        .unwrap()
}

/// A screen of `children`, laid out at a phone's width with [`Album`].
fn pictured(children: Value) -> Page {
    let screen = serde_json::json!({
        "screen": "Tiny", "data": {}, "types": {}, "fixtures": { "default": {} }, "conditions": {},
        "root": { "id": "tiny", "component": "Screen", "children": children }
    });
    let tokens = read("tokens.json");
    let screen = Screen::read(&screen, Shared::default());
    let root =
        screen.instantiate(State::Fixture("default"), Breakpoint::for_width(&tokens, 390), None).unwrap();
    layout(&root, &tokens, &screen, 390.0, Lang::default(), &mut Fixed, &Album).unwrap()
}

/// What is drawn of `page` cut to the box `shot`: the state it pictures.
fn inside(page: &Page, shot: underdraw_layout::Rect) -> &[Draw] {
    page.draws
        .iter()
        .find_map(|d| match d {
            Draw::Clip { rect, draws, .. } if *rect == shot => Some(draws.as_slice()),
            _ => None,
        })
        .expect("the state is drawn cut to the box")
}

/// The first box `draws` fills.
fn filled(draws: &[Draw]) -> (underdraw_layout::Rect, f32) {
    draws
        .iter()
        .find_map(|d| match d {
            Draw::Box { rect, radius, fill: Some(_), .. } => Some((*rect, *radius)),
            _ => None,
        })
        .expect("a filled box")
}

#[test]
fn a_wireframe_draws_the_state_it_names_scaled_to_its_box_without_its_labels() {
    let page = pictured(serde_json::json!([
        { "id": "shot", "component": "Wireframe", "screen": "List", "state": "empty" }
    ]));
    let list = Album.picture_of_list();
    let shot = placed(&page, "shot").rect;
    assert_eq!(shot.width, 390.0);
    let tall = list.height * shot.width / list.width;
    assert!((shot.height - tall).abs() <= 0.5, "as tall as the state for its width: {shot:?}, {tall}");
    assert!(page.unpictured.is_empty());
    assert!(!list.marks().is_empty(), "the button is numbered in its own picture");
    let inside = inside(&page, shot);
    assert!(!inside.iter().any(|d| matches!(d, Draw::Tag { .. })), "its labels are left out: {inside:?}");
    assert!(page.nodes.iter().all(|n| n.id != "go"), "nothing in it is a node of this screen");
    let scale = (shot.width / list.width).min(shot.height / list.height);
    let left = shot.x + (shot.width - list.width * scale) / 2.0;
    let ((button, radius), (drawn, drawn_radius)) = (filled(list.wireframe()), filled(inside));
    let expected = (left + button.x * scale, shot.y + button.y * scale, button.width * scale, radius * scale);
    assert_eq!((drawn.x, drawn.y, drawn.width, drawn_radius), expected);
}

#[test]
fn a_wireframe_given_a_shape_fits_the_state_in_it_at_the_top() {
    let page = pictured(serde_json::json!([
        { "id": "shot", "component": "Wireframe", "screen": "List", "state": "empty", "aspectRatio": "8:1" }
    ]));
    let list = Album.picture_of_list();
    let shot = placed(&page, "shot").rect;
    assert_eq!(shot.width, 390.0);
    assert!((shot.height - 48.75).abs() <= 0.5, "{shot:?}");
    // Too low for the state at its width: scaled to its height, in its
    // middle across, at its top.
    let scale = shot.height / list.height;
    assert!(list.width * scale < shot.width);
    let (button, _) = filled(list.wireframe());
    let (drawn, _) = filled(inside(&page, shot));
    let left = shot.x + (shot.width - list.width * scale) / 2.0;
    let expected = (left + button.x * scale, shot.y + button.y * scale, button.width * scale);
    assert_eq!((drawn.x, drawn.y, drawn.width), expected);
}

#[test]
fn a_wireframe_that_cannot_be_drawn_is_crossed_out_and_said_at_its_node() {
    let page = pictured(serde_json::json!([
        { "id": "card", "component": "Card", "children": [
            { "id": "shot", "component": "Wireframe", "screen": "List", "state": "gone" } ] },
        { "id": "none", "component": "Wireframe" }
    ]));
    let at: Vec<(&str, &str)> = page.unpictured.iter().map(|f| (f.at.as_str(), f.problem.as_str())).collect();
    assert_eq!(at[0], ("shot", "no List Some(\"gone\")"));
    assert_eq!(at[1].0, "none");
    assert!(at[1].1.contains("give it \"screen\""), "{at:?}");
    let shot = placed(&page, "shot").rect;
    assert!(page.draws.iter().any(|d| matches!(d, Draw::Cross { rect, .. } if *rect == shot)));
}

#[test]
fn a_label_sits_left_of_its_node_clear_of_the_text_at_its_top_left() {
    let own = serde_json::json!({ "components": { "Picture": { "look": {
        "box": "column", "stroke": "line",
        "children": [ { "text": "390", "ink": "muted" }, { "text": "a state, drawn small" } ] } },
        "Sheet": { "look": { "box": "column", "padding": 24, "gap": 8, "children": ["$children"] } } } });
    let column = serde_json::json!([{ "id": "body", "component": "Sheet", "children": [
        { "id": "first", "component": "Picture", "on": { "tap": { "navigate": "Other" } } },
        { "id": "second", "component": "Picture", "on": { "tap": { "navigate": "Other" } } } ] }]);
    let page = tiny_with(column, Some(&own));
    let (first, second) = (placed(&page, "first"), placed(&page, "second"));
    assert_eq!((first.label, second.label), (Some(1), Some(2)), "each named by its own number");
    let tags: Vec<underdraw_layout::Rect> = page
        .marks()
        .iter()
        .filter_map(|d| match d {
            Draw::Tag { rect, ink: Ink::Label, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(tags.len(), 2, "{tags:?}");
    let texts: Vec<underdraw_layout::Rect> = page
        .draws
        .iter()
        .filter_map(|d| match d {
            Draw::Text { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    let overlaps = |a: underdraw_layout::Rect, b: underdraw_layout::Rect| {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    };
    for (tag, node) in tags.iter().zip([first, second]) {
        assert!(tag.x + tag.width <= node.rect.x, "left of {}: {tag:?} {:?}", node.id, node.rect);
        assert_eq!(tag.y, node.rect.y, "level with the top of {}", node.id);
        assert!(!texts.iter().any(|t| overlaps(*tag, *t)), "on no text: {tag:?} {texts:?}");
    }
}

#[test]
fn a_foot_sits_on_the_bottom_of_its_column_after_everything_else() {
    // A pane beside a taller one: its foot is drawn on the pane's bottom
    // edge, after the cards, with the space left between them.
    let page = tiny(serde_json::json!([
        { "id": "panes", "component": "Columns", "gap": 0, "columns": ["1fr", "1fr"], "children": [
            { "id": "long", "component": "Stack", "children": [
                { "id": "l1", "component": "Text", "text": "a" }, { "id": "l2", "component": "Text", "text": "b" },
                { "id": "l3", "component": "Text", "text": "c" }, { "id": "l4", "component": "Text", "text": "d" },
                { "id": "l5", "component": "Text", "text": "e" }, { "id": "l6", "component": "Text", "text": "f" } ] },
            { "id": "side", "component": "Stack",
              "foot": { "id": "send", "component": "Button", "text": "Send" },
              "children": [ { "id": "card", "component": "Text", "text": "a card" } ] } ] }
    ]));
    let (side, card, send) =
        (placed(&page, "side").rect, placed(&page, "card").rect, placed(&page, "send").rect);
    assert_eq!(side.height, placed(&page, "long").rect.height, "the pane is as tall as the other");
    assert_eq!(send.y + send.height, side.y + side.height, "the foot closes the pane");
    assert!(send.y > card.y + card.height, "below the cards, with room between: {card:?} {send:?}");
}

#[test]
fn a_row_of_small_controls_keeps_every_word_whole_under_its_labels_and_marks() {
    // Chips side by side, no gap between them: a number left of one, or a
    // "?" over its top-right corner, would sit on the end of the word beside it.
    let own = serde_json::json!({ "components": {
        "Bar": { "look": { "box": "row", "padding": 24, "gap": 0, "children": ["$children"] } },
        "Chip": { "look": { "box": "row", "padding": [4, 4], "stroke": "line",
            "children": [ { "text": "{text}" } ] } } } });
    let chip = |id: &str, text: &str, assumed: bool| {
        let mut chip = serde_json::json!({ "id": id, "component": "Chip", "text": text,
            "on": { "tap": { "navigate": "Other" } } });
        if assumed {
            chip["assumed"] = serde_json::json!("a guess");
        }
        chip
    };
    let page = tiny_with(
        serde_json::json!([{ "id": "bar", "component": "Bar", "children": [
            chip("inspect", "Inspect", true), chip("listen", "Agent listening", true),
            chip("stop", "Stop", false) ] }]),
        Some(&own),
    );
    let ids = ["inspect", "listen", "stop"];
    let nodes: Vec<&Placed> = ids.iter().map(|id| placed(&page, id)).collect();
    assert_eq!(nodes.iter().map(|n| n.label).collect::<Vec<_>>(), [Some(1), Some(2), Some(3)]);
    let words: Vec<underdraw_layout::Rect> = page
        .wireframe()
        .iter()
        .filter_map(|d| match d {
            Draw::Text { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    let overlaps = |a: underdraw_layout::Rect, b: underdraw_layout::Rect| {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    };
    let near = |a: underdraw_layout::Rect, b: underdraw_layout::Rect| {
        let grown = underdraw_layout::Rect {
            x: b.x - 2.0,
            y: b.y - 2.0,
            width: b.width + 4.0,
            height: b.height + 4.0,
        };
        overlaps(a, grown)
    };
    let mut tags = Vec::new();
    for draw in page.marks() {
        let Draw::Tag { rect, text, ink } = draw else { continue };
        let node = match ink {
            Ink::Label => nodes.iter().find(|n| n.label.map(|l| l.to_string()).as_ref() == Some(text)),
            _ => nodes.iter().find(|n| n.guess_tag == Some(*rect)),
        };
        let node = node.unwrap_or_else(|| panic!("{text:?} at {rect:?} names no chip"));
        assert!(near(*rect, node.rect), "{text:?} sits by {}: {rect:?} {:?}", node.id, node.rect);
        for other in nodes.iter().filter(|n| n.id != node.id) {
            for word in words.iter().filter(|w| overlaps(**w, other.rect)) {
                assert!(
                    !overlaps(*rect, *word),
                    "{text:?} of {} covers {}'s text: {rect:?} {word:?}",
                    node.id,
                    other.id
                );
            }
        }
        tags.push(*rect);
    }
    assert_eq!(tags.len(), 5, "three numbers and two \"?\": {tags:?}");
    for (i, a) in tags.iter().enumerate() {
        assert!(!tags.iter().skip(i + 1).any(|b| overlaps(*a, *b)), "no tag on another: {tags:?}");
    }
}

#[test]
fn a_badge_in_a_row_is_centred_across_as_its_siblings_are() {
    let page = tiny(serde_json::json!([
        { "id": "head", "component": "Row", "gap": "space.2", "children": [
            { "id": "name", "component": "Text", "text": "Home" },
            { "id": "tag", "component": "Badge", "text": "Default" },
            { "id": "more", "component": "IconButton", "icon": "more_vert" } ] },
        { "id": "alone", "component": "Badge", "text": "Default" }
    ]));
    let middle = |id: &str| {
        let rect = placed(&page, id).rect;
        rect.y + rect.height / 2.0
    };
    assert_eq!(middle("tag"), middle("name"), "centred on the row, as the text is");
    let (alone, screen) = (placed(&page, "alone").rect, placed(&page, "tiny").rect);
    assert!(alone.width < screen.width / 2.0, "in a column, as wide as its words: {alone:?}");
}
