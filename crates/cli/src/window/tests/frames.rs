//! Each surface of the gallery window drawn to a PNG, as the window would
//! show it, and held to a golden PNG kept in `crates/cli/goldens/gallery`.
//!
//! [`held`] is the glue: a test gives it a root, a window size and its acts
//! on the gallery (pick a line, click a node, drag a region, press keys),
//! and the window's real frame is rendered headless on the GPU `render`
//! uses, one pixel a point. A frame that strays from its golden fails the
//! test, and is written beside the path the failure names to be looked at.
//! `UNDERDRAW_GOLDENS=rewrite cargo nextest run -p underdraw frames`
//! rewrites every golden from the frames drawn instead; look at each before
//! committing it.

use std::io::BufReader;

use super::*;

/// Where the goldens are kept.
const GOLDENS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/goldens/gallery");
/// The variable that, set to `rewrite`, rewrites the goldens.
const REWRITE: &str = "UNDERDRAW_GOLDENS";
/// How far a channel of a pixel may stray from the golden's before the
/// pixel differs: antialiasing, not a change.
const CHANNEL: u8 = 32;
/// How many pixels in a million may differ before the frame does: a line
/// of text changed is hundreds of pixels at 1440 × 900.
const PER_MILLION: usize = 20;

/// The window this size by default, as it opens.
const WINDOW: (f64, f64) = (1440.0, 900.0);

/// The gallery over `root`, `size` points, after `acts`: its frame rendered
/// on the GPU and held to the golden named `name`. The gallery is handed
/// back for what the test asserts of it.
fn held(name: &str, root: &Path, size: (f64, f64), acts: impl FnOnce(&mut Gallery)) -> Gallery {
    let mut gallery = Gallery::new(root, text::Typesetter::new(None).unwrap());
    gallery.size = size;
    // A root of a test's own is named for its process: its line is named
    // the same in every run, for the frame to hold.
    if let Some(line) = gallery.lines.first_mut().filter(|line| line.row.dir == root && !line.row.described) {
        line.row.name = "root".to_string();
    }
    acts(&mut gallery);
    gallery.clamp();
    let scene = gallery.frame();
    let (width, height) = (size.0 as u32, size.1 as u32);
    let mut gpu = gpu::Gpu::open().expect("drawing the window needs a GPU");
    let pixels = gpu.render(&scene, width, height).unwrap();
    hold(name, width, height, &pixels);
    gallery
}

/// `pixels`, `width` × `height`, held to the golden named `name`, or
/// written as it when [`REWRITE`] says so.
fn hold(name: &str, width: u32, height: u32, pixels: &[u8]) {
    let golden = Path::new(GOLDENS).join(format!("{name}.png"));
    if std::env::var(REWRITE).as_deref() == Ok("rewrite") {
        std::fs::create_dir_all(GOLDENS).unwrap();
        crate::write_png(&golden, width, height, pixels).unwrap();
        return;
    }
    let Ok(file) = std::fs::File::open(&golden) else {
        panic!("no golden {}: {REWRITE}=rewrite writes it", golden.display())
    };
    let mut reader = png::Decoder::new(BufReader::new(file)).read_info().unwrap();
    let mut kept = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut kept).unwrap();
    let differing = if (info.width, info.height) == (width, height) {
        kept.chunks(4)
            .zip(pixels.chunks(4))
            .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > CHANNEL))
            .count()
    } else {
        usize::MAX
    };
    if differing > pixels.len() / 4 * PER_MILLION / 1_000_000 {
        let drawn = std::env::temp_dir().join(format!("underdraw-frame-{name}.png"));
        crate::write_png(&drawn, width, height, pixels).unwrap();
        panic!(
            "the frame {name} strays from its golden {} ({}): drawn at {}; {REWRITE}=rewrite rewrites the golden",
            golden.display(),
            if differing == usize::MAX {
                format!("{}×{} kept, {width}×{height} drawn", info.width, info.height)
            } else {
                format!("{differing} pixels differ")
            },
            drawn.display()
        );
    }
}

/// Note `text` written with N on node `id` of the phone's picture.
fn note_on(gallery: &mut Gallery, id: &str, text: &str) {
    press_keys(gallery, &["n"]);
    gallery.cursor = on_node(gallery, 0, id);
    gallery.press();
    gallery.release();
    type_text(gallery, text);
    press_keys(gallery, &["Enter"]);
}

/// Whether a line of the sidebar reads `line`.
fn panel_has(gallery: &Gallery, line: &str) -> bool {
    gallery.panel().iter().any(|l| l.trim() == line)
}

#[test]
fn the_tree_lists_directories_descriptions_states_phases_journeys_and_counts_faults_and_notes() {
    let root = shop("frame-tree");
    std::fs::create_dir_all(root.join("broken")).unwrap();
    std::fs::write(root.join("broken").join(crate::SCREEN), "{}").unwrap();
    let gallery = held("tree", &root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        note_on(gallery, "order-0", "rename it");
    });
    let line = |name: &str| gallery.lines.iter().find(|l| l.row.name == name).unwrap();
    assert!(!line("broken").row.faults.is_empty(), "a description with faults");
    assert!(line("cancel-an-order").journey, "a journey");
    assert!(gallery.lines.iter().any(|l| matches!(l.state, Some(State::Phase(_)))), "a phase");
    let Line { row, state, .. } = line("orders");
    assert_eq!(gallery.open_notes(row, state.as_ref()), 1, "its note counted");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_state_at_both_widths_with_its_labels() {
    let gallery =
        held("state", Path::new(DRAFTS), WINDOW, |gallery| show(gallery, "airbnb-listing", "default"));
    assert_eq!(captions(&gallery), ["default · 390", "default · 1120"]);
}

#[test]
fn an_overview_with_its_arrows_and_journey_marks() {
    let gallery = held("overview", Path::new(DRAFTS), WINDOW, |gallery| {
        let order = description_in(gallery, "shop/order");
        showing(gallery, order);
    });
    assert!(!gallery.arrows().is_empty(), "its transitions");
    assert_eq!(passes_shown(&gallery).len(), 2, "the journey's marks");
}

#[test]
fn the_inbox_of_the_root_grouped_by_description() {
    let root = root_of("frame-inbox", &[("orders", "orders"), ("checkout", "pay/checkout")]);
    let dir = root.join("orders");
    let whole =
        Reference { description: dir.clone(), on: On::State { state: State::Fixture("default".into()) } };
    notes::add(&whole, "too dense").unwrap();
    notes::add(&whole, "a bigger title").unwrap();
    notes::answer(&dir, "N2", "the title is a heading now").unwrap();
    let gallery = held("inbox", &root, WINDOW, |gallery| gallery.pick(0));
    let card = gallery.inbox_card().expect("the inbox of everything under the root");
    assert_eq!(card.title, "waits on you · 6");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_flow_with_a_screen_not_drawn_yet() {
    let root = root_of("frame-flow", &[("airbnb-listing", "shop/listing"), ("checkout", "shop/checkout")]);
    let gallery = held("flow", &root, WINDOW, |gallery| {
        let shop = gallery.lines.iter().position(|line| line.row.name == "shop").unwrap();
        showing(gallery, shop);
    });
    let flow = flow_shown(&gallery);
    assert!(flow.map.places.iter().any(|place| matches!(place.screen, Screen::Undrawn(_))));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_description_with_faults() {
    let root = root_of("frame-faults", &[]);
    std::fs::create_dir_all(root.join("broken")).unwrap();
    std::fs::write(root.join("broken").join(crate::SCREEN), "{}").unwrap();
    let gallery = held("faults", &root, WINDOW, |gallery| gallery.pick(1));
    assert!(matches!(&gallery.picture, Picture::Faults(faults) if !faults.is_empty()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_journey_playing() {
    let gallery = held("journey", Path::new(DRAFTS), WINDOW, |gallery| {
        let journey = journey_of(gallery, "cancel-an-order");
        showing(gallery, journey);
        press_keys(gallery, &["→"]);
    });
    assert_eq!(played(&gallery).0, 2, "its second step, its node marked");
}

#[test]
fn a_prototype_act_and_its_outcomes() {
    let gallery = held("act", Path::new(DRAFTS), WINDOW, |gallery| {
        show(gallery, "orders", "cancel-ready");
        act_on(gallery, "cd-confirm");
    });
    assert!(panel_has(&gallery, "ok → default") && panel_has(&gallery, "shipped → cancel-ready"));
}

#[test]
fn a_region_dragged_for_a_note() {
    let root = root_of("frame-region", &[("orders", "orders")]);
    let gallery = held("region", &root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        press_keys(gallery, &["n"]);
        drag(gallery, 0, (10.0, 145.0), (380.0, 175.0));
    });
    assert!(!gallery.regions().is_empty(), "drawn dashed");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_arrow_picked_in_the_overview() {
    let gallery = held("arrow", Path::new(DRAFTS), WINDOW, |gallery| {
        let orders = line_of(gallery, "orders");
        showing(gallery, orders);
        gallery.cursor = on_arrow(gallery, 0);
        gallery.press();
    });
    assert_eq!(gallery.arrow, Some(0));
}

#[test]
fn note_pins_open_and_answered_with_its_reply() {
    let root = root_of("frame-notes", &[("orders", "orders")]);
    let mut gallery = held("notes", &root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        note_on(gallery, "nav-export", "rename it");
        note_on(gallery, "order-0-cancel", "too red");
        notes::answer(&root.join("orders"), "N1", "renamed it Export").unwrap();
        gallery.reload();
        gallery.cursor = on_pin(gallery, "N1");
        gallery.hover();
    });
    let inks: Vec<Ink> = gallery.pins().into_iter().filter(|pin| !pin.faint).map(|pin| pin.ink).collect();
    assert_eq!(inks, [Ink::Answered, Ink::Note]);
    assert!(!gallery.reply_shown().is_empty(), "the reply shown");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_assumption_picked_by_its_question_mark() {
    let root = root_of("frame-assumption", &[("checkout", "checkout")]);
    let gallery = held("assumption", &root, WINDOW, |gallery| {
        let checkout = line_of(gallery, "checkout");
        let mut offset = 1;
        loop {
            showing(gallery, checkout + offset);
            if matches!(&gallery.picture, Picture::Pages(pages) if pages[0].has("card-number")) {
                break;
            }
            offset += 1;
        }
        // Scrolled to it, as a person scrolls before clicking.
        gallery.cursor = (gallery.tree + MARGIN, MARGIN);
        gallery.wheel(0.0, on_guess(gallery, 0, "card-number").1 - WINDOW.1 / 3.0);
        gallery.cursor = on_guess(gallery, 0, "card-number");
        gallery.press();
    });
    assert_eq!(gallery.node.as_deref(), Some("card-number"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_shared_component_in_the_components_section() {
    let root = shop("frame-components");
    let gallery = held("components", &root, WINDOW, |gallery| show(gallery, "OrderRow", "pending"));
    assert_eq!(captions(&gallery), ["pending · 390", "pending · 1120"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_node_picked_its_card_in_the_sidebar_with_its_details_unfolded() {
    let gallery = held("node", Path::new(DRAFTS), WINDOW, |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.cursor = on_node(gallery, 0, "nav-save");
        gallery.press();
        gallery.details = true;
    });
    assert!(panel_has(&gallery, "id  nav-save"));
}

#[test]
fn the_sidebar_hidden_the_pictures_take_its_width() {
    let gallery = held("sidebar-hidden", Path::new(DRAFTS), WINDOW, |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.toggle_sidebar();
    });
    assert_eq!(gallery.pane_right(), WINDOW.0);
}

#[test]
fn a_narrow_window_at_half_size_scrolled_across() {
    let gallery = held("narrow", Path::new(DRAFTS), (1000.0, 900.0), |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.cursor = (gallery.tree + MARGIN, MARGIN);
        gallery.wheel(1e6, 0.0);
        gallery.cursor = on_node(gallery, 1, "nav-save");
        gallery.press();
    });
    assert_eq!(gallery.zoom(gallery.pane()), SMALLEST);
    assert_eq!(gallery.node.as_deref(), Some("nav-save"), "picked, scaled and scrolled");
}

#[test]
fn the_tree_dragged_narrow_its_names_cut_at_its_edge() {
    let gallery = held("tree-narrow", Path::new(DRAFTS), WINDOW, |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.cursor = (gallery.tree, MARGIN);
        gallery.press();
        gallery.cursor = (0.0, MARGIN);
        gallery.resize_tree();
        gallery.release();
    });
    assert_eq!(gallery.tree, TREE_LEAST);
}

#[test]
fn the_window_in_chinese() {
    let root = root_of("frame-zh", &[("checkout", "checkout")]);
    let gallery = held("zh", &root, WINDOW, |gallery| {
        gallery.lang = Lang::Zh;
        let checkout = line_of(gallery, "checkout");
        showing(gallery, checkout + 1);
        let Picture::Pages(pages) = &gallery.picture else { panic!("no picture") };
        let guessed = pages[0].nodes.iter().find(|node| node.assumed.is_some()).unwrap().id.clone();
        gallery.cursor = on_node(gallery, 0, &guessed);
        gallery.press();
    });
    assert!(panel_has(&gallery, "▸ 详情"));
    std::fs::remove_dir_all(root).unwrap();
}
