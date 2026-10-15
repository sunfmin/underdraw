//! The gallery: every screen description under a directory, shown in a
//! window of its own process (ADR 0017). The command finds them, starts the
//! window and answers at once; the window ([`crate::window`]) runs on until
//! the person closes it. There is one window per root: while it is open,
//! the command brings it forward instead of starting another. The window
//! [`watch`]es its root, to be drawn from the files as they now stand.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, bail};
use serde_json::{Value, json};
use underdraw_format::{Checked, Finding, Lang, ScreenParams};

use crate::draw::Shown;
use crate::notes::NOTES;
use crate::project::DOMAIN;
use crate::{KINDS, SCREEN, TOKENS, check_at};

/// Directories a description keeps beside it that hold no description.
const LEFT_OUT: [&str; 2] = ["renders", "platforms"];

/// How long the window's process has to say whether its window opened.
const OPENING: Duration = Duration::from_secs(30);

/// How long an open window has to answer that it is brought forward.
const ANSWERING: Duration = Duration::from_secs(2);

/// How often the window looks at its root for files changed.
const WATCHING: Duration = Duration::from_millis(250);

/// The files of a description the window draws from: the person's notes
/// among them, which the agent answers, and the domain of a project whose
/// root is at or under the window's.
const DRAWN_FROM: [&str; 5] = [SCREEN, TOKENS, KINDS, NOTES, DOMAIN];

/// One row of the tree: a directory, or a description.
#[derive(Debug, PartialEq)]
pub struct Row {
    /// How many directories in from the root.
    pub depth: usize,
    /// The directory's own name.
    pub name: String,
    /// The directory.
    pub dir: PathBuf,
    /// Whether the directory holds a description.
    pub described: bool,
    /// What `check` finds wrong with the description: none when it can be
    /// drawn.
    pub faults: Vec<Finding>,
    /// The states the description can be shown in, as `check` finds them:
    /// its fixtures, then its phases, each in the order written, found
    /// even while it has faults. None for a directory, or one that cannot
    /// be read.
    pub states: Vec<Shown>,
}

impl Row {
    /// The description's directory, when this directory holds one.
    pub fn description(&self) -> Option<&Path> {
        self.described.then_some(self.dir.as_path())
    }
}

/// What `check` makes of the description in `dir`, among the screens whose
/// params `others` holds; where it cannot even be read, why, as its one
/// finding, at its screen.json.
pub fn checked(dir: &Path, others: &ScreenParams) -> Checked {
    check_at(dir, others).unwrap_or_else(|e| Checked {
        screen: String::new(),
        ok: false,
        fixtures: Vec::new(),
        phases: Vec::new(),
        conditions: Vec::new(),
        findings: vec![Finding { at: SCREEN.to_string(), problem: format!("{e:#}") }],
    })
}

/// The tree of every description under `root`, as the directories are
/// arranged, each directory before what it holds and in order of name:
/// renders/, platforms/, hidden directories and directories leading to no
/// description left out. `root` is a row of its own only when it is itself
/// a description. Each description is checked once, all at once, among
/// the others: a navigate to one is held to its params.
pub fn tree(root: &Path) -> Vec<Row> {
    let mut rows = descriptions(root);
    let others = crate::links::params(root);
    let others = &others;
    std::thread::scope(|scope| {
        for Row { dir, described, faults, states, .. } in &mut rows {
            if *described {
                scope.spawn(move || {
                    let found = checked(dir, others);
                    let fixtures = found.fixtures.into_iter().map(Shown::Fixture);
                    *states = fixtures.chain(found.phases.into_iter().map(Shown::Phase)).collect();
                    *faults = found.findings;
                });
            }
        }
    });
    rows
}

/// The tree of [`tree`], no description checked yet: each row's faults
/// empty.
pub fn descriptions(root: &Path) -> Vec<Row> {
    let mut rows = Vec::new();
    if root.join(SCREEN).is_file() {
        let name = root.file_name().map_or_else(|| ".".to_string(), |n| n.to_string_lossy().to_string());
        rows.push(Row {
            depth: 0,
            name,
            dir: root.to_path_buf(),
            described: true,
            faults: Vec::new(),
            states: Vec::new(),
        });
        walk(root, 1, &mut rows);
    } else {
        walk(root, 0, &mut rows);
    }
    rows
}

/// `dir` as the answers name it: relative to `root`, "." for `root` itself.
pub fn within(root: &Path, dir: &Path) -> String {
    match dir.strip_prefix(root) {
        Ok(within) if within.as_os_str().is_empty() => ".".to_string(),
        Ok(within) => within.to_string_lossy().to_string(),
        Err(_) => dir.to_string_lossy().to_string(),
    }
}

/// The rows of what `dir` holds, `depth` in, added to `rows`.
fn walk(dir: &Path, depth: usize, rows: &mut Vec<Row>) {
    for (name, path) in children(dir) {
        let at = rows.len();
        let described = path.join(SCREEN).is_file();
        rows.push(Row { depth, name, dir: path.clone(), described, faults: Vec::new(), states: Vec::new() });
        walk(&path, depth + 1, rows);
        if !described && rows.len() == at + 1 {
            rows.pop();
        }
    }
}

/// The directories in `dir` the tree may list, with their names, in order
/// of name: renders/, platforms/ and hidden directories left out.
pub(crate) fn children(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut children: Vec<(String, PathBuf)> = entries
        .flatten()
        // Not following a link: a link back up would never end.
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .map(|entry| (entry.file_name().to_string_lossy().to_string(), entry.path()))
        .filter(|(name, _)| !name.starts_with('.') && !LEFT_OUT.contains(&name.as_str()))
        .collect();
    children.sort();
    children
}

/// What the window is drawn from under `root`, as it now stands: every
/// directory the tree may list, and when and how large each file it draws
/// from was last written. Two differ when a description, its tokens or its
/// kinds changed, or a directory appeared or went; renders/ is not in it.
#[derive(Debug, PartialEq)]
pub struct Stamp(Vec<(PathBuf, Option<(SystemTime, u64)>)>);

impl Stamp {
    /// What the window is drawn from under `root` now.
    pub fn of(root: &Path) -> Stamp {
        let mut stamp = Vec::new();
        stamp_dir(root, &mut stamp);
        Stamp(stamp)
    }
}

/// `dir`, its files drawn from and the journeys listed under it, and every
/// directory in it, added to `stamp`.
fn stamp_dir(dir: &Path, stamp: &mut Vec<(PathBuf, Option<(SystemTime, u64)>)>) {
    stamp.push((dir.to_path_buf(), None));
    let files = DRAWN_FROM.iter().map(|file| dir.join(file));
    for path in files.chain(crate::journeys::listed(dir)) {
        if let Ok(meta) = std::fs::metadata(&path) {
            stamp.push((path, Some((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len()))));
        }
    }
    for (_, path) in children(dir) {
        stamp_dir(&path, stamp);
    }
}

/// Looks at `root` every [`WATCHING`], on a thread of its own, and calls
/// `changed` each time what the window is drawn from differs from the last
/// look; stops once `changed` says no one is listening any more.
pub fn watch(root: &Path, changed: impl Fn() -> bool + Send + 'static) {
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let mut last = Stamp::of(&root);
        loop {
            std::thread::sleep(WATCHING);
            let now = Stamp::of(&root);
            if now != last {
                if !changed() {
                    return;
                }
                last = now;
            }
        }
    });
}

/// `underdraw gallery <root>`: the window started in a process of its own,
/// and what it shows.
pub fn gallery(root: &Path, lang: Lang) -> anyhow::Result<Value> {
    if !root.is_dir() {
        bail!("{} is not a directory: pass the directory the screen descriptions are under", root.display());
    }
    let root = root.canonicalize().with_context(|| format!("cannot read {}", root.display()))?;
    let (mut descriptions, mut faulty) = (Vec::new(), Vec::new());
    for row in tree(&root) {
        let Some(dir) = row.description() else { continue };
        if !row.faults.is_empty() {
            faulty.push(within(&root, dir));
        }
        descriptions.push(within(&root, dir));
    }
    if descriptions.is_empty() {
        bail!(
            "there is no screen description under {}: a description is a directory holding {SCREEN}",
            root.display()
        );
    }
    if let Some(pid) = bring_forward(&root) {
        return Ok(
            json!({ "root": root, "descriptions": descriptions, "faults": faulty, "opened": false, "pid": pid }),
        );
    }
    display()?;
    let pid = open(&root, lang)?;
    Ok(json!({ "root": root, "descriptions": descriptions, "faults": faulty, "opened": true, "pid": pid }))
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
fn open(root: &Path, lang: Lang) -> anyhow::Result<u32> {
    let exe = std::env::current_exe().context("cannot find the underdraw binary to start the window with")?;
    let mut command = Command::new(exe);
    command
        .arg("gallery")
        .arg("--window")
        .arg("--lang")
        .arg(lang.name())
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

/// Where the window over `root` says which port it answers on: a file in
/// the temporary directory named for `root`, which is canonical. The window
/// writes it and removes it on closing; one left by a window that crashed
/// names a port nothing answers on, or something else does.
fn address(root: &Path) -> PathBuf {
    // FNV-1a: the same name from every build, unlike std's hasher.
    let hash = root.as_os_str().as_encoded_bytes().iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    std::env::temp_dir().join(format!("underdraw-gallery-{hash:016x}.port"))
}

/// The open window over `root` asked to come forward; its pid, when there
/// is one and it answered. A window closed or crashed answers nothing.
fn bring_forward(root: &Path) -> Option<u32> {
    let port: u16 = std::fs::read_to_string(address(root)).ok()?.trim().parse().ok()?;
    let at = (Ipv4Addr::LOCALHOST, port).into();
    let mut stream = TcpStream::connect_timeout(&at, ANSWERING).ok()?;
    stream.set_read_timeout(Some(ANSWERING)).ok()?;
    writeln!(stream, "{}", json!({ "root": root })).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    let answer: Value = serde_json::from_str(&line).ok()?;
    // Another root's window, or not a window at all, on a port reused.
    if answer["root"].as_str() != root.to_str() {
        return None;
    }
    answer["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok())
}

/// The window over `root` answering a later run of the command, until this
/// is dropped: `forward` brings the window forward, and says whether it
/// could.
pub struct Answering {
    address: PathBuf,
    port: u16,
}

impl Answering {
    /// Starts answering on a port of its own, on a thread of its own; `None`
    /// when no port could be had, and then every run opens another window.
    pub fn start(root: &Path, forward: impl Fn() -> bool + Send + 'static) -> Option<Answering> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
        let port = listener.local_addr().ok()?.port();
        let address = address(root);
        // Written whole, then put in place, so a run never reads half of it.
        let written = address.with_extension(format!("{}", std::process::id()));
        std::fs::write(&written, port.to_string()).ok()?;
        std::fs::rename(&written, &address).ok()?;
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                answer(stream, &root, &forward);
            }
        });
        Some(Answering { address, port })
    }
}

/// One run of the command answered, if it asks after this window's root.
fn answer(stream: TcpStream, root: &Path, forward: &impl Fn() -> bool) {
    let _ = stream.set_read_timeout(Some(ANSWERING));
    let mut line = String::new();
    let mut reader = BufReader::new(&stream);
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let asked: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
    if asked["root"].as_str() != root.to_str() || !forward() {
        return;
    }
    let _ = writeln!(&stream, "{}", json!({ "root": root, "pid": std::process::id() }));
}

impl Drop for Answering {
    /// The address removed, unless a later window has written its own.
    fn drop(&mut self) {
        let ours =
            std::fs::read_to_string(&self.address).is_ok_and(|port| port.trim() == self.port.to_string());
        if ours {
            let _ = std::fs::remove_file(&self.address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows as (depth, name, description, whether it has faults).
    fn rows(root: &Path) -> Vec<(usize, String, Option<PathBuf>, bool)> {
        tree(root)
            .into_iter()
            .map(|row| {
                (
                    row.depth,
                    row.name.clone(),
                    row.description().map(Path::to_path_buf),
                    !row.faults.is_empty(),
                )
            })
            .collect()
    }

    fn row(depth: usize, name: &str, description: Option<&Path>) -> (usize, String, Option<PathBuf>, bool) {
        (depth, name.to_string(), description.map(Path::to_path_buf), description.is_some())
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
            rows(&root),
            vec![
                row(0, "a-flow", None),
                row(1, "sign-in", Some(&root.join("a-flow/sign-in"))),
                row(1, "sign-up", Some(&root.join("a-flow/sign-up"))),
                row(2, "nested", Some(&root.join("a-flow/sign-up/nested"))),
                row(0, "b-screen", Some(&root.join("b-screen"))),
            ]
        );
        let screen = root.join("b-screen");
        assert_eq!(rows(&screen), vec![row(0, "b-screen", Some(&screen))], "a root that is a description");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_description_with_faults_is_listed_with_what_check_finds() {
        let root = std::env::temp_dir().join(format!("underdraw-gallery-faults-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["empty", "not-json"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("empty").join(SCREEN), "{}").unwrap();
        std::fs::write(root.join("not-json").join(SCREEN), "{").unwrap();
        let rows = tree(&root);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["empty", "not-json"], "both are listed");
        let checked = check_at(&root.join("empty"), &ScreenParams::new()).unwrap();
        assert!(!checked.findings.is_empty());
        let found =
            |faults: &[Finding]| faults.iter().map(|f| (f.at.clone(), f.problem.clone())).collect::<Vec<_>>();
        assert_eq!(found(&rows[0].faults), found(&checked.findings), "as `check` names them");
        assert_eq!(rows[1].faults.len(), 1);
        assert_eq!(rows[1].faults[0].at, SCREEN);
        assert!(rows[1].faults[0].problem.contains("is not JSON"), "{}", rows[1].faults[0].problem);
        assert!(
            tree(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts")))
                .iter()
                .all(|row| row.faults.is_empty()),
            "the drafts have none"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn screens_sharing_tokens_one_directory_above_have_no_faults() {
        let draft = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../drafts/todo"));
        let root = std::env::temp_dir().join(format!("underdraw-gallery-shared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["one", "two"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            for file in [SCREEN, KINDS] {
                std::fs::copy(draft.join(file), root.join(dir).join(file)).unwrap();
            }
        }
        std::fs::copy(draft.join(TOKENS), root.join(TOKENS)).unwrap();
        let rows = tree(&root);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.faults.is_empty()), "{:?}", rows);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_stamp_changes_with_what_the_window_is_drawn_from_and_nothing_else() {
        let root = std::env::temp_dir().join(format!("underdraw-gallery-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("screen")).unwrap();
        std::fs::write(root.join("screen").join(SCREEN), "{}").unwrap();
        let mut last = Stamp::of(&root);
        let mut step = |what: &str, change: &dyn Fn(), differs: bool| {
            change();
            let now = Stamp::of(&root);
            assert_eq!(now != last, differs, "{what}");
            last = now;
        };
        let dir = root.join("screen");
        step("nothing", &|| {}, false);
        step("the description edited", &|| std::fs::write(dir.join(SCREEN), "{ }").unwrap(), true);
        step("its tokens written", &|| std::fs::write(dir.join(TOKENS), "{}").unwrap(), true);
        step("its kinds written", &|| std::fs::write(dir.join(KINDS), "{}").unwrap(), true);
        step("a note written or answered", &|| std::fs::write(dir.join(NOTES), "{}").unwrap(), true);
        step("its tokens removed", &|| std::fs::remove_file(dir.join(TOKENS)).unwrap(), true);
        step("tokens above it written", &|| std::fs::write(root.join(TOKENS), "{}").unwrap(), true);
        step("the domain written", &|| std::fs::write(root.join(DOMAIN), "{}").unwrap(), true);
        step(
            "a journey written",
            &|| {
                std::fs::create_dir_all(root.join("journeys")).unwrap();
                std::fs::write(root.join("journeys").join("go.json"), "{}").unwrap();
            },
            true,
        );
        step("a journey edited", &|| std::fs::write(root.join("journeys/go.json"), "{ }").unwrap(), true);
        step("a directory added", &|| std::fs::create_dir_all(root.join("a/b")).unwrap(), true);
        step("a directory removed", &|| std::fs::remove_dir_all(root.join("a")).unwrap(), true);
        step(
            "a render written",
            &|| {
                std::fs::create_dir_all(dir.join("renders")).unwrap();
                std::fs::write(dir.join("renders").join("default.png"), "png").unwrap();
            },
            false,
        );
        step("another file written", &|| std::fs::write(dir.join("notes.md"), "notes").unwrap(), false);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn watching_says_when_a_file_changed_until_no_one_listens() {
        let root = std::env::temp_dir().join(format!("underdraw-gallery-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("screen")).unwrap();
        let (said, heard) = mpsc::channel();
        watch(&root, move || said.send(()).is_ok());
        std::thread::sleep(WATCHING * 2);
        assert!(heard.try_recv().is_err(), "nothing changed");
        std::fs::write(root.join("screen").join(SCREEN), "{}").unwrap();
        assert!(heard.recv_timeout(WATCHING * 20).is_ok(), "a description appeared");
        std::thread::sleep(WATCHING * 2);
        assert!(heard.try_recv().is_err(), "said once for one change");
        drop(heard);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_open_window_is_brought_forward_and_only_over_its_own_root() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let base = std::env::temp_dir().join(format!("underdraw-gallery-forward-{}", std::process::id()));
        let (one, two) = (base.join("one"), base.join("two"));
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let forwarded = Arc::new(AtomicUsize::new(0));
        let counted = forwarded.clone();
        let answering = Answering::start(&one, move || {
            counted.fetch_add(1, Ordering::SeqCst);
            true
        })
        .unwrap();
        assert_eq!(bring_forward(&one), Some(std::process::id()));
        assert_eq!(bring_forward(&one), Some(std::process::id()), "every run, not only the first");
        assert_eq!(forwarded.load(Ordering::SeqCst), 2);
        assert_eq!(bring_forward(&two), None, "another root has no window");

        drop(answering);
        assert!(!address(&one).exists(), "a closed window takes its address with it");
        assert_eq!(bring_forward(&one), None, "a closed window");

        // A window crashed: its address is left, naming a port nothing answers on.
        let gone = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port();
        std::fs::write(address(&one), gone.to_string()).unwrap();
        assert_eq!(bring_forward(&one), None, "a crashed window");
        std::fs::remove_file(address(&one)).unwrap();

        // A window whose event loop has ended cannot come forward.
        let ending = Answering::start(&two, || false).unwrap();
        assert_eq!(bring_forward(&two), None, "a window that cannot come forward");
        drop(ending);
        std::fs::remove_dir_all(base).unwrap();
    }
}
