//! The screen description format (docs/format): reads one and names every
//! fault in it. Pure: the caller reads the files and hands over the JSON.

// Flux's refinements of the standard library — what gives a `Vec` its length.
// Naming the crates is what brings them into the check; under a plain build
// both are empty.
extern crate flux_alloc;
extern crate flux_core;

pub mod assumption;
mod breakpoint;
mod check;
mod components;
mod conditions;
mod domain;
mod expr;
mod field;
mod fixture;
pub mod icons;
mod instance;
mod jobs;
mod journeys;
mod links;
mod model;
mod operations;
mod overlay;
mod params;
mod schema;
mod template;
mod transitions;

// Never called: each is there for Flux to refuse.
#[cfg(flux)]
#[allow(dead_code)]
mod rejected;

pub use assumption::Lang;
pub use breakpoint::Breakpoint;
pub use check::{Checked, Condition, Finding, check, check_among};
pub use components::{Components, FILTERS, LOOK, Takes, Track, takes};
pub use conditions::{Condition as Definition, Conditions};
pub use domain::KEY as DOMAIN;
pub use expr::{Atom, Expr, Op};
pub use field::{Base, By, Derived, Field};
pub use instance::{Call, Node, State, instantiate};
pub use jobs::{STATES as JOB_STATES, STATUS as JOB_STATUS};
pub use journeys::walk as walk_journey;
pub use links::{Navigate, assume_undrawn, navigates};
pub use model::{Model, model};
pub use operations::{BulkOf, Does, Fill, Operation, Operations};
pub use overlay::{Anchor, Edge, Placement};
pub use params::{ScreenParams, params};
pub use schema::{Schema, Shape};
pub use transitions::{Transition, read as transitions, writes, written_node};
