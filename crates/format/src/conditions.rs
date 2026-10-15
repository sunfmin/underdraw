//! The screen's named conditions (docs/format/expressions.md): each read,
//! typed strictly, and evaluated against a fixture. A condition named
//! `Type.name` is declared on a type (ADR 0020): its paths start from the
//! type's fields, and a value of the type reads it as a Bool field, which
//! [`Conditions::fill`] writes into each fixture.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::check::Findings;
use crate::expr::{self, Atom, Expr, Op};
use crate::field::{Base, Field};
use crate::schema::{Schema, Scope, object};

pub struct Condition {
    /// Its name: `hasDates`, or `Order.cancellable` for one on a type.
    pub name: String,
    pub label: String,
    pub when: Expr,
    /// The type it is declared on, when it is.
    pub on: Option<String>,
}

impl Condition {
    /// Its name as a value of its type reads it (`cancellable`), or its
    /// whole name for one over the screen's data.
    pub fn short(&self) -> &str {
        match (&self.on, self.name.split_once('.')) {
            (Some(_), Some((_, short))) => short,
            _ => &self.name,
        }
    }
}

pub struct Conditions(Vec<Condition>);

/// The type of one side of a comparison.
enum Side {
    Str,
    Num,
    Bool,
    Null,
    Field(Box<Field>),
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
            let on = match name.split_once('.') {
                None => None,
                Some((type_name, short)) => {
                    if schema.fields_of(type_name).is_none() {
                        findings.add(
                            &at,
                            &format!("{type_name} is not an object type: a condition on a type is named <Type>.<name>, its type declared in types"),
                        );
                        continue;
                    }
                    if !crate::field::is_identifier(short) {
                        findings
                            .add(&at, &format!("{short:?}: a condition on a type is named <Type>.<name>"));
                        continue;
                    }
                    Some(type_name.to_string())
                }
            };
            match expr::parse(when) {
                Ok(when) => read.push(Condition { name: name.clone(), label, when, on }),
                Err(problem) => findings.add(&at, &problem),
            }
        }
        // Inside a condition on a type, a name is first another of that
        // type's: `cancellable` in one on Order is `Order.cancellable`.
        let mut names = Vec::new();
        for condition in &read {
            names.push(condition.name.clone());
        }
        for condition in &mut read {
            if let Some(on) = &condition.on {
                qualify(&mut condition.when, on, &names);
            }
        }
        let conditions = Conditions(read);
        for condition in &conditions.0 {
            let at = format!("conditions.{}", condition.name);
            let roots = match &condition.on {
                Some(on) => schema.fields_of(on).unwrap_or_default(),
                None => schema.roots(),
            };
            conditions.misnamed(condition, schema, &at, findings);
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

    /// A condition on a type that names a field of it, or uses a condition
    /// it cannot: one over the screen's data has none to read, and one over
    /// the data reaches one on a type only through a value of it.
    fn misnamed(&self, condition: &Condition, schema: &Schema, at: &str, findings: &mut Findings) {
        if let Some(on) = &condition.on
            && schema.fields_of(on).unwrap_or_default().iter().any(|(field, _)| field == condition.short())
        {
            findings
                .add(at, &format!("{on} has a field {}: name the condition otherwise", condition.short()));
        }
        let mut used = Vec::new();
        uses(&condition.when, &mut used);
        for name in &used {
            let Some(other) = self.get(name) else {
                continue;
            };
            match (&condition.on, &other.on) {
                (Some(on), None) => findings.add(
                    at,
                    &format!("{name} is a condition over the screen's data, and one on {on} reads only an {on}'s fields"),
                ),
                (None, Some(other_on)) => findings.add(
                    at,
                    &format!(
                        "{name} is a condition on {other_on}: use it through a value of {other_on}, as <value>.{}",
                        other.short()
                    ),
                ),
                _ => {}
            }
        }
    }

    /// Each fixture's data with every condition on a type written into each
    /// value of that type, as a Bool field of its short name, so whatever
    /// reads the data reads `item.cancellable` as it reads `item.id`.
    pub fn fill(&self, schema: &Schema, fixtures: &mut [(String, Value)]) {
        let roots = schema.roots();
        for (_, data) in fixtures.iter_mut() {
            let Some(fields) = data.as_object_mut() else {
                continue;
            };
            self.fill_fields(schema, &roots, fields);
        }
    }

    fn fill_fields(&self, schema: &Schema, scope: &Scope, fields: &mut Map<String, Value>) {
        for (name, field) in scope {
            if let Some(value) = fields.get_mut(name) {
                self.fill_value(schema, field, value);
            }
        }
    }

    fn fill_value(&self, schema: &Schema, field: &Field, value: &mut Value) {
        if field.array {
            let element = field.element();
            if let Value::Array(items) = value {
                for item in items {
                    self.fill_value(schema, &element, item);
                }
            }
            return;
        }
        let type_name = field.base.name();
        let Some(scope) = schema.fields_of(type_name) else {
            return;
        };
        if !value.is_object() {
            return;
        }
        if let Some(fields) = value.as_object_mut() {
            self.fill_fields(schema, &scope, fields);
        }
        let mut held = Vec::new();
        for condition in &self.0 {
            if condition.on.as_deref() == Some(type_name) {
                held.push((condition.short().to_string(), self.holds(&condition.name, value)));
            }
        }
        if let Some(fields) = value.as_object_mut() {
            for (short, holds) in held {
                fields.insert(short, Value::Bool(holds));
            }
        }
    }

    /// For a condition on a type, whether it holds of some value of the
    /// type in `data`, and whether it fails of some: `data` filled first.
    pub fn of_values(&self, schema: &Schema, name: &str, data: &Value) -> (bool, bool) {
        let mut seen = (false, false);
        if let Some(condition) = self.get(name)
            && let Some(on) = &condition.on
        {
            let mut values = Vec::new();
            for (root, field) in schema.roots() {
                if let Some(value) = data.get(&root) {
                    schema.values_of(on, &field, value, &mut values);
                }
            }
            for value in values {
                match value.get(condition.short()) {
                    Some(Value::Bool(true)) => seen.0 = true,
                    _ => seen.1 = true,
                }
            }
        }
        seen
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
        let mut found = None;
        for used in &names {
            if found.is_none() && self.contains(used) {
                found = self.cycle(used, stack);
            }
        }
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

    /// The same of the condition on a type `name`, read through the value
    /// at `value`: each path it guarantees, from there (`item.note`).
    pub fn guards_through(&self, name: &str, value: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        for path in self.guards(name) {
            found.insert(format!("{value}.{path}"));
        }
        found
    }

    /// Whether `name` is a condition declared on a type.
    pub fn is_on_type(&self, name: &str) -> bool {
        self.get(name).is_some_and(|condition| condition.on.is_some())
    }

    /// The conditions that hold whenever `name` does: itself, and those its
    /// top-level `&&` chain names, and theirs.
    pub fn implied(&self, name: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        self.collect_implied(name, &mut found);
        found
    }

    /// The range `field`, at `path`, is known to lie in wherever each of
    /// `held` holds: its own, narrowed by each comparison of `path` with a
    /// literal in a held condition's top-level `&&` chain. `!= ""` makes a
    /// string at least one long; `== "…"` gives its length; `<`, `<=`, `>`,
    /// `>=` and `==` bound a number.
    pub fn narrowed(&self, held: &BTreeSet<String>, path: &str, field: &Field) -> (Option<f64>, Option<f64>) {
        let (mut lo, mut hi) = (field.lo, field.hi);
        let numeric = matches!(field.base, Base::Int | Base::Number);
        let int = field.base == Base::Int;
        for name in held {
            let Some(condition) = self.get(name) else {
                continue;
            };
            let mut chain = vec![&condition.when];
            while let Some(expr) = chain.pop() {
                let (op, literal) = match expr {
                    Expr::And(items) => {
                        chain.extend(items);
                        continue;
                    }
                    Expr::Atom(Atom::Group(inner)) => {
                        chain.push(inner);
                        continue;
                    }
                    Expr::Compare(op, left, right) => {
                        if is_name(left, path) {
                            (*op, right)
                        } else if is_name(right, path) {
                            (op.flipped(), left)
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                let bound = match literal {
                    Atom::Str(text) if !numeric => length_bound(op, text),
                    Atom::Num(n) if numeric => number_bound(op, *n, int),
                    _ => None,
                };
                let Some((at_least, at_most)) = bound else {
                    continue;
                };
                if let Some(at_least) = at_least {
                    lo = Some(lo.map_or(at_least, |lo| lo.max(at_least)));
                }
                if let Some(at_most) = at_most {
                    hi = Some(hi.map_or(at_most, |hi| hi.min(at_most)));
                }
            }
        }
        (lo, hi)
    }

    fn collect_implied(&self, name: &str, found: &mut BTreeSet<String>) {
        let Some(condition) = self.get(name) else {
            return;
        };
        if !found.insert(name.to_string()) {
            return;
        }
        let mut chain = vec![&condition.when];
        while let Some(expr) = chain.pop() {
            match expr {
                Expr::And(items) => chain.extend(items),
                Expr::Atom(Atom::Group(inner)) => chain.push(inner),
                Expr::Atom(Atom::Name(used)) => self.collect_implied(used, found),
                _ => {}
            }
        }
    }

    fn collect_guards(&self, expr: &Expr, stack: &mut Vec<String>, found: &mut BTreeSet<String>) {
        match expr {
            Expr::And(items) => {
                for item in items {
                    self.collect_guards(item, stack, found);
                }
            }
            Expr::Atom(Atom::Name(name)) => {
                if let Some(condition) = self.get(name)
                    && !stack.contains(name)
                {
                    stack.push(name.clone());
                    self.collect_guards(&condition.when, stack, found);
                    stack.pop();
                }
            }
            Expr::Atom(Atom::Group(inner)) => self.collect_guards(inner, stack, found),
            Expr::Compare(Op::Ne, left, right) => {
                let path = match (left, right) {
                    (Atom::Name(path), Atom::Null) => path,
                    (Atom::Null, Atom::Name(path)) => path,
                    _ => return,
                };
                if !self.contains(path) {
                    found.insert(path.clone());
                }
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
                Some(resolved) => Some(Side::Field(Box::new(resolved.field))),
                None => {
                    match schema.misused(roots, path) {
                        Some(problem) => findings.add(at, &problem),
                        None => {
                            findings.add(at, &format!("{path} is neither a condition nor a path in data"))
                        }
                    }
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
                None => {
                    let mut at = Some(data);
                    for segment in name.split('.') {
                        at = at.and_then(|v| v.get(segment));
                    }
                    at.cloned().unwrap_or(Value::Null)
                }
            },
        }
    }
}

/// `expr` with each name that is a condition on the type `on`, written
/// short (`cancellable`), given its whole name (`Order.cancellable`).
fn qualify(expr: &mut Expr, on: &str, names: &[String]) {
    let atom = |atom: &mut Atom| match atom {
        Atom::Name(name) => {
            let whole = format!("{on}.{name}");
            if names.contains(&whole) {
                *name = whole;
            }
        }
        Atom::Group(inner) => qualify(inner, on, names),
        _ => {}
    };
    match expr {
        Expr::Or(items) | Expr::And(items) => {
            for item in items {
                qualify(item, on, names);
            }
        }
        Expr::Not(inner) => qualify(inner, on, names),
        Expr::Atom(a) => atom(a),
        Expr::Compare(_, l, r) => {
            atom(l);
            atom(r);
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
        Expr::Or(items) | Expr::And(items) => {
            for item in items {
                uses(item, names);
            }
        }
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

/// Where a node or a branch asks a yes/no (`if`, `disabled`): `!` to turn
/// it round, then a condition's name or a Bool path.
pub(crate) struct UseSite<'a> {
    pub(crate) negated: bool,
    pub(crate) name: &'a str,
}

impl<'a> UseSite<'a> {
    pub(crate) fn parse(used: &'a str) -> UseSite<'a> {
        match used.strip_prefix('!') {
            Some(name) => UseSite { negated: true, name },
            None => UseSite { negated: false, name: used },
        }
    }
}

/// Whether `atom` is the path `path`.
fn is_name(atom: &Atom, path: &str) -> bool {
    match atom {
        Atom::Name(name) => name == path,
        _ => false,
    }
}

/// What `length op text` says of a string's length: at least one long
/// for `!= ""`, as long as `text` for `== text`.
fn length_bound(op: Op, text: &str) -> Option<(Option<f64>, Option<f64>)> {
    match op {
        Op::Ne if text.is_empty() => Some((Some(1.0), None)),
        Op::Eq => {
            let length = text.chars().count() as f64;
            Some((Some(length), Some(length)))
        }
        _ => None,
    }
}

/// What `value op n` says of a number: its least and its most, each
/// whole where it is an `int`.
fn number_bound(op: Op, n: f64, int: bool) -> Option<(Option<f64>, Option<f64>)> {
    match op {
        Op::Eq => Some((Some(n), Some(n))),
        Op::Ge => Some((Some(if int { n.ceil() } else { n }), None)),
        Op::Gt => Some((Some(if int { n.floor() + 1.0 } else { n }), None)),
        Op::Le => Some((None, Some(if int { n.floor() } else { n }))),
        Op::Lt => Some((None, Some(if int { n.ceil() - 1.0 } else { n }))),
        Op::Ne => None,
    }
}
