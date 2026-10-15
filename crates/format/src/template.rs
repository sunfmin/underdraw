//! Templates (docs/format/expressions.md): text with `{path}` and
//! `{path|format}` placeholders. A placeholder holds a path and nothing else.

use flux_rs::attrs::*;

/// How a number is shown: between `min` and `max` decimals. Platforms take
/// these as minimum and maximum fraction digits, which none allows past 20.
#[refined_by(min: int, max: int)]
#[invariant(min <= max && max <= 20)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decimals {
    #[field(u8[min])]
    pub(crate) min: u8,
    #[field(u8[max])]
    pub(crate) max: u8,
}

impl Decimals {
    pub const LIMIT: u8 = 20;

    /// The decimals from `min` to `max`, or none when they run backwards or
    /// past the limit.
    #[spec(fn(min: u8, max: u8) -> Option<Decimals[min, max]>[min <= max && max <= 20])]
    pub fn new(min: u8, max: u8) -> Option<Decimals> {
        if min <= max && max <= Self::LIMIT { Some(Decimals { min, max }) } else { None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Money,
    Date,
    DateShort,
    Count,
    Number(Decimals),
}

impl Format {
    fn parse(text: &str) -> Option<Format> {
        match text {
            "money" => Some(Format::Money),
            "date" => Some(Format::Date),
            "date:short" => Some(Format::DateShort),
            "count" => Some(Format::Count),
            _ => {
                let digits = text.strip_prefix("number:")?;
                let (min, max) = match digits.split_once('-') {
                    Some((min, max)) => (min.parse().ok()?, max.parse().ok()?),
                    None => {
                        let fixed = digits.parse().ok()?;
                        (fixed, fixed)
                    }
                };
                Decimals::new(min, max).map(Format::Number)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placeholder {
    pub path: String,
    pub format: Option<Format>,
}

/// Every placeholder in `text`, in order, or what is wrong with the first one
/// that is not one. `{{` and `}}` are literal braces.
pub fn placeholders(text: &str) -> Result<Vec<Placeholder>, String> {
    let mut found = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
            }
            '{' => {
                let mut inside = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    inside.push(c);
                }
                if !closed {
                    return Err(format!("{{{inside} is never closed; a literal brace is {{{{"));
                }
                found.push(placeholder(&inside)?);
            }
            '}' => return Err("a lone } — a literal brace is }}".to_string()),
            _ => {}
        }
    }
    Ok(found)
}

/// The placeholder `{inside}`: a dotted path, then optionally `|` and a format.
fn placeholder(inside: &str) -> Result<Placeholder, String> {
    let (path, format) = match inside.split_once('|') {
        Some((path, format)) => (path, Some(format)),
        None => (inside, None),
    };
    if !path.split('.').all(crate::field::is_identifier) {
        return Err(format!(
            "{{{inside}}} is not a placeholder: it holds a path and an optional |format, no operators"
        ));
    }
    let format = match format {
        None => None,
        Some(text) => Some(Format::parse(text).ok_or_else(|| {
            format!(
                "{{{inside}}}: unknown format {text:?} (money, date, date:short, count, number:N, number:N-M with N ≤ M ≤ {})",
                Decimals::LIMIT
            )
        })?),
    };
    Ok(Placeholder { path: path.to_string(), format })
}

/// The path `text` is when it is one whole placeholder, with no format:
/// `"{item}"`, which gives the value itself.
pub(crate) fn whole(text: &str) -> Option<String> {
    let found = placeholders(text).ok()?;
    match found.as_slice() {
        [only] if only.format.is_none() && text == format!("{{{}}}", only.path) => Some(only.path.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn number(min: u8, max: u8) -> Option<Format> {
        Decimals::new(min, max).map(Format::Number)
    }

    #[test]
    fn reads_placeholders_and_formats() {
        let found = placeholders("{a.b} 共 {c|money} · {d|number:0-1} {{x}} {e|number:2}").unwrap();
        let read: Vec<_> = found.iter().map(|p| (p.path.as_str(), p.format)).collect();
        assert_eq!(
            read,
            [("a.b", None), ("c", Some(Format::Money)), ("d", number(0, 1)), ("e", number(2, 2))]
        );
    }

    #[test]
    fn refuses_logic_and_bad_formats() {
        for text in ["{a ? b : c}", "{a.b|number:2-1}", "{a|number:0-21}", "{a|upper}", "{a", "a }", "{}"] {
            assert!(placeholders(text).is_err(), "{text:?} should be refused");
        }
    }

    proptest! {
        // Any text is either read or refused with a reason: reading never panics.
        #[test]
        fn never_panics(text in ".*") {
            let _ = placeholders(&text);
        }

        // A format that is read keeps its decimals in order and within the limit.
        #[test]
        fn decimals_hold_their_rule(min in 0u8..40, max in 0u8..40) {
            match Format::parse(&format!("number:{min}-{max}")) {
                Some(Format::Number(d)) => prop_assert!(d.min <= d.max && d.max <= Decimals::LIMIT),
                Some(other) => prop_assert!(false, "read as {other:?}"),
                None => prop_assert!(min > max || max > Decimals::LIMIT),
            }
        }
    }
}
