//! A capture: the elements of a running UI, read off it by the platform's
//! UI test tools and written in one neutral form.
//!
//! ```json
//! { "width": 390,
//!   "elements": [ { "id": "reserve-button", "x": 16, "y": 780, "width": 358, "height": 48,
//!                   "text": "预订", "enabled": true } ] }
//! ```

use serde_json::Value;
use underdraw_layout::Rect;

/// One element: its test id, where it is when the UI tool reads frames,
/// and what it says and whether it can be used when the tool reads those.
pub struct Element {
    pub id: String,
    pub frame: Option<Rect>,
    pub text: Option<String>,
    pub enabled: Option<bool>,
}

pub struct Capture {
    /// The width the UI was captured at, in points (CSS pixels on the web).
    pub width: f32,
    pub elements: Vec<Element>,
}

/// The keys of a frame, all or none of which an element has.
const FRAME: [&str; 4] = ["x", "y", "width", "height"];

impl Capture {
    /// `value` read as a capture; every fault in it, each with its fix, when
    /// it is not one.
    pub fn read(value: &Value) -> Result<Capture, Vec<String>> {
        let mut faults = Vec::new();
        let width = match value.get("width").and_then(Value::as_f64) {
            Some(width) if width >= 1.0 => width.min(10_000.0) as f32,
            _ => {
                faults.push(
                    "the capture needs \"width\": the width the UI was captured at, in points, such as 390"
                        .to_string(),
                );
                1.0
            }
        };
        let mut elements = Vec::new();
        match value.get("elements") {
            Some(Value::Array(written)) => {
                for (index, element) in written.iter().enumerate() {
                    match read_element(element) {
                        Ok(element) => elements.push(element),
                        Err(fault) => faults.push(format!("elements[{index}]: {fault}")),
                    }
                }
            }
            _ => faults.push(
                "the capture needs \"elements\": [{\"id\", \"x\", \"y\", \"width\", \"height\", \"text\", \"enabled\"}, …]"
                    .to_string(),
            ),
        }
        if faults.is_empty() { Ok(Capture { width, elements }) } else { Err(faults) }
    }

    /// The first element with `id`.
    pub fn get(&self, id: &str) -> Option<&Element> {
        for element in &self.elements {
            if element.id == id {
                return Some(element);
            }
        }
        None
    }
}

fn read_element(value: &Value) -> Result<Element, String> {
    let Some(id) = value.get("id").and_then(Value::as_str) else {
        return Err("needs \"id\": the element's test id, which is the node's id".to_string());
    };
    let mut numbers = Vec::new();
    for key in FRAME {
        if let Some(n) = value.get(key).and_then(Value::as_f64) {
            numbers.push(n as f32);
        }
    }
    let frame = match numbers.as_slice() {
        [] => None,
        [x, y, width, height] => Some(Rect { x: *x, y: *y, width: *width, height: *height }),
        _ => {
            return Err(format!(
                "{id}: a frame is \"x\", \"y\", \"width\" and \"height\", all numbers, or none"
            ));
        }
    };
    let text = match value.get("text") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => return Err(format!("{id}: \"text\" is the words the element shows, a string")),
    };
    let enabled = match value.get("enabled") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(enabled)) => Some(*enabled),
        Some(_) => return Err(format!("{id}: \"enabled\" is true or false")),
    };
    Ok(Element { id: id.to_string(), frame, text, enabled })
}
