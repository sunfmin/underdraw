//! Renders the Airbnb draft with the real binary, on this machine's GPU,
//! and reads the PNG it writes and the JSON it answers.

use std::process::Command;

use serde_json::Value;

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/airbnb-listing");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

fn out(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("underdraw-render-{}", std::process::id()));
    dir.join(name).to_str().unwrap().to_string()
}

/// The width and height a PNG file says it has.
fn png_size(path: &str) -> (u32, u32) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{path} is a PNG");
    let number = |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    (number(16), number(20))
}

#[test]
fn renders_a_phone_and_a_desktop() {
    let phone = out("phone.png");
    let (answer, code) = underdraw(&["render", DRAFT, "--out", &phone]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        (answer["state"]["fixture"].as_str(), answer["breakpoint"].as_str()),
        (Some("default"), Some("compact"))
    );
    assert_eq!(answer["width"], 390.0);
    let pixels = (answer["pixels"][0].as_u64().unwrap() as u32, answer["pixels"][1].as_u64().unwrap() as u32);
    assert_eq!(png_size(&phone), pixels);
    assert_eq!(pixels.0, 780, "2 pixels per point");
    let nodes = answer["nodes"].as_array().unwrap();
    let reserve = nodes.iter().find(|n| n["id"] == "reserve-button").expect("the reserve button is placed");
    assert!(reserve["label"].is_u64(), "a person can act on it, so it is numbered: {reserve}");

    let desktop = out("desktop.png");
    let (answer, code) = underdraw(&["render", DRAFT, "--width", "1120", "--scale", "1", "--out", &desktop]);
    assert_eq!((code, answer["breakpoint"].as_str()), (0, Some("regular")));
    assert_eq!(png_size(&desktop).0, 1120);
    assert!(answer["nodes"].as_array().unwrap().iter().any(|n| n["id"] == "booking-card"));
}

#[test]
fn renders_a_phase() {
    let path = out("loading.png");
    let (answer, code) = underdraw(&["render", DRAFT, "--phase", "loading", "--out", &path]);
    assert_eq!((code, answer["state"]["phase"].as_str()), (0, Some("loading")), "{answer}");
}

#[test]
fn refuses_what_it_cannot_draw_and_says_why() {
    let (answer, code) = underdraw(&["render", DRAFT, "--fixture", "nope"]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("the fixtures are"), "{answer}");

    let (answer, code) = underdraw(&["render", DRAFT, "--phase", "nope"]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("the phases are"), "{answer}");

    let dir = std::env::temp_dir().join(format!("underdraw-render-faulty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = std::fs::read_to_string(format!("{DRAFT}/screen.json")).unwrap();
    std::fs::write(dir.join("screen.json"), text.replacen("\"{listing.title}\"", "\"{listing.nope}\"", 1))
        .unwrap();
    let (answer, code) = underdraw(&["render", dir.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("underdraw check"), "{answer}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn given_its_root_a_navigate_to_a_screen_not_drawn_yet_is_assumed() {
    let drafts = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts");
    let orders = format!("{drafts}/orders");
    let assumed = |answer: &Value, id: &str| {
        let node = answer["nodes"].as_array().unwrap().iter().find(|n| n["id"] == id).unwrap();
        node["assumed"].as_str().map(str::to_string)
    };
    let (answer, code) = underdraw(&[
        "render",
        &orders,
        "--width",
        "1120",
        "--root",
        drafts,
        "--out",
        &out("orders-root.png"),
    ]);
    assert_eq!(code, 0, "{answer}");
    assert!(assumed(&answer, "order-0").is_some_and(|why| why.contains("OrderDetail")), "{answer:#}");
    assert_eq!(assumed(&answer, "nav-export"), None, "ExportDocuments is drawn");

    let (answer, code) = underdraw(&["render", &orders, "--width", "1120", "--out", &out("orders.png")]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(assumed(&answer, "order-0"), None, "without its root, navigates are not resolved");
}
