//! What acting on a node does in the gallery's click-through prototype, as
//! the description says it (docs/adr/0018): a transition declared from the
//! state shown on the node's event goes to its state; a `navigate` goes to
//! another screen and `back` to where the person came from. Where nothing
//! is declared, the act says so rather than doing nothing.

use serde_json::{Map, Value};

/// The event a click raises on a node: `tap`, or the node's first event
/// when it has no `tap`.
const CLICK: &str = "tap";

/// What acting on a node comes to.
#[derive(Debug, PartialEq)]
pub enum Act {
    /// The screen is then in state `to`.
    Go { to: String },
    /// The event calls an operation and more than one of its outcomes is
    /// declared: the person chooses which it answered, each `(outcome, to)`
    /// in the order written.
    Choose(Vec<(String, String)>),
    /// The event goes to the screen named, by its "screen".
    Navigate(String),
    /// The event goes back to where the person came from.
    Back,
    /// No transition is declared from the state on the event.
    Nothing,
}

/// The event a click raises on a node whose handlers are `events`.
pub fn event(events: &Map<String, Value>) -> &str {
    if events.contains_key(CLICK) {
        return CLICK;
    }
    events.keys().next().map_or(CLICK, String::as_str)
}

/// What acting on a node does in state `state` of `screen`: the node by its
/// id as written (`written`), its handlers with their cases decided
/// (`events`), and the event a click raises on it (see [`event`]).
pub fn act(screen: &Value, state: &str, written: &str, events: &Map<String, Value>) -> Act {
    let event = event(events);
    if let Some(handler) = events.get(event) {
        if let Some(Value::String(to)) = handler.get("navigate") {
            return Act::Navigate(to.clone());
        }
        if handler.get("back").is_some() {
            return Act::Back;
        }
    }
    let declared: Vec<(Option<String>, String)> = underdraw_format::transitions(screen)
        .into_iter()
        .filter(|t| t.from == state && t.node == written && t.event == event)
        .map(|t| (t.outcome, t.to))
        .collect();
    match declared.as_slice() {
        [] => Act::Nothing,
        [(_, to)] => Act::Go { to: to.clone() },
        _ => {
            Act::Choose(declared.into_iter().map(|(outcome, to)| (outcome.unwrap_or_default(), to)).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn events(on: Value) -> Map<String, Value> {
        on.as_object().cloned().unwrap()
    }

    fn screen() -> Value {
        json!({ "transitions": [
            { "from": "list", "on": "row-{index}-cancel.tap", "to": "dialog" },
            { "from": "dialog", "on": "confirm.tap", "outcome": "ok", "to": "list" },
            { "from": "dialog", "on": "confirm.tap", "outcome": "shipped", "to": "dialog" },
            { "from": "list", "on": "field.change", "to": "typing" },
        ] })
    }

    #[test]
    fn a_declared_transition_goes_to_its_state() {
        let on = events(json!({ "tap": { "open": "dialog" } }));
        assert_eq!(act(&screen(), "list", "row-{index}-cancel", &on), Act::Go { to: "dialog".into() });
    }

    #[test]
    fn several_outcomes_are_chosen_from() {
        let on = events(json!({ "tap": { "call": "cancelOrder" } }));
        assert_eq!(
            act(&screen(), "dialog", "confirm", &on),
            Act::Choose(vec![("ok".into(), "list".into()), ("shipped".into(), "dialog".into())])
        );
    }

    #[test]
    fn only_from_the_state_shown() {
        let on = events(json!({ "tap": { "call": "cancelOrder" } }));
        assert_eq!(act(&screen(), "list", "confirm", &on), Act::Nothing);
        assert_eq!(act(&screen(), "dialog", "other", &on), Act::Nothing, "nor another node");
    }

    #[test]
    fn navigate_and_back_leave_the_screen() {
        let on = events(json!({ "tap": { "navigate": "OrderDetail", "with": { "id": "1" } } }));
        assert_eq!(act(&screen(), "list", "row", &on), Act::Navigate("OrderDetail".into()));
        let on = events(json!({ "tap": { "back": true } }));
        assert_eq!(act(&screen(), "list", "nav-back", &on), Act::Back);
    }

    #[test]
    fn a_node_without_tap_raises_its_first_event() {
        let on = events(json!({ "change": { "set": "text" } }));
        assert_eq!(event(&on), "change");
        assert_eq!(act(&screen(), "list", "field", &on), Act::Go { to: "typing".into() });
        assert_eq!(event(&Map::new()), "tap", "a node with no handler at all");
    }
}
