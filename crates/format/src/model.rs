//! A screen that passed `check`, for whoever generates from it or lays it
//! out: its parts, as [`Screen`] read them, and every state it can be
//! shown in.

use std::ops::Deref;

use crate::check::{Checked, check};
use crate::conditions::Condition;
use crate::instance::State;
use crate::screen::Screen;

pub struct Model<'s> {
    /// What `check` said of it: its name, fixtures and phases, and which
    /// fixtures each condition holds in. Never with a finding.
    pub checked: Checked,
    screen: &'s Screen,
}

/// A model is its screen, which passed `check`: `model.schema`,
/// `model.instantiate(…)`.
impl Deref for Model<'_> {
    type Target = Screen;

    fn deref(&self) -> &Screen {
        self.screen
    }
}

impl<'s> Model<'s> {
    /// `screen`, given what `check` said of it; what `check` said, back,
    /// when it has faults, since nothing generated from a faulty
    /// description can be trusted.
    pub fn read(screen: &'s Screen, checked: Checked) -> Result<Model<'s>, Box<Checked>> {
        if !checked.ok {
            return Err(Box::new(checked));
        }
        Ok(Model { checked, screen })
    }

    /// The screen's name.
    pub fn screen(&self) -> &str {
        &self.checked.screen
    }

    pub fn conditions(&self) -> &[Condition] {
        self.screen.conditions.all()
    }

    /// Every state it can be shown in: each fixture, then each phase.
    pub fn states(&self) -> Vec<State<'_>> {
        let fixtures = self.checked.fixtures.iter().map(|name| State::Fixture(name));
        fixtures.chain(self.checked.phases.iter().map(|name| State::Phase(name))).collect()
    }
}

/// `screen` checked; what `check` says of it when it has faults.
pub fn model(screen: &Screen) -> Result<Model<'_>, Box<Checked>> {
    Model::read(screen, check(screen))
}
