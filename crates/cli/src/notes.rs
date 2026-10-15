//! What the person says of a description, pinned where they said it
//! (ADR 0019): kept in notes.json beside its screen.json, meant to be
//! committed. The person writes a note in the gallery, on a node, a whole
//! state, a transition's arrow or a navigate's arrow; the agent
//! answers it with one line; the person closes it, or reopens it with a
//! line added. The agent never creates, closes or reopens one: what it
//! wants to ask is an assumption. Each change is read from the file as it
//! now stands and written back whole, so the window and the agent never
//! undo each other's.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::SCREEN;
use crate::reference::{On, Reference};

/// The file a description's notes are kept in, beside its screen.json.
pub const NOTES: &str = "notes.json";

/// Where a note is in its life: written, answered by the agent, closed by
/// the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Answered,
    Closed,
}

/// Who said a line of a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum By {
    Person,
    Agent,
}

/// One line of a note's thread: what the person wrote, or the agent's reply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Said {
    pub by: By,
    pub says: String,
}

/// One note, pinned as ⌘C references what it is on: a node as the
/// description writes it with the instance the person pointed at, a whole
/// state, a transition or a navigate. A navigate's note is kept by the
/// description the navigate is written in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `N1`, `N2`…: numbered in the order written, within the description.
    pub id: String,
    pub status: Status,
    /// What it is on, written as its fields: `state` and `node`, `state`,
    /// `transition` or `navigate`.
    #[serde(flatten)]
    pub on: On,
    /// What the person wrote, then each reply and each line the person
    /// added on reopening it, in order.
    pub thread: Vec<Said>,
    /// Where acting on the node should go, when the person proposed it
    /// where nothing was declared: from `state`, on an event, to a state
    /// or another screen. The agent declares it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposes: Option<Proposal>,
}

/// Where the person proposes acting on a node should go, where the
/// description declares nothing: on `on`, the node's id as written and its
/// event (`"cd-keep.tap"`), to a state of the same description, as a
/// transition would, or to another screen, as a `navigate` would.
/// Written as one object: `{"on", "to"}` or `{"on", "navigate"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub on: String,
    #[serde(flatten)]
    pub goes: Goes,
}

/// Where a proposal goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Goes {
    /// A state, a fixture or a phase, by name.
    To(String),
    /// A screen, by the name its description declares.
    Navigate(String),
}

impl Proposal {
    /// The proposal as the person says it, from state `from`: the line a
    /// note holding it starts its thread with.
    pub fn says(&self, from: &str) -> String {
        match &self.goes {
            Goes::To(to) => format!("{} in {from} should go to {to}: declare the transition", self.on),
            Goes::Navigate(screen) => format!("{} in {from} should navigate to {screen}", self.on),
        }
    }
}

impl Note {
    /// The reference to what the note is on, in the description in `dir`.
    pub fn reference(&self, dir: &Path) -> Reference {
        Reference { description: dir.to_path_buf(), on: self.on.clone() }
    }

    /// The agent's last reply, once it has answered.
    pub fn reply(&self) -> Option<&str> {
        self.thread.iter().rev().find(|said| said.by == By::Agent).map(|said| said.says.as_str())
    }

    /// The note as the `notes` command answers it: with the description it
    /// is in and its reference as one line.
    pub fn answer(&self, root: &Path, dir: &Path) -> Value {
        let mut answer = json!({
            "description": crate::gallery::within(root, dir),
            "id": self.id,
            "status": self.status,
            "thread": self.thread,
            "proposes": self.proposes,
            "reference": self.reference(dir).line(),
        });
        if let (Some(answer), Ok(Value::Object(on))) =
            (answer.as_object_mut(), serde_json::to_value(&self.on))
        {
            answer.extend(on);
        }
        answer
    }
}

/// How many of a description's notes are open, and how many answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub open: usize,
    pub answered: usize,
}

impl Counts {
    pub fn of(notes: &[Note]) -> Counts {
        let count = |status| notes.iter().filter(|note| note.status == status).count();
        Counts { open: count(Status::Open), answered: count(Status::Answered) }
    }

    pub fn add(self, other: Counts) -> Counts {
        Counts { open: self.open + other.open, answered: self.answered + other.answered }
    }
}

/// notes.json as written.
#[derive(Default, Serialize, Deserialize)]
struct File {
    notes: Vec<Note>,
}

/// The notes of the description in `dir`, in the order written; none when
/// it has no notes.json.
pub fn read(dir: &Path) -> anyhow::Result<Vec<Note>> {
    let file = dir.join(NOTES);
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", file.display())),
    };
    let read: File = serde_json::from_str(&text).with_context(|| {
        format!(
            "{} is not notes as the gallery writes them: undo the hand edit (`git diff` it) at the line and column named here",
            file.display()
        )
    })?;
    Ok(read.notes)
}

/// `notes` written whole as the notes of the description in `dir`: to a
/// file beside it first, then put in place, so a reader never sees half.
fn write(dir: &Path, notes: Vec<Note>) -> anyhow::Result<()> {
    let file = dir.join(NOTES);
    let mut text = serde_json::to_string_pretty(&File { notes })?;
    text.push('\n');
    let written = dir.join(format!(".{NOTES}.{}", std::process::id()));
    std::fs::write(&written, text).with_context(|| format!("cannot write {}", written.display()))?;
    std::fs::rename(&written, &file).with_context(|| format!("cannot write {}", file.display()))
}

/// The notes of the description in `dir` changed by `change` and written
/// back, read from the file as it now stands; what `change` answers.
fn change<T>(dir: &Path, change: impl FnOnce(&mut Vec<Note>) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let mut notes = read(dir)?;
    let answer = change(&mut notes)?;
    write(dir, notes)?;
    Ok(answer)
}

/// A note the person wrote, `says`, on what `on` references, added to its
/// description's notes: open, numbered after the last. Its id.
pub fn add(on: &Reference, says: &str) -> anyhow::Result<String> {
    added(on, said(says)?, None)
}

/// A note proposing where acting on the node `on` references should go,
/// `proposal`, added to its description's notes as [`add`] adds one, its
/// thread saying the proposal. Its id.
pub fn propose(on: &Reference, proposal: Proposal) -> anyhow::Result<String> {
    let On::Node { state, .. } = &on.on else {
        bail!("a proposal is on a node, where acting on it should go: pick one first");
    };
    added(on, proposal.says(state.name()), Some(proposal))
}

/// A note saying `says`, and proposing `proposes`, on what `on` references,
/// added to its description's notes: open, numbered after the last. Its id.
fn added(on: &Reference, says: String, proposes: Option<Proposal>) -> anyhow::Result<String> {
    change(&on.description, |notes| {
        let last = notes.iter().filter_map(|note| note.id.strip_prefix('N')?.parse::<u32>().ok()).max();
        let id = format!("N{}", last.unwrap_or(0) + 1);
        notes.push(Note {
            id: id.clone(),
            status: Status::Open,
            on: on.on.clone(),
            thread: vec![Said { by: By::Person, says }],
            proposes,
        });
        Ok(id)
    })
}

/// Note `id` of the description in `dir` answered by the agent with
/// `reply`, one line. The note as it now stands, and whether that changed
/// it: answered again with the same reply, nothing changes; with another,
/// that reply takes the place of the last. A closed note is not answered.
pub fn answer(dir: &Path, id: &str, reply: &str) -> anyhow::Result<(Note, bool)> {
    let reply = reply.trim();
    if reply.is_empty() || reply.contains('\n') {
        bail!("--reply is one line saying what was done, or why not: not empty, and no line breaks");
    }
    change(dir, |notes| {
        let note = find(notes, dir, id)?;
        let changed = match note.status {
            Status::Closed => bail!(
                "{id} is closed: the person closed it, and only the person reopens it, in the gallery; answer an open one"
            ),
            Status::Answered if note.reply() == Some(reply) => false,
            Status::Answered => {
                let last = note.thread.iter().rposition(|said| said.by == By::Agent);
                if let Some(last) = last {
                    note.thread[last].says = reply.to_string();
                }
                true
            }
            Status::Open => {
                note.thread.push(Said { by: By::Agent, says: reply.to_string() });
                note.status = Status::Answered;
                true
            }
        };
        Ok((note.clone(), changed))
    })
}

/// Note `id` of the description in `dir` closed by the person.
pub fn close(dir: &Path, id: &str) -> anyhow::Result<()> {
    change(dir, |notes| {
        find(notes, dir, id)?.status = Status::Closed;
        Ok(())
    })
}

/// Note `id` of the description in `dir` reopened by the person, saying
/// `says` too: open again, for the agent to answer.
pub fn reopen(dir: &Path, id: &str, says: &str) -> anyhow::Result<()> {
    let says = said(says)?;
    change(dir, |notes| {
        let note = find(notes, dir, id)?;
        note.thread.push(Said { by: By::Person, says });
        note.status = Status::Open;
        Ok(())
    })
}

/// What the person wrote, its ends trimmed; refused when empty.
fn said(says: &str) -> anyhow::Result<String> {
    let says = says.trim();
    if says.is_empty() {
        bail!("a note says something: write it first");
    }
    Ok(says.to_string())
}

/// Note `id` among `notes`, of the description in `dir`.
fn find<'a>(notes: &'a mut [Note], dir: &Path, id: &str) -> anyhow::Result<&'a mut Note> {
    let ids: Vec<String> = notes.iter().map(|note| note.id.clone()).collect();
    match notes.iter_mut().find(|note| note.id == id) {
        Some(note) => Ok(note),
        None => bail!("{} has no note {id}; its notes are {ids:?}", dir.join(NOTES).display()),
    }
}

/// Why the agent may not create, close or reopen a note.
pub const THE_PERSONS: &str = "the agent cannot create, close or reopen a note: the person does that in the gallery (`underdraw gallery <root>`). Answer an open one with `underdraw notes <dir> --answer <id> --reply \"…\"`; to ask the person something, write an assumption (`assumed`) in the description";

/// How long `--wait` lets pass between two readings of the notes.
const WAIT_BETWEEN: std::time::Duration = std::time::Duration::from_millis(200);

/// `underdraw notes <path>`: the open notes of the description at `path`,
/// or of every description under it, each with its reference; or, given
/// `answer`, that note of the description at `path` answered with `reply`.
/// With `wait`, the open notes once there is any: at once when some are
/// open already, else as soon as the person writes or reopens one. Nothing
/// is kept between runs; the files are read again each time.
pub fn notes(path: &Path, answer: Option<&str>, reply: Option<&str>, wait: bool) -> anyhow::Result<Value> {
    if !path.is_dir() {
        bail!("{} is not a directory: pass a description's directory, or a root of them", path.display());
    }
    let path = path.canonicalize().with_context(|| format!("cannot read {}", path.display()))?;
    if let Some(id) = answer {
        if wait {
            bail!("--wait waits for a note to answer: run it without --answer, then answer what it lists");
        }
        if !path.join(SCREEN).is_file() {
            bail!(
                "{} holds no {SCREEN}: --answer takes the directory of the description the note is in, as \"description\" names it under the root",
                path.display()
            );
        }
        let Some(reply) = reply else {
            bail!("--answer needs --reply \"…\": one line saying what was done, or why not");
        };
        let (note, changed) = self::answer(&path, id, reply)?;
        let root = path.parent().unwrap_or(&path).to_path_buf();
        return Ok(json!({ "note": note.answer(&root, &path), "changed": changed }));
    }
    if reply.is_some() {
        bail!("--reply answers a note: give the note with --answer <id>");
    }
    loop {
        let (open, counts) = listed(&path)?;
        if !wait || counts.open > 0 {
            return Ok(
                json!({ "root": path, "notes": open, "open": counts.open, "answered": counts.answered }),
            );
        }
        std::thread::sleep(WAIT_BETWEEN);
    }
}

/// The open notes of every description under `root`, as `notes` answers
/// each, and how many there are open and answered: read from the files as
/// they now stand, descriptions added since included.
fn listed(root: &Path) -> anyhow::Result<(Vec<Value>, Counts)> {
    let dirs: Vec<PathBuf> = crate::gallery::descriptions(root)
        .into_iter()
        .filter_map(|row| row.description().map(Path::to_path_buf))
        .collect();
    let (mut open, mut counts) = (Vec::new(), Counts::default());
    for dir in &dirs {
        let notes = read(dir)?;
        counts = counts.add(Counts::of(&notes));
        open.extend(
            notes.iter().filter(|note| note.status == Status::Open).map(|note| note.answer(root, dir)),
        );
    }
    Ok((open, counts))
}

/// What `check` says of the notes of the description in `dir`: how many are
/// open and answered, or why they cannot be read. Never a fault.
pub fn counted(dir: &Path) -> Value {
    match read(dir) {
        Ok(notes) => json!(Counts::of(&notes)),
        Err(e) => json!({ "open": 0, "answered": 0, "error": format!("{e:#}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{Origin, Shown};
    use crate::reference::{Navigate, Pointed, Transition};

    /// A directory of its own named `name`, holding an empty description.
    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("underdraw-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SCREEN), "{}").unwrap();
        dir
    }

    fn on(dir: &Path) -> Reference {
        let origin =
            Origin { written: "order-{index}".into(), at: "/root/children/1/item".into(), instance: vec![2] };
        Reference {
            description: dir.to_path_buf(),
            on: On::Node {
                state: Shown::Fixture("default".into()),
                node: Pointed { origin, label: Some(4), width: 390 },
            },
        }
    }

    #[test]
    fn a_note_is_written_answered_closed_and_reopened() {
        let dir = dir("life");
        assert_eq!(read(&dir).unwrap(), [], "no notes.json: no notes");
        assert_eq!(add(&on(&dir), "  the price is cut  ").unwrap(), "N1");
        assert_eq!(add(&on(&dir), "and the date").unwrap(), "N2", "numbered after the last");
        let notes = read(&dir).unwrap();
        assert_eq!(notes[0].status, Status::Open);
        assert_eq!(notes[0].thread, [Said { by: By::Person, says: "the price is cut".into() }]);
        assert_eq!(notes[0].reference(&dir), on(&dir), "pinned as ⌘C references it");
        assert_eq!(Counts::of(&notes), Counts { open: 2, answered: 0 });

        let (note, changed) = answer(&dir, "N1", "made it wrap").unwrap();
        assert!(changed);
        assert_eq!((note.status, note.reply()), (Status::Answered, Some("made it wrap")));
        let before = std::fs::read_to_string(dir.join(NOTES)).unwrap();
        let (_, changed) = answer(&dir, "N1", "made it wrap").unwrap();
        assert!(!changed, "answered twice the same: nothing more");
        assert_eq!(std::fs::read_to_string(dir.join(NOTES)).unwrap(), before);
        let (note, changed) = answer(&dir, "N1", "made it wrap at 390").unwrap();
        assert!(changed && note.thread.len() == 2, "another reply takes the last one's place");

        reopen(&dir, "N1", "still cut at 1120").unwrap();
        let note = read(&dir).unwrap().remove(0);
        assert_eq!(note.status, Status::Open);
        assert_eq!(note.thread.last().unwrap(), &Said { by: By::Person, says: "still cut at 1120".into() });
        answer(&dir, "N1", "fixed at 1120 too").unwrap();
        assert_eq!(read(&dir).unwrap()[0].thread.len(), 4, "a reopened note is answered anew");
        close(&dir, "N1").unwrap();
        let error = answer(&dir, "N1", "more").unwrap_err().to_string();
        assert!(error.contains("only the person reopens it"), "{error}");
        assert_eq!(Counts::of(&read(&dir).unwrap()), Counts { open: 1, answered: 0 });
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_reply_is_one_line_and_names_a_note_there_is() {
        let dir = dir("refused");
        add(&on(&dir), "the price").unwrap();
        for reply in ["", "  ", "one\ntwo"] {
            assert!(answer(&dir, "N1", reply).is_err(), "{reply:?}");
        }
        let error = answer(&dir, "N9", "done").unwrap_err().to_string();
        assert!(error.contains("no note N9") && error.contains("[\"N1\"]"), "{error}");
        assert!(add(&on(&dir), " ").is_err(), "a note says something");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn notes_json_is_written_as_the_reference_names_the_node() {
        let dir = dir("file");
        add(&on(&dir), "the price").unwrap();
        let file: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(NOTES)).unwrap()).unwrap();
        assert_eq!(
            file,
            json!({ "notes": [{
                "id": "N1",
                "status": "open",
                "state": { "fixture": "default" },
                "node": { "id": "order-{index}", "at": "/root/children/1/item", "instance": [2], "label": 4, "width": 390 },
                "thread": [{ "by": "person", "says": "the price" }],
            }] })
        );
        let proposal =
            Proposal { on: "order-{index}.tap".into(), goes: Goes::Navigate("OrderDetail".into()) };
        assert_eq!(propose(&on(&dir), proposal.clone()).unwrap(), "N2");
        let file: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(NOTES)).unwrap()).unwrap();
        assert_eq!(file["notes"][0].get("proposes"), None, "a plain note proposes nothing");
        assert_eq!(
            file["notes"][1]["proposes"],
            json!({ "on": "order-{index}.tap", "navigate": "OrderDetail" })
        );
        assert_eq!(
            file["notes"][1]["thread"],
            json!([{ "by": "person", "says": "order-{index}.tap in default should navigate to OrderDetail" }])
        );
        let to = Proposal { on: "cd-keep.tap".into(), goes: Goes::To("default".into()) };
        assert_eq!(serde_json::to_value(&to).unwrap(), json!({ "on": "cd-keep.tap", "to": "default" }));
        assert_eq!(read(&dir).unwrap()[1].proposes, Some(proposal), "read back as written");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_note_on_a_whole_state_or_an_arrow_is_written_as_what_it_is_on() {
        let dir = dir("arrows");
        let state = On::State { state: Shown::Phase("loading".into()) };
        let transition = On::Transition {
            transition: Transition {
                from: "default".into(),
                on: "order-{index}-cancel.tap".into(),
                outcome: None,
                to: "cancel-dialog".into(),
            },
        };
        let navigate = On::Navigate {
            navigate: Navigate {
                from: "Orders".into(),
                node: "nav-export".into(),
                to: "ExportDocuments".into(),
            },
        };
        for on in [&state, &transition, &navigate] {
            add(&Reference { description: dir.clone(), on: on.clone() }, "here").unwrap();
        }
        add(&on(&dir), "and here").unwrap();
        let file: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(NOTES)).unwrap()).unwrap();
        let pinned: Vec<Value> = file["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|note| {
                let mut note = note.clone();
                for key in ["id", "status", "thread"] {
                    note.as_object_mut().unwrap().remove(key);
                }
                note
            })
            .collect();
        assert_eq!(
            pinned[..3],
            [
                json!({ "state": { "phase": "loading" } }),
                json!({ "transition": { "from": "default", "on": "order-{index}-cancel.tap", "to": "cancel-dialog" } }),
                json!({ "navigate": { "from": "Orders", "node": "nav-export", "to": "ExportDocuments" } }),
            ]
        );
        let read: Vec<On> = read(&dir).unwrap().into_iter().map(|note| note.on).collect();
        assert_eq!(read, [state, transition, navigate, on(&dir).on], "each read back as written");

        std::fs::write(dir.join(NOTES), "{").unwrap();
        assert!(counted(&dir)["error"].as_str().unwrap().contains("notes.json"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
