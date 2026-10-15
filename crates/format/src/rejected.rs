//! Programs the refinements exist to refuse. Each one breaks one rule the way
//! a careless caller would, and Flux must reject it: weaken the rule and the
//! check goes red here. Compiled only under Flux, never run.

use flux_rs::attrs::*;

use crate::expr::Cursor;
use crate::template::Decimals;

/// The decimals of `number:N-M` run from N up to M, never backwards.
#[should_fail]
fn decimals_backwards() -> Decimals {
    Decimals { min: 2, max: 1 }
}

/// No platform shows more than 20 decimals.
#[should_fail]
fn decimals_past_the_limit() -> Decimals {
    Decimals { min: 0, max: 21 }
}

/// `Decimals::new` answers Some exactly when the rule holds.
#[should_fail]
fn decimals_new_backwards_is_some() -> Decimals {
    Decimals::new(2, 1).unwrap()
}

/// The parser never stands past its last token.
#[should_fail]
fn cursor_past_the_end() -> Cursor {
    Cursor { tokens: Vec::new(), pos: 1 }
}
