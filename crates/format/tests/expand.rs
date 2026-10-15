//! What a node means inside an instance of a component, said once for
//! check, instantiation and handoff: ids joined to the instance's, raw and
//! as written; a slot read where the instance is; the paths the params are
//! given; and a component that uses itself refused.

use serde_json::{Map, Value, json};
use underdraw_format::{Instances, Met, Screen, Shared};

fn screen() -> Value {
    json!({
        "components": {
            "Card": { "params": { "order": "string", "open": "bool" }, "nodes": { "component": "Box", "children": [
                { "id": "badge", "component": "Badge", "with": { "text": "{order.number}" } },
                { "slot": "children" }
            ] } },
            "Badge": { "params": { "text": "string" }, "nodes": { "component": "Text", "text": "{text}" } },
            "Loop": { "params": {}, "nodes": { "component": "Box", "children": [ { "id": "again", "component": "Loop" } ] } }
        }
    })
}

fn node(value: &Value) -> &Map<String, Value> {
    value.as_object().unwrap()
}

#[test]
fn a_node_inside_an_instance_is_named_by_the_instance_joined_to_its_own() {
    let screen = screen();
    let screen = Screen::read(&screen, Shared::default());
    let subtrees = screen.components.subtrees();
    let mut instances: Instances<&str> = Instances::new(subtrees);
    assert_eq!(instances.ids("row-{index}", "row-2"), ("row-2".to_string(), "row-{index}".to_string()));
    let card = json!({ "id": "row-{index}", "component": "Card", "with": { "order": "{item}", "open": "{menuOpen}" } });
    let Met::Instance(component) = instances.met(node(&card)) else { panic!("an instance") };
    instances.enter(component, node(&card), "row-2".into(), "row-{index}".into(), "screen").unwrap();
    let badge = &component.nodes["children"][0];
    assert_eq!(instances.ids("badge", "badge"), ("row-2-badge".to_string(), "row-{index}-badge".to_string()));
    let Met::Instance(inner) = instances.met(node(badge)) else { panic!("an instance inside one") };
    instances.enter(inner, node(badge), "row-2-badge".into(), "row-{index}-badge".into(), "card").unwrap();
    assert_eq!(
        instances.ids("text", "text"),
        ("row-2-badge-text".to_string(), "row-{index}-badge-text".to_string())
    );
    assert_eq!(
        instances.through("text"),
        "item.number",
        "a param given a path stands for it, through each instance"
    );
    instances.leave();
    assert_eq!(instances.through("open"), "menuOpen");
    assert_eq!(instances.through("item"), "item", "a path that is no param is the instance's own");
}

#[test]
fn a_slot_is_what_the_instance_gives_read_where_the_instance_is() {
    let screen = screen();
    let screen = Screen::read(&screen, Shared::default());
    let subtrees = screen.components.subtrees();
    let mut instances: Instances<&str> = Instances::new(subtrees);
    let card = json!({ "id": "row", "component": "Card", "with": { "order": "{item}" },
                       "children": { "id": "title", "component": "Text" } });
    let Met::Instance(component) = instances.met(node(&card)) else { panic!("an instance") };
    instances.enter(component, node(&card), "row".into(), "row".into(), "the screen's scope").unwrap();
    let Met::Slot(key) = instances.met(node(&component.nodes["children"][1])) else { panic!("a slot") };
    let frame = instances.leave().expect("inside the card");
    assert_eq!(frame.at, "the screen's scope", "what it gives is read where the instance is");
    assert_eq!(frame.given(&key), Some(&card["children"]));
    assert_eq!(instances.ids("title", "title").0, "title", "under its own id, outside the instance");
    instances.resume(frame);
    assert_eq!(instances.ids("badge", "badge").0, "row-badge", "and back inside after");
}

#[test]
fn a_slot_outside_any_instance_has_nothing_to_stand_for() {
    let screen = screen();
    let screen = Screen::read(&screen, Shared::default());
    let subtrees = screen.components.subtrees();
    let mut instances: Instances<()> = Instances::new(subtrees);
    assert!(matches!(instances.met(node(&json!({ "slot": "children" }))), Met::Slot(_)));
    assert!(instances.leave().is_none());
    assert!(matches!(instances.met(node(&json!({ "id": "a", "component": "Text" }))), Met::Node));
}

#[test]
fn a_component_that_uses_itself_is_refused() {
    let screen = screen();
    let screen = Screen::read(&screen, Shared::default());
    let subtrees = screen.components.subtrees();
    let mut instances: Instances<()> = Instances::new(subtrees);
    let outer = json!({ "id": "loop", "component": "Loop" });
    let Met::Instance(component) = instances.met(node(&outer)) else { panic!("an instance") };
    instances.enter(component, node(&outer), "loop".into(), "loop".into(), ()).unwrap();
    let again = &component.nodes["children"][0];
    let Met::Instance(inner) = instances.met(node(again)) else { panic!("an instance") };
    assert_eq!(
        instances.enter(inner, node(again), "loop-again".into(), "loop-again".into(), ()),
        Err("component Loop uses itself: Loop → Loop".to_string())
    );
    assert_eq!(instances.ids("x", "x").0, "loop-x", "and not walked into");
}
