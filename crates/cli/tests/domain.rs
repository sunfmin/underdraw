//! A project whose screens share one domain (ADR 0020), run with the real
//! binary: drafts/shop's two screens each call cancelOrder, declared only
//! in its domain.json, and are checked, rendered, handed off and given a
//! CLI as the same screens declaring it themselves are.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value, json};

const SHOP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/shop");
const SCREENS: [&str; 2] = ["orders", "order"];

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Each component the shop shares, by its name, as its file writes it.
fn shared_components() -> Vec<(String, Value)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(Path::new(SHOP).join("components")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        found.push((name, read(&path)));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// A fresh, empty directory of the test's own, named `name`.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("underdraw-domain-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A copy of the shop at `to`, each screen's screen.json given by `screen`
/// from the one written; with its domain.json and the components it
/// shares when `domain` holds.
fn copy_shop(to: &Path, domain: bool, screen: &dyn Fn(Value) -> Value) {
    std::fs::copy(Path::new(SHOP).join("tokens.json"), to.join("tokens.json")).unwrap();
    if domain {
        std::fs::copy(Path::new(SHOP).join("domain.json"), to.join("domain.json")).unwrap();
        std::fs::create_dir_all(to.join("components")).unwrap();
        for (name, _) in shared_components() {
            let file = format!("components/{name}.json");
            std::fs::copy(Path::new(SHOP).join(&file), to.join(&file)).unwrap();
        }
    }
    for name in SCREENS {
        let (from, dir) = (Path::new(SHOP).join(name), to.join(name));
        std::fs::create_dir_all(dir.join("platforms")).unwrap();
        std::fs::copy(from.join("platforms/web.json"), dir.join("platforms/web.json")).unwrap();
        let written = screen(read(&from.join("screen.json")));
        std::fs::write(dir.join("screen.json"), written.to_string()).unwrap();
    }
}

#[test]
fn two_screens_call_an_operation_only_the_domain_declares() {
    let domain = read(&Path::new(SHOP).join("domain.json"));
    assert!(domain["operations"].get("cancelOrder").is_some());
    let out = scratch("renders");
    for name in SCREENS {
        let dir = Path::new(SHOP).join(name);
        let written = read(&dir.join("screen.json"));
        let declares = written.get("operations").and_then(|ops| ops.get("cancelOrder"));
        assert_eq!(declares, None, "{name} declares cancelOrder nowhere");
        let path = dir.to_str().unwrap();

        let (checked, code) = underdraw(&["check", path]);
        assert_eq!((code, &checked["ok"]), (0, &json!(true)), "{checked}");

        let png = out.join(format!("{name}.png"));
        let (rendered, code) = underdraw(&["render", path, "--out", png.to_str().unwrap()]);
        assert_eq!(code, 0, "{rendered}");
        let root = Path::new(rendered["project"].as_str().unwrap()).canonicalize().unwrap();
        assert_eq!(root, Path::new(SHOP).canonicalize().unwrap(), "the root is where domain.json is");

        let (handed, code) = underdraw(&["handoff", path, "--platform", "web"]);
        assert_eq!((code, &handed["ok"]), (0, &json!(true)), "{handed}");
        let calls: Vec<&Value> = handed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|n| n["events"].as_array().into_iter().flatten())
            .collect();
        assert!(calls.iter().any(|event| event["operation"] == "cancelOrder"), "{calls:?}");
    }
    let (checked, code) = underdraw(&["check", SHOP]);
    assert_eq!((code, &checked["ok"]), (0, &json!(true)), "{checked}");
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn what_is_derived_is_the_same_as_when_each_screen_declares_the_domain() {
    let domain = read(&Path::new(SHOP).join("domain.json"));
    let shared = scratch("shared");
    copy_shop(&shared, true, &|screen| screen);
    let declared = scratch("declared");
    copy_shop(&declared, false, &|mut screen| {
        for key in ["types", "conditions", "operations"] {
            let Some(theirs) = domain.get(key).and_then(Value::as_object) else { continue };
            let mut merged: Map<String, Value> = theirs.clone();
            if let Some(Value::Object(own)) = screen.get(key) {
                merged.extend(own.clone());
            }
            screen[key] = Value::Object(merged);
        }
        for (name, component) in shared_components() {
            screen["components"][name] = component;
        }
        screen
    });
    for name in SCREENS {
        let (with, without) = (shared.join(name), declared.join(name));
        let derived = |command: &[&str], dir: &Path| {
            let mut args = command.to_vec();
            args.insert(1, dir.to_str().unwrap());
            let (answer, code) = underdraw(&args);
            assert_eq!(code, 0, "{answer}");
            answer
        };
        for command in [&["cli"][..], &["handoff", "--platform", "web"], &["schema"]] {
            assert_eq!(derived(command, &with), derived(command, &without), "{name}: {command:?}");
        }
        let golden = derived(&["cli"], &with)["golden"].clone();
        assert!(golden.as_array().unwrap().iter().any(|g| g["command"] == "cancel-order"), "{golden}");
    }
    std::fs::remove_dir_all(shared).unwrap();
    std::fs::remove_dir_all(declared).unwrap();
}

#[test]
fn check_names_a_screens_own_that_shadows_the_domains() {
    let project = scratch("shadows");
    copy_shop(&project, true, &|mut screen| {
        screen["types"] = json!({"Order": {"id": "string"}});
        let mut operations = screen.get("operations").cloned().unwrap_or(json!({}));
        operations["cancelOrder"] = json!({"command": "Cancels it", "params": {"id": "string"}});
        screen["operations"] = operations;
        screen
    });
    std::fs::write(
        project.join("domain.json"),
        {
            let mut domain = read(&Path::new(SHOP).join("domain.json"));
            domain["conditions"] = json!({"always": {"when": "true", "label": "Always"}});
            domain
        }
        .to_string(),
    )
    .unwrap();
    let dir = project.join("order");
    let mut screen = read(&dir.join("screen.json"));
    screen["conditions"] = json!({"always": {"when": "true", "label": "Mine"}});
    std::fs::write(dir.join("screen.json"), screen.to_string()).unwrap();

    let (checked, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!((code, &checked["ok"]), (1, &json!(false)), "{checked}");
    let shadows: Vec<&str> = checked["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["problem"].as_str().unwrap().contains("shadows the domain's"))
        .map(|f| f["at"].as_str().unwrap())
        .collect();
    assert_eq!(shadows, ["types.Order", "conditions.always", "operations.cancelOrder"], "{checked}");

    let (refused, code) = underdraw(&["cli", dir.to_str().unwrap()]);
    assert_eq!(code, 2, "derived from nothing while it shadows: {refused}");

    std::fs::write(project.join("domain.json"), "{").unwrap();
    let (refused, code) = underdraw(&["check", dir.to_str().unwrap()]);
    assert_eq!(code, 2, "{refused}");
    assert!(refused["error"].as_str().unwrap().contains("domain.json is not JSON"), "{refused}");
    std::fs::remove_dir_all(project).unwrap();
}

#[test]
fn help_documents_domain_json() {
    let help = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    assert!(help(&["--help"]).contains("domain.json at the project root"));
    let check = help(&["check", "--help"]);
    assert!(check.contains("DOMAIN.JSON") && check.contains("shadows the domain's"), "{check}");
    assert!(help(&["cli", "--help"]).contains("domain.json"));
    assert!(help(&["handoff", "--help"]).contains("domain.json"));
}

#[test]
fn a_condition_on_a_type_gates_each_call_that_requires_it() {
    let domain = read(&Path::new(SHOP).join("domain.json"));
    assert!(domain["conditions"].get("Order.cancellable").is_some());
    assert_eq!(domain["operations"]["cancelOrder"]["requires"], "Order.cancellable");
    let (checked, code) = underdraw(&["check", Path::new(SHOP).join("orders").to_str().unwrap()]);
    assert_eq!((code, &checked["ok"]), (0, &json!(true)), "{checked}");
    assert_eq!(checked["conditions"][0]["name"], "Order.cancellable", "{checked}");

    let project = scratch("type-condition");
    copy_shop(&project, true, &|screen| screen);
    // The cancel button is in the OrderRow both screens share.
    let row = project.join("components/OrderRow.json");
    let written = std::fs::read_to_string(&row).unwrap();
    let gate = r#""if": "order.cancellable", "#;
    assert!(written.contains(gate), "{written}");
    let problems = |name: &str, gate: &str| {
        std::fs::write(&row, written.replace(r#""if": "order.cancellable", "#, gate)).unwrap();
        let (checked, code) = underdraw(&["check", project.join(name).to_str().unwrap()]);
        assert_eq!(code, 1, "{checked}");
        checked["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["problem"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    for name in SCREENS {
        let ungated = problems(name, "");
        assert!(
            ungated.iter().any(|p| p.contains("calls cancelOrder, which requires Order.cancellable")
                && p.contains("\"!order.cancellable\"")),
            "{name}: {ungated:?}"
        );
    }
    let misused = problems("order", r#""if": "order.number.cancellable", "#);
    assert!(
        misused.iter().any(|p| p.contains("cancellable is a condition on Order, and order.number is")),
        "{misused:?}"
    );
    std::fs::remove_dir_all(project).unwrap();
}

#[test]
fn help_documents_conditions_on_types() {
    let help = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    assert!(help(&["--help"]).contains("declared on a type"));
    let check = help(&["check", "--help"]);
    assert!(
        check.contains("A condition on a type is named <Type>.<name>")
            && check.contains("\"requires\": \"Order.cancellable\""),
        "{check}"
    );
    assert!(help(&["cli", "--help"]).contains("Order.cancellable"));
}
