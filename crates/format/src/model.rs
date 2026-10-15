//! A description that passed `check`, read once into its parts for whoever
//! generates from it or lays it out: the view model's types, the
//! conditions, each fixture's data with defaults filled in, and every state
//! it can be shown in.

use serde_json::Value;

use crate::breakpoint::Breakpoint;
use crate::check::{Checked, Findings, check};
use crate::conditions::{Condition, Conditions};
use crate::fixture;
use crate::instance::{Node, State, build};
use crate::jobs;
use crate::operations::Operations;
use crate::schema::Schema;
use crate::transitions::{self, Transition};

pub struct Model {
    /// What `check` said of it: its name, fixtures and phases, and which
    /// fixtures each condition holds in. Never with a finding.
    pub checked: Checked,
    pub schema: Schema,
    pub conditions: Conditions,
    pub operations: Operations,
    /// Every fixture by name, in the order written, merged and defaulted.
    pub fixtures: Vec<(String, Value)>,
    /// The transitions between its states, in the order written.
    pub transitions: Vec<Transition>,
    description: Value,
}

impl Model {
    /// `screen` read into its parts, given what `check` said of it; what
    /// `check` said, back, when it has faults, since nothing generated from
    /// a faulty description can be trusted.
    pub fn read(screen: &Value, checked: Checked) -> Result<Model, Box<Checked>> {
        if !checked.ok {
            return Err(Box::new(checked));
        }
        let mut findings = Findings::default();
        let screen = &jobs::expand(screen, &mut findings);
        let schema = Schema::read(screen, &mut findings);
        let fixtures = fixture::read(screen, &schema, &mut findings);
        let conditions = Conditions::read(screen, &schema, &mut findings);
        let operations = Operations::read(screen, &schema, &conditions, &mut findings);
        let transitions = transitions::read(screen);
        Ok(Model {
            checked,
            schema,
            conditions,
            operations,
            fixtures,
            transitions,
            description: screen.clone(),
        })
    }

    /// The screen's name.
    pub fn screen(&self) -> &str {
        &self.checked.screen
    }

    pub fn conditions(&self) -> &[Condition] {
        self.conditions.all()
    }

    /// Every state it can be shown in: each fixture, then each phase.
    pub fn states(&self) -> Vec<State<'_>> {
        let fixtures = self.checked.fixtures.iter().map(|name| State::Fixture(name));
        fixtures.chain(self.checked.phases.iter().map(|name| State::Phase(name))).collect()
    }

    /// The screen in `state` at `breakpoint`.
    pub fn instantiate(&self, state: State, breakpoint: Breakpoint) -> Result<Node, String> {
        build(&self.description, &self.conditions, &self.fixtures, state, breakpoint)
    }
}

/// `screen` checked, then read into its parts; what `check` says of it when
/// it has faults. `kinds` is the project's kinds.json, when it has one.
pub fn model(screen: &Value, kinds: Option<&Value>) -> Result<Model, Box<Checked>> {
    Model::read(screen, check(screen, kinds))
}
