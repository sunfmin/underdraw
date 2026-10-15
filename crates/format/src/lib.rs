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
mod expand;
mod expr;
mod field;
mod fixture;
pub mod icons;
mod ids;
mod instance;
mod jobs;
mod journeys;
mod links;
pub mod look;
mod model;
mod operations;
mod overlay;
mod params;
mod schema;
mod screen;
mod showcase;
mod subtrees;
mod template;
mod transitions;
pub mod words;

// Never called: each is there for Flux to refuse.
#[cfg(flux)]
#[allow(dead_code)]
mod rejected;

pub use breakpoint::Breakpoint;
pub use check::{Checked, Condition, Finding, check, check_among};
pub use components::{Body, Components, Track, in_shared};
pub use conditions::{Condition as Definition, Conditions};
pub use expand::{Frame, Instances, Met, join as join_id};
pub use expr::{Atom, Expr, Op};
pub use field::{Base, By, Derived, Field};
pub use instance::{Call, Evaluated, Instantiated, Node, SLOTS, State};
pub use jobs::{STATES as JOB_STATES, STATUS as JOB_STATUS};
pub use journeys::{named as journey_screen, walk as walk_journey};
pub use links::{Navigate, Undrawn};
pub use model::{Model, model};
pub use operations::{BulkOf, Does, Fill, Operation, Operations};
pub use overlay::{Anchor, Edge, Placement};
pub use params::ScreenParams;
pub use schema::{Schema, Shape};
pub use screen::{Screen, Shared};
pub use showcase::{showcase, shows};
pub use subtrees::{Subtree, Subtrees};
pub use transitions::Transition;
pub use words::{Lang, fill};
