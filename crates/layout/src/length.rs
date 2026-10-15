//! A length a look computes (`{depth} * space.4 + 12`), parsed by
//! underdraw-format: what it comes to in one drawing, and the range it can
//! come to at all, from its operands' bounds, which `check` holds to
//! `0..=MOST` points. Drawing holds every operand and every step to that
//! range as well, which Flux proves leaves no step to overflow.

use flux_rs::attrs::*;
use serde_json::Value;
use underdraw_format::Field;
use underdraw_format::length::{Computed, MOST, Operand};

use crate::tokens::Tokens;

/// Whole points from 0 to [`MOST`] (10,000): two multiplied stay below
/// `u32::MAX`, so neither a product nor a sum of two can overflow.
#[refined_by(n: int)]
#[invariant(n <= 10000)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Points {
    #[field(u32[n])]
    n: u32,
}

impl Points {
    /// `n`, or [`MOST`] when it is more.
    #[spec(fn(n: u32) -> Points)]
    fn held(n: u32) -> Points {
        if n > 10_000 { Points { n: 10_000 } } else { Points { n } }
    }

    /// A whole number given as JSON, held to the range: 0 for anything
    /// else, a negative number or a fraction among them.
    fn of(value: &Value) -> Points {
        let n = value.as_u64().unwrap_or(0).min(u64::from(MOST));
        Points::held(u32::try_from(n).unwrap_or(MOST))
    }

    fn times(self, other: Points) -> Points {
        Points::held(self.n * other.n)
    }

    fn plus(self, other: Points) -> Points {
        Points::held(self.n + other.n)
    }

    pub(crate) fn get(self) -> u32 {
        self.n
    }
}

/// What `computed` comes to, each param read by `param` and each token
/// from `tokens`; none when it does not parse, which `check` names.
pub(crate) fn points(computed: &Computed, tokens: &Tokens, param: &dyn Fn(&str) -> Value) -> Option<Points> {
    let products = computed.parsed.as_ref().ok()?;
    let mut sum = Points::held(0);
    for product in products {
        let mut multiplied = Points::held(1);
        for operand in &product.operands {
            let value = match operand {
                Operand::Whole(n) => Value::from(*n),
                Operand::Token(name) => {
                    tokens.find_length(&Value::from(name.as_str())).map_or(Value::Null, |n| {
                        serde_json::Number::from_f64(f64::from(n)).map_or(Value::Null, Value::Number)
                    })
                }
                Operand::Param(name) => param(name),
            };
            multiplied = multiplied.times(Points::of(&whole(&value)));
        }
        sum = sum.plus(multiplied);
    }
    Some(sum)
}

/// `value` as a whole number when it is one, written as a float or not.
fn whole(value: &Value) -> Value {
    match value.as_f64() {
        Some(n) if n.fract() == 0.0 && n >= 0.0 && n <= f64::from(MOST) => Value::from(n as u64),
        _ => value.clone(),
    }
}

/// The least and the most `computed` can come to, from each operand's
/// bounds: a param's from its type in `typed`, a token's its value in
/// `tokens`; or what keeps it from being known (a token tokens.json does
/// not name, or that is no whole length). A param with no bounds was
/// already named by underdraw-format's check, and is taken here as 0.
pub(crate) fn range(
    computed: &Computed,
    tokens: &Value,
    typed: &[(String, Field)],
) -> Result<(f64, f64), String> {
    let Ok(products) = &computed.parsed else {
        return Ok((0.0, 0.0));
    };
    let (mut least, mut most) = (0.0, 0.0);
    for product in products {
        let (mut low, mut high) = (1.0, 1.0);
        for operand in &product.operands {
            let (lo, hi) = match operand {
                Operand::Whole(n) => (f64::from(*n), f64::from(*n)),
                Operand::Token(name) => {
                    let n = token(tokens, name)?;
                    (n, n)
                }
                Operand::Param(name) => match typed.iter().find(|(param, _)| param == name) {
                    Some((_, field)) => (field.lo.unwrap_or(0.0), field.hi.unwrap_or(0.0)),
                    None => (0.0, 0.0),
                },
            };
            let corners = [low * lo, low * hi, high * lo, high * hi];
            low = corners.iter().copied().fold(f64::INFINITY, f64::min);
            high = corners.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        }
        least += low;
        most += high;
    }
    Ok((least, most))
}

/// The length the token `name` names in `tokens`, or why it names none.
fn token(tokens: &Value, name: &str) -> Result<f64, String> {
    let mut at = Some(tokens);
    for segment in name.split('.') {
        at = at.and_then(|value| value.get(segment));
    }
    match at.and_then(Value::as_f64) {
        Some(n) if n >= 0.0 && n.fract() == 0.0 => Ok(n),
        Some(n) => Err(format!("{name} is {n} in tokens.json, and a computed length takes whole points")),
        None => Err(format!("{name} is not a length tokens.json names: name one it does, such as space.4")),
    }
}

/// Every computed length of a component `screen` declares that cannot be
/// known to stay from 0 to [`MOST`] points, at where it is.
pub(crate) fn faults(components: &underdraw_format::Components, tokens: &Value) -> Vec<(String, String)> {
    let mut faults = Vec::new();
    for (component, at, computed) in components.lengths() {
        let typed = components.typed(component).unwrap_or_default();
        match range(computed, tokens, typed) {
            Err(problem) => faults.push((at, problem)),
            Ok((least, most)) if least < 0.0 || most > f64::from(MOST) => faults.push((
                at,
                format!(
                    "{} comes to {least}..{most} points from its operands' bounds, and a length is 0 to {MOST}: narrow the params' bounds, or take a smaller token",
                    computed.written
                ),
            )),
            Ok(_) => {}
        }
    }
    faults
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_length_comes_to_its_products_added() {
        let tokens = json!({ "space": { "4": 16 } });
        let computed = Computed::read("{depth} * space.4 + 12").unwrap();
        let depth = |n: u64| move |_: &str| Value::from(n);
        assert_eq!(points(&computed, &Tokens(&tokens), &depth(0)).map(Points::get), Some(12));
        assert_eq!(points(&computed, &Tokens(&tokens), &depth(3)).map(Points::get), Some(60));
        let huge = Computed::read("{depth} * 10000 * 10000").unwrap();
        assert_eq!(points(&huge, &Tokens(&tokens), &depth(9)).map(Points::get), Some(MOST));
    }

    #[test]
    fn a_range_comes_from_the_operands_bounds() {
        let tokens = json!({ "space": { "4": 16, "half": 2.5 } });
        let typed = |lo: f64, hi: f64| {
            let field = Field { lo: Some(lo), hi: Some(hi), ..Field::of(underdraw_format::Base::Int) };
            vec![("depth".to_string(), field)]
        };
        let computed = Computed::read("{depth} * space.4 + 12").unwrap();
        assert_eq!(range(&computed, &tokens, &typed(0.0, 4.0)), Ok((12.0, 76.0)));
        assert_eq!(range(&computed, &tokens, &typed(-2.0, 4.0)), Ok((-20.0, 76.0)));
        let missing = Computed::read("{depth} * space.9").unwrap();
        assert!(range(&missing, &tokens, &typed(0.0, 4.0)).unwrap_err().contains("space.9 is not a length"));
        let half = Computed::read("{depth} * space.half").unwrap();
        assert!(range(&half, &tokens, &typed(0.0, 4.0)).unwrap_err().contains("whole points"));
    }
}
