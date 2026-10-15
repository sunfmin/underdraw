//! A reference to what the person picked in the gallery, naming it as the
//! description writes it so that an agent can act on it without asking
//! where: a node in a state, a whole state, a transition's arrow in a
//! description's overview, or a navigate's arrow in a directory's flow.
//! A whole description, its overview picked, is a reference too: the
//! whole screen. What ⌘C copies, as one line, and what a note is pinned to.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use underdraw_format::{Lang, say};

use crate::draw::{Origin, Shown};

/// What is picked in one description: the directory it is in, and what in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    /// The directory the description is in; for a navigate, the one it is
    /// written in.
    pub description: PathBuf,
    pub on: On,
}

/// What in a description a reference names. Written, in notes.json, as the
/// fields of the one it is: `{"state", "node"}`, `{"state"}`,
/// `{"transition"}`, `{"navigate"}` or `{"whole": "screen"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum On {
    /// A node of one state, and the instance of it pointed at.
    Node { state: Shown, node: Pointed },
    /// A whole state.
    State { state: Shown },
    /// A transition the description declares, its arrow in the overview.
    Transition { transition: Transition },
    /// A navigate written in the description, its arrow in a flow.
    Navigate { navigate: Navigate },
    /// The whole screen, apart from any one state: its overview.
    Screen { whole: Whole },
}

/// What a note on a whole description is on: written `"screen"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Whole {
    Screen,
}

/// A node as the description writes it, and the one instance of it the
/// person pointed at: which data showed it, its label, and the width.
/// Written as one object: `{"id", "at", "instance", "label", "width"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pointed {
    #[serde(flatten)]
    pub origin: Origin,
    pub label: Option<u32>,
    pub width: u32,
}

/// A transition as `transitions` in screen.json writes it: from a state, on
/// `<node id as written>.<event>`, for a call the outcome, to a state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub to: String,
}

/// A navigate as `underdraw check <root>` reports it among its links: the
/// screen it is in, the node it is on by its id as written, and the screen
/// it names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Navigate {
    pub from: String,
    pub node: String,
    pub to: String,
}

impl On {
    /// The state it is in, for a node or a whole state.
    pub fn state(&self) -> Option<&Shown> {
        match self {
            On::Node { state, .. } | On::State { state } => Some(state),
            On::Transition { .. } | On::Navigate { .. } | On::Screen { .. } => None,
        }
    }

    /// Whether `other` names the same thing a note is held on (ADR 0023):
    /// the same node as written, in the same state, whichever instance of
    /// it was pointed at; the same whole state; the same arrow.
    pub fn same(&self, other: &On) -> bool {
        match (self, other) {
            (On::Node { state, node }, On::Node { state: other, node: pointed }) => {
                state == other
                    && node.origin.written == pointed.origin.written
                    && node.origin.at == pointed.origin.at
            }
            _ => self == other,
        }
    }

    /// What it is, as the gallery's sidebar says a note is on it, in
    /// `lang`: `on <id> in <state> at <width>`, `on the whole of <state>`,
    /// `on transition …`, `on navigate …`, `on the whole screen`.
    pub fn said(&self, lang: Lang) -> String {
        match self {
            On::Node { state, node } => {
                say!(lang, on_node, node = node.origin.written, state = state.name(), width = node.width)
            }
            On::State { state } => say!(lang, on_state, state = state.name()),
            On::Transition { transition: t } => {
                let outcome = t
                    .outcome
                    .as_ref()
                    .map(|outcome| say!(lang, transition_outcome, outcome))
                    .unwrap_or_default();
                say!(lang, on_transition, from = t.from, on = t.on, outcome, to = t.to)
            }
            On::Navigate { navigate: n } => say!(lang, on_navigate, from = n.from, node = n.node, to = n.to),
            On::Screen { .. } => say!(lang, on_screen),
        }
    }
}

impl Transition {
    /// `from <state> on <node>.<event> [outcome <outcome>] to <state>`.
    pub fn said(&self) -> String {
        let outcome = self.outcome.as_ref().map(|outcome| format!(" outcome {outcome}")).unwrap_or_default();
        format!("from {} on {}{outcome} to {}", self.from, self.on, self.to)
    }
}

impl Navigate {
    /// `from <screen> on <node> to <screen>`.
    pub fn said(&self) -> String {
        format!("from {} on {} to {}", self.from, self.node, self.to)
    }
}

impl Reference {
    /// The reference as one readable line: the description's directory,
    /// then what in it. For a state, the state; for a node, the state, the
    /// node's id as written and where in screen.json, then the instance
    /// pointed at, as context; for an arrow, the transition or navigate
    /// with the keys screen.json and `check` name it by; for the whole
    /// screen, that.
    pub fn line(&self) -> String {
        let state = |state: &Shown| match state {
            Shown::Fixture(name) => format!("fixture {name}"),
            Shown::Phase(name) => format!("phase {name}"),
        };
        let description = self.description.display();
        match &self.on {
            On::State { state: shown } => format!("{description} · {}", state(shown)),
            On::Node { state: shown, node: Pointed { origin, label, width } } => {
                let mut context = Vec::new();
                if !origin.instance.is_empty() {
                    context.push(instance(&origin.instance).trim_start().to_string());
                }
                context.push(label.map_or_else(|| "no label".to_string(), |label| format!("label {label}")));
                context.push(format!("width {width}"));
                format!(
                    "{description} · {} · {} at {} · pointed at {}",
                    state(shown),
                    origin.written,
                    written_in(&origin.at, &self.description),
                    context.join(", ")
                )
            }
            On::Transition { transition } => format!("{description} · transition {}", transition.said()),
            On::Navigate { navigate } => format!("{description} · navigate {}", navigate.said()),
            On::Screen { .. } => format!("{description} · the whole screen"),
        }
    }
}

/// Where the node at `at` (a JSON pointer into the description as read) is
/// written: in screen.json, or, inside a component the project shares, in
/// its components/<Name>.json. In the description in `description` that
/// shows a shared component on its own, a node outside the component's
/// nodes is the instance the gallery draws it as, written nowhere.
fn written_in(at: &str, description: &std::path::Path) -> String {
    match underdraw_format::in_shared(at) {
        Some((name, inside)) => format!("/{inside} in {}/{name}.json", crate::project::COMPONENTS),
        None => match crate::project::showcased(description) {
            Some((name, _)) => format!("the instance shown of {}/{name}.json", crate::project::COMPONENTS),
            None => format!("{at} in screen.json"),
        },
    }
}

/// The element of each repeat around a node, outermost first, after a
/// space: ` [2][0]`; nothing outside any repeat.
pub fn instance(of: &[usize]) -> String {
    match of {
        [] => String::new(),
        _ => format!(" {}", of.iter().map(|index| format!("[{index}]")).collect::<String>()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// Where the shop's orders list writes its first order's total: in
    /// OrderRow, the component the project shares.
    fn shared_node_at() -> String {
        fn find(node: &underdraw_format::Node) -> Option<String> {
            if node.written_id.ends_with("total") {
                return Some(node.written_at.clone());
            }
            node.children.iter().find_map(find)
        }
        let orders = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/shop/orders"));
        let (screen, _) = crate::read(orders).unwrap();
        let state = underdraw_format::State::Fixture("default");
        find(&screen.instantiate(state, underdraw_format::Breakpoint::Compact, None).unwrap()).unwrap()
    }

    #[test]
    fn a_state_names_its_description_and_itself() {
        let state = Reference {
            description: "/d/todo".into(),
            on: On::State { state: Shown::Phase("loading".into()) },
        };
        assert_eq!(state.line(), "/d/todo · phase loading");
        let screen = Reference { description: "/d/todo".into(), on: On::Screen { whole: Whole::Screen } };
        assert_eq!(screen.line(), "/d/todo · the whole screen");
    }

    #[test]
    fn a_node_names_its_template_where_it_is_written_and_the_instance_pointed_at() {
        let origin =
            Origin { written: "order-{index}".into(), at: "/root/children/1/item".into(), instance: vec![2] };
        let node = Reference {
            description: "/d/orders".into(),
            on: On::Node {
                state: Shown::Fixture("default".into()),
                node: Pointed { origin, label: Some(4), width: 390 },
            },
        };
        assert_eq!(
            node.line(),
            "/d/orders · fixture default · order-{index} at /root/children/1/item in screen.json · pointed at [2], label 4, width 390"
        );
        assert_eq!(
            written_in(&shared_node_at(), Path::new("/d/orders")),
            "/nodes/children/1 in components/OrderRow.json",
            "a node of a component the project shares is written in its file"
        );
        let shop =
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/shop/components/OrderStatus"));
        assert_eq!(
            written_in("/root/children/0", shop),
            "the instance shown of components/OrderStatus.json",
            "the instance the gallery shows a shared component as is written nowhere"
        );
    }

    #[test]
    fn an_arrow_names_its_transition_or_navigate_by_the_keys_it_is_written_with() {
        let transition = Transition {
            from: "default".into(),
            on: "order-{index}-cancel.tap".into(),
            outcome: Some("ok".into()),
            to: "cancel-dialog".into(),
        };
        let reference = Reference { description: "/d/orders".into(), on: On::Transition { transition } };
        assert_eq!(
            reference.line(),
            "/d/orders · transition from default on order-{index}-cancel.tap outcome ok to cancel-dialog"
        );
        let navigate =
            Navigate { from: "Orders".into(), node: "nav-export".into(), to: "ExportDocuments".into() };
        let reference = Reference { description: "/d/orders".into(), on: On::Navigate { navigate } };
        assert_eq!(reference.line(), "/d/orders · navigate from Orders on nav-export to ExportDocuments");
    }

    #[test]
    fn what_a_note_is_on_is_said_in_each_language() {
        let origin = |written: &str| Origin { written: written.into(), at: "/root".into(), instance: vec![] };
        let default = || Shown::Fixture("default".into());
        let transition = |outcome: Option<&str>| On::Transition {
            transition: Transition {
                from: "default".into(),
                on: "order-{index}-cancel.tap".into(),
                outcome: outcome.map(Into::into),
                to: "cancel-dialog".into(),
            },
        };
        let cases = [
            (
                On::Node {
                    state: default(),
                    node: Pointed { origin: origin("order-{index}"), label: None, width: 390 },
                },
                "on order-{index} in default at 390",
                "在 order-{index}，default，宽 390",
            ),
            (
                On::State { state: Shown::Phase("loading".into()) },
                "on the whole of loading",
                "在整个 loading",
            ),
            (
                transition(Some("ok")),
                "on transition from default on order-{index}-cancel.tap outcome ok to cancel-dialog",
                "在过渡上：从 default 经 order-{index}-cancel.tap，结果 ok 到 cancel-dialog",
            ),
            (
                transition(None),
                "on transition from default on order-{index}-cancel.tap to cancel-dialog",
                "在过渡上：从 default 经 order-{index}-cancel.tap 到 cancel-dialog",
            ),
            (
                On::Navigate {
                    navigate: Navigate {
                        from: "Orders".into(),
                        node: "nav-export".into(),
                        to: "ExportDocuments".into(),
                    },
                },
                "on navigate from Orders on nav-export to ExportDocuments",
                "在跳转上：从 Orders 经 nav-export 到 ExportDocuments",
            ),
            (On::Screen { whole: Whole::Screen }, "on the whole screen", "在整个屏幕"),
        ];
        for (on, en, zh) in cases {
            assert_eq!(on.said(Lang::En), en);
            assert_eq!(on.said(Lang::Zh), zh);
        }
    }
}
