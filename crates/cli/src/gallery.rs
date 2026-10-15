//! The gallery: every screen description under a directory, shown in a
//! window of its own process (ADR 0017). The command finds them, starts the
//! window and answers at once; the window ([`crate::window`]) runs on until
//! the person closes it.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, bail};
use serde_json::{Value, json};

use crate::SCREEN;

/// Directories a description keeps beside it that hold no description.
const LEFT_OUT: [&str; 2] = ["renders", "platforms"];

/// How long the window's process has to say whether its window opened.
const OPENING: Duration = Duration::from_secs(30);

/// One row of the tree: a directory, or a description.
#[derive(Debug, PartialEq)]
pub struct Row {
    /// How many directories in from the root.
    pub depth: usize,
    /// The directory's own name.
    pub name: String,
    /// The description's directory, when this directory holds one.
    pub description: Option<PathBuf>,
}

/// The tree of every description under `root`, as the directories are
/// arranged, each directory before what it holds and in order of name:
/// renders/, platforms/, hidden directories and directories leading to no
/// description left out. `root` is a row of its own only when it is itself
/// a description.
pub fn tree(root: &Path) -> Vec<Row> {
    let mut rows = Vec::new();
    if root.join(SCREEN).is_file() {
        let name = root.file_name().map_or_else(|| ".".to_string(), |n| n.to_string_lossy().to_string());
        rows.push(Row { depth: 0, name, description: Some(root.to_path_buf()) });
        walk(root, 1, &mut rows);
    } else {
        walk(root, 0, &mut rows);
    }
    rows
}

/// The rows of what `dir` holds, `depth` in, added to `rows`.
fn walk(dir: &Path, depth: usize, rows: &mut Vec<Row>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<(String, PathBuf)> = entries
        .flatten()
        // Not following a link: a link back up would never end.
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .map(|entry| (entry.file_name().to_string_lossy().to_string(), entry.path()))
        .filter(|(name, _)| !name.starts_with('.') && !LEFT_OUT.contains(&name.as_str()))
        .collect();
    children.sort();
    for (name, path) in children {
        let at = rows.len();
        let description = path.join(SCREEN).is_file().then(|| path.clone());
        let holds_one = description.is_some();
        rows.push(Row { depth, name, description });
        walk(&path, depth + 1, rows);
        if !holds_one && rows.len() == at + 1 {
            rows.pop();
        }
    }
}

/// `underdraw gallery <root>`: the window started in a process of its own,
/// and what it shows.
pub fn gallery(root: &Path) -> anyhow::Result<Value> {
    if !root.is_dir() {
        bail!("{} is not a directory: pass the directory the screen descriptions are under", root.display());
    }
    let root = root.canonicalize().with_context(|| format!("cannot read {}", root.display()))?;
    let descriptions: Vec<String> = tree(&root)
        .into_iter()
        .filter_map(|row| row.description)
        .map(|dir| match dir.strip_prefix(&root) {
            Ok(within) if within.as_os_str().is_empty() => ".".to_string(),
            Ok(within) => within.to_string_lossy().to_string(),
            Err(_) => dir.to_string_lossy().to_string(),
        })
        .collect();
    if descriptions.is_empty() {
        bail!(
            "there is no screen description under {}: a description is a directory holding {SCREEN}",
            root.display()
        );
    }
    display()?;
    let pid = open(&root)?;
    Ok(json!({ "root": root, "descriptions": descriptions, "opened": true, "pid": pid }))
}

/// What to do where no window can be opened.
const NO_DISPLAY: &str = "the gallery needs a display, which a session over SSH or in CI does not have: run it where the person sees the screen, or draw a state to a PNG with `underdraw render`";

/// Whether there is a display to open a window on, where the environment
/// says so: X11 and Wayland name theirs.
fn display() -> anyhow::Result<()> {
    if cfg!(all(unix, not(target_os = "macos"))) {
        let named = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        if !named("DISPLAY") && !named("WAYLAND_DISPLAY") {
            bail!("no display: neither DISPLAY nor WAYLAND_DISPLAY is set; {NO_DISPLAY}");
        }
    }
    Ok(())
}

/// The window over `root` started in a process of its own, which outlives
/// this one; its pid, once it says its window is open.
fn open(root: &Path) -> anyhow::Result<u32> {
    let exe = std::env::current_exe().context("cannot find the underdraw binary to start the window with")?;
    let mut command = Command::new(exe);
    command
        .arg("gallery")
        .arg("--window")
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // A group of its own, so that ^C in the terminal that ran the command
    // does not close the window.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().context("cannot start the window's process")?;
    let pid = child.id();
    let stdout = child.stdout.take().context("the window's process has no stdout")?;
    let (said, heard) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = said.send(line);
    });
    let line = match heard.recv_timeout(OPENING) {
        Ok(line) => line,
        Err(_) => {
            let _ = child.kill();
            bail!("the window did not open within {}s; {NO_DISPLAY}", OPENING.as_secs());
        }
    };
    let report: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
    if report["opened"] == true {
        return Ok(pid);
    }
    let _ = child.kill();
    let why = report["error"].as_str().unwrap_or("its process ended without saying why");
    bail!("no window could be opened ({why}); {NO_DISPLAY}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(depth: usize, name: &str, description: Option<&Path>) -> Row {
        Row { depth, name: name.to_string(), description: description.map(Path::to_path_buf) }
    }

    #[test]
    fn lists_descriptions_as_their_directories_are_arranged() {
        let root = std::env::temp_dir().join(format!("underdraw-gallery-tree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in [
            "b-screen/renders",
            "b-screen/platforms",
            "a-flow/sign-in",
            "a-flow/sign-up/nested",
            "empty/deeper",
            ".hidden",
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for dir in ["b-screen", "a-flow/sign-in", "a-flow/sign-up", "a-flow/sign-up/nested", ".hidden"] {
            std::fs::write(root.join(dir).join(SCREEN), "{}").unwrap();
        }
        assert_eq!(
            tree(&root),
            vec![
                row(0, "a-flow", None),
                row(1, "sign-in", Some(&root.join("a-flow/sign-in"))),
                row(1, "sign-up", Some(&root.join("a-flow/sign-up"))),
                row(2, "nested", Some(&root.join("a-flow/sign-up/nested"))),
                row(0, "b-screen", Some(&root.join("b-screen"))),
            ]
        );
        let screen = root.join("b-screen");
        assert_eq!(tree(&screen), vec![row(0, "b-screen", Some(&screen))], "a root that is a description");
        std::fs::remove_dir_all(root).unwrap();
    }
}
