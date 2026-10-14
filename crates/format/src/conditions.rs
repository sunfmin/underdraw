//! The screen's named conditions (docs/format/expressions.md): each read,
//! typed strictly, and evaluated against a fixture.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::check::Findings;
use crate::expr::{self, Atom, Expr, Op};
use crate::field::{Base, Field};
use crate::schema::{Schema, Scope, object};

pub struct Condition {
    pub name: String,
    pub label: String,
    when: Expr,
}

pub struct Conditions(Vec<Condition>);

/// The type of one side of a comparison.
enum Side {
    Str,
    Num,
    Bool,
    Null,
    Field(Field),
}

impl Conditions {
    pub fn read(screen: &Value, schema: &Schema, findings: &mut Findings) -> Conditions {
        let mut read = Vec::new();
        for (name, body) in object(screen, "conditions") {
            let at = format!("conditions.{name}");
            let label = body.get("label").and_then(Value::as_str).unwrap_or_default().to_string();
            let Some(when) = body.get("when").and_then(Value::as_str) else {
                findings.add(&at, "a condition is {\"when\": …, \"label\": …}");
                continue;
            };
            match expr::parse(when) {
                Ok(when) => read.push(Condition { name: name.clone(), label, when }),
                Err(problem) => findings.add(&at, &problem),
            }
        }
        let conditions = Conditions(read);
        let roots = schema.roots();
        for condition in &conditions.0 {
            let at = format!("conditions.{}", condition.name);
            conditions.typecheck(&condition.when, schema, &roots, &at, findings);
            if let Some(cycle) = conditions.cycle(&condition.name, &mut Vec::new()) {
                findings.add(&at, &format!("refers to itself: {}", cycle.join(" → ")));
            }
        }
        conditions
    }

    pub fn all(&self) -> &[Condition] {
        &self.0
    }

    fn get(&self, name: &str) -> Option<&Condition> {
        self.0.iter().find(|c| c.name == name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// The path of names from `name` back to itself, if it uses itself.
    fn cycle(&self, name: &str, stack: &mut Vec<String>) -> Option<Vec<String>> {
        if stack.iter().any(|n| n == name) {
            let mut path = stack.clone();
            path.push(name.to_string());
            return Some(path);
        }
        let condition = self.get(name)?;
        stack.push(name.to_string());
        let mut names = Vec::new();
        uses(&condition.when, &mut names);
        let found = names.iter().filter(|n| self.contains(n)).find_map(|n| self.cycle(n, stack));
        stack.pop();
        found
    }

    /// Whether the condition `name` holds of `data`.
    pub fn holds(&self, name: &str, data: &Value) -> bool {
        self.get(name).is_some_and(|c| self.eval(&c.when, data, &mut vec![name.to_string()]))
    }

    /// The paths a condition guarantees are not null when it holds: each
    /// `path != null` in its top-level `&&` chain, and the same of the
    /// conditions named there.
    pub fn guards(&self, name: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        if let Some(condition) = self.get(name) {
            self.collect_guards(&condition.when, &mut vec![name.to_string()], &mut found);
        }
        found
    }

    fn collect_guards(&self, expr: &Expr, stack: &mut Vec<String>, found: &mut BTreeSet<String>) {
        match expr {
            Expr::And(items) => {
                for item in items {
                    self.collect_guards(item, stack, found);
                }
            }
            Expr::Atom(Atom::Name(name)) if !stack.contains(name) => {
                if let Some(condition) = self.get(name) {
                    stack.push(name.clone());
                    self.collect_guards(&condition.when, stack, found);
                    stack.pop();
                }
            }
            Expr::Atom(Atom::Group(inner)) => self.collect_guards(inner, stack, found),
            Expr::Compare(Op::Ne, Atom::Name(path), Atom::Null)
            | Expr::Compare(Op::Ne, Atom::Null, Atom::Name(path))
                if !self.contains(path) =>
            {
                found.insert(path.clone());
            }
            _ => {}
        }
    }

    fn typecheck(&self, expr: &Expr, schema: &Schema, roots: &Scope, at: &str, findings: &mut Findings) {
        match expr {
            Expr::Or(items) | Expr::And(items) => {
                for item in items {
                    self.typecheck(item, schema, roots, at, findings);
                }
            }
            Expr::Not(inner) => self.typecheck(inner, schema, roots, at, findings),
            Expr::Atom(atom) => match self.side(atom, schema, roots, at, findings) {
                Some(Side::Bool) | None => {}
                Some(Side::Field(field)) if field.is_plain_bool() => {}
                Some(Side::Field(field)) => findings.add(
                    at,
                    &format!(
                        "{} is {}, not a Bool: there is no truthiness; write != null, > 0 or == \"…\"",
                        show(atom),
                        field.describe()
                    ),
                ),
                Some(_) => findings.add(at, &format!("{} is not a condition", show(atom))),
            },
            Expr::Compare(op, left, right) => {
                let (Some(l), Some(r)) = (
                    self.side(left, schema, roots, at, findings),
                    self.side(right, schema, roots, at, findings),
                ) else {
                    return;
                };
                self.compare(*op, (left, &l), (right, &r), schema, at, findings);
            }
        }
    }

    fn compare(
        &self,
        op: Op,
        (left, l): (&Atom, &Side),
        (right, r): (&Atom, &Side),
        schema: &Schema,
        at: &str,
        findings: &mut Findings,
    ) {
        let (lk, rk) = (kind(l, schema), kind(r, schema));
        if lk == "null" || rk == "null" {
            let other = if lk == "null" { r } else { l };
            if op.orders() {
                findings.add(at, "null is compared only with == or !=");
            } else if let Side::Field(field) = other
                && !field.nullable
            {
                findings.add(
                    at,
                    &format!("{} cannot be null, so this comparison never changes", field.describe()),
                );
            }
            return;
        }
        if lk == "enum" || rk == "enum" {
            let (field, literal) = if lk == "enum" { (l, right) } else { (r, left) };
            let members = match field {
                Side::Field(field) => schema.members(field).unwrap_or_default(),
                _ => &[],
            };
            match literal {
                Atom::Str(s) if members.iter().any(|m| m == s) => {}
                Atom::Str(s) => findings.add(at, &format!("{s:?} is not a member: {members:?}")),
                _ => findings.add(at, "an enum is compared only with a string literal"),
            }
            return;
        }
        if lk != rk {
            findings.add(at, &format!("cannot compare {} with {}", show(left), show(right)));
        } else if op.orders() && lk != "number" && lk != "date" {
            findings.add(at, &format!("{lk} values have no order; {} needs numbers or dates", op.symbol()));
        }
    }

    fn side(
        &self,
        atom: &Atom,
        schema: &Schema,
        roots: &Scope,
        at: &str,
        findings: &mut Findings,
    ) -> Option<Side> {
        match atom {
            Atom::Str(_) => Some(Side::Str),
            Atom::Num(_) => Some(Side::Num),
            Atom::Bool(_) => Some(Side::Bool),
            Atom::Null => Some(Side::Null),
            Atom::Group(inner) => {
                self.typecheck(inner, schema, roots, at, findings);
                Some(Side::Bool)
            }
            Atom::Name(name) if self.contains(name) => Some(Side::Bool),
            Atom::Name(path) => match schema.resolve(roots, path) {
                Some(resolved) => Some(Side::Field(resolved.field)),
                None => {
                    findings.add(at, &format!("{path} is neither a condition nor a path in data"));
                    None
                }
            },
        }
    }

    fn eval(&self, expr: &Expr, data: &Value, stack: &mut Vec<String>) -> bool {
        match expr {
            Expr::Or(items) => items.iter().any(|e| self.eval(e, data, stack)),
            Expr::And(items) => items.iter().all(|e| self.eval(e, data, stack)),
            Expr::Not(inner) => !self.eval(inner, data, stack),
            Expr::Atom(atom) => self.value(atom, data, stack) == Value::Bool(true),
            Expr::Compare(op, left, right) => {
                let (l, r) = (self.value(left, data, stack), self.value(right, data, stack));
                match op {
                    Op::Eq => same(&l, &r),
                    Op::Ne => !same(&l, &r),
                    _ => match order(&l, &r) {
                        Some(ordering) => match op {
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
    }

    fn value(&self, atom: &Atom, data: &Value, stack: &mut Vec<String>) -> Value {
        match atom {
            Atom::Str(s) => Value::from(s.as_str()),
            Atom::Num(n) => Value::from(*n),
            Atom::Bool(b) => Value::Bool(*b),
            Atom::Null => Value::Null,
            Atom::Group(inner) => Value::Bool(self.eval(inner, data, stack)),
            Atom::Name(name) => match self.get(name) {
                Some(_) if stack.contains(name) => Value::Bool(false),
                Some(condition) => {
                    stack.push(name.clone());
                    let holds = self.eval(&condition.when, data, stack);
                    stack.pop();
                    Value::Bool(holds)
                }
                None => name.split('.').try_fold(data, |v, seg| v.get(seg)).cloned().unwrap_or(Value::Null),
            },
        }
    }
}

/// The names `expr` uses: paths and conditions alike.
fn uses(expr: &Expr, names: &mut Vec<String>) {
    let atom = |atom: &Atom, names: &mut Vec<String>| match atom {
        Atom::Name(name) => names.push(name.clone()),
        Atom::Group(inner) => uses(inner, names),
        _ => {}
    };
    match expr {
        Expr::Or(items) | Expr::And(items) => items.iter().for_each(|e| uses(e, names)),
        Expr::Not(inner) => uses(inner, names),
        Expr::Atom(a) => atom(a, names),
        Expr::Compare(_, l, r) => {
            atom(l, names);
            atom(r, names);
        }
    }
}

fn kind(side: &Side, schema: &Schema) -> &'static str {
    match side {
        Side::Str => "string",
        Side::Num => "number",
        Side::Bool => "bool",
        Side::Null => "null",
        Side::Field(field) if field.array => "array",
        Side::Field(field) if schema.members(field).is_some() => "enum",
        Side::Field(field) => match field.base {
            Base::Int | Base::Number => "number",
            Base::Bool => "bool",
            Base::Date => "date",
            Base::String | Base::Url | Base::Icon | Base::Image => "string",
            Base::Money => "money",
            Base::Named(_) => "object",
        },
    }
}

fn show(atom: &Atom) -> String {
    match atom {
        Atom::Name(n) => n.clone(),
        Atom::Str(s) => format!("{s:?}"),
        Atom::Num(n) => n.to_string(),
        Atom::Bool(b) => b.to_string(),
        Atom::Null => "null".to_string(),
        Atom::Group(_) => "(…)".to_string(),
    }
}

/// Equal as the format means it: numbers by value, whether written 5 or 5.0.
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
