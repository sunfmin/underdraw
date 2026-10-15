//! What the person says of a description, pinned where they said it
//! (ADR 0019): kept in notes.json beside its screen.json, meant to be
//! committed. The person writes a note in the gallery, on a node, a
//! region, a whole state, a transition's arrow or a navigate's arrow; the agent
//! answers it with one line; the person closes it, or reopens it with a
//! line added. The agent never creates, closes or reopens one: what it
//! wants to ask is an assumption. A note whose target is gone from the
//! description closes itself, saying why; reopened, it is on the whole
//! state. An assumption is answered with a note on its node: ✓ says the
//! guess was right, and that note closes itself once the agent has taken
//! the guess out; ✗ says it was wrong, with what is right, and runs as any
//! note does. Each change is read from the file as it
//! now stands and written back whole, so the window and the agent never
//! undo each other's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::SCREEN;
use crate::draw::{self, Shown};
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
/// description writes it with the instance the person pointed at, a region
/// dragged with the nodes it covered, a whole state, a transition or a navigate. A navigate's note is kept by the
/// description the navigate is written in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `N1`, `N2`…: numbered in the order written, within the description.
    pub id: String,
    pub status: Status,
    /// What it is on, written as its fields: `state` and `node`, `state`
    /// and `region`, `state`, `transition` or `navigate`.
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
    /// Why it closed itself, once what it is pinned to was gone from the
    /// description: `order-{index} is gone (removed or renamed)`. None for
    /// a note the person closed, or one never closed; taken away when the
    /// person reopens it, on the whole state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_because: Option<String>,
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
    /// ✓: the guess was right; the agent takes it out of the description,
    /// and the note closes itself.
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

    /// Whether the guess is still made of the node written `written` in
    /// `screen`, in the description in `dir`, among the screens `drawn`
    /// under the root: its `assumed` still written; a screen it named still
    /// navigated to and not drawn; its component still its component
    /// and still unknown.
    fn stands(
        &self,
        screen: &Value,
        dir: &Path,
        written: &str,
        drawn: impl FnOnce() -> BTreeSet<String>,
    ) -> bool {
        let node = underdraw_format::written_node(screen, written);
        match self {
            Guess::Author => node.is_some_and(|node| node.contains_key("assumed")),
            Guess::Undrawn(screens) => {
                let undrawn = undrawn(screen, written, &drawn());
                screens.iter().any(|screen| undrawn.contains(screen))
            }
            Guess::Component(component) => {
                let still =
                    node.and_then(|node| node.get("component")).and_then(Value::as_str) == Some(component);
                let kinds = crate::read_kinds(dir).ok().flatten();
                let components = screen.get("components").and_then(Value::as_object);
                still
                    && !underdraw_format::Kinds::read(kinds.as_ref()).has(component)
                    && !components.is_some_and(|components| components.contains_key(component))
            }
        }
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
        if let Some(why) = &self.closed_because {
            answer["closed_because"] = json!(why);
        }
        if let Some(answers) = &self.answers {
            answer["answers"] = json!(answers);
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
/// thread saying the proposal. Its id.
pub fn propose(on: &Reference, proposal: Proposal) -> anyhow::Result<String> {
    let On::Node { state, .. } = &on.on else {
        bail!("a proposal is on a node, where acting on it should go: pick one first");
    };
    added(on, proposal.says(state.name()), Some(proposal), None)
}

/// A note saying `says`, and proposing `proposes`, on what `on` references,
/// added to its description's notes: open, numbered after the last. Its id.
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
            status: Status::Open,
            on: on.on.clone(),
            thread: vec![Said { by: By::Person, says }],
            proposes,
            closed_because: None,
            answers,
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
/// `says` too: open again, for the agent to answer. One that closed itself
/// as what it was on went is on the whole state now: the one it was in, or
/// the transition was from, while the description has it, else its first.
pub fn reopen(dir: &Path, id: &str, says: &str) -> anyhow::Result<()> {
    let says = said(says)?;
    change(dir, |notes| {
        let note = find(notes, dir, id)?;
        if note.closed_because.is_some() {
            note.on = On::State { state: whole(dir, &note.on)? };
            note.closed_because = None;
        }
        note.thread.push(Said { by: By::Person, says });
        note.status = Status::Open;
        Ok(())
    })
}

/// The state of the description in `dir` that a note on `on`, gone, is
/// on once reopened: the state `on` names, or the state a transition is
/// from, while the description has it; else its first.
fn whole(dir: &Path, on: &On) -> anyhow::Result<Shown> {
    let states = draw::states(dir)?;
    let was = match on {
        On::Transition { transition } => Some(transition.from.as_str()),
        on => on.state().map(Shown::name),
    };
    let found = states.iter().find(|state| Some(state.name()) == was).or(states.first());
    found.cloned().ok_or_else(|| {
        anyhow!("{} has no state to pin the note to: give it a fixture first", dir.join(SCREEN).display())
    })
}

/// Why `note` closes itself, in `screen`, the description in `dir` as it
/// is now written, among the screens `drawn` under the root: what it is
/// pinned to is gone, or, for a ✓, the assumption it confirmed has been
/// taken out. None while neither is.
fn closes(
    screen: &Value,
    dir: &Path,
    note: &Note,
    drawn: impl FnOnce() -> BTreeSet<String>,
) -> Option<String> {
    if let Some(why) = gone(screen, &note.on) {
        return Some(why);
    }
    let (Some(answers), On::Node { node, .. }) = (&note.answers, &note.on) else {
        return None;
    };
    let written = &node.origin.written;
    let taken = answers.answer == Answer::Right && !answers.guess.stands(screen, dir, written, drawn);
    taken.then(|| format!("the assumption on {written}, confirmed, is taken out"))
}

/// Why what `on` is pinned to is gone from `screen`, the description as it
/// is now written; none while it is there. A node is gone once no tree
/// writes its id; a region once its state is, or every node it covered;
/// a transition or a navigate once none is written with the same keys.
fn gone(screen: &Value, on: &On) -> Option<String> {
    let removed = |what: &str| Some(format!("{what} is gone (removed or renamed)"));
    if let Some(state) = on.state() {
        let (key, kind) = match state {
            Shown::Fixture(_) => ("fixtures", "fixture"),
            Shown::Phase(_) => ("phases", "phase"),
        };
        let there =
            screen.get(key).and_then(Value::as_object).is_some_and(|all| all.contains_key(state.name()));
        if !there {
            return removed(&format!("{kind} {}", state.name()));
        }
    }
    let writes = |id: &str| underdraw_format::writes(screen, id);
    match on {
        On::Node { node, .. } if !writes(&node.origin.written) => removed(&node.origin.written),
        On::Region { region, .. }
            if !region.nodes.is_empty() && region.nodes.iter().all(|node| !writes(&node.written)) =>
        {
            let covered: Vec<&str> = region.nodes.iter().map(|node| node.written.as_str()).collect();
            Some(format!("the nodes it covered, {}, are gone (removed or renamed)", covered.join(", ")))
        }
        On::Transition { transition } => {
            let declared = underdraw_format::transitions(screen).into_iter().any(|declared| {
                declared.from == transition.from
                    && format!("{}.{}", declared.node, declared.event) == transition.on
                    && declared.outcome == transition.outcome
                    && declared.to == transition.to
            });
            (!declared).then(|| format!("transition {} is gone (removed or changed)", transition.said()))
        }
        On::Navigate { navigate } => {
            let from = screen.get("screen").and_then(Value::as_str) == Some(navigate.from.as_str());
            let written = underdraw_format::navigates(screen)
                .into_iter()
                .any(|written| written.node == navigate.node && written.to == navigate.to);
            (!(from && written)).then(|| format!("navigate {} is gone (removed or changed)", navigate.said()))
        }
        On::Node { .. } | On::Region { .. } | On::State { .. } => None,
    }
}

/// The notes of the description in `dir` that are not closed and whose
/// target is gone from it, or that confirmed an assumption since taken
/// out, closed, each saying why (`closed_because`): whoever reads the
/// notes first, the window or a command, closes them. A screen not drawn
/// yet is judged among the descriptions under `root`. Judged only once the
/// description checks clean, so that one half edited closes nothing. The
/// notes it closed, as they now stand.
pub fn settle(dir: &Path, root: &Path) -> anyhow::Result<Vec<Note>> {
    let notes = read(dir)?;
    let Ok((screen, _)) = crate::read(dir) else {
        return Ok(Vec::new());
    };
    let drawn = std::cell::OnceCell::new();
    let drawn = || drawn.get_or_init(|| crate::links::drawn(root)).clone();
    let closing = |note: &Note| note.status != Status::Closed && closes(&screen, dir, note, drawn).is_some();
    if !notes.iter().any(closing) {
        return Ok(Vec::new());
    }
    let clean = crate::read_kinds(dir)
        .and_then(|kinds| crate::checked(&screen, dir, kinds.as_ref()))
        .is_ok_and(|checked| checked.ok);
    if !clean {
        return Ok(Vec::new());
    }
    change(dir, |notes| {
        let mut closed = Vec::new();
        for note in notes.iter_mut().filter(|note| note.status != Status::Closed) {
            if let Some(why) = closes(&screen, dir, note, drawn) {
                note.status = Status::Closed;
                note.closed_because = Some(why);
                closed.push(note.clone());
            }
        }
        Ok(closed)
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
/// or of every description under it, each with its reference, or with
/// `closed` the closed ones; or, given `answer`, that note of the
/// description at `path` answered with `reply`. With `wait`, the open
/// notes once there is any: at once when some are open already, else as
/// soon as the person writes or reopens one. Nothing is kept between runs;
/// the files are read again each time, and a note whose target is gone
/// closed ([`settle`]).
pub fn notes(
    path: &Path,
    answer: Option<&str>,
    reply: Option<&str>,
    wait: bool,
    closed: bool,
) -> anyhow::Result<Value> {
    if !path.is_dir() {
        bail!("{} is not a directory: pass a description's directory, or a root of them", path.display());
    }
    let path = path.canonicalize().with_context(|| format!("cannot read {}", path.display()))?;
    if let Some(id) = answer {
        if wait || closed {
            bail!(
                "--answer answers one note: run it without --wait or --closed, which list notes, then answer what they list"
            );
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
    if wait && closed {
        bail!("--wait waits for an open note; --closed lists the closed ones: give one of them");
    }
    let status = if closed { Status::Closed } else { Status::Open };
    loop {
        let (open, counts) = listed(&path, status)?;
        if !wait || counts.open > 0 {
            return Ok(
                json!({ "root": path, "notes": open, "open": counts.open, "answered": counts.answered }),
            );
        }
        std::thread::sleep(WAIT_BETWEEN);
    }
}

/// The notes of every description under `root` that are `status`, as
/// `notes` answers each, and how many there are open and answered: read
/// from the files as they now stand, descriptions added since included,
/// each note whose target is gone closed first.
fn listed(root: &Path, status: Status) -> anyhow::Result<(Vec<Value>, Counts)> {
    let dirs: Vec<PathBuf> = crate::gallery::descriptions(root)
        .into_iter()
        .filter_map(|row| row.description().map(Path::to_path_buf))
        .collect();
    let (mut open, mut counts) = (Vec::new(), Counts::default());
    for dir in &dirs {
        settle(dir, root)?;
        let notes = read(dir)?;
        counts = counts.add(Counts::of(&notes));
        open.extend(notes.iter().filter(|note| note.status == status).map(|note| note.answer(root, dir)));
    }
    Ok((open, counts))
}

/// What `check` says of the notes of the description in `dir`, under
/// `root`: how many are open and answered, each that closes itself closed
/// first, or why they cannot be read. Never a fault.
pub fn counted(dir: &Path, root: &Path) -> Value {
    match settle(dir, root).and_then(|_| read(dir)) {
        Ok(notes) => json!(Counts::of(&notes)),
        Err(e) => json!({ "open": 0, "answered": 0, "error": format!("{e:#}") }),
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
        assert_eq!(notes[0].thread, [Said { by: By::Person, says: RIGHT.into() }], "✓ needs no line");
        assert_eq!(notes[1].thread[0].says, "hosted fields");
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
    fn a_guess_stands_while_the_description_still_makes_it() {
        let dir = dir("stands");
        let screen = json!({ "root": { "id": "list", "component": "Column", "children": [
            { "id": "card", "component": "TextField", "assumed": "drawn here; or hosted" },
            { "id": "gauge", "component": "Gauge" },
            { "id": "row", "component": "Row", "on": { "tap": { "navigate": "OrderDetail" } } },
        ] } });
        let none = BTreeSet::new;
        assert_eq!(Guess::of(&screen, "card", "TextField", &none()), Guess::Author);
        assert_eq!(Guess::of(&screen, "row", "Row", &none()), Guess::Undrawn(vec!["OrderDetail".into()]));
        assert_eq!(Guess::of(&screen, "gauge", "Gauge", &none()), Guess::Component("Gauge".into()));
        for (guess, written) in [
            (Guess::Author, "card"),
            (Guess::Undrawn(vec!["OrderDetail".into()]), "row"),
            (Guess::Component("Gauge".into()), "gauge"),
        ] {
            assert!(guess.stands(&screen, &dir, written, none), "{guess:?}");
        }
        let drawn = || BTreeSet::from(["OrderDetail".to_string()]);
        assert!(!Guess::Undrawn(vec!["OrderDetail".into()]).stands(&screen, &dir, "row", drawn), "drawn now");
        let taken = json!({ "root": { "id": "list", "component": "Column", "children": [
            { "id": "card", "component": "TextField" },
            { "id": "gauge", "component": "Text" },
        ] } });
        assert!(!Guess::Author.stands(&taken, &dir, "card", none), "its assumed taken out");
        assert!(
            !Guess::Component("Gauge".into()).stands(&taken, &dir, "gauge", none),
            "another component now"
        );
        assert!(
            !Guess::Component("Row".into()).stands(&screen, &dir, "row", none),
            "a component underdraw knows"
        );
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
        assert!(counted(&dir, &dir)["error"].as_str().unwrap().contains("notes.json"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn what_a_note_is_on_is_gone_once_the_description_no_longer_writes_it() {
        let screen = json!({
            "screen": "OrderList",
            "fixtures": { "default": {} },
            "phases": { "loading": { "id": "spinner" } },
            "root": { "id": "list", "children": [
                { "id": "nav-export", "on": { "tap": { "navigate": "ExportDocuments" } } },
                { "id": "order-{index}" },
            ] },
            "transitions": [{ "from": "default", "on": "nav-export.tap", "to": "loading" }],
        });
        let node = |written: &str| Origin { written: written.into(), at: String::new(), instance: vec![] };
        let default = Shown::Fixture("default".into());
        let on_node = |state: Shown, written: &str| On::Node {
            state,
            node: Pointed { origin: node(written), label: None, width: 390 },
        };
        let region = |nodes: Vec<Origin>| On::Region {
            state: default.clone(),
            region: Region { area: Frame { x: 0, y: 0, width: 1, height: 1 }, width: 390, nodes },
        };
        let transition = |to: &str| On::Transition {
            transition: Transition {
                from: "default".into(),
                on: "nav-export.tap".into(),
                outcome: None,
                to: to.into(),
            },
        };
        let navigate = |from: &str| On::Navigate {
            navigate: Navigate { from: from.into(), node: "nav-export".into(), to: "ExportDocuments".into() },
        };
        for there in [
            on_node(default.clone(), "order-{index}"),
            on_node(Shown::Phase("loading".into()), "spinner"),
            On::State { state: default.clone() },
            region(vec![]),
            region(vec![node("gone"), node("list")]),
            transition("loading"),
            navigate("OrderList"),
        ] {
            assert_eq!(gone(&screen, &there), None, "{there:?}");
        }
        let said = |on: On| gone(&screen, &on).unwrap();
        assert_eq!(said(on_node(default.clone(), "order-{id}")), "order-{id} is gone (removed or renamed)");
        assert_eq!(
            said(on_node(Shown::Fixture("empty".into()), "list")),
            "fixture empty is gone (removed or renamed)"
        );
        assert_eq!(
            said(On::State { state: Shown::Phase("error".into()) }),
            "phase error is gone (removed or renamed)"
        );
        assert_eq!(
            said(region(vec![node("a"), node("b")])),
            "the nodes it covered, a, b, are gone (removed or renamed)"
        );
        assert_eq!(
            said(transition("default")),
            "transition from default on nav-export.tap to default is gone (removed or changed)"
        );
        assert_eq!(
            said(navigate("Orders")),
            "navigate from Orders on nav-export to ExportDocuments is gone (removed or changed)"
        );
    }
}
