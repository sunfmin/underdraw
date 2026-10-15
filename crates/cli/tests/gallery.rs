//! Opens the gallery over the drafts with the real binary, reads the JSON it
//! answers, and closes the window it started.

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
    let (answer, code) = underdraw(&["gallery", DRAFTS], &[]);
    if code == 2 {
        // No display here (CI, SSH): that is the answer to expect.
        assert!(answer["error"].as_str().unwrap().contains("display"), "{answer}");
        return;
    }
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["opened"], true);
    let found: Vec<&str> =
        answer["descriptions"].as_array().unwrap().iter().map(|d| d.as_str().unwrap()).collect();
    assert!(found.contains(&"airbnb-listing") && found.contains(&"todo"), "{found:?}");
    assert!(!found.iter().any(|d| d.contains("platforms") || d.contains("captures")), "{found:?}");
    let pid = answer["pid"].as_u64().unwrap().to_string();
    #[cfg(unix)]
    let closed = Command::new("kill").arg(&pid).status().unwrap();
    #[cfg(windows)]
    let closed = Command::new("taskkill").args(["/PID", &pid, "/F"]).status().unwrap();
    assert!(closed.success(), "the window's process {pid} was running");
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
