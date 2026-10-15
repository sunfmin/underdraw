//! The one grammar of a yes/no (docs/format/expressions.md), read by one
//! parser into one tree wherever it is written: a condition's `when`, a use
//! site (`if`, `disabled`, …) and a look's `if`. A name is read as written,
//! a path or a placeholder (`value|first`). What each position accepts of
//! the grammar is a check over the tree, decided by where it is used: a
//! `when`'s names are paths ([`when`]), a use site is names joined by `&&`
//! (`conditions::UseSite`), a look's `if` takes it whole. What a name
//! refers to, and whether the types fit, is decided by `conditions` for a
//! screen and by `look` for a component.
//!
//! ```text
//! expr    = and ("||" and)*
//! and     = not ("&&" not)*
//! not     = "!" not | compare
//! compare = atom (("==" | "!=" | "<" | "<=" | ">" | ">=") atom)?
//! atom    = name | literal | "(" expr ")"
//! ```

use flux_rs::attrs::*;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Or,
    And,
    Not,
    Op(Op),
    Open,
    Close,
    Str(String),
    Num(f64),
    Name(String),
}

impl Token {
    fn show(&self) -> String {
        match self {
            Token::Or => "||".to_string(),
            Token::And => "&&".to_string(),
            Token::Not => "!".to_string(),
            Token::Op(op) => op.symbol().to_string(),
            Token::Open => "(".to_string(),
            Token::Close => ")".to_string(),
            Token::Str(s) => format!("{s:?}"),
            Token::Num(n) => n.to_string(),
            Token::Name(n) => n.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    pub fn symbol(self) -> &'static str {
        match self {
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }

    /// The same comparison with its sides swapped: `0 < x` is `x > 0`.
    pub fn flipped(self) -> Op {
        match self {
            Op::Lt => Op::Gt,
            Op::Le => Op::Ge,
            Op::Gt => Op::Lt,
            Op::Ge => Op::Le,
            same => same,
        }
    }

    pub fn orders(self) -> bool {
        matches!(self, Op::Lt | Op::Le | Op::Gt | Op::Ge)
    }

    /// Whether `left` and `right` stand in this relation: numbers by value,
    /// strings by their characters; what has no order is neither less nor
    /// greater.
    pub fn holds(self, left: &Value, right: &Value) -> bool {
        match self {
            Op::Eq => same(left, right),
            Op::Ne => !same(left, right),
            _ => match order(left, right) {
                Some(ordering) => match self {
                    Op::Lt => ordering.is_lt(),
                    Op::Le => ordering.is_le(),
                    Op::Gt => ordering.is_gt(),
                    _ => ordering.is_ge(),
                },
                None => false,
            },
        }
    }
}

fn same(l: &Value, r: &Value) -> bool {
    match (l.as_f64(), r.as_f64()) {
        (Some(a), Some(b)) => a == b,
        _ => l == r,
    }
}

fn order(l: &Value, r: &Value) -> Option<std::cmp::Ordering> {
    match (l, r) {
        (Value::Number(_), Value::Number(_)) => l.as_f64()?.partial_cmp(&r.as_f64()?),
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// A condition, its names each an `N`: a path into the data or another
/// condition's name for a screen, a placeholder for a look.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr<N = String> {
    Or(Vec<Expr<N>>),
    And(Vec<Expr<N>>),
    Not(Box<Expr<N>>),
    Compare(Op, Atom<N>, Atom<N>),
    Atom(Atom<N>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Atom<N = String> {
    /// A path into the data, or the name of another condition.
    Name(N),
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    Group(Box<Expr<N>>),
}

impl Expr<String> {
    /// Every name it uses, in the order written.
    pub(crate) fn names<'e>(&'e self, names: &mut Vec<&'e str>) {
        match self {
            Expr::Or(items) | Expr::And(items) => {
                for item in items {
                    item.names(names);
                }
            }
            Expr::Not(inner) => inner.names(names),
            Expr::Compare(_, left, right) => {
                left.names(names);
                right.names(names);
            }
            Expr::Atom(atom) => atom.names(names),
        }
    }

    /// The same condition with each name read by `read`.
    pub(crate) fn named<N>(self, read: &impl Fn(String) -> N) -> Expr<N> {
        match self {
            Expr::Or(items) => Expr::Or(named_all(items, read)),
            Expr::And(items) => Expr::And(named_all(items, read)),
            Expr::Not(inner) => Expr::Not(Box::new(inner.named(read))),
            Expr::Compare(op, left, right) => Expr::Compare(op, left.named(read), right.named(read)),
            Expr::Atom(atom) => Expr::Atom(atom.named(read)),
        }
    }
}

fn named_all<N>(items: Vec<Expr>, read: &impl Fn(String) -> N) -> Vec<Expr<N>> {
    let mut all = Vec::new();
    for item in items {
        all.push(item.named(read));
    }
    all
}

impl Atom<String> {
    fn names<'e>(&'e self, names: &mut Vec<&'e str>) {
        match self {
            Atom::Name(name) => names.push(name),
            Atom::Group(inner) => inner.names(names),
            _ => {}
        }
    }

    fn named<N>(self, read: &impl Fn(String) -> N) -> Atom<N> {
        match self {
            Atom::Name(name) => Atom::Name(read(name)),
            Atom::Str(s) => Atom::Str(s),
            Atom::Num(n) => Atom::Num(n),
            Atom::Bool(b) => Atom::Bool(b),
            Atom::Null => Atom::Null,
            Atom::Group(inner) => Atom::Group(Box::new(inner.named(read))),
        }
    }
}

/// A screen condition's `when`: the grammar, each name a path, `order.total`.
pub fn when(text: &str) -> Result<Expr, String> {
    let expr = parse(text)?;
    let mut names = Vec::new();
    expr.names(&mut names);
    for name in names {
        if let Some(problem) = not_a_path(name) {
            return Err(problem);
        }
    }
    Ok(expr)
}

/// Why `name` is not a path, said as a `when` that wrote it is refused: at
/// the first character a path cannot hold.
fn not_a_path(name: &str) -> Option<String> {
    let mut chars = name.chars();
    let first = chars.next()?;
    let wrong = if first.is_ascii_alphabetic() || first == '_' {
        chars.find(|&c| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))?
    } else {
        first
    };
    Some(if wrong == '|' { "| is written ||".to_string() } else { format!("unexpected {wrong:?}") })
}

/// An expression, each name as it was written: a path, or a placeholder,
/// a path then its filters (`value|first`, `price|or:0`), where `||` is
/// still "or". Whatever the position, this is what reads it.
pub fn parse(text: &str) -> Result<Expr, String> {
    let mut cursor = Cursor::new(lex(text)?);
    let expr = or(&mut cursor)?;
    match cursor.peek() {
        None => Ok(expr),
        Some(token) => Err(format!("unexpected {} after a whole condition", token.show())),
    }
}

/// The tokens and how many of them are read. Reading never passes the end.
#[refined_by(pos: int, len: int)]
#[invariant(pos <= len)]
pub(crate) struct Cursor {
    #[field(Vec<Token>[len])]
    pub(crate) tokens: Vec<Token>,
    #[field(usize[pos])]
    pub(crate) pos: usize,
}

impl Cursor {
    fn new(tokens: Vec<Token>) -> Cursor {
        Cursor { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// The next token, which is then read; none at the end.
    fn next(&mut self) -> Option<Token> {
        let pos = self.pos;
        if pos < self.tokens.len() {
            let token = self.tokens[pos].clone();
            self.pos = pos + 1;
            Some(token)
        } else {
            None
        }
    }
}

fn or(cursor: &mut Cursor) -> Result<Expr, String> {
    let first = and(cursor)?;
    let mut rest = Vec::new();
    while cursor.peek() == Some(&Token::Or) {
        cursor.next();
        rest.push(and(cursor)?);
    }
    Ok(join(first, rest, Expr::Or))
}

fn and(cursor: &mut Cursor) -> Result<Expr, String> {
    let first = not(cursor)?;
    let mut rest = Vec::new();
    while cursor.peek() == Some(&Token::And) {
        cursor.next();
        rest.push(not(cursor)?);
    }
    Ok(join(first, rest, Expr::And))
}

/// `first` alone, or `first` and the `rest` under one operator.
fn join(first: Expr, mut rest: Vec<Expr>, make: fn(Vec<Expr>) -> Expr) -> Expr {
    if rest.is_empty() {
        first
    } else {
        let mut all = vec![first];
        all.append(&mut rest);
        make(all)
    }
}

fn not(cursor: &mut Cursor) -> Result<Expr, String> {
    if cursor.peek() == Some(&Token::Not) {
        cursor.next();
        Ok(Expr::Not(Box::new(not(cursor)?)))
    } else {
        compare(cursor)
    }
}

fn compare(cursor: &mut Cursor) -> Result<Expr, String> {
    let left = atom(cursor)?;
    let op = match cursor.peek() {
        Some(Token::Op(op)) => *op,
        _ => return Ok(Expr::Atom(left)),
    };
    cursor.next();
    Ok(Expr::Compare(op, left, atom(cursor)?))
}

fn atom(cursor: &mut Cursor) -> Result<Atom, String> {
    match cursor.next() {
        Some(Token::Open) => {
            let inner = or(cursor)?;
            match cursor.next() {
                Some(Token::Close) => Ok(Atom::Group(Box::new(inner))),
                _ => Err("a ( is never closed".to_string()),
            }
        }
        Some(Token::Str(s)) => Ok(Atom::Str(s)),
        Some(Token::Num(n)) => Ok(Atom::Num(n)),
        Some(Token::Name(name)) => Ok(match name.as_str() {
            "true" => Atom::Bool(true),
            "false" => Atom::Bool(false),
            "null" => Atom::Null,
            _ => Atom::Name(name),
        }),
        Some(token) => Err(format!("expected a name, a literal or (, found {}", token.show())),
        None => Err("the condition ends where a value is expected".to_string()),
    }
}

fn lex(text: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        chars.next();
        let token = match c {
            _ if c.is_whitespace() => continue,
            '|' | '&' => {
                if chars.next() != Some(c) {
                    return Err(format!("{c} is written {c}{c}"));
                }
                if c == '|' { Token::Or } else { Token::And }
            }
            '=' => {
                if chars.next() != Some('=') {
                    return Err("equality is written ==".to_string());
                }
                Token::Op(Op::Eq)
            }
            '!' | '<' | '>' => {
                let with_eq = chars.peek() == Some(&'=');
                if with_eq {
                    chars.next();
                }
                match (c, with_eq) {
                    ('!', true) => Token::Op(Op::Ne),
                    ('!', false) => Token::Not,
                    ('<', true) => Token::Op(Op::Le),
                    ('<', false) => Token::Op(Op::Lt),
                    ('>', true) => Token::Op(Op::Ge),
                    _ => Token::Op(Op::Gt),
                }
            }
            '(' => Token::Open,
            ')' => Token::Close,
            '"' => {
                let mut s = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    s.push(c);
                }
                if !closed {
                    return Err(format!("the string \"{s} is never closed"));
                }
                Token::Str(s)
            }
            _ if c.is_ascii_digit() || c == '-' => {
                let mut s = c.to_string();
                while let Some(&d) = chars.peek() {
                    if !(d.is_ascii_digit() || d == '.') {
                        break;
                    }
                    s.push(d);
                    chars.next();
                }
                Token::Num(s.parse().map_err(|_| format!("{s} is not a number"))?)
            }
            _ if c.is_alphabetic() || c == '_' || c == '$' => {
                let mut s = c.to_string();
                while let Some(&d) = chars.peek() {
                    // A single `|` begins a filter; `||` is "or".
                    let filter = d == '|' && chars.clone().nth(1) != Some('|');
                    if !(d.is_alphanumeric() || "_.:%$".contains(d) || filter) {
                        break;
                    }
                    s.push(d);
                    chars.next();
                }
                Token::Name(s)
            }
            _ => return Err(format!("unexpected {c:?}")),
        };
        tokens.push(token);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn name(n: &str) -> Atom {
        Atom::Name(n.to_string())
    }

    #[test]
    fn binds_and_tighter_than_or() {
        let parsed = parse("a || b && !c.d != null").unwrap();
        let expected = Expr::Or(vec![
            Expr::Atom(name("a")),
            Expr::And(vec![
                Expr::Atom(name("b")),
                Expr::Not(Box::new(Expr::Compare(Op::Ne, name("c.d"), Atom::Null))),
            ]),
        ]);
        assert_eq!(parsed, expected);
    }

    #[test]
    fn reads_literals_and_groups() {
        let parsed = parse("(status == \"loaded\") && n >= -1.5").unwrap();
        let expected = Expr::And(vec![
            Expr::Atom(Atom::Group(Box::new(Expr::Compare(
                Op::Eq,
                name("status"),
                Atom::Str("loaded".to_string()),
            )))),
            Expr::Compare(Op::Ge, name("n"), Atom::Num(-1.5)),
        ]);
        assert_eq!(parsed, expected);
    }

    #[test]
    fn refuses_what_is_not_the_grammar() {
        for text in ["", "a &", "a = b", "(a", "a b", "a ? b : c", "\"open", "a | b", "a == == b"] {
            assert!(parse(text).is_err(), "{text:?} should be refused");
        }
    }

    #[test]
    fn reads_a_placeholder_as_one_name() {
        let parsed = parse("price|or:0 > 0 || value|first").unwrap();
        let expected = Expr::Or(vec![
            Expr::Compare(Op::Gt, name("price|or:0"), Atom::Num(0.0)),
            Expr::Atom(name("value|first")),
        ]);
        assert_eq!(parsed, expected);
    }

    #[test]
    fn a_when_takes_only_paths_for_names() {
        assert!(when("order.total > 0 && _a.b_1").is_ok());
        for (text, said) in [
            ("a|b", "| is written ||"),
            ("a && price|or:0 > 1", "| is written ||"),
            ("$a", "unexpected '$'"),
            ("a:b", "unexpected ':'"),
            ("été", "unexpected 'é'"),
        ] {
            assert_eq!(when(text), Err(said.to_string()), "{text:?}");
        }
    }

    proptest! {
        #[test]
        fn never_panics(text in ".*") {
            let _ = when(&text);
        }
    }
}
