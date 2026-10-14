//! The screen description format (docs/format): reads one and names every
//! fault in it. Pure: the caller reads the files and hands over the JSON.

// Flux's refinements of the standard library — what gives a `Vec` its length.
// Naming the crates is what brings them into the check; under a plain build
// both are empty.
extern crate flux_alloc;
extern crate flux_core;

mod check;
mod conditions;
mod expr;
mod field;
mod fixture;
mod schema;
mod template;

#[cfg(flux)]
mod rejected;

pub use check::{Checked, Condition, Finding, check};
