//! A component the project shares, shown on its own (docs/adr/0021): its
//! `components/<Name>.json` carries `fixtures` of its params, and is read
//! as a description of one instance of it, its params the view model and
//! each fixture a state. So `check` holds each fixture to the params, and
//! the gallery draws it, and pins notes on its nodes, as it does a
//! screen's.

use serde_json::{Map, Value};

/// The key a showcase holds the name of the component it shows under: it
/// declares no screen a navigate can reach.
pub const KEY: &str = "$showcase";

/// The description that shows the shared component `name`, whose file
/// holds `component`, in each of its fixtures: one instance of it, named
/// as its name is in kebab case (`OrderRow` is `order-row`, so its
/// `cancel` is `order-row-cancel`), given each param from the fixture.
/// A typed param (`{"order": "Order"}`) is a field of that type, held as
/// view state: a component's set or toggle on a param acts on what was
/// passed, and here the showcase alone holds it. A look's param
/// (`["status"]`) takes text, a number or true or false, as the first
/// fixture to give it does.
pub fn showcase(name: &str, component: &Value) -> Value {
    let fixtures = component.get("fixtures").cloned().unwrap_or_else(|| Value::Object(Map::new()));
    let mut data = Map::new();
    let mut instance = Map::new();
    instance.insert("id".to_string(), Value::String(kebab(name)));
    instance.insert("component".to_string(), Value::String(name.to_string()));
    match component.get("params") {
        Some(Value::Object(params)) => {
            let mut with = Map::new();
            for (param, field) in params {
                data.insert(param.clone(), held(field));
                with.insert(param.clone(), Value::String(format!("{{{param}}}")));
            }
            instance.insert("with".to_string(), Value::Object(with));
        }
        Some(Value::Array(params)) => {
            for param in params.iter().filter_map(Value::as_str) {
                data.insert(param.to_string(), Value::String(given(&fixtures, param).to_string()));
                instance.insert(param.to_string(), Value::String(format!("{{{param}}}")));
            }
        }
        _ => {}
    }
    let mut root = Map::new();
    root.insert("id".to_string(), Value::String("showcase".to_string()));
    root.insert("component".to_string(), Value::String("Screen".to_string()));
    root.insert("padding".to_string(), Value::from(16));
    root.insert("children".to_string(), Value::Array(vec![Value::Object(instance)]));
    let mut shown = Map::new();
    shown.insert("screen".to_string(), Value::String(name.to_string()));
    shown.insert(KEY.to_string(), Value::String(name.to_string()));
    shown.insert("data".to_string(), Value::Object(data));
    shown.insert("fixtures".to_string(), fixtures);
    shown.insert("root".to_string(), Value::Object(root));
    Value::Object(shown)
}

/// The name of the shared component `screen` shows, when it is a
/// showcase.
pub fn shows(screen: &Value) -> Option<&str> {
    screen.get(KEY).and_then(Value::as_str)
}

/// The param `field` as a field of the showcase's data, marked view
/// state. One that says it is filled or derived is left as written, so its
/// own finding is not joined by another.
fn held(field: &Value) -> Value {
    let mut object = match field {
        Value::String(_) => {
            let mut object = Map::new();
            object.insert("type".to_string(), field.clone());
            object
        }
        Value::Object(object) if !object.contains_key("from") && !object.contains_key("derived") => {
            object.clone()
        }
        _ => return field.clone(),
    };
    object.insert("view".to_string(), Value::Bool(true));
    Value::Object(object)
}

/// The type of what the first of `fixtures` to give the look's param
/// `param` gives: `string` when it is text, or none gives it.
fn given(fixtures: &Value, param: &str) -> &'static str {
    let Some(fixtures) = fixtures.as_object() else {
        return "string";
    };
    for fixture in fixtures.values() {
        match fixture.get(param) {
            Some(Value::Number(_)) => return "number",
            Some(Value::Bool(_)) => return "bool",
            Some(Value::Null) | None => {}
            Some(_) => return "string",
        }
    }
    "string"
}

/// `name` in kebab case: `OrderRow` is `order-row`.
fn kebab(name: &str) -> String {
    let mut kebab = String::new();
    for c in name.chars() {
        if c.is_uppercase() {
            if !kebab.is_empty() {
                kebab.push('-');
            }
            kebab.extend(c.to_lowercase());
        } else {
            kebab.push(c);
        }
    }
    kebab
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_component_is_shown_as_one_instance_given_each_param() {
        let row = json!({ "params": { "order": "Order" }, "fixtures": { "pending": { "order": {} } }, "nodes": {} });
        let shown = showcase("OrderRow", &row);
        assert_eq!(shows(&shown), Some("OrderRow"));
        assert_eq!(shown["data"], json!({ "order": { "type": "Order", "view": true } }));
        assert_eq!(shown["fixtures"], row["fixtures"]);
        assert_eq!(
            shown["root"]["children"][0],
            json!({ "id": "order-row", "component": "OrderRow", "with": { "order": "{order}" } })
        );

        let pill = json!({ "params": ["status", "count", "on"], "fixtures": {
            "a": { "status": null, "on": true }, "b": { "status": "pending", "count": 2 } } });
        let shown = showcase("OrderStatus", &pill);
        assert_eq!(shown["data"], json!({ "status": "string", "count": "number", "on": "bool" }));
        assert_eq!(
            shown["root"]["children"][0],
            json!({ "id": "order-status", "component": "OrderStatus",
                    "status": "{status}", "count": "{count}", "on": "{on}" })
        );
        assert_eq!(showcase("Chip", &json!({ "look": {} }))["fixtures"], json!({}), "none written");
        assert_eq!(shows(&json!({ "screen": "Orders" })), None);
    }
}
