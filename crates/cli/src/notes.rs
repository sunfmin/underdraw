//! What the person and the agent say of a description, pinned where it is
//! said (ADR 0019, ADR 0023): kept in notes.json beside its screen.json,
//! meant to be committed. A note is a conversation on a node, a region, a
//! whole state, a transition's arrow or a navigate's arrow: the person's
//! messages and the agent's, in the order said. Only the person starts
//! one, in the gallery; the agent answers in it, at any length, and may
//! ask back. It is never closed: while the person spoke last it waits on
//! the agent, and once the agent did it is answered; that is read from
//! the messages, never stored. What the agent wants to ask unprompted is
//! an assumption. An assumption is answered with a note on its node: ✓
//! says the guess was right; ✗ says it was wrong, with what is right.
//! Each change is read from the file as it now stands and written back
//! whole, so the window and the agent never undo each other's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::SCREEN;
use crate::reference::{On, Reference};

/// The file a description's notes are kept in, beside its screen.json; a
/// shared component's, in `components/<Name>/notes.json`.
pub const NOTES: &str = "notes.json";

/// Where a conversation stands, read from who spoke last: never stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stands {
    /// The person spoke last: it waits on the agent.
    Waiting,
    /// The agent spoke last.
    Answered,
}

/// Who said a message of a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum By {
    Person,
    Agent,
}

/// One message of a note: what the person wrote, or what the agent answered,
/// as many lines as it takes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Said {
    pub by: By,
    pub says: String,
}

/// One note, a conversation, pinned as ⌘C references what it is on: a node as the
/// description writes it with the instance the person pointed at, a region
/// dragged with the nodes it covered, a whole state, a transition or a navigate. A navigate's note is kept by the
/// description the navigate is written in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `N1`, `N2`…: numbered in the order written, within the description.
    pub id: String,
    /// What it is on, written as its fields: `state` and `node`, `state`
    /// and `region`, `state`, `transition` or `navigate`.
    #[serde(flatten)]
    pub on: On,
    /// What the person and the agent said, in the order said; the
    /// person's first.
    pub messages: Vec<Said>,
    /// Where acting on the node should go, when the person proposed it
    /// where nothing was declared: from `state`, on an event, to a state
    /// or another screen. The agent declares it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposes: Option<Proposal>,
    /// The assumption on its node it answers, and how: none for a note
    /// that answers none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answers: Option<Answers>,
}

/// What a note answering an assumption says of it: the guess as the panel
/// showed it, what made it, and the person's answer. Written as one
/// object: `{"assumed", "guess", "answer"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Answers {
    pub assumed: String,
    pub guess: Guess,
    pub answer: Answer,
}

/// The person's answer to an assumption.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    /// ✓: the guess was right; the agent takes it out of the description.
    Right,
    /// ✗: the guess was wrong, and the note says what is right.
    Wrong,
}

/// What made an assumption: written `"author"`, `{"undrawn": [<screen>,
/// …]}` or `{"component": <component>}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Guess {
    /// The node's own `assumed`, written by the agent.
    Author,
    /// A navigate to these screens, which no description under the root
    /// declares: screens not drawn yet.
    Undrawn(Vec<String>),
    /// A component underdraw does not know.
    Component(String),
}

impl Guess {
    /// What made the assumption on the node written `written` in `screen`,
    /// of component `component`, drawn among the screens `drawn`: its own
    /// `assumed`, else a navigate to a screen not drawn, else its component.
    pub fn of(screen: &Value, written: &str, component: &str, drawn: &BTreeSet<String>) -> Guess {
        let node = underdraw_format::written_node(screen, written);
        if node.is_some_and(|node| node.contains_key("assumed")) {
            return Guess::Author;
        }
        let undrawn = undrawn(screen, written, drawn);
        if undrawn.is_empty() { Guess::Component(component.to_string()) } else { Guess::Undrawn(undrawn) }
    }
}

/// The screens the node written `written` in `screen` navigates to that
/// are not among `drawn`.
fn undrawn(screen: &Value, written: &str, drawn: &BTreeSet<String>) -> Vec<String> {
    underdraw_format::navigates(screen)
        .into_iter()
        .filter(|navigate| navigate.node == written && !drawn.contains(&navigate.to))
        .map(|navigate| navigate.to)
        .collect()
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

    /// Where the conversation stands: waiting on the agent while the
    /// person spoke last, answered once the agent did.
    pub fn stands(&self) -> Stands {
        match self.messages.last() {
            Some(Said { by: By::Agent, .. }) => Stands::Answered,
            _ => Stands::Waiting,
        }
    }

    /// Whether it waits on the agent: the person spoke last.
    pub fn waits(&self) -> bool {
        self.stands() == Stands::Waiting
    }

    /// What the agent said last, once it has answered.
    pub fn reply(&self) -> Option<&str> {
        self.messages.iter().rev().find(|said| said.by == By::Agent).map(|said| said.says.as_str())
    }

    /// The note as the `notes` command answers it: with the description it
    /// is in, where it stands, and its reference as one line.
    pub fn answer(&self, root: &Path, dir: &Path) -> Value {
        let mut answer = json!({
            "description": crate::gallery::within(root, dir),
            "id": self.id,
            "stands": self.stands(),
            "messages": self.messages,
            "proposes": self.proposes,
            "reference": self.reference(dir).line(),
        });
        if let (Some(answer), Ok(Value::Object(on))) =
            (answer.as_object_mut(), serde_json::to_value(&self.on))
        {
            answer.extend(on);
        }
        if let Some(answers) = &self.answers {
            answer["answers"] = json!(answers);
        }
        answer
    }
}

/// How many of a description's notes wait on the agent, and how many are
/// answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub waiting: usize,
    pub answered: usize,
}

impl Counts {
    pub fn of(notes: &[Note]) -> Counts {
        let waiting = notes.iter().filter(|note| note.waits()).count();
        Counts { waiting, answered: notes.len() - waiting }
    }

    pub fn add(self, other: Counts) -> Counts {
        Counts { waiting: self.waiting + other.waiting, answered: self.answered + other.answered }
    }
}

/// notes.json as written.
#[derive(Default, Serialize, Deserialize)]
struct File {
    notes: Vec<Note>,
}

/// The keys only a note written before a note became a conversation has
/// (ADR 0023): such a file is not read, but reported, to be deleted.
const OLD: [&str; 3] = ["status", "thread", "closed_because"];

/// Why the notes.json in `dir` is in the old form, written before a note
/// became a conversation: what `check` reports of it, to delete it. None
/// when there is none, or it is not in the old form.
pub fn old_form(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(NOTES)).ok()?;
    let file: Value = serde_json::from_str(&text).ok()?;
    let old = file.get("notes")?.as_array()?.iter().any(|note| OLD.iter().any(|key| note.get(key).is_some()));
    old.then(|| {
        format!(
            "{NOTES} holds notes in the old form, from before a note became a conversation (\"status\", \"thread\"): they are not converted, delete it (`git rm {}`)",
            dir.join(NOTES).display()
        )
    })
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
    if let Some(old) = old_form(dir) {
        bail!("{old}");
    }
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
/// A shared component's are kept in `components/<Name>/`, which its first
/// note makes.
fn write(dir: &Path, notes: Vec<Note>) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
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

/// A note the person started, saying `says`, on what `on` references,
/// added to its description's notes, numbered after the last. Its id.
pub fn add(on: &Reference, says: &str) -> anyhow::Result<String> {
    added(on, said(says)?, None, None)
}

/// What a ✓ note says when the person writes nothing more.
pub const RIGHT: &str = "✓ right, keep it: take the guess out of the description";

/// A note answering the assumption on the node `on` references, as
/// `answers` says, added to its description's notes as [`add`] adds one:
/// ✓ says `says`, or [`RIGHT`] when that is empty; ✗ must say what is
/// right, and is refused without it. Its id.
pub fn judge(on: &Reference, answers: Answers, says: &str) -> anyhow::Result<String> {
    if !matches!(on.on, On::Node { .. }) {
        bail!("an assumption is on a node: pick the node with the orange \"?\" first");
    }
    let says = match (answers.answer, says.trim()) {
        (Answer::Right, "") => RIGHT.to_string(),
        (Answer::Wrong, "") => bail!("✗ says what is right, for the agent to change: write it first"),
        (_, says) => says.to_string(),
    };
    added(on, says, None, Some(answers))
}

/// A note proposing where acting on the node `on` references should go,
/// `proposal`, added to its description's notes as [`add`] adds one, its
/// first message saying the proposal. Its id.
pub fn propose(on: &Reference, proposal: Proposal) -> anyhow::Result<String> {
    let On::Node { state, .. } = &on.on else {
        bail!("a proposal is on a node, where acting on it should go: pick one first");
    };
    added(on, proposal.says(state.name()), Some(proposal), None)
}

/// A note the person started saying `says`, and proposing `proposes`, on
/// what `on` references, added to its description's notes, numbered after
/// the last. Its id.
fn added(
    on: &Reference,
    says: String,
    proposes: Option<Proposal>,
    answers: Option<Answers>,
) -> anyhow::Result<String> {
    change(&on.description, |notes| {
        let last = notes.iter().filter_map(|note| note.id.strip_prefix('N')?.parse::<u32>().ok()).max();
        let id = format!("N{}", last.unwrap_or(0) + 1);
        notes.push(Note {
            id: id.clone(),
            on: on.on.clone(),
            messages: vec![Said { by: By::Person, says }],
            proposes,
            answers,
        });
        Ok(id)
    })
}

/// Note `id` of the description in `dir` answered by the agent with
/// `reply`, as many lines as it takes, kept as written but for its ends.
/// The note as it now stands, and whether that changed it: the same reply
/// as the agent's last message, given again, changes nothing.
pub fn answer(dir: &Path, id: &str, reply: &str) -> anyhow::Result<(Note, bool)> {
    let reply = reply.trim();
    if reply.is_empty() {
        bail!("--reply says what was done, or why not, or asks back: it is not empty");
    }
    change(dir, |notes| {
        let note = find(notes, dir, id)?;
        let said = Said { by: By::Agent, says: reply.to_string() };
        let changed = note.messages.last() != Some(&said);
        if changed {
            note.messages.push(said);
        }
        Ok((note.clone(), changed))
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

/// Why the agent may not start a note.
pub const THE_PERSONS: &str = "the agent cannot start a note: the person does that in the gallery (`underdraw gallery <root>`). Answer one with `underdraw notes <dir> --answer <id> --reply \"…\"`; to ask the person something unprompted, write an assumption (`assumed`) in the description";

/// How long `--wait` lets pass between two readings of the notes.
const WAIT_BETWEEN: std::time::Duration = std::time::Duration::from_millis(200);

/// `underdraw notes <path>`: the notes waiting on the agent of the
/// description at `path`, or of every description under it, each with its
/// reference; or, given `answer`, that note of the description at `path`
/// answered with `reply`. With `wait`, the waiting notes once there is
/// any: at once when some wait already, else as soon as the person says
/// something. Nothing is kept between runs; the files are read again each
/// time.
pub fn notes(path: &Path, answer: Option<&str>, reply: Option<&str>, wait: bool) -> anyhow::Result<Value> {
    if !path.is_dir() {
        bail!("{} is not a directory: pass a description's directory, or a root of them", path.display());
    }
    let path = path.canonicalize().with_context(|| format!("cannot read {}", path.display()))?;
    if let Some(id) = answer {
        if wait {
            bail!(
                "--answer answers one note: run it without --wait, which lists notes, then answer what it lists"
            );
        }
        if !crate::describes(&path) {
            bail!(
                "{} holds no {SCREEN} and shows no shared component: --answer takes the directory of the description the note is in, as \"description\" names it under the root",
                path.display()
            );
        }
        let Some(reply) = reply else {
            bail!("--answer needs --reply \"…\": what was done, or why not, or what you ask back");
        };
        let (note, changed) = self::answer(&path, id, reply)?;
        let root = path.parent().unwrap_or(&path).to_path_buf();
        return Ok(json!({ "note": note.answer(&root, &path), "changed": changed }));
    }
    if reply.is_some() {
        bail!("--reply answers a note: give the note with --answer <id>");
    }
    loop {
        let (waiting, counts) = listed(&path)?;
        if !wait || counts.waiting > 0 {
            return Ok(json!({
                "root": path,
                "notes": waiting,
                "waiting": counts.waiting,
                "answered": counts.answered,
            }));
        }
        std::thread::sleep(WAIT_BETWEEN);
    }
}

/// The notes of every description under `root` that wait on the agent, as
/// `notes` answers each, and how many there are waiting and answered: read
/// from the files as they now stand, descriptions added since included.
fn listed(root: &Path) -> anyhow::Result<(Vec<Value>, Counts)> {
    let dirs: Vec<PathBuf> = crate::gallery::descriptions(root)
        .into_iter()
        .filter_map(|row| row.description().map(Path::to_path_buf))
        .collect();
    let (mut waiting, mut counts) = (Vec::new(), Counts::default());
    for dir in &dirs {
        let notes = read(dir)?;
        counts = counts.add(Counts::of(&notes));
        waiting.extend(notes.iter().filter(|note| note.waits()).map(|note| note.answer(root, dir)));
    }
    Ok((waiting, counts))
}

/// What `check` says of the notes of the description in `dir`: how many
/// wait on the agent and how many are answered, or why they cannot be
/// read. Never a fault of its own: a notes.json in the old form is one
/// `check` reports among the description's findings.
pub fn counted(dir: &Path) -> Value {
    match read(dir) {
        Ok(notes) => json!(Counts::of(&notes)),
        Err(e) => json!({ "waiting": 0, "answered": 0, "error": format!("{e:#}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{Origin, Shown};
    use crate::reference::{Frame, Navigate, Pointed, Region, Transition};

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
    fn a_note_waits_or_is_answered_by_who_spoke_last() {
        let dir = dir("life");
        assert_eq!(read(&dir).unwrap(), [], "no notes.json: no notes");
        assert_eq!(add(&on(&dir), "  the price is cut  ").unwrap(), "N1");
        assert_eq!(add(&on(&dir), "and the date").unwrap(), "N2", "numbered after the last");
        let notes = read(&dir).unwrap();
        assert_eq!(notes[0].stands(), Stands::Waiting, "the person spoke last");
        assert_eq!(notes[0].messages, [Said { by: By::Person, says: "the price is cut".into() }]);
        assert_eq!(notes[0].reference(&dir), on(&dir), "pinned as ⌘C references it");
        assert_eq!(Counts::of(&notes), Counts { waiting: 2, answered: 0 });

        let (note, changed) = answer(&dir, "N1", "made it wrap").unwrap();
        assert!(changed);
        assert_eq!(
            (note.stands(), note.reply()),
            (Stands::Answered, Some("made it wrap")),
            "the agent spoke last"
        );
        let before = std::fs::read_to_string(dir.join(NOTES)).unwrap();
        let (_, changed) = answer(&dir, "N1", "made it wrap").unwrap();
        assert!(!changed, "answered twice the same: nothing more");
        assert_eq!(std::fs::read_to_string(dir.join(NOTES)).unwrap(), before);
        let (note, changed) = answer(&dir, "N1", "should it wrap at 1120 too?").unwrap();
        assert!(changed && note.messages.len() == 3, "the agent may say more, and ask back");
        assert_eq!(note.stands(), Stands::Answered);

        let file: Value = serde_json::from_str(&before).unwrap();
        for key in ["status", "closed_because", "thread"] {
            assert!(file["notes"][0].get(key).is_none(), "{key} is not stored: {file}");
        }
        assert_eq!(Counts::of(&read(&dir).unwrap()), Counts { waiting: 1, answered: 1 });
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_reply_keeps_every_line_it_has() {
        let dir = dir("lines");
        add(&on(&dir), "the price").unwrap();
        let reply = "made it wrap.\n\nTwo questions:\n  - at 1120 too?\n  - and the date?";
        let (note, _) = answer(&dir, "N1", &format!("\n{reply}\n")).unwrap();
        assert_eq!(note.reply(), Some(reply), "kept as written, but for its ends");
        assert_eq!(read(&dir).unwrap()[0].reply(), Some(reply), "and read back so");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn notes_json_in_the_old_form_is_not_read_but_reported_to_delete() {
        let dir = dir("old");
        assert_eq!(old_form(&dir), None, "no notes.json");
        let old = json!({ "notes": [{
            "id": "N1", "status": "open", "state": { "fixture": "default" },
            "thread": [{ "by": "person", "says": "x" }],
        }] });
        std::fs::write(dir.join(NOTES), old.to_string()).unwrap();
        let said = old_form(&dir).unwrap();
        assert!(said.contains("old form") && said.contains("git rm") && said.contains(NOTES), "{said}");
        let error = read(&dir).unwrap_err().to_string();
        assert!(error.contains("old form"), "{error}");
        assert!(counted(&dir)["error"].as_str().unwrap().contains("old form"));
        std::fs::remove_dir_all(&dir).unwrap();
        let dir = self::dir("new");
        add(&on(&dir), "x").unwrap();
        assert_eq!(old_form(&dir), None, "the new form");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_assumption_is_answered_right_or_wrong_and_listed_with_how() {
        let dir = dir("judged");
        let answers =
            |answer| Answers { assumed: "Card fields; or hosted".into(), guess: Guess::Author, answer };
        assert_eq!(judge(&on(&dir), answers(Answer::Right), " ").unwrap(), "N1");
        let error = judge(&on(&dir), answers(Answer::Wrong), "").unwrap_err().to_string();
        assert!(error.contains("✗ says what is right"), "{error}");
        assert_eq!(judge(&on(&dir), answers(Answer::Wrong), "hosted fields").unwrap(), "N2");
        let state =
            Reference { description: dir.clone(), on: On::State { state: Shown::Fixture("default".into()) } };
        assert!(judge(&state, answers(Answer::Right), "").is_err(), "an assumption is on a node");

        let notes = read(&dir).unwrap();
        assert_eq!(notes[0].messages, [Said { by: By::Person, says: RIGHT.into() }], "✓ needs no line");
        assert_eq!(notes[1].messages[0].says, "hosted fields");
        let file: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(NOTES)).unwrap()).unwrap();
        assert_eq!(
            file["notes"][1]["answers"],
            json!({ "assumed": "Card fields; or hosted", "guess": "author", "answer": "wrong" })
        );
        assert_eq!(notes[0].answer(&dir, &dir)["answers"]["answer"], "right", "`notes` lists how it answers");
        assert_eq!(notes[1].answers.as_ref().unwrap().answer, Answer::Wrong);
        let others = [Guess::Undrawn(vec!["OrderDetail".into()]), Guess::Component("Gauge".into())];
        assert_eq!(
            others.map(|guess| serde_json::to_value(guess).unwrap()),
            [json!({ "undrawn": ["OrderDetail"] }), json!({ "component": "Gauge" })]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_guess_is_what_made_the_assumption() {
        let screen = json!({ "root": { "id": "list", "component": "Column", "children": [
            { "id": "card", "component": "TextField", "assumed": "drawn here; or hosted" },
            { "id": "gauge", "component": "Gauge" },
            { "id": "row", "component": "Row", "on": { "tap": { "navigate": "OrderDetail" } } },
        ] } });
        let none = BTreeSet::new;
        assert_eq!(Guess::of(&screen, "card", "TextField", &none()), Guess::Author);
        assert_eq!(Guess::of(&screen, "row", "Row", &none()), Guess::Undrawn(vec!["OrderDetail".into()]));
        assert_eq!(Guess::of(&screen, "gauge", "Gauge", &none()), Guess::Component("Gauge".into()));
        let drawn = BTreeSet::from(["OrderDetail".to_string()]);
        assert_eq!(Guess::of(&screen, "row", "Row", &drawn), Guess::Component("Row".into()), "drawn now");
    }

    #[test]
    fn a_reply_says_something_and_names_a_note_there_is() {
        let dir = dir("refused");
        add(&on(&dir), "the price").unwrap();
        for reply in ["", "  ", "\n\n"] {
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
                "state": { "fixture": "default" },
                "node": { "id": "order-{index}", "at": "/root/children/1/item", "instance": [2], "label": 4, "width": 390 },
                "messages": [{ "by": "person", "says": "the price" }],
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
            file["notes"][1]["messages"],
            json!([{ "by": "person", "says": "order-{index}.tap in default should navigate to OrderDetail" }])
        );
        let to = Proposal { on: "cd-keep.tap".into(), goes: Goes::To("default".into()) };
        assert_eq!(serde_json::to_value(&to).unwrap(), json!({ "on": "cd-keep.tap", "to": "default" }));
        assert_eq!(read(&dir).unwrap()[1].proposes, Some(proposal), "read back as written");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_region_note_keeps_its_box_and_the_nodes_it_covered() {
        let dir = dir("region");
        let covered =
            Origin { written: "nav-export".into(), at: "/root/children/0".into(), instance: vec![] };
        let region =
            Region { area: Frame { x: 8, y: 40, width: 200, height: 60 }, width: 390, nodes: vec![covered] };
        let on = Reference {
            description: dir.clone(),
            on: On::Region { state: Shown::Fixture("default".into()), region },
        };
        assert_eq!(add(&on, "too busy").unwrap(), "N1");
        let file: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(NOTES)).unwrap()).unwrap();
        assert_eq!(
            file["notes"][0]["region"],
            json!({
                "box": { "x": 8, "y": 40, "width": 200, "height": 60 },
                "width": 390,
                "nodes": [{ "id": "nav-export", "at": "/root/children/0", "instance": [] }],
            })
        );
        assert!(file["notes"][0].get("node").is_none(), "a region note is on no one node");
        let note = read(&dir).unwrap().remove(0);
        assert_eq!(note.reference(&dir), on, "read back as written");
        let answered = note.answer(&dir, &dir);
        assert_eq!(answered["region"], file["notes"][0]["region"], "`notes` lists its box and nodes");
        assert!(answered["reference"].as_str().unwrap().contains("region 8, 40 · 200 × 60"));
        let error =
            propose(&on, Proposal { on: "x.tap".into(), goes: Goes::To("default".into()) }).unwrap_err();
        assert!(error.to_string().contains("a proposal is on a node"), "{error}");
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
                for key in ["id", "messages"] {
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
