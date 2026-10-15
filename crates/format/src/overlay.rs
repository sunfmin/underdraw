//! Overlays: where a node in an `overlay` slot floats over its node. An
//! anchor is read and checked here and drawn by underdraw-layout, so what
//! each anchor means is said once.

use serde_json::{Map, Value};

use crate::check::Findings;

/// Where along one axis a floating box sits on what it floats over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    Start,
    Center,
    End,
}

impl Edge {
    /// Every edge, by the name a look's `pin` gives it.
    pub const ALL: [Edge; 3] = [Edge::Start, Edge::Center, Edge::End];

    pub fn key(self) -> &'static str {
        match self {
            Edge::Start => "start",
            Edge::Center => "center",
            Edge::End => "end",
        }
    }

    pub fn parse(name: &str) -> Option<Edge> {
        Edge::ALL.into_iter().find(|edge| edge.key() == name)
    }
}

/// Where an overlay floats over its node: a corner, the middle, or along
/// the bottom edge from side to side (a sheet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    BottomTrailing,
    TopLeading,
    TopTrailing,
    BottomLeading,
    Center,
    Bottom,
}

/// Where an anchor puts its overlay: each axis at an edge, and whether it
/// lies flush along the bottom, as wide as its node, or stands in from the
/// edges it sits on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: Edge,
    pub y: Edge,
    pub flush: bool,
}

impl Anchor {
    /// Every anchor, the default first.
    pub const ALL: [Anchor; 6] = [
        Anchor::BottomTrailing,
        Anchor::TopLeading,
        Anchor::TopTrailing,
        Anchor::BottomLeading,
        Anchor::Center,
        Anchor::Bottom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Anchor::BottomTrailing => "bottom-trailing",
            Anchor::TopLeading => "top-leading",
            Anchor::TopTrailing => "top-trailing",
            Anchor::BottomLeading => "bottom-leading",
            Anchor::Center => "center",
            Anchor::Bottom => "bottom",
        }
    }

    pub fn parse(name: &str) -> Option<Anchor> {
        Anchor::ALL.into_iter().find(|anchor| anchor.key() == name)
    }

    /// The anchor of an overlay whose props (responsive values already
    /// chosen) are `props`: the default when it names none.
    pub fn of(props: &Map<String, Value>) -> Anchor {
        props.get("anchor").and_then(Value::as_str).and_then(Anchor::parse).unwrap_or(Anchor::BottomTrailing)
    }

    /// Where it puts its overlay.
    pub fn placement(self) -> Placement {
        let (x, y) = match self {
            Anchor::BottomTrailing => (Edge::End, Edge::End),
            Anchor::TopLeading => (Edge::Start, Edge::Start),
            Anchor::TopTrailing => (Edge::End, Edge::Start),
            Anchor::BottomLeading => (Edge::Start, Edge::End),
            Anchor::Center => (Edge::Center, Edge::Center),
            Anchor::Bottom => (Edge::Start, Edge::End),
        };
        Placement { x, y, flush: self == Anchor::Bottom }
    }

    /// A fault at `at` when `anchor` is not an anchor.
    pub(crate) fn fault(anchor: &Value, at: &str, findings: &mut Findings) {
        if anchor.as_str().and_then(Anchor::parse).is_none() {
            let anchor = anchor.as_str().map_or_else(|| anchor.to_string(), str::to_string);
            let all = Anchor::ALL.map(Anchor::key).join(", ");
            findings.add(at, &format!("{anchor} is not an anchor: write {all}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_anchor_is_read_back_from_its_name() {
        for anchor in Anchor::ALL {
            assert_eq!(Anchor::parse(anchor.key()), Some(anchor));
        }
        assert_eq!(Anchor::parse("middle"), None);
    }

    #[test]
    fn only_the_sheet_is_flush() {
        let flush: Vec<Anchor> = Anchor::ALL.into_iter().filter(|a| a.placement().flush).collect();
        assert_eq!(flush, [Anchor::Bottom]);
        let sheet = Anchor::Bottom.placement();
        assert_eq!((sheet.x, sheet.y), (Edge::Start, Edge::End));
    }

    #[test]
    fn an_overlay_naming_no_anchor_sits_bottom_trailing() {
        assert_eq!(Anchor::of(&Map::new()), Anchor::BottomTrailing);
    }
}
