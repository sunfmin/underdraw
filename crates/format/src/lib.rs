//! The screen description format (docs/format): reads one and names every
//! fault in it. Pure: the caller reads the files and hands over the JSON.

// Flux's refinements of the standard library — what gives a `Vec` its length.
// Naming the crates is what brings them into the check; under a plain build
// both are empty.
extern crate flux_alloc;
extern crate flux_core;

mod breakpoint;
mod check;
mod conditions;
mod expr;
mod field;
mod fixture;
pub mod icons;
mod instance;
mod kinds;
mod model;
mod schema;
mod template;

// Never called: each is there for Flux to refuse.
#[cfg(flux)]
#[allow(dead_code)]
mod rejected;

pub use breakpoint::Breakpoint;
pub use check::{Checked, Condition, Finding, check};
pub use conditions::{Condition as Definition, Conditions};
pub use expr::{Atom, Expr, Op};
pub use field::{Base, Field};
pub use instance::{ANCHORS, Node, State, instantiate};
pub use kinds::{FILTERS, Kinds, LOOK_KEYS, Track};
pub use model::{Model, model};
pub use schema::{Schema, Shape};
