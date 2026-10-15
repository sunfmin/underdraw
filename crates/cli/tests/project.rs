//! A project whose tokens.json is one directory above its screens, run
//! with the real binary: each screen is checked, rendered and handed off
//! as the same screen with its tokens beside it is.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

const DRAFT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../format/tests/airbnb-listing");

fn underdraw(args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_underdraw")).args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let answer =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not one JSON object ({e}): {stdout}"));
    (answer, out.status.code().unwrap())
}

/// A copy of the draft's screen and platforms in `dir`, without its tokens.
fn screen_without_tokens(dir: &Path) {
    std::fs::create_dir_all(dir.join("platforms")).unwrap();
    for file in ["screen.json", "platforms/ios.json"] {
        std::fs::copy(format!("{DRAFT}/{file}"), dir.join(file)).unwrap();
    }
}

#[test]
fn tokens_one_directory_above_two_screens_are_theirs() {
    let project = std::env::temp_dir().join(format!("underdraw-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    let screens = [project.join("listing"), project.join("listing-again")];
    for dir in &screens {
        screen_without_tokens(dir);
    }
    std::fs::copy(format!("{DRAFT}/tokens.json"), project.join("tokens.json")).unwrap();
    let out = project.join("renders");
    let (before, code) =
        underdraw(&["render", DRAFT, "--width", "1120", "--out", out.join("draft.png").to_str().unwrap()]);
    assert_eq!(code, 0, "{before}");
    let (handed_before, code) = underdraw(&["handoff", DRAFT, "--platform", "ios"]);
    assert_eq!(code, 0, "{handed_before}");

    for (i, dir) in screens.iter().enumerate() {
        let path = dir.to_str().unwrap();
        let (checked, code) = underdraw(&["check", path]);
        assert_eq!((code, &checked["ok"]), (0, &json!(true)), "{checked}");

        let png = out.join(format!("{i}.png"));
        let (rendered, code) =
            underdraw(&["render", path, "--width", "1120", "--out", png.to_str().unwrap()]);
        assert_eq!(code, 0, "{rendered}");
        assert_eq!(rendered["breakpoint"], "regular", "the shared tokens' breakpoints");
        assert_eq!(rendered["nodes"], before["nodes"], "laid out as with its own tokens");
        assert_eq!(rendered["project"], json!(dir), "no domain.json: its own directory");
        assert_eq!(rendered["tokens"], json!(project.join("tokens.json")));

        let (handed, code) = underdraw(&["handoff", path, "--platform", "ios"]);
        assert_eq!(code, 0, "{handed}");
        assert_eq!(handed, handed_before, "handed off as with its own tokens");
    }
    let (checked, code) = underdraw(&["check", project.to_str().unwrap()]);
    assert_eq!((code, &checked["ok"]), (0, &json!(true)), "{checked}");

    // The screen's own tokens win: with no breakpoints, every width is compact.
    std::fs::write(screens[0].join("tokens.json"), "{}").unwrap();
    let png = out.join("own.png");
    let (rendered, code) = underdraw(&[
        "render",
        screens[0].to_str().unwrap(),
        "--width",
        "1120",
        "--out",
        png.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{rendered}");
    assert_eq!(rendered["tokens"], json!(screens[0].join("tokens.json")));
    std::fs::remove_dir_all(project).unwrap();
}
