//! One field of the view model, in the notation of docs/format/data.md:
//! `type range? default?`, such as `string 1..50`, `number? 0..5`,
//! `bool = false`.

use serde_json::Value;

/// A built-in type, or the name of one declared in `types`.
#[derive(Clone, Debug, PartialEq)]
pub enum Base {
    String,
    Int,
    Number,
    Bool,
    Date,
    Url,
    Image,
    Icon,
    Money,
    Named(String),
}

impl Base {
    fn from_name(name: &str) -> Base {
        match name {
            "string" => Base::String,
            "int" => Base::Int,
            "number" => Base::Number,
            "bool" => Base::Bool,
            "date" => Base::Date,
            "url" => Base::Url,
            "image" => Base::Image,
            "icon" => Base::Icon,
            "money" => Base::Money,
            other => Base::Named(other.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Base::String => "string",
            Base::Int => "int",
            Base::Number => "number",
            Base::Bool => "bool",
            Base::Date => "date",
            Base::Url => "url",
            Base::Image => "image",
            Base::Icon => "icon",
            Base::Money => "money",
            Base::Named(name) => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub base: Base,
    pub array: bool,
    pub nullable: bool,
    /// The range: a number's value, a string's length, an array's count.
    pub lo: Option<f64>,
    pub hi: Option<f64>,
    /// What a fixture that leaves the field out holds. A nullable field's is
    /// null; a field without one must be written.
    pub default: Option<Value>,
    /// What it means, when the description says.
    pub doc: Option<String>,
    /// Who computes it and how, for a derived field.
    pub derived: Option<Derived>,
    /// The query whose result fills it: for a field of `data` the server
    /// answers, never one the screen or the app holds.
    pub from: Option<String>,
    /// True of view state: what only the screen holds (an open menu, a
    /// popup's subject), never domain data an operation answers with.
    pub view: bool,
}

impl Field {
    pub fn of(base: Base) -> Field {
        Field {
            base,
            array: false,
            nullable: false,
            lo: None,
            hi: None,
            default: None,
            doc: None,
            derived: None,
            from: None,
            view: false,
        }
    }

    /// The type of one element of this field, when it is an array.
    pub fn element(&self) -> Field {
        Field::of(self.base.clone())
    }

    /// Whether a value of it is one yes/no, null or not: a bool position,
    /// which takes an expression (`"!canBack"`).
    pub fn is_bool(&self) -> bool {
        self.base == Base::Bool && !self.array
    }

    /// True of a non-null, non-array Bool: what a condition may test as is.
    pub fn is_plain_bool(&self) -> bool {
        self.base == Base::Bool && !self.array && !self.nullable
    }

    /// The type as written, without range or default: `Highlight[]`, `date?`.
    pub fn describe(&self) -> String {
        let array = if self.array { "[]" } else { "" };
        let null = if self.nullable { "?" } else { "" };
        format!("{}{array}{null}", self.base.name())
    }

    /// The range as written, either side open: `1..20`, `..20`, `0..`;
    /// none when both are.
    pub fn range(&self) -> Option<String> {
        if self.lo.is_none() && self.hi.is_none() {
            return None;
        }
        let side = |bound: Option<f64>| bound.map(|bound| bound.to_string()).unwrap_or_default();
        Some(format!("{}..{}", side(self.lo), side(self.hi)))
    }

    /// Whether `size` (a number's value, a string's length, a list's count)
    /// lies inside the range.
    pub fn holds_size(&self, size: f64) -> bool {
        self.lo.is_none_or(|lo| size >= lo) && self.hi.is_none_or(|hi| size <= hi)
    }
}

/// Who computes a derived field: the app, for the screen alone, or the
/// server, which answers with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum By {
    App,
    Server,
}

impl By {
    pub fn name(self) -> &'static str {
        match self {
            By::App => "app",
            By::Server => "server",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Derived {
    pub by: By,
    /// How, in one line: `docs' length`.
    pub how: String,
}

/// Reads a field written either as a type string or as an object with `type`,
/// and optionally `doc`, `derived`, `from` and `view`.
pub fn read(spec: &Value) -> Result<Field, String> {
    match spec {
        Value::String(text) => parse(text),
        Value::Object(object) => {
            if let Some(key) = object
                .keys()
                .find(|key| !matches!(key.as_str(), "type" | "doc" | "derived" | "from" | "view"))
            {
                return Err(format!(
                    "a field object holds only type, doc, derived, from and view, not {key}"
                ));
            }
            let mut field = match object.get("type") {
                Some(Value::String(text)) => parse(text)?,
                _ => return Err("a field object needs a `type` string".to_string()),
            };
            field.doc = object.get("doc").and_then(Value::as_str).map(str::to_string);
            field.derived = match object.get("derived") {
                None => None,
                Some(written) => Some(derived(written)?),
            };
            field.from = match object.get("from") {
                None => None,
                Some(Value::String(query)) => Some(query.clone()),
                Some(_) => {
                    return Err("from names the query that fills it: \"from\": \"listDocs\"".to_string());
                }
            };
            field.view = match object.get("view") {
                None => false,
                Some(Value::Bool(view)) => *view,
                Some(_) => return Err("view is true or false".to_string()),
            };
            Ok(field)
        }
        _ => Err("a field is a type string, or an object with a `type`".to_string()),
    }
}

/// `{"app": how}` or `{"server": how}`: who derives a field, and how.
fn derived(written: &Value) -> Result<Derived, String> {
    let entry =
        written.as_object().filter(|object| object.len() == 1).and_then(|object| object.iter().next());
    match entry {
        Some((by, Value::String(how))) if by == "app" => Ok(Derived { by: By::App, how: how.clone() }),
        Some((by, Value::String(how))) if by == "server" => Ok(Derived { by: By::Server, how: how.clone() }),
        _ => Err("derived says who computes it, and how: {\"app\": \"docs' length\"} for what the app works out for the screen, {\"server\": \"the person owns it\"} for what the server answers with".to_string()),
    }
}

pub fn parse(text: &str) -> Result<Field, String> {
    let (written, default) = match text.split_once('=') {
        Some((written, default)) => (written.trim(), Some(default.trim())),
        None => (text.trim(), None),
    };
    let mut words = written.split_whitespace();
    let Some(name) = words.next() else {
        return Err("the type is empty".to_string());
    };
    let range = words.next();
    if let Some(extra) = words.next() {
        return Err(format!("{text:?}: unexpected {extra:?} after the range"));
    }
    let (name, nullable) = strip(name, "?");
    let (name, array) = strip(name, "[]");
    if !is_identifier(name) {
        return Err(format!("{text:?}: {name:?} is not a type name"));
    }
    let (lo, hi) = match range {
        None => (None, None),
        Some(range) => {
            let Some((lo, hi)) = range.split_once("..") else {
                return Err(format!("{text:?}: a range is written lo..hi, either side open"));
            };
            (bound(lo, text)?, bound(hi, text)?)
        }
    };
    if let (Some(lo), Some(hi)) = (lo, hi)
        && lo > hi
    {
        return Err(format!("{text:?}: the range runs backwards"));
    }
    let default = match default {
        None => nullable.then_some(Value::Null),
        Some(literal) => Some(
            serde_json::from_str(literal)
                .map_err(|_| format!("{text:?}: default {literal} is not a JSON literal"))?,
        ),
    };
    if array && default.as_ref().is_some_and(|value| !value.is_null()) {
        return Err(format!("{text:?}: an array has no default; write [] in the fixture"));
    }
    Ok(Field {
        base: Base::from_name(name),
        array,
        nullable,
        lo,
        hi,
        default,
        doc: None,
        derived: None,
        from: None,
        view: false,
    })
}

fn strip<'a>(name: &'a str, suffix: &str) -> (&'a str, bool) {
    match name.strip_suffix(suffix) {
        Some(stripped) => (stripped, true),
        None => (name, false),
    }
}

fn bound(side: &str, text: &str) -> Result<Option<f64>, String> {
    if side.is_empty() {
        return Ok(None);
    }
    side.parse().map(Some).map_err(|_| format!("{text:?}: {side:?} is not a number"))
}

/// A name: a letter or `_`, then letters, digits and `_`.
pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_part() {
        let cases = [
            ("string 1..50", "string", false, false, Some(1.0), Some(50.0), None),
            ("number? 0..5", "number", false, true, Some(0.0), Some(5.0), Some(Value::Null)),
            ("image[] 5..", "image", true, false, Some(5.0), None, None),
            ("bool = false", "bool", false, false, None, None, Some(Value::Bool(false))),
            ("Highlight[] ..3", "Highlight", true, false, None, Some(3.0), None),
            ("string ..80 = \"\"", "string", false, false, None, Some(80.0), Some(Value::from(""))),
        ];
        for (text, base, array, nullable, lo, hi, default) in cases {
            let field = parse(text).unwrap();
            assert_eq!(
                (field.base.name(), field.array, field.nullable, field.lo, field.hi, field.default),
                (base, array, nullable, lo, hi, default),
                "{text}"
            );
        }
    }

    #[test]
    fn refuses_what_it_cannot_mean() {
        for text in ["", "string 5..1", "image[] = [\"a\"]", "int 1..x", "two words here", "bad-name"] {
            assert!(parse(text).is_err(), "{text:?} should be refused");
        }
    }
}
