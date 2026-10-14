//! The grammar of a condition's `when` (docs/format/expressions.md). Syntax
//! only: what a name refers to, and whether the types fit, is decided by
//! `conditions`.
//!
//! ```text
//! expr    = and ("||" and)*
//! and     = not ("&&" not)*
//! not     = "!" not | compare
//! compare = atom (("==" | "!=" | "<" | "<=" | ">" | ">=") atom)?
//! atom    = name | literal | "(" expr ")"
//! ```

use flux_rs::attrs::*;

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

    pub fn orders(self) -> bool {
        matches!(self, Op::Lt | Op::Le | Op::Gt | Op::Ge)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Or(Vec<Expr>),
    And(Vec<Expr>),
    Not(Box<Expr>),
    Compare(Op, Atom, Atom),
    Atom(Atom),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Atom {
    /// A path into the data, or the name of another condition.
    Name(String),
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    Group(Box<Expr>),
}

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
    #[spec(fn(tokens: Vec<Token>[@n]) -> Cursor[0, n])]
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
            _ if c.is_ascii_alphabetic() || c == '_' => {
                let mut s = c.to_string();
                while let Some(&d) = chars.peek() {
                    if !(d.is_ascii_alphanumeric() || d == '_' || d == '.') {
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

    proptest! {
        #[test]
        fn never_panics(text in ".*") {
            let _ = parse(&text);
        }
    }
}
