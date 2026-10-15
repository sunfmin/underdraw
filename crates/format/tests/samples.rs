//! Samples the domain shares: a value written once for the project, which a
//! fixture of any screen starts from with `"$sample"`, as `"$patch"` starts
//! one from another fixture, its own fields merged over it.

use serde_json::{Value, json};
use underdraw_format::{Breakpoint, Checked, Screen, Shared, State, check};

fn domain() -> Value {
    json!({
        "types": {
            "Stop": { "screen": "string 1..", "here": "bool = false" },
            "Journey": { "name": "string 1..", "stops": "Stop[] 1.." }
        },
        "samples": {
            "twoJourneys": [
                { "name": "Buy", "stops": [ { "screen": "orders" }, { "screen": "order" } ] },
                { "name": "Cancel", "stops": [ { "screen": "order", "here": true } ] }
            ],
            "buy": { "name": "Buy", "stops": [ { "screen": "orders" } ] },
            "picked": { "journeys": [ { "name": "Pick", "stops": [ { "screen": "tree" } ] } ], "title": "Picked" }
        }
    })
}

/// A screen listing journeys, its fixtures as `fixtures` writes them.
fn screen(fixtures: Value) -> Value {
    json!({
        "screen": "Journeys",
        "params": ["title"],
        "data": { "journeys": "Journey[]", "title": "string", "first": "Journey?" },
        "fixtures": fixtures,
        "root": { "id": "root", "component": "Screen", "children": [
            { "id": "title", "component": "Text", "text": "{title}" },
            { "id": "list", "component": "List", "each": "journeys", "item": {
                "id": "journey-{index}", "component": "Text", "text": "{item.name}" } }
        ] }
    })
}

fn read(description: &Value) -> Screen {
    Screen::read(description, Shared { domain: Some(&domain()), components: None })
}

fn check_with_domain(description: &Value) -> Checked {
    check(&read(description))
}

#[test]
fn a_fixture_starting_from_samples_reads_and_renders_as_written_out() {
    let sampled = screen(json!({
        "default": { "journeys": { "$sample": "twoJourneys" }, "title": "All",
                     "first": { "$sample": "buy", "name": "Buy again" } },
        "picked": { "$sample": "picked", "first": null }
    }));
    let written = screen(json!({
        "default": { "journeys": domain()["samples"]["twoJourneys"], "title": "All",
                     "first": { "name": "Buy again", "stops": [ { "screen": "orders" } ] } },
        "picked": { "journeys": [ { "name": "Pick", "stops": [ { "screen": "tree" } ] } ], "title": "Picked",
                    "first": null }
    }));
    let checked = check_with_domain(&sampled);
    assert!(checked.ok, "{:#?}", checked.findings);
    let (sampled, written) = (read(&sampled), read(&written));
    assert_eq!(sampled.fixtures, written.fixtures);
    for name in ["default", "picked"] {
        for breakpoint in Breakpoint::ALL {
            let drawn = |screen: &Screen| screen.instantiate(State::Fixture(name), breakpoint, None).unwrap();
            assert_eq!(drawn(&sampled), drawn(&written), "{name}");
        }
    }
}

#[test]
fn a_sample_of_the_wrong_type_is_a_finding_naming_the_field_and_the_sample() {
    let wrong = screen(json!({
        "default": { "journeys": { "$sample": "buy" }, "title": "All" }
    }));
    let checked = check_with_domain(&wrong);
    assert!(
        checked
            .findings
            .iter()
            .any(|f| f.at == "fixtures.default.journeys" && f.problem.contains("sample buy")),
        "{:#?}",
        checked.findings
    );

    let list = screen(json!({
        "default": { "journeys": [], "title": "All", "first": { "$sample": "twoJourneys" } }
    }));
    let checked = check_with_domain(&list);
    assert!(
        checked.findings.iter().any(|f| f.at == "fixtures.default.first"
            && f.problem.contains("Journey")
            && f.problem.contains("sample twoJourneys")),
        "{:#?}",
        checked.findings
    );
}

#[test]
fn a_sample_the_domain_does_not_share_is_named() {
    let missing = screen(json!({ "default": { "journeys": { "$sample": "three" }, "title": "All" } }));
    let checked = check_with_domain(&missing);
    assert!(
        checked.findings.iter().any(|f| f.at == "fixtures.default.journeys" && f.problem.contains("three")),
        "{:#?}",
        checked.findings
    );

    let merged = screen(json!({
        "default": { "journeys": { "$sample": "twoJourneys", "name": "x" }, "title": "All" }
    }));
    let checked = check_with_domain(&merged);
    assert!(
        checked
            .findings
            .iter()
            .any(|f| f.at == "fixtures.default.journeys" && f.problem.contains("not an object")),
        "{:#?}",
        checked.findings
    );
}
