//! Opens the gallery with the real binary, reads the JSON it answers, and
//! closes the windows it started.

use std::process::Command;

use serde_json::Value;

const DRAFTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts");

fn underdraw(args: &[&str], env: &[&str]) -> (Value, i32) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_underdraw"));
    for name in env {
        command.env_remove(name);
    }
    let out = command.args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

#[test]
fn opens_one_window_over_every_draft_and_answers_at_once() {
    // A copy of its own: over `drafts` itself, a window the person has open
    // there would be brought forward instead.
    let root = std::env::temp_dir().join(format!("underdraw-gallery-drafts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(DRAFTS.as_ref(), &root);
    let Some(answer) = opened(root.to_str().unwrap()) else {
        std::fs::remove_dir_all(root).unwrap();
        return;
    };
    assert_eq!(answer["faults"], serde_json::json!([]), "no draft has faults");
    let found: Vec<&str> =
        answer["descriptions"].as_array().unwrap().iter().map(|d| d.as_str().unwrap()).collect();
    assert!(found.contains(&"airbnb-listing") && found.contains(&"todo"), "{found:?}");
    assert!(!found.iter().any(|d| d.contains("platforms") || d.contains("captures")), "{found:?}");
    close(&answer);
    std::fs::remove_dir_all(root).unwrap();
}

/// Copies the directory `from` and everything under it to `to`.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn names_the_descriptions_with_faults() {
    let root = std::env::temp_dir().join(format!("underdraw-gallery-faults-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["fine", "broken"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in ["screen.json", "tokens.json"] {
        std::fs::copy(format!("{DRAFTS}/todo/{file}"), root.join("fine").join(file)).unwrap();
    }
    std::fs::write(root.join("broken").join("screen.json"), "{}").unwrap();
    if let Some(answer) = opened(root.to_str().unwrap()) {
        assert_eq!(answer["descriptions"], serde_json::json!(["broken", "fine"]), "both are listed");
        assert_eq!(answer["faults"], serde_json::json!(["broken"]));
        close(&answer);
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// The gallery's answer over `root` once its window opened; none where
/// there is no display to open it on (CI, SSH), which is the answer to
/// expect there.
fn opened(root: &str) -> Option<Value> {
    let (answer, code) = underdraw(&["gallery", root], &[]);
    if code == 2 {
        assert!(answer["error"].as_str().unwrap().contains("display"), "{answer}");
        return None;
    }
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["opened"], true);
    Some(answer)
}

/// The window's process `answer` names ended, as a crash would end it.
fn close(answer: &Value) {
    let pid = answer["pid"].as_u64().unwrap().to_string();
    #[cfg(unix)]
    let closed = Command::new("kill").arg(&pid).status().unwrap();
    #[cfg(windows)]
    let closed = Command::new("taskkill").args(["/PID", &pid, "/F"]).status().unwrap();
    assert!(closed.success(), "the window's process {pid} was running");
}

#[test]
fn one_window_per_root_brought_forward_while_it_is_open() {
    let base = std::env::temp_dir().join(format!("underdraw-gallery-once-{}", std::process::id()));
    let (one, two) = (base.join("one"), base.join("two"));
    for root in [&one, &two] {
        std::fs::create_dir_all(root.join("screen")).unwrap();
        std::fs::write(root.join("screen").join("screen.json"), "{}").unwrap();
    }
    let (one, two) = (one.to_str().unwrap(), two.to_str().unwrap());
    let (first, code) = underdraw(&["gallery", one], &[]);
    if code == 2 {
        assert!(first["error"].as_str().unwrap().contains("display"), "{first}");
        return;
    }
    assert_eq!((code, &first["opened"]), (0, &Value::Bool(true)), "{first}");
    let (again, _) = underdraw(&["gallery", one], &[]);
    assert_eq!(again["opened"], false, "brought forward, not opened: {again}");
    assert_eq!(again["pid"], first["pid"], "the same window");
    let (other, _) = underdraw(&["gallery", two], &[]);
    assert_eq!(other["opened"], true, "another root, another window: {other}");
    assert_ne!(other["pid"], first["pid"]);
    close(&first);
    // Ended with no chance to tidy up, as a crash ends it.
    std::thread::sleep(std::time::Duration::from_millis(200));
    let (after, _) = underdraw(&["gallery", one], &[]);
    assert_eq!(after["opened"], true, "a window that crashed does not stop the next: {after}");
    assert_ne!(after["pid"], first["pid"]);
    close(&after);
    close(&other);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn refuses_a_directory_with_no_description() {
    let dir = std::env::temp_dir().join(format!("underdraw-gallery-empty-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("renders")).unwrap();
    std::fs::write(dir.join("renders").join("screen.json"), "{}").unwrap();
    let (answer, code) = underdraw(&["gallery", dir.to_str().unwrap()], &[]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("no screen description"), "{answer}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// Where X11 and Wayland name no display, no window is tried.
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn says_when_there_is_no_display() {
    let (answer, code) = underdraw(&["gallery", DRAFTS], &["DISPLAY", "WAYLAND_DISPLAY"]);
    assert_eq!(code, 2);
    assert!(answer["error"].as_str().unwrap().contains("display"), "{answer}");
}
