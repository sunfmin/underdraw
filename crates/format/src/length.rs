//! A length a look computes: `{depth} * space.4 + 12`. A look's length key
//! (padding, gap, margin, size, …) may multiply a length by an int and add
//! lengths, over whole points, tokens and the look's own int params, each
//! bounded (`"depth": "int 0..4"`), so the length it comes to is known to
//! stay in range. There is no division, no fraction and no floating point;
//! nowhere else in a description computes, so the same words anywhere else
//! are [`arithmetic`], which `check` refuses. underdraw-layout evaluates it
//! and proves the range; a platform is handed it as written.
//!
//! ```text
//! sum     = product ("+" product)*
//! product = operand ("*" operand)*
//! operand = whole number (points, or an int) | token (a length) | "{" param "}" (an int)
//! ```

use serde_json::Value;

use crate::field::{Base, Field};

/// The keys of a look that take a length, and so may compute one.
pub const KEYS: [&str; 9] =
    ["gap", "padding", "margin", "size", "width", "height", "basis", "radius", "itemWidth"];

/// The most a length may come to, in points: what check proves a computed
/// one never passes, and what underdraw-layout holds it to.
pub const MOST: u32 = 10_000;

/// A computed length, as written and parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct Computed {
    pub written: String,
    /// The terms it adds, or why it is no length.
    pub parsed: Result<Vec<Product>, String>,
}

/// Operands multiplied: one length, and ints.
#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    pub operands: Vec<Operand>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    /// A whole number: points, or an int when a token is the length.
    Whole(u32),
    /// A token naming a length: `space.4`.
    Token(String),
    /// A param of the look, an int: `{depth}`.
    Param(String),
}

/// One piece of what is written, before it is read as a length.
#[derive(Clone, Debug, PartialEq)]
enum Piece {
    Number(String),
    Name(String),
    Param(String),
    Plus,
    Times,
    Minus,
    Divide,
}

impl Piece {
    fn operator(&self) -> bool {
        matches!(self, Piece::Plus | Piece::Times | Piece::Minus | Piece::Divide)
    }
}

/// Whether `text` asks for arithmetic: an operator outside its braces.
fn operates(text: &str) -> bool {
    let mut braced = false;
    for c in text.chars() {
        match c {
            '{' => braced = true,
            '}' => braced = false,
            '*' | '×' | '+' | '/' if !braced => return true,
            _ => {}
        }
    }
    false
}

/// The pieces `text` is made of, or none when something in it is not one.
fn pieces(text: &str) -> Option<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let piece = match c {
            _ if c.is_whitespace() => continue,
            '*' | '×' => Piece::Times,
            '+' => Piece::Plus,
            '-' => Piece::Minus,
            '/' => Piece::Divide,
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
                    return None;
                }
                Piece::Param(inside.trim().to_string())
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let mut number = c.to_string();
                while let Some(&d) = chars.peek() {
                    if !(d.is_ascii_digit() || d == '.') {
                        break;
                    }
                    number.push(d);
                    chars.next();
                }
                Piece::Number(number)
            }
            _ if c.is_alphabetic() || c == '_' => {
                let mut name = c.to_string();
                while let Some(&d) = chars.peek() {
                    if !(d.is_alphanumeric() || d == '_' || d == '.') {
                        break;
                    }
                    name.push(d);
                    chars.next();
                }
                Piece::Name(name)
            }
            _ => return None,
        };
        pieces.push(piece);
    }
    Some(pieces)
}

/// Whether the pieces are operands joined by operators, one between each
/// two, with at least one operator.
fn joined(pieces: &[Piece]) -> bool {
    let mut operand_next = true;
    for piece in pieces {
        if piece.operator() == operand_next {
            return false;
        }
        operand_next = !operand_next;
    }
    !operand_next && pieces.iter().any(Piece::operator)
}

/// Whether `text` computes over data: values joined by `+`, `-`, `*` or
/// `/`, one of them a `{placeholder}` (`{level} + 1`, `{depth} * space.4`).
/// Words alone (`MM / YY`, `1 / 2`) compute nothing. Only a look's length
/// computes; check refuses it in a node's props.
pub fn arithmetic(text: &str) -> bool {
    operates(text)
        && pieces(text).is_some_and(|pieces| {
            joined(&pieces) && pieces.iter().any(|piece| matches!(piece, Piece::Param(_)))
        })
}

/// What `{depth} * space.4` is, said once wherever it is refused.
pub const ONLY_A_LOOK: &str = "only a look computes, and only a length: give the look the operands as props (\"depth\": \"{item.depth}\") and write the length in its look, \"{depth} * space.4\"; any other value computed from others is a derived field (`underdraw help data-and-types`)";

impl Computed {
    /// `text` as a computed length, when it asks for arithmetic (an
    /// operator outside its braces); none when it is a plain length,
    /// token or template.
    pub fn read(text: &str) -> Option<Computed> {
        operates(text).then(|| Computed { written: text.to_string(), parsed: parse(text) })
    }

    /// The params it reads, in the order written.
    pub fn params(&self) -> Vec<&str> {
        let mut params = Vec::new();
        for product in self.parsed.iter().flatten() {
            for operand in &product.operands {
                if let Operand::Param(name) = operand {
                    params.push(name.as_str());
                }
            }
        }
        params
    }

    /// The tokens it reads, in the order written.
    pub fn tokens(&self) -> Vec<&str> {
        let mut tokens = Vec::new();
        for product in self.parsed.iter().flatten() {
            for operand in &product.operands {
                if let Operand::Token(name) = operand {
                    tokens.push(name.as_str());
                }
            }
        }
        tokens
    }

    /// What is wrong with it, read against the look's `typed` params (none
    /// when they are listed, untyped): why it does not parse; else each
    /// param that is no int with both bounds, and each term that is not a
    /// length (a length times a length, or ints alone).
    pub fn faults(&self, typed: Option<&[(String, Field)]>) -> Vec<String> {
        let products = match &self.parsed {
            Ok(products) => products,
            Err(why) => return vec![format!("{} is not a length: {why}", self.written)],
        };
        let mut faults = Vec::new();
        for product in products {
            for operand in &product.operands {
                if let Operand::Param(name) = operand {
                    let field = typed.and_then(|typed| typed.iter().find(|(param, _)| param == name));
                    if let Some(fault) = unbounded(name, field.map(|(_, field)| field)) {
                        faults.push(fault);
                    }
                }
            }
            let lengths =
                product.operands.iter().filter(|operand| matches!(operand, Operand::Token(_))).count();
            let wholes =
                product.operands.iter().filter(|operand| matches!(operand, Operand::Whole(_))).count();
            let shown = product.operands.iter().map(Operand::show).collect::<Vec<_>>().join(" * ");
            if lengths > 1 {
                faults.push(format!(
                    "{shown} multiplies a length by a length: one length is multiplied by ints, \"{{depth}} * space.4\""
                ));
            } else if lengths == 0 && wholes == 0 {
                faults.push(format!(
                    "{shown} is an int, not a length: multiply it by a token or a number of points, \"{shown} * space.4\""
                ));
            }
        }
        faults
    }
}

/// Why the param `name`, typed `field`, cannot be multiplied into a length:
/// it is not an int with both bounds.
fn unbounded(name: &str, field: Option<&Field>) -> Option<String> {
    let bounded = |field: &Field| field.lo.is_some() && field.hi.is_some();
    match field {
        Some(field) if field.base == Base::Int && !field.array && !field.nullable && bounded(field) => None,
        Some(field) if field.base == Base::Int && !field.array && !field.nullable => Some(format!(
            "{{{name}}} has no bounds, so the length it computes is not known to stay in range: give it both, \"{name}\": \"int 0..4\""
        )),
        Some(field) => Some(format!(
            "{{{name}}} is {}, and a length is multiplied by an int: type it \"{name}\": \"int 0..4\"",
            field.base.name()
        )),
        None => Some(format!(
            "{{{name}}} has no bounds, so the length it computes is not known to stay in range: type it in \"params\", \"{name}\": \"int 0..4\""
        )),
    }
}

impl Operand {
    fn show(&self) -> String {
        match self {
            Operand::Whole(n) => n.to_string(),
            Operand::Token(name) => name.clone(),
            Operand::Param(name) => format!("{{{name}}}"),
        }
    }
}

/// `text` read as the terms it adds.
fn parse(text: &str) -> Result<Vec<Product>, String> {
    let Some(pieces) = pieces(text) else {
        return Err("it holds something other than whole numbers, tokens, {params}, * and +".to_string());
    };
    if pieces.contains(&Piece::Divide) {
        return Err("a length is never divided: multiply by an int, or name the length you mean as a token"
            .to_string());
    }
    if pieces.contains(&Piece::Minus) {
        return Err("a length only adds and multiplies, never subtracts: add a smaller length, or name the one you mean as a token".to_string());
    }
    if !joined(&pieces) {
        return Err(
            "write operands joined by * and +, one between each two, such as \"{depth} * space.4 + 12\""
                .to_string(),
        );
    }
    let mut products = Vec::new();
    let mut operands = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Plus => products.push(Product { operands: std::mem::take(&mut operands) }),
            Piece::Times | Piece::Minus | Piece::Divide => {}
            Piece::Number(number) => operands.push(Operand::Whole(whole(&number)?)),
            Piece::Name(name) => operands.push(Operand::Token(name)),
            Piece::Param(name) => {
                if !crate::field::is_identifier(&name) {
                    return Err(format!(
                        "{{{name}}} is not a param: a length reads a param whole, \"{{depth}}\", with no path or filter"
                    ));
                }
                operands.push(Operand::Param(name));
            }
        }
    }
    products.push(Product { operands });
    Ok(products)
}

/// `number` as whole points, or why it is not.
fn whole(number: &str) -> Result<u32, String> {
    match number.parse::<u32>() {
        Ok(n) if n <= MOST => Ok(n),
        Ok(_) => Err(format!("{number} is more than a length may be, {MOST} points")),
        Err(_) if number.parse::<f64>().is_ok() => {
            Err(format!("{number} is a fraction, and a length is whole points: write a whole number"))
        }
        Err(_) => Err(format!("{number} is not a number")),
    }
}

/// `value`, a look's length key as written, when it computes: the string
/// itself, or any side of `[v, h]` or `[top, right, bottom, left]`.
pub fn written_in(value: &Value) -> bool {
    match value {
        Value::String(text) => operates(text),
        Value::Array(sides) => sides.iter().any(written_in),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(range: &str) -> Field {
        crate::field::read(&Value::from(range)).unwrap()
    }

    #[test]
    fn a_length_is_products_added() {
        let computed = Computed::read("{depth} * space.4 + 12").unwrap();
        assert_eq!(
            computed.parsed,
            Ok(vec![
                Product { operands: vec![Operand::Param("depth".into()), Operand::Token("space.4".into())] },
                Product { operands: vec![Operand::Whole(12)] },
            ])
        );
        assert_eq!(computed.params(), ["depth"]);
        assert_eq!(computed.tokens(), ["space.4"]);
        assert_eq!(Computed::read("{depth} × 16").unwrap().params(), ["depth"]);
    }

    #[test]
    fn a_plain_length_or_template_computes_nothing() {
        for text in ["space.4", "16", "{width|or:88%}", "70%"] {
            assert_eq!(Computed::read(text), None, "{text}");
        }
    }

    #[test]
    fn division_fractions_and_subtraction_are_refused() {
        let why = |text: &str| Computed::read(text).unwrap().parsed.unwrap_err();
        assert!(why("space.4 / 2").contains("never divided"));
        assert!(why("{depth} * 1.5").contains("1.5 is a fraction"));
        assert!(why("{depth} * 16 + -4").contains("never subtracts"));
        assert!(why("{depth} * * 4").contains("one between each two"));
        assert!(why("{item.depth} * 4").contains("reads a param whole"));
    }

    #[test]
    fn a_param_needs_both_bounds_and_a_term_one_length() {
        let typed = vec![("depth".to_string(), int("int 0..4")), ("open".to_string(), int("int 0.."))];
        let faults = |text: &str| Computed::read(text).unwrap().faults(Some(&typed));
        assert!(faults("{depth} * space.4 + 12").is_empty());
        assert!(faults("{depth} * 16").is_empty());
        assert!(faults("{open} * space.4")[0].contains("{open} has no bounds"));
        assert!(faults("{depth} * space.4 * space.2")[0].contains("a length by a length"));
        assert!(faults("{depth} + space.4")[0].contains("{depth} is an int, not a length"));
        let listed = Computed::read("{depth} * space.4").unwrap().faults(None);
        assert!(listed[0].contains("type it in \"params\""));
    }

    #[test]
    fn arithmetic_is_data_joined_by_operators() {
        for text in ["{a} * 2", "{a} + {b}", "{a} / 2", "space.4 × {n}"] {
            assert!(arithmetic(text), "{text}");
        }
        for text in ["Buy 2 + get 1", "{a}", "C++", "{a} + ", "a, b", "2026-11-12", "MM / YY", "1 / 2"] {
            assert!(!arithmetic(text), "{text}");
        }
    }
}
