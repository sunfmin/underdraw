//! A screen description read once into everything derived from it: the
//! description with its project's domain written in, the table of the
//! components it may use, the view model's types, each fixture's data
//! with defaults and conditions filled in, its params, conditions,
//! operations and transitions, and what reading them found. `check`,
//! instantiation, layout, handoff and journeys all take a [`Screen`], so
//! none of them reads the description again.

use serde_json::{Map, Value};

use crate::breakpoint::Breakpoint;
use crate::check::{Finding, Findings};
use crate::components::Components;
use crate::conditions::Conditions;
use crate::domain;
use crate::field::Field;
use crate::fixture;
use crate::instance::{Node, State, build};
use crate::links::{self, Navigate, Undrawn};
use crate::operations::Operations;
use crate::params;
use crate::schema::{Schema, object};
use crate::showcase;
use crate::transitions::{self, Transition};

/// What a screen's project shares with every screen in it: the domain
/// (domain.json at its root, ADR 0020) and its components (each
/// `components/<Name>.json`, its contents by its name, ADR 0021).
#[derive(Clone, Copy, Default)]
pub struct Shared<'a> {
    pub domain: Option<&'a Value>,
    pub components: Option<&'a Map<String, Value>>,
}

/// One screen description, read once.
pub struct Screen {
    /// The description with the domain's types, conditions and operations
    /// written in, and what each long command derives.
    description: Value,
    /// Every component it may use, each a look or nodes.
    pub components: Components,
    pub schema: Schema,
    /// Every fixture by name, in the order written, merged, defaulted and
    /// with derived conditions filled in.
    pub fixtures: Vec<(String, Value)>,
    /// The params it declares, each with its field: none when it has no
    /// "params".
    pub params: Option<Vec<(String, Field)>>,
    pub conditions: Conditions,
    pub operations: Operations,
    /// The transitions between its states, in the order written.
    pub transitions: Vec<Transition>,
    /// Every fault reading it found, in the order found: `check` reports
    /// them with the rest.
    findings: Vec<Finding>,
}

impl Screen {
    /// The description `description` read with what its project `shared`.
    /// What reading it finds is kept with it, for `check` to report.
    pub fn read(description: &Value, shared: Shared) -> Screen {
        let mut findings = Findings::default();
        let description = domain::expand(description, shared.domain, &mut findings);
        let none = Map::new();
        let components = Components::read(&description, shared.components.unwrap_or(&none), &mut findings);
        let schema = Schema::read(&description, &mut findings);
        let mut fixtures = fixture::read(&description, &schema, &mut findings);
        if let Some(name) = showcase::shows(&description) {
            if fixtures.is_empty() {
                findings.add(
                    "fixtures",
                    &format!("{name} is shared by the project, so it is shown on its own: give components/{name}.json \"fixtures\", each a value of its params, to draw it in"),
                );
            }
        }
        let params = params::read(&description, &schema, &mut findings);
        let conditions = Conditions::read(&description, &schema, &mut findings);
        conditions.fill(&schema, &mut fixtures);
        let operations = Operations::read(&description, &schema, &conditions, &mut findings);
        let transitions = transitions::read(&description);
        Screen {
            description,
            components,
            schema,
            fixtures,
            params,
            conditions,
            operations,
            transitions,
            findings: findings.0,
        }
    }

    /// The description as read: with the domain's written in.
    pub fn description(&self) -> &Value {
        &self.description
    }

    /// Its name, as `screen` gives it.
    pub fn name(&self) -> &str {
        self.description.get("screen").and_then(Value::as_str).unwrap_or_default()
    }

    /// Every fault reading it found.
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Its fixtures' names, in the order written.
    pub fn fixture_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for (name, _) in &self.fixtures {
            names.push(name.clone());
        }
        names
    }

    /// Its phases' names, in the order written.
    pub fn phases(&self) -> Vec<String> {
        let mut phases = Vec::new();
        for (name, _) in object(&self.description, "phases") {
            if !name.starts_with('$') {
                phases.push(name.clone());
            }
        }
        phases
    }

    /// The state named `name`: a fixture of that name, or else a phase.
    pub fn state<'n>(&self, name: &'n str) -> State<'n> {
        if self.fixtures.iter().any(|(fixture, _)| fixture == name) {
            State::Fixture(name)
        } else {
            State::Phase(name)
        }
    }

    /// The screen in `state` at `breakpoint`, each node that navigates to
    /// a screen `undrawn` says is not drawn yet marked assumed. Expected to
    /// have passed `check`; what it cannot make sense of is an error.
    pub fn instantiate(
        &self,
        state: State,
        breakpoint: Breakpoint,
        undrawn: Option<&Undrawn>,
    ) -> Result<Node, String> {
        build(self, state, breakpoint, undrawn)
    }

    /// Whether it writes a node with the id `id`, as written (a template
    /// inside an item), in its trees, phases or components.
    pub fn writes(&self, id: &str) -> bool {
        self.written_node(id).is_some()
    }

    /// The fields of the node it writes with the id `id`, as written; none
    /// when it writes none.
    pub fn written_node(&self, id: &str) -> Option<&Map<String, Value>> {
        transitions::written(&self.description, self.components.subtrees()).get(id).copied()
    }

    /// Every `navigate` in it, as [`links::navigates`] says.
    pub fn navigates(&self) -> Vec<Navigate> {
        links::navigates(&self.description, self.components.subtrees())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use super::*;
    use crate::check::check;
    use crate::model::Model;

    thread_local! {
        static READS: RefCell<BTreeMap<&'static str, usize>> = const { RefCell::new(BTreeMap::new()) };
    }

    /// Counts one read of the part `part` on this thread.
    pub(crate) fn count(part: &'static str) {
        READS.with(|reads| *reads.borrow_mut().entry(part).or_default() += 1);
    }

    fn reads() -> BTreeMap<&'static str, usize> {
        READS.with(|reads| std::mem::take(&mut *reads.borrow_mut()))
    }

    /// The shop draft's orders screen, with the domain and components
    /// the project shares.
    fn shop() -> Screen {
        let json = |text: &str| serde_json::from_str::<Value>(text).unwrap();
        let domain = json(include_str!("../../../drafts/shop/domain.json"));
        let mut components = Map::new();
        components
            .insert("OrderRow".into(), json(include_str!("../../../drafts/shop/components/OrderRow.json")));
        components.insert(
            "OrderStatus".into(),
            json(include_str!("../../../drafts/shop/components/OrderStatus.json")),
        );
        let description = json(include_str!("../../../drafts/shop/orders/screen.json"));
        Screen::read(&description, Shared { domain: Some(&domain), components: Some(&components) })
    }

    #[test]
    fn each_part_is_read_once_per_screen() {
        reads();
        let screen = shop();
        let checked = check(&screen);
        let model =
            Model::read(&screen, checked.clone()).unwrap_or_else(|checked| panic!("{:#?}", checked.findings));
        for state in model.states() {
            for breakpoint in Breakpoint::ALL {
                model.instantiate(state, breakpoint, None).unwrap();
            }
        }
        screen.navigates();
        let once: BTreeMap<&str, usize> = [
            "components",
            "conditions",
            "domain",
            "fixtures",
            "operations",
            "params",
            "schema",
            "transitions",
        ]
        .into_iter()
        .map(|part| (part, 1))
        .collect();
        assert_eq!(reads(), once, "{:#?}", checked.findings);
    }
}
