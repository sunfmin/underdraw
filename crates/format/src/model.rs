//! A description that passed `check`, read into its parts for whoever
//! generates from it: the view model's types, the conditions, and each
//! fixture's data with defaults filled in.

use serde_json::Value;

use crate::check::{Checked, Findings, check};
use crate::conditions::{Condition, Conditions};
use crate::fixture;
use crate::schema::Schema;

pub struct Model {
    pub screen: String,
    pub schema: Schema,
    pub conditions: Conditions,
    /// Every fixture by name, in the order written, merged and defaulted.
    pub fixtures: Vec<(String, Value)>,
}

impl Model {
    pub fn conditions(&self) -> &[Condition] {
        self.conditions.all()
    }
}

/// `screen` read into its parts; what `check` says of it when it has faults,
/// since nothing generated from a faulty description can be trusted.
/// `kinds` is the project's kinds.json, when it has one.
pub fn model(screen: &Value, kinds: Option<&Value>) -> Result<Model, Box<Checked>> {
    let checked = check(screen, kinds);
    if !checked.ok {
        return Err(Box::new(checked));
    }
    let mut findings = Findings::default();
    let schema = Schema::read(screen, &mut findings);
    let fixtures = fixture::read(screen, &schema, &mut findings);
    let conditions = Conditions::read(screen, &schema, &mut findings);
    Ok(Model { screen: checked.screen, schema, conditions, fixtures })
}
