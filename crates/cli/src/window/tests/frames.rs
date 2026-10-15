//! Each surface of the gallery window, acted on as a person would and held
//! to the few lines of text, or numbers, that say what its test is about:
//! never to its pixels, nor to all it shows.
//!
//! What most of a frame shows is text and structure: the tree's lines, the
//! pictures and what is picked on them, the pins, the sidebar's cards.
//! [`acted`] is the glue: a test gives it a root, a window size and its acts
//! on the gallery (pick a line, click a node, press keys), and
//! [`shows_lines`] holds what the window then shows, as [`said`] writes it,
//! to the lines the test chose, a failure naming the line missing. So a
//! draft added, or a word changed, fails only the tests about it. Where the
//! look is the point (the scrollbars, the pictures at half size, the marks
//! hidden, the narrow window) a test asserts it from the window's state: a
//! scrollbar's thumb there or not, the zoom, what is drawn over the
//! pictures. Those draw a fixture root of their own ([`fixture`]); the
//! journey's surfaces draw a frozen shop of their own ([`frozen_shop`]).

use super::*;

/// The description the look's tests draw: a copy of the Airbnb listing
/// frozen for the tests, which no draft changes.
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../format/tests/airbnb-listing");

/// The shop the journey's tests draw: a copy of the shop draft frozen for
/// the tests, its journey walkable, with a broken journey of its own.
const SHOP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shop");

/// The window this size by default, as it opens.
const WINDOW: (f64, f64) = (1440.0, 900.0);

/// A narrow window: its pictures' pane too narrow for both pages at half
/// size.
const NARROW: (f64, f64) = (1000.0, 900.0);

/// The gallery over `root`, `size` points, after `acts`, its panes fitted
/// to what they then hold. A root of a test's own is named for its
/// process: its line is named `root` in every run, and again after acts
/// that reload the tree.
fn acted(root: &Path, size: (f64, f64), acts: impl FnOnce(&mut Gallery)) -> Gallery {
    let mut gallery = opened(root, size);
    let named = |gallery: &mut Gallery| {
        if let Some(line) =
            gallery.lines.first_mut().filter(|line| line.row.dir == root && !line.row.described)
        {
            line.row.name = "root".to_string();
        }
    };
    named(&mut gallery);
    acts(&mut gallery);
    named(&mut gallery);
    gallery.clamp();
    gallery
}

/// A root of its own named `name`, holding the frozen Airbnb listing
/// ([`FIXTURE`]) as the description `at`.
fn fixture(name: &str, at: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("underdraw-window-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let at = root.join(at);
    std::fs::create_dir_all(&at).unwrap();
    for file in [crate::SCREEN, crate::TOKENS] {
        std::fs::copy(Path::new(FIXTURE).join(file), at.join(file)).unwrap();
    }
    root
}

/// A root of its own named `name`, holding the frozen shop ([`SHOP`]) as
/// its directory `shop`.
fn frozen_shop(name: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let path = entry.unwrap().path();
            let into = to.join(path.file_name().unwrap());
            if path.is_dir() { copy(&path, &into) } else { std::fs::copy(&path, into).map(drop).unwrap() }
        }
    }
    let root = std::env::temp_dir().join(format!("underdraw-window-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy(Path::new(SHOP), &root.join("shop"));
    root
}

/// That the window shows each of `lines`, as [`said`] writes it, its indent
/// aside; a failure names the line missing and all that is shown.
fn shows_lines(gallery: &mut Gallery, lines: &[&str]) {
    let said = said(gallery);
    for line in lines {
        assert!(said.lines().any(|shown| shown.trim() == *line), "{line:?} is not shown in:\n{said}");
    }
}

/// Whether a line the window shows, as [`said`] writes it, reads `line`,
/// its indent aside.
fn shows_line(gallery: &mut Gallery, line: &str) -> bool {
    said(gallery).lines().any(|shown| shown.trim() == line)
}

/// Whether the scrollbar `scrolls` is drawn: its pane overflows.
fn bar_shown(gallery: &Gallery, scrolls: Scrolls) -> bool {
    gallery.panes.bar(scrolls).is_some_and(|bar| bar.thumb().is_some())
}

/// What the window shows, as text, a heading for each part: the tree's
/// lines, the picture shown and what is picked on it, the pins, and the
/// sidebar's cards and what its box holds.
fn said(gallery: &mut Gallery) -> String {
    let mut out = vec!["tree".to_string()];
    out.extend(tree_shown(gallery).into_iter().map(|line| format!("  {line}")));
    out.push("picture".to_string());
    out.extend(picture_shown(gallery).into_iter().map(|line| format!("  {line}")));
    out.push("sidebar".to_string());
    for card in gallery.sidebar_cards() {
        out.push(format!("  {}", card.title));
        out.extend(card.blocks.iter().flat_map(Block::says).map(|says| format!("    {}", says.trim_end())));
    }
    let typed = gallery.sidebar.talk().text();
    if !typed.is_empty() {
        out.push(format!("  box: {typed:?}"));
    }
    out.join("\n") + "\n"
}

/// The tree's lines as drawn, each indented two spaces a level: `▸` before
/// the line picked, and its marks after its name in brackets.
fn tree_shown(gallery: &mut Gallery) -> Vec<String> {
    let (mut lines, mut picked, mut marks) = (Vec::new(), false, Vec::new());
    for draw in gallery.tree_draws() {
        match draw {
            Draw::Box { fill: Some(Ink::Fill), .. } => picked = true,
            Draw::Text { text, font, .. } if font == MARK => marks.push(text),
            Draw::Text { text, rect, .. } => {
                let depth = ((f64::from(rect.x) - 14.0) / INDENT).round() as usize;
                let mark = if picked { "▸ " } else { "" };
                let marked =
                    if marks.is_empty() { String::new() } else { format!("  [{}]", marks.join(", ")) };
                lines.push(format!("{}{mark}{text}{marked}", "  ".repeat(depth)));
                (picked, marks) = (false, Vec::new());
            }
            _ => {}
        }
    }
    lines
}

/// The picture shown, as text: a state's pages by their captions, the node
/// picked and its box in each page, the nodes marked ↗ and → in each, and
/// the pins; an overview's states, its arrows and the journeys marked on
/// it; a flow's words; a description's faults.
fn picture_shown(gallery: &mut Gallery) -> Vec<String> {
    let mut out = Vec::new();
    match &gallery.picture {
        Picture::Pages(shown @ Pages { pages, .. }) => {
            let leading = shown.leading(&gallery.desk());
            for (index, page) in pages.iter().enumerate() {
                out.push(format!("page {}", page.caption));
                if let Some((id, _)) = gallery.node()
                    && let Some(node) = page.nodes.iter().find(|node| node.id == id)
                {
                    let r = node.rect;
                    out.push(format!("  picked {id}: {}, {} · {} × {}", r.x, r.y, r.width, r.height));
                }
                for leads in [Leads::Screen, Leads::State] {
                    let ids: Vec<&str> = leading
                        .iter()
                        .filter(|l| l.page == index && l.leads == leads)
                        .map(|l| l.id.as_str())
                        .collect();
                    if !ids.is_empty() {
                        out.push(format!("  {} {}", leads.badge(), ids.join(", ")));
                    }
                }
            }
            if gallery.playing.is_some() {
                let (step, marked) = played(gallery);
                out.push(format!("journey step {step}, marked {marked:?}"));
            }
            for (id, picture, ink, faint) in pins_shown(gallery) {
                let faint = if faint { ", faint" } else { "" };
                out.push(format!("pin {id} on page {picture}: {ink:?}{faint}"));
            }
        }
        Picture::Overview(_) => {
            let (names, arrows) = overview(gallery);
            out.push(format!("overview {}", names.join(", ")));
            for (index, (from, to, label)) in arrows.into_iter().enumerate() {
                let picked = if gallery.arrow() == Some(index) { "▸ " } else { "" };
                out.push(format!("  {picked}{from} → {to}: {label}"));
            }
            for (state, journeys) in passes_shown(gallery) {
                out.push(format!("  journeys through {state}: {}", journeys.join(", ")));
            }
        }
        Picture::Flow(flow) => {
            out.push("flow".to_string());
            out.extend(flow_texts(flow).into_iter().map(|text| format!("  {text}")));
        }
        Picture::Faults(_) => {
            out.push("faults".to_string());
            let listed = faults_listed(gallery);
            out.extend(listed.into_iter().filter_map(|draw| match draw {
                Draw::Text { text, .. } => Some(format!("  {text}")),
                _ => None,
            }));
        }
    }
    out
}

/// That every text of the sidebar fits the box it is set in, inside the
/// sidebar's gutters: none is cut, in English or in Chinese.
fn fits(gallery: &mut Gallery) {
    let (left, width) = (gallery.panes.pictures().x1, gallery.panes.side_width());
    let (cards, foot) = gallery.placed().expect("the sidebar is shown").draws(true);
    let (inside, outside) = (left + chrome::GUTTER, left + width - chrome::GUTTER);
    for draw in cards.iter().chain(&foot) {
        let Draw::Text { rect, text, font, max_lines, .. } = draw else { continue };
        let (x, right) = (f64::from(rect.x), f64::from(rect.x + rect.width));
        assert!(x >= inside && right <= outside, "{text:?} set outside the sidebar's gutters");
        let wrap = if *max_lines == Some(1) { None } else { Some(rect.width) };
        let set = gallery.typesetter.text(text, *font, wrap);
        assert!(
            set.width <= rect.width + 0.5,
            "{text:?} is wider than its box: {} > {}",
            set.width,
            rect.width
        );
        assert!(
            set.height <= rect.height + 0.5,
            "{text:?} is taller than its box: {} > {}",
            set.height,
            rect.height
        );
    }
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
    let mut gallery = acted(&root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        say_on(gallery, 0, "order-0", "rename it");
    });
    shows_lines(
        &mut gallery,
        &[
            "root/",
            "→ cancel-an-order",
            "broken  [2 faults]",
            "components/",
            "orders  [1 note]",
            "▸ default  [1 note]",
        ],
    );
    let line = |name: &str| gallery.lines.iter().find(|l| l.row.name == name).unwrap();
    assert!(!line("broken").row.faults.is_empty(), "a description with faults");
    assert!(line("cancel-an-order").journey, "a journey");
    assert!(gallery.lines.iter().any(|l| matches!(l.state, Some(State::Phase(_)))), "a phase");
    let Line { row, state, .. } = line("orders");
    assert_eq!(gallery.waiting_notes(row, state.as_ref()), 1, "its note counted");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_state_at_both_widths_with_its_labels() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| show(gallery, "airbnb-listing", "default"));
    assert_eq!(captions(&gallery), ["default · 390", "default · 1120"]);
    shows_lines(&mut gallery, &["default · the whole state"]);
    assert!(tags_drawn(&mut gallery) > 0, "its labels drawn");
}

#[test]
fn a_state_and_an_overview_with_their_marks_hidden() {
    let root = fixture("frame-bare", "airbnb-listing");
    let mut gallery = acted(&root, WINDOW, |gallery| show(gallery, "airbnb-listing", "default"));
    assert!(tags_drawn(&mut gallery) > 0 && leads_drawn(&gallery) > 0, "labels and marks, before");
    gallery.use_tool(Tool::Marks);
    assert_eq!(tags_drawn(&mut gallery), 0, "no label, no \"?\"");
    assert_eq!(leads_drawn(&gallery), 0, "no purple mark");
    std::fs::remove_dir_all(root).unwrap();

    let root = frozen_shop("frame-overview-bare");
    let order = root.join("shop").join("order");
    let mut gallery = acted(&root, WINDOW, |gallery| {
        let line = gallery.lines.iter().position(|line| line.row.dir == order && line.state.is_none());
        showing(gallery, line.unwrap());
    });
    assert!(passes_drawn(&mut gallery), "the journey's marks, before");
    gallery.use_tool(Tool::Marks);
    assert!(!passes_drawn(&mut gallery), "no journey's mark");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_overview_with_its_arrows_and_journey_marks() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| {
        let order = description_in(gallery, "shop/order");
        showing(gallery, order);
    });
    shows_lines(
        &mut gallery,
        &[
            "open → cancelled: 1 · ok",
            "journeys through open: → cancel-an-order · step 2",
            "ShopOrder · the whole screen",
        ],
    );
    assert!(
        matches!(&gallery.picture, Picture::Overview(overview) if !overview.arrows.is_empty()),
        "its transitions"
    );
    assert_eq!(passes_shown(&gallery).len(), 2, "the journey's marks");
}

#[test]
fn the_inbox_of_the_root_grouped_by_description() {
    let root = root_of("frame-inbox", &[("orders", "orders"), ("checkout", "pay/checkout")]);
    let dir = root.join("orders");
    let whole =
        Reference { description: dir.clone(), on: On::State { state: State::Fixture("default".into()) } };
    notes::say(&whole, "too dense").unwrap();
    notes::say(&whole, "a bigger title").unwrap();
    notes::answer(&dir, "N1", "the title is a heading now").unwrap();
    let mut gallery = acted(&root, WINDOW, |gallery| gallery.pick(0));
    let card = gallery.inbox_card().expect("the inbox of everything under the root");
    assert_eq!(card.title, "waits on you · 6");
    shows_lines(&mut gallery, &["root · these screens", "? nav-export in default"]);
    fits(&mut gallery);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        gallery.lang = Lang::Zh;
        gallery.pick(0);
    });
    assert_eq!(gallery.inbox_card().unwrap().title, "等你处理 · 6");
    shows_lines(&mut gallery, &["root · 这些屏幕", "? nav-export（default）"]);
    fits(&mut gallery);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_flow_with_a_screen_not_drawn_yet() {
    let root = root_of("frame-flow", &[("airbnb-listing", "shop/listing"), ("checkout", "shop/checkout")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        let shop = gallery.lines.iter().position(|line| line.row.name == "shop").unwrap();
        showing(gallery, shop);
    });
    shows_lines(&mut gallery, &["flow", "checkout", "listing", "AddressBook", "shop · these screens"]);
    let flow = flow_shown(&gallery);
    assert!(flow.map.places.iter().any(|place| matches!(place.screen, Screen::Undrawn(_))));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_description_with_faults() {
    let root = root_of("frame-faults", &[]);
    std::fs::create_dir_all(root.join("broken")).unwrap();
    std::fs::write(root.join("broken").join(crate::SCREEN), "{}").unwrap();
    let mut gallery = acted(&root, WINDOW, |gallery| gallery.pick(1));
    shows_lines(
        &mut gallery,
        &[
            "▸ broken  [2 faults]",
            "broken has 2 faults, so it is not drawn: `underdraw check` it",
            "the screen needs a root node",
        ],
    );
    assert!(
        matches!(&gallery.picture, Picture::Faults(Faults { findings: faults, .. }) if !faults.is_empty())
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_journey_playing() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| {
        let journey = journey_of(gallery, "cancel-an-order");
        showing(gallery, journey);
        press_keys(gallery, &["→"]);
    });
    fits(&mut gallery);
    assert_eq!(played(&gallery).0, 2, "its second step, its node marked");
    shows_lines(
        &mut gallery,
        &[
            "journey cancel-an-order · step 2 of 3 · shop/order · open",
            "acts on summary-cancel (label 1) · tap, answered ok (marked)",
            "← previous step",
            "→ next step",
        ],
    );
}

#[test]
fn a_prototype_act_and_its_outcomes() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| {
        show(gallery, "orders", "cancel-ready");
        act_on(gallery, "cd-confirm");
    });
    fits(&mut gallery);
    assert!(panel_has(&gallery, "→ default · ok") && panel_has(&gallery, "→ cancel-ready · shipped"));
    shows_lines(&mut gallery, &["cd-confirm.tap in cancel-ready calls cancelOrder: click how it answered"]);
}

#[test]
fn the_whole_state_talked_about_with_more_typed_in_the_box() {
    let root = root_of("frame-talk", &[("orders", "orders")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        type_text(gallery, "the list is too dense");
        press_keys(gallery, &["Enter"]);
        notes::answer(
            &root.join("orders"),
            "N1",
            "gave each row more room; should the filters fold away too?",
        )
        .unwrap();
        gallery.reload();
        type_text(gallery, "yes, behind a button");
        press_keys(gallery, &["⇧Enter"]);
        type_text(gallery, "named Filters");
    });
    fits(&mut gallery);
    assert_eq!(talk_shown(&gallery)[0], "note N1 · answered");
    assert_eq!(gallery.foot().map(|foot| foot.does), Some(Some(Do::Send)), "Send can send what is typed");
    shows_lines(&mut gallery, &["pin N1 on page 0: Line", "box: \"yes, behind a button\\nnamed Filters\""]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_arrow_picked_in_the_overview() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| {
        let orders = line_of(gallery, "orders");
        showing(gallery, orders);
        gallery.press_at(on_arrow(gallery, 0));
    });
    assert_eq!(gallery.picked, Picked::Arrow(0));
    shows_lines(
        &mut gallery,
        &[
            "▸ default → cancel-dialog: 10",
            "transition from default on order-{index}-cancel.tap to cancel-dialog",
        ],
    );
}

#[test]
fn note_pins_open_and_answered_with_its_reply() {
    let root = root_of("frame-notes", &[("orders", "orders")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        say_on(gallery, 0, "nav-export", "rename it");
        say_on(gallery, 0, "order-0-cancel", "too red");
        notes::answer(&root.join("orders"), "N1", "renamed it Export").unwrap();
        gallery.reload();
        gallery.move_to(on_pin(gallery, "N1"));
    });
    let inks: Vec<Ink> = gallery.pins().into_iter().filter(|pin| !pin.faint).map(|pin| pin.ink).collect();
    assert_eq!(inks, [Ink::Answered, Ink::Note]);
    assert!(!gallery.reply_shown().is_empty(), "the reply shown");
    shows_lines(&mut gallery, &["orders  [1 note]", "note N2 · waiting on the agent", "too red"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn where_each_note_stands_marked_on_the_pictures_and_counted_in_the_sidebar() {
    let root = root_of("frame-stands", &[("orders", "orders")]);
    let dir = root.join("orders");
    let mut gallery = acted(&root, WINDOW, |gallery| {
        gallery.time = Some(1_000_023);
        show(gallery, "orders", "default");
        for (id, says) in [("nav-export", "rename it"), ("search", "wider"), ("load-more", "say how many")] {
            say_on(gallery, 0, id, says);
        }
        // N1 and N2 are answered, N3 taken; then the person reads N2's
        // answer, and says more in N4, which waits on the agent.
        notes::hand_over(&dir, 1_000_000).unwrap();
        notes::answer(&dir, "N1", "renamed it Export").unwrap();
        notes::answer(&dir, "N2", "made it wider").unwrap();
        notes::seen(&dir, &["N2".to_string()]).unwrap();
        gallery.reload();
        say_on(gallery, 0, "order-0-cancel", "too red");
        gallery.press_at(on_pin(gallery, "N3"));
        gallery.move_to((0.0, 0.0));
    });
    fits(&mut gallery);
    let inks: Vec<(String, Ink)> =
        gallery.pins().into_iter().filter(|pin| !pin.faint).map(|pin| (pin.id, pin.ink)).collect();
    assert_eq!(
        inks,
        [
            ("N1".to_string(), Ink::Answered),
            ("N2".to_string(), Ink::Line),
            ("N3".to_string(), Ink::Note),
            ("N4".to_string(), Ink::Note),
        ],
        "unread, read, taken and waiting"
    );
    assert_eq!(
        talk_shown(&gallery),
        ["note N3 · waiting on the agent", "say how many", "the agent is changing it (23s)"]
    );
    shows_lines(&mut gallery, &["orders  [2 notes]", "▸ default  [2 notes]"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_assumption_picked_by_its_question_mark() {
    let root = root_of("frame-assumption", &[("checkout", "checkout")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        let checkout = line_of(gallery, "checkout");
        let mut offset = 1;
        loop {
            showing(gallery, checkout + offset);
            if matches!(&gallery.picture, Picture::Pages(Pages { pages, .. }) if pages[0].has("card-number"))
            {
                break;
            }
            offset += 1;
        }
        // Scrolled to it, as a person scrolls before clicking.
        gallery.move_to((gallery.panes.pictures().x0 + MARGIN, MARGIN));
        gallery.wheel_by(0.0, on_guess(gallery, 0, "card-number").1 - WINDOW.1 / 3.0);
        gallery.press_at(on_guess(gallery, 0, "card-number"));
    });
    fits(&mut gallery);
    assert_eq!(picked_node(&gallery), Some("card-number"));
    shows_lines(
        &mut gallery,
        &["assumed, for you to answer", "Card fields are drawn here", "✓ right, keep it", "✗ wrong…"],
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_note_read_as_a_conversation() {
    let root = root_of("frame-conversation", &[("orders", "orders")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        say_on(gallery, 0, "nav-export", "rename it, and say what it exports");
        notes::answer(&root.join("orders"), "N1", "renamed it Export orders").unwrap();
        gallery.reload();
        gallery.press_at(on_pin(gallery, "N1"));
        // The pointer moved off the pin: its reply is not drawn over the
        // conversation read in the sidebar.
        gallery.move_to((0.0, 0.0));
    });
    fits(&mut gallery);
    assert_eq!(
        talk_shown(&gallery),
        ["note N1 · answered", "rename it, and say what it exports", "renamed it Export orders"]
    );
    assert!(!gallery.panel().iter().any(|line| line == "not yet…" || line == "close"), "never closed");
    assert!(gallery.reply_shown().is_empty(), "no reply over the pictures");
    shows_lines(&mut gallery, &["pin N1 on page 0: Line"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_wrong_assumption_written_with_an_input_method() {
    let root = root_of("frame-rejecting", &[("orders", "orders")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        gallery.press_at(on_guess(gallery, 0, "nav-export"));
        assert!(click_panel_line(gallery, "✗ wrong…"));
        type_text(gallery, "导出是");
        gallery.compose(Ime::Preedit("dui hua kuang".to_string(), None));
    });
    fits(&mut gallery);
    assert!(gallery.rejecting.is_some() && gallery.sidebar.talk().is_composing());
    shows_lines(
        &mut gallery,
        &[
            "✗: write what is right in the box below and send it; not sent empty · Esc takes the ✗ back",
            "box: \"导出是\"",
        ],
    );
    assert!(!shows_line(&mut gallery, "✓ right, keep it"), "the choice made");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_confirmation_waiting_to_be_undone() {
    let root = root_of("frame-confirming", &[("orders", "orders")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        show(gallery, "orders", "default");
        gallery.press_at(on_guess(gallery, 0, "nav-export"));
        assert!(click_panel_line(gallery, "✓ right, keep it"));
    });
    fits(&mut gallery);
    assert!(gallery.confirming.is_some());
    shows_lines(
        &mut gallery,
        &["✓ right on nav-export in default at 390", "written to notes.json in 5s, for the agent", "undo"],
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_shared_component_in_the_components_section() {
    let root = shop("frame-components");
    let mut gallery = acted(&root, WINDOW, |gallery| show(gallery, "OrderRow", "pending"));
    assert_eq!(captions(&gallery), ["pending · 390", "pending · 1120"]);
    shows_lines(&mut gallery, &["components/", "▸ pending", "pending · the whole state"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_node_picked_its_card_in_the_sidebar_with_its_details_unfolded() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.press_at(on_node(gallery, 0, "nav-save"));
        gallery.details = true;
    });
    fits(&mut gallery);
    assert!(panel_has(&gallery, "id  nav-save"));
    shows_lines(
        &mut gallery,
        &["IconButton", "calls  toggleSaved", "▾ details", "at 390  label 3 · box 334, 8 · 40 × 40"],
    );
}

#[test]
fn the_sidebar_hidden_the_pictures_take_its_width() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.chord("\\");
    });
    assert!(gallery.panes.side().is_none(), "hidden");
    assert_eq!(gallery.panes.pictures().x1, WINDOW.0);
    assert!(!bar_shown(&gallery, Scrolls::Side), "nor its scrollbar");
    shows_lines(&mut gallery, &["page default · 390", "page default · 1120"]);
}

#[test]
fn the_sidebar_dragged_narrow_its_cards_rewrapped() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.press_at(on_node(gallery, 1, "nav-save"));
        gallery.let_go();
        gallery.press_at((gallery.panes.pictures().x1, MARGIN));
        gallery.move_to((WINDOW.0, MARGIN));
        gallery.let_go();
    });
    assert_eq!(gallery.panes.side_width(), SIDEBAR_LEAST);
    fits(&mut gallery);
    shows_lines(&mut gallery, &["calls  toggleSaved"]);
}

#[test]
fn the_window_in_chinese() {
    let root = root_of("frame-zh", &[("checkout", "checkout")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        gallery.lang = Lang::Zh;
        let checkout = line_of(gallery, "checkout");
        showing(gallery, checkout + 1);
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("no picture") };
        let guessed = pages[0].nodes.iter().find(|node| node.assumed.is_some()).unwrap().id.clone();
        gallery.press_at(on_node(gallery, 0, &guessed));
    });
    fits(&mut gallery);
    assert!(panel_has(&gallery, "▸ 详情"));
    shows_lines(&mut gallery, &["假定，等你回答", "✓ 对，保留", "↗ AddressBook · 还没画"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_window_in_japanese() {
    let root = root_of("frame-ja", &[("checkout", "checkout")]);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        gallery.speak(Lang::Ja).unwrap();
        let checkout = line_of(gallery, "checkout");
        showing(gallery, checkout + 1);
        let Picture::Pages(Pages { pages, .. }) = &gallery.picture else { panic!("no picture") };
        let guessed = pages[0].nodes.iter().find(|node| node.assumed.is_some()).unwrap().id.clone();
        gallery.press_at(on_node(gallery, 0, &guessed));
    });
    fits(&mut gallery);
    assert!(panel_has(&gallery, "▸ 詳細"));
    shows_lines(
        &mut gallery,
        &["推測、あなたの回答待ち", "✓ 正しい、このまま", "↗ AddressBook · まだ描かれていません"],
    );
    std::fs::remove_dir_all(root).unwrap();
}

// The look, from the window's state, over a fixture root of its own.

#[test]
fn the_window_its_pictures_at_half_size_and_scrollbars_shown_while_needed() {
    let root = fixture("frame-window", "airbnb-listing");
    let mut gallery = acted(&root, WINDOW, |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.press_at(on_node(gallery, 0, "nav-save"));
        gallery.details = true;
    });
    fits(&mut gallery);
    assert_eq!(picked_node(&gallery), Some("nav-save"));
    assert_eq!(zoom(&gallery), SMALLEST, "both pages at half size");
    assert!(bar_shown(&gallery, Scrolls::Down), "the listing taller than the pane");
    assert!(bar_shown(&gallery, Scrolls::Across), "both pages wider than the pane");
    assert!(!bar_shown(&gallery, Scrolls::Tree), "the tree fits");
    assert!(!bar_shown(&gallery, Scrolls::Side), "the sidebar fits");
    shows_lines(
        &mut gallery,
        &["airbnb-listing", "▸ default", "picked nav-save: 334, 8 · 40 × 40", "id  nav-save"],
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_narrow_window_at_half_size_scrolled_across() {
    let root = fixture("frame-narrow", "airbnb-listing");
    let gallery = acted(&root, NARROW, |gallery| {
        show(gallery, "airbnb-listing", "default");
        gallery.move_to((gallery.panes.pictures().x0 + MARGIN, MARGIN));
        gallery.wheel_by(1e6, 0.0);
        gallery.press_at(on_node(gallery, 1, "nav-save"));
    });
    assert_eq!(zoom(&gallery), SMALLEST);
    assert!(bar_shown(&gallery, Scrolls::Across), "wider than the pane");
    let bar = gallery.panes.bar(Scrolls::Across).unwrap();
    assert_eq!(gallery.panes.scrolled(Scrolls::Across), bar.content - bar.pane.width(), "as far as its end");
    assert_eq!(picked_node(&gallery), Some("nav-save"), "picked, scaled and scrolled");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_tree_dragged_narrow_its_names_cut_at_its_edge() {
    let listing = "a-listing-whose-name-runs-past-the-narrowest-tree";
    let root = fixture("frame-tree-narrow", listing);
    let mut gallery = acted(&root, WINDOW, |gallery| {
        show(gallery, listing, "default");
        gallery.press_at((gallery.panes.pictures().x0, MARGIN));
        gallery.move_to((0.0, MARGIN));
        gallery.let_go();
    });
    let tree = gallery.panes.pictures().x0;
    assert_eq!(tree, TREE_LEAST);
    let draws = gallery.tree_draws();
    let Some(Draw::Text { rect, font, .. }) =
        draws.iter().find(|draw| matches!(draw, Draw::Text { text, .. } if text == listing))
    else {
        panic!("{listing} is not in the tree")
    };
    assert!(f64::from(rect.x + rect.width) < tree, "its box ends inside the tree");
    let set = gallery.typesetter.text(listing, *font, None);
    assert!(set.width > rect.width, "its name runs past its box, cut at its edge");
    std::fs::remove_dir_all(root).unwrap();
}

// The journey's every surface, over the frozen shop.

/// The gallery playing the shop's journey to its step `step`, from 1.
fn playing_to(gallery: &mut Gallery, step: usize) {
    let journey = journey_of(gallery, "cancel-an-order");
    showing(gallery, journey);
    for _ in 1..step {
        press_keys(gallery, &["→"]);
    }
    assert_eq!(played(gallery).0, step);
}

#[test]
fn the_journey_in_the_tree_nothing_picked() {
    let root = frozen_shop("frame-journey-tree");
    let mut gallery = acted(&root, WINDOW, |_| {});
    assert!(gallery.line.is_none() && gallery.lines.iter().any(|line| line.journey));
    shows_lines(
        &mut gallery,
        &["→ cancel-an-order", "→ refund-a-cancelled-order  [1 fault]", "Nothing picked"],
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_journey_played_at_its_first_middle_and_last_steps() {
    let root = frozen_shop("frame-journey-steps");
    let steps = [
        (
            1,
            "journey cancel-an-order · step 1 of 3 · shop/orders · default",
            "acts on order-{index} (label 1) · tap (marked)",
        ),
        (
            2,
            "journey cancel-an-order · step 2 of 3 · shop/order · open",
            "acts on summary-cancel (label 1) · tap, answered ok (marked)",
        ),
        (3, "journey cancel-an-order · step 3 of 3 · shop/order · cancelled", "the journey ends here"),
    ];
    for (step, heading, acts) in steps {
        let mut gallery = acted(&root, WINDOW, |gallery| playing_to(gallery, step));
        fits(&mut gallery);
        assert_eq!(zoom(&gallery), SMALLEST, "step {step} at half size");
        shows_lines(&mut gallery, &["▸ → cancel-an-order", heading, acts]);
        assert_eq!(shows_line(&mut gallery, "← previous step"), step > 1, "step {step}");
        assert_eq!(shows_line(&mut gallery, "→ next step"), step < 3, "step {step}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_step_of_the_journey_with_a_node_picked() {
    let root = frozen_shop("frame-journey-node");
    let mut gallery = acted(&root, WINDOW, |gallery| {
        playing_to(gallery, 2);
        gallery.press_at(on_node(gallery, 0, "summary-total"));
    });
    fits(&mut gallery);
    assert_eq!(picked_node(&gallery), Some("summary-total"));
    assert_eq!(played(&gallery).0, 2, "still played");
    shows_lines(&mut gallery, &["journey cancel-an-order · step 2 of 3 · shop/order · open", "Text"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_overview_with_the_journey_marked_on_its_states() {
    let root = frozen_shop("frame-journey-overview");
    let order = root.join("shop").join("order");
    let mut gallery = acted(&root, WINDOW, |gallery| {
        let line = gallery.lines.iter().position(|line| line.row.dir == order && line.state.is_none());
        showing(gallery, line.unwrap());
    });
    assert_eq!(passes_shown(&gallery).len(), 2, "the journey's marks");
    assert!(passes_drawn(&mut gallery), "drawn");
    shows_lines(&mut gallery, &["journeys through open: → cancel-an-order · step 2"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_broken_journey_picked_its_faults_shown() {
    let root = frozen_shop("frame-journey-broken");
    let mut gallery = acted(&root, WINDOW, |gallery| {
        let journey = journey_of(gallery, "refund-a-cancelled-order");
        showing(gallery, journey);
    });
    assert!(gallery.playing.is_none());
    assert!(matches!(&gallery.picture, Picture::Faults(Faults { findings, .. }) if !findings.is_empty()));
    shows_lines(
        &mut gallery,
        &["refunded is not a state of order: name a fixture or a phase (open, shipped, cancelled)"],
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_journey_playing_in_a_narrow_window() {
    let root = frozen_shop("frame-journey-narrow");
    let mut gallery = acted(&root, NARROW, |gallery| playing_to(gallery, 2));
    fits(&mut gallery);
    assert_eq!(zoom(&gallery), SMALLEST);
    assert!(bar_shown(&gallery, Scrolls::Across), "the pages wider than the narrow pane");
    shows_lines(&mut gallery, &["journey cancel-an-order · step 2 of 3 · shop/order · open"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_node_picked_offers_where_acting_on_it_goes_in_buttons_floating_beside_it() {
    let mut gallery = acted(drafts(), WINDOW, |gallery| {
        show(gallery, "orders", "cancel-ready");
        gallery.press_at(on_node(gallery, 0, "cd-confirm"));
    });
    fits(&mut gallery);
    assert_eq!(picked_node(&gallery), Some("cd-confirm"));
    assert_eq!(gallery.seen().offers.len(), 2, "one for each outcome");
}
