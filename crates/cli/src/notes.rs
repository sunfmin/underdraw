//! What the person and the agent say of a description, pinned where it is
//! said (ADR 0019, ADR 0023): kept in notes.json beside its screen.json,
//! meant to be committed. A note is a conversation on a node, a whole
//! state, a transition's arrow or a navigate's arrow: the person's
//! messages and the agent's, in the order said, one for each. Only the
//! person starts one, in the gallery, and says more in it there; the agent answers in it, at any length, and may
//! ask back. It is never closed: while the person spoke last it waits on
//! the agent, and once the agent did it is answered; that is read from
//! the messages, never stored. What is stored is what tells the window
//! more: when `--wait` handed the person's message to the agent (it is
//! taken), and whether the person has seen the agent's answer (until then
//! it is unread). What the agent wants to ask unprompted is
//! an assumption. An assumption is answered with a note on its node: ✓
//! says the guess was right; ✗ says it was wrong, with what is right. A
//! note whose node or arrow is gone joins its state's note ([`settle`]).
//! Each change is read from the file as it now stands and written back
//! whole, so the window and the agent never undo each other's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use underdraw_format::Screen;

use crate::SCREEN;
use crate::draw::Shown;
use crate::reference::{On, Reference};

/// The file a description's notes are kept in, beside its screen.json; a
/// shared component's, in `components/<Name>/notes.json`.
pub const NOTES: &str = "notes.json";

/// Where a conversation stands, read from who spoke last and what was
/// marked on that message: never stored itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stands {
    /// The person spoke last, and `--wait` has not handed it to the agent
    /// yet: it waits on the agent.
    Waiting,
    /// The person spoke last, and `--wait` handed it to the agent, which
    /// is changing it: it still waits on the agent.
    Taken,
    /// The agent spoke last, and the person has not seen the conversation
    /// since.
    Unread,
    /// The agent spoke last, and the person has seen it.
    Read,
}

/// Who said a message of a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum By {
    Person,
    Agent,
    /// underdraw itself: the line saying where the messages after it were,
    /// in a note on what is gone, before they joined this note. Neither
    /// person nor agent; where the note stands is read past it.
    Underdraw,
}

/// One message of a note: what the person wrote, or what the agent answered,
/// as many lines as it takes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Said {
    pub by: By,
    pub says: String,
    /// When `--wait` handed the person's message to the agent, in seconds
    /// since 1970: none until then, and on any other's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken: Option<u64>,
    /// Whether the agent's message is yet to be seen by the person: from
    /// when the agent says it until the gallery shows its conversation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unread: bool,
}

impl Said {
    /// `says`, said by `by`: not taken, not unread.
    pub fn new(by: By, says: impl Into<String>) -> Said {
        Said { by, says: says.into(), taken: None, unread: false }
    }
}

/// The time now, in seconds since 1970, as a message is marked taken.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// One note, a conversation, pinned as ⌘C references what it is on: a node as the
/// description writes it with the instance the person pointed at, a whole
/// state, a transition or a navigate. A navigate's note is kept by the
/// description the navigate is written in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `N1`, `N2`…: numbered in the order written, within the description.
    pub id: String,
    /// What it is on, written as its fields: `state` and `node`, `state`,
    /// `transition` or `navigate`. A note written on a region before
    /// regions went (ADR 0023) is read as on its whole state.
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
    pub fn of(screen: &Screen, written: &str, component: &str, drawn: &BTreeSet<String>) -> Guess {
        let node = screen.written_node(written);
        if node.is_some_and(|node| node.contains_key("assumed")) {
            return Guess::Author;
        }
        let undrawn = undrawn(screen, written, drawn);
        if undrawn.is_empty() { Guess::Component(component.to_string()) } else { Guess::Undrawn(undrawn) }
    }
}

/// The screens the node written `written` in `screen` navigates to that
/// are not among `drawn`.
fn undrawn(screen: &Screen, written: &str, drawn: &BTreeSet<String>) -> Vec<String> {
    screen
        .navigates()
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
    /// person spoke last, taken once `--wait` handed that message over;
    /// answered once the agent spoke, unread until the person has seen it.
    pub fn stands(&self) -> Stands {
        match self.messages.iter().rev().find(|said| said.by != By::Underdraw) {
            Some(Said { by: By::Agent, unread: true, .. }) => Stands::Unread,
            Some(Said { by: By::Agent, .. }) => Stands::Read,
            Some(Said { taken: Some(_), .. }) => Stands::Taken,
            _ => Stands::Waiting,
        }
    }

    /// Whether it waits on the agent: the person spoke last, taken or not.
    pub fn waits(&self) -> bool {
        matches!(self.stands(), Stands::Waiting | Stands::Taken)
    }

    /// When the person's last message was handed to the agent, in seconds
    /// since 1970, while the agent is changing it ([`Stands::Taken`]).
    pub fn taken(&self) -> Option<u64> {
        let last = self.messages.iter().rev().find(|said| said.by != By::Underdraw)?;
        if last.by == By::Person { last.taken } else { None }
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

/// What the person said, `says`, added to the note on what `on`
/// references: the last of its description's notes on the same thing
/// ([`On::same`]), where the conversation goes on; or a note started on
/// it, numbered after the last, when there is none. The note's id.
pub fn say(on: &Reference, says: &str) -> anyhow::Result<String> {
    let says = said(says)?;
    let found = change(&on.description, |notes| {
        let note = notes.iter_mut().rev().find(|note| note.on.same(&on.on));
        Ok(note.map(|note| {
            note.messages.push(Said::new(By::Person, says.clone()));
            note.id.clone()
        }))
    })?;
    match found {
        Some(id) => Ok(id),
        None => added(on, says, None, None),
    }
}

/// What a ✓ note says when the person writes nothing more.
pub const RIGHT: &str = "✓ right, keep it: take the guess out of the description";

/// A note answering the assumption on the node `on` references, as
/// `answers` says, added to its description's notes, numbered after the last:
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
/// `proposal`, added to its description's notes, numbered after the last, its
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
            messages: vec![Said::new(By::Person, says)],
            proposes,
            answers,
        });
        Ok(id)
    })
}

/// Note `id` of the description in `dir` answered by the agent with
/// `reply`, as many lines as it takes, kept as written but for its ends,
/// unread until the person has seen it. The note as it now stands, and
/// whether that changed it: the same reply as the agent's last message,
/// given again, changes nothing.
pub fn answer(dir: &Path, id: &str, reply: &str) -> anyhow::Result<(Note, bool)> {
    let reply = reply.trim();
    if reply.is_empty() {
        bail!("--reply says what was done, or why not, or asks back: it is not empty");
    }
    change(dir, |notes| {
        let note = find(notes, dir, id)?;
        let last = note.messages.last();
        let changed = !last.is_some_and(|last| last.by == By::Agent && last.says == reply);
        if changed {
            note.messages.push(Said { unread: true, ..Said::new(By::Agent, reply) });
        }
        Ok((note.clone(), changed))
    })
}

/// Each of `notes` that waits on the agent marked taken at `now`: each
/// message the person said since the agent last spoke, not taken yet.
/// Whether any was.
fn take(notes: &mut [Note], now: u64) -> bool {
    let mut taken = false;
    for note in notes.iter_mut().filter(|note| note.waits()) {
        let since = note.messages.iter().rposition(|said| said.by == By::Agent).map_or(0, |at| at + 1);
        for said in &mut note.messages[since..] {
            if said.by == By::Person && said.taken.is_none() {
                said.taken = Some(now);
                taken = true;
            }
        }
    }
    taken
}

/// The notes of the description in `dir` that wait on the agent handed to
/// it at `now`, in seconds since 1970: each marked taken ([`take`]).
/// Written only when that changes any; whether it did.
pub fn hand_over(dir: &Path, now: u64) -> anyhow::Result<bool> {
    if !read(dir)?.iter().any(|note| note.stands() == Stands::Waiting) {
        return Ok(false);
    }
    change(dir, |notes| Ok(take(notes, now)))
}

/// The notes `ids` of the description in `dir` seen by the person: each
/// message of the agent's in them read. Written only when that changes
/// any; whether it did.
pub fn seen(dir: &Path, ids: &[String]) -> anyhow::Result<bool> {
    let unread = |note: &Note| ids.contains(&note.id) && note.messages.iter().any(|said| said.unread);
    if !read(dir)?.iter().any(unread) {
        return Ok(false);
    }
    change(dir, |notes| {
        for note in notes.iter_mut().filter(|note| ids.contains(&note.id)) {
            for said in &mut note.messages {
                said.unread = false;
            }
        }
        Ok(true)
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
/// something; each message handed over is marked taken, so the window
/// tells the person the agent is changing it. Nothing is kept between
/// runs; the files are read again each time.
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
        let (waiting, counts) = listed(&path, wait)?;
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
/// from the files as they now stand, descriptions added since included,
/// each note on what is gone joined its state's first ([`settle`]); given
/// `take`, each marked taken now, unless it was already.
fn listed(root: &Path, take: bool) -> anyhow::Result<(Vec<Value>, Counts)> {
    let dirs: Vec<PathBuf> = crate::gallery::descriptions(root)
        .into_iter()
        .filter_map(|row| row.description().map(Path::to_path_buf))
        .collect();
    let (mut waiting, mut counts) = (Vec::new(), Counts::default());
    for dir in &dirs {
        settle(dir)?;
        if take {
            hand_over(dir, now())?;
        }
        let notes = read(dir)?;
        counts = counts.add(Counts::of(&notes));
        waiting.extend(notes.iter().filter(|note| note.waits()).map(|note| note.answer(root, dir)));
    }
    Ok((waiting, counts))
}

/// What `check` says of the notes of the description in `dir`: how many
/// wait on the agent and how many are answered, each on what is gone
/// joined its state's first ([`settle`]), or why they cannot be read.
/// Never a fault of its own: a notes.json in the old form is one `check`
/// reports among the description's findings.
pub fn counted(dir: &Path) -> Value {
    match settle(dir).and_then(|_| read(dir)) {
        Ok(notes) => json!(Counts::of(&notes)),
        Err(e) => json!({ "waiting": 0, "answered": 0, "error": format!("{e:#}") }),
    }
}

/// Where a note on `on` was, now that it is gone from `screen`, the
/// description as it is now written: the line its messages follow in the
/// note they join (`on `price`, now gone`). None while it is there. A node
/// is gone once no tree writes its id, or its state is; a region once its
/// state is, or every node it covered; a whole state once the description
/// has no state by its name; a transition or a navigate once none is
/// written with the same keys.
fn gone(screen: &Screen, on: &On) -> Option<String> {
    let now_gone = |was: String| Some(format!("on {was}, now gone"));
    let state_gone = on.state().filter(|state| {
        let key = match state {
            Shown::Fixture(_) => "fixtures",
            Shown::Phase(_) => "phases",
        };
        !screen
            .description()
            .get(key)
            .and_then(Value::as_object)
            .is_some_and(|all| all.contains_key(state.name()))
    });
    let in_state = |was: String| match state_gone {
        Some(state) => now_gone(format!("{was} in {}", said_state(state))),
        None => now_gone(was),
    };
    let writes = |id: &str| screen.writes(id);
    match on {
        On::Node { node, .. } if state_gone.is_some() || !writes(&node.origin.written) => {
            in_state(format!("`{}`", node.origin.written))
        }
        On::State { state } if state_gone.is_some() => {
            now_gone(format!("the whole of {}", said_state(state)))
        }
        On::Transition { transition } => {
            let declared = screen.transitions.iter().any(|declared| {
                declared.from == transition.from
                    && format!("{}.{}", declared.node, declared.event) == transition.on
                    && declared.outcome == transition.outcome
                    && declared.to == transition.to
            });
            if declared { None } else { now_gone(format!("transition {}", transition.said())) }
        }
        On::Navigate { navigate } => {
            let from = screen.name() == navigate.from;
            let written = screen
                .navigates()
                .into_iter()
                .any(|written| written.node == navigate.node && written.to == navigate.to);
            if from && written { None } else { now_gone(format!("navigate {}", navigate.said())) }
        }
        On::Node { .. } | On::State { .. } => None,
    }
}

/// A state as the line of a gone note names it: fixture `empty`.
fn said_state(state: &Shown) -> String {
    match state {
        Shown::Fixture(name) => format!("fixture `{name}`"),
        Shown::Phase(name) => format!("phase `{name}`"),
    }
}

/// The state whose note a gone note on `on` joins, among `states`, the
/// description's in order, fixtures first: the state it was in, or the
/// transition was from, while the description has it; else the first.
fn joins(on: &On, states: &[Shown]) -> Option<Shown> {
    let was = match on {
        On::Transition { transition } => states.iter().find(|state| state.name() == transition.from),
        on => on.state().and_then(|was| states.iter().find(|state| *state == was)),
    };
    was.or(states.first()).cloned()
}

/// `notes` with each note on what is gone from `screen` joined to its
/// state's note, among `states` ([`joins`]): its messages, after a line
/// saying where they were, in the order the notes were, go in before what
/// the person said in that note since the agent last spoke, so what waited
/// on the agent still waits. A state with no note gets the first gone
/// note itself, pinned to the whole state and keeping its id. Whether any
/// joined.
fn join(notes: &mut Vec<Note>, screen: &Screen, states: &[Shown]) -> bool {
    let mut moved: Vec<Vec<Said>> = vec![Vec::new(); notes.len()];
    let mut gone_from = vec![false; notes.len()];
    let mut joined = false;
    for i in 0..notes.len() {
        let Some((was, state)) = gone(screen, &notes[i].on).zip(joins(&notes[i].on, states)) else {
            continue;
        };
        joined = true;
        let whole = On::State { state };
        let line = Said::new(By::Underdraw, was);
        let home = (0..notes.len()).find(|&home| home != i && !gone_from[home] && notes[home].on == whole);
        match home {
            Some(home) => {
                gone_from[i] = true;
                moved[home].push(line);
                moved[home].append(&mut notes[i].messages);
            }
            None => {
                let note = &mut notes[i];
                note.on = whole;
                note.messages.insert(0, line);
                note.proposes = None;
                note.answers = None;
            }
        }
    }
    let mut was_gone = gone_from.into_iter();
    let mut moved = moved.into_iter();
    notes.retain_mut(|note| {
        let block = moved.next().unwrap_or_default();
        let messages = &mut note.messages;
        let since = messages.iter().rposition(|said| said.by != By::Person).map_or(0, |at| at + 1);
        messages.splice(since..since, block);
        !was_gone.next().unwrap_or(false)
    });
    joined
}

/// The notes of the description in `dir` on what is gone from it — a node
/// removed or renamed, a state, an arrow — each joined to its state's note
/// ([`join`]), so nothing said is lost: whoever reads the notes first, the
/// window or a command, joins them, and reading again joins nothing more.
/// Judged only once the description checks clean, so that one half edited
/// moves nothing.
pub fn settle(dir: &Path) -> anyhow::Result<()> {
    let notes = read(dir)?;
    let Ok((screen, _)) = crate::read(dir) else {
        return Ok(());
    };
    if !notes.iter().any(|note| gone(&screen, &note.on).is_some()) {
        return Ok(());
    }
    let Ok(checked) = crate::checked(&screen, dir) else {
        return Ok(());
    };
    if !checked.ok {
        return Ok(());
    }
    let states: Vec<Shown> = checked
        .fixtures
        .into_iter()
        .map(Shown::Fixture)
        .chain(checked.phases.into_iter().map(Shown::Phase))
        .collect();
    change(dir, |notes| {
        join(notes, &screen, &states);
        Ok(())
    })
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
    fn a_note_waits_or_is_answered_by_who_spoke_last() {
        let dir = dir("life");
        assert_eq!(read(&dir).unwrap(), [], "no notes.json: no notes");
        assert_eq!(say(&on(&dir), "  the price is cut  ").unwrap(), "N1");
        let whole =
            Reference { description: dir.clone(), on: On::State { state: Shown::Fixture("default".into()) } };
        assert_eq!(say(&whole, "and the date").unwrap(), "N2", "numbered after the last");
        let notes = read(&dir).unwrap();
        assert_eq!(notes[0].stands(), Stands::Waiting, "the person spoke last");
        assert_eq!(notes[0].messages, [Said::new(By::Person, "the price is cut")]);
        assert_eq!(notes[0].reference(&dir), on(&dir), "pinned as ⌘C references it");
        assert_eq!(Counts::of(&notes), Counts { waiting: 2, answered: 0 });

        let (note, changed) = answer(&dir, "N1", "made it wrap").unwrap();
        assert!(changed);
        assert_eq!(
            (note.stands(), note.reply()),
            (Stands::Unread, Some("made it wrap")),
            "the agent spoke last"
        );
        let before = std::fs::read_to_string(dir.join(NOTES)).unwrap();
        let (_, changed) = answer(&dir, "N1", "made it wrap").unwrap();
        assert!(!changed, "answered twice the same: nothing more");
        assert_eq!(std::fs::read_to_string(dir.join(NOTES)).unwrap(), before);
        let (note, changed) = answer(&dir, "N1", "should it wrap at 1120 too?").unwrap();
        assert!(changed && note.messages.len() == 3, "the agent may say more, and ask back");
        assert_eq!(note.stands(), Stands::Unread);

        let file: Value = serde_json::from_str(&before).unwrap();
        for key in ["status", "closed_because", "thread"] {
            assert!(file["notes"][0].get(key).is_none(), "{key} is not stored: {file}");
        }
        assert_eq!(Counts::of(&read(&dir).unwrap()), Counts { waiting: 1, answered: 1 });
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_message_is_waiting_then_taken_then_answered_unread_then_read() {
        let dir = dir("stands");
        say(&on(&dir), "the price is cut").unwrap();
        let stands = || read(&dir).unwrap()[0].stands();
        assert_eq!(stands(), Stands::Waiting, "not handed to the agent yet");
        assert_eq!(read(&dir).unwrap()[0].taken(), None);

        let mut notes = read(&dir).unwrap();
        assert!(take(&mut notes, 1_000), "handed over");
        assert!(!take(&mut notes, 2_000), "taken once: the time it was first handed over stays");
        write(&dir, notes).unwrap();
        assert_eq!(stands(), Stands::Taken);
        assert!(read(&dir).unwrap()[0].waits(), "taken, it still waits on the agent");
        assert_eq!(read(&dir).unwrap()[0].taken(), Some(1_000));
        let file: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(NOTES)).unwrap()).unwrap();
        assert_eq!(file["notes"][0]["messages"][0]["taken"], 1_000, "kept in notes.json");

        say(&on(&dir), "and the date").unwrap();
        assert_eq!(stands(), Stands::Waiting, "said more since: that is not handed over yet");

        answer(&dir, "N1", "made it wrap").unwrap();
        assert_eq!(stands(), Stands::Unread, "the agent spoke last, not seen yet");
        assert_eq!(read(&dir).unwrap()[0].taken(), None, "answered: nothing is being changed");
        let (_, changed) = answer(&dir, "N1", "made it wrap").unwrap();
        assert!(!changed, "the same reply again: nothing more");

        assert!(!seen(&dir, &["N9".into()]).unwrap(), "another note seen: nothing written");
        assert!(seen(&dir, &["N1".into()]).unwrap());
        assert_eq!(stands(), Stands::Read, "kept read in notes.json, for the window opened again");
        let before = std::fs::read_to_string(dir.join(NOTES)).unwrap();
        assert!(!seen(&dir, &["N1".into()]).unwrap(), "seen again: nothing written");
        assert_eq!(std::fs::read_to_string(dir.join(NOTES)).unwrap(), before);
        assert!(!before.contains("unread"), "a read message says nothing of it: {before}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_reply_keeps_every_line_it_has() {
        let dir = dir("lines");
        say(&on(&dir), "the price").unwrap();
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
        say(&on(&dir), "x").unwrap();
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
        assert_eq!(notes[0].messages, [Said::new(By::Person, RIGHT)], "✓ needs no line");
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
        let screen = Screen::read(&screen, Default::default());
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
        say(&on(&dir), "the price").unwrap();
        for reply in ["", "  ", "\n\n"] {
            assert!(answer(&dir, "N1", reply).is_err(), "{reply:?}");
        }
        let error = answer(&dir, "N9", "done").unwrap_err().to_string();
        assert!(error.contains("no note N9") && error.contains("[\"N1\"]"), "{error}");
        assert!(say(&on(&dir), " ").is_err(), "a note says something");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn notes_json_is_written_as_the_reference_names_the_node() {
        let dir = dir("file");
        say(&on(&dir), "the price").unwrap();
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
    fn saying_more_goes_on_in_the_note_on_the_same_thing_or_starts_one() {
        let dir = dir("say");
        assert_eq!(say(&on(&dir), "the price is cut").unwrap(), "N1", "none yet: one started");
        let mut other = on(&dir);
        if let On::Node { node, .. } = &mut other.on {
            (node.origin.instance, node.label, node.width) = (vec![0], Some(9), 1120);
        }
        assert_eq!(say(&other, "  and the date  ").unwrap(), "N1", "another instance of the node: its note");
        let state =
            Reference { description: dir.clone(), on: On::State { state: Shown::Fixture("default".into()) } };
        assert_eq!(say(&state, "too busy").unwrap(), "N2", "the whole state: a note of its own");
        answer(&dir, "N1", "made it wrap").unwrap();
        assert_eq!(say(&on(&dir), "better").unwrap(), "N1");
        let notes = read(&dir).unwrap();
        let said: Vec<(By, &str)> = notes[0].messages.iter().map(|m| (m.by, m.says.as_str())).collect();
        assert_eq!(
            said,
            [
                (By::Person, "the price is cut"),
                (By::Person, "and the date"),
                (By::Agent, "made it wrap"),
                (By::Person, "better")
            ],
            "in the order said"
        );
        assert_eq!(notes[0].stands(), Stands::Waiting, "the person spoke last again");
        assert!(say(&state, "   ").is_err(), "nothing said is not sent");
        assert_eq!(read(&dir).unwrap()[1].messages.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_note_written_on_a_region_is_read_as_on_its_whole_state() {
        let dir = dir("region");
        let region = json!({ "notes": [{
            "id": "N1",
            "state": { "fixture": "default" },
            "region": { "box": { "x": 8, "y": 40, "width": 200, "height": 60 }, "width": 390, "nodes": [] },
            "messages": [{ "by": "person", "says": "too busy" }],
        }]});
        std::fs::write(dir.join(NOTES), region.to_string()).unwrap();
        let note = read(&dir).unwrap().remove(0);
        assert_eq!(note.on, On::State { state: Shown::Fixture("default".into()) });
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
            say(&Reference { description: dir.clone(), on: on.clone() }, "here").unwrap();
        }
        say(&on(&dir), "and here").unwrap();
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
    #[test]
    fn a_note_on_what_is_gone_joins_its_states_note_saying_where_it_was() {
        let screen = json!({
            "screen": "Orders",
            "fixtures": { "default": {}, "empty": {} },
            "root": { "id": "list", "component": "Column", "children": [
                { "id": "price", "component": "Text" },
                { "id": "go", "component": "Button", "on": { "tap": { "navigate": "Next" } } },
            ] },
            "transitions": [{ "from": "default", "on": "go.tap", "to": "empty" }],
        });
        let screen = Screen::read(&screen, Default::default());
        let states = [Shown::Fixture("default".into()), Shown::Fixture("empty".into())];
        let node = |state: &str, id: &str| On::Node {
            state: Shown::Fixture(state.into()),
            node: Pointed {
                origin: Origin { written: id.into(), at: "/root".into(), instance: vec![] },
                label: None,
                width: 390,
            },
        };
        let whole = |state: Shown| On::State { state };
        let transition = |to: &str| On::Transition {
            transition: Transition {
                from: "default".into(),
                on: "go.tap".into(),
                outcome: None,
                to: to.into(),
            },
        };
        let navigate = |to: &str| On::Navigate {
            navigate: Navigate { from: "Orders".into(), node: "go".into(), to: to.into() },
        };
        let person = |says: &str| Said::new(By::Person, says);
        let agent = |says: &str| Said::new(By::Agent, says);
        let line = |says: &str| Said::new(By::Underdraw, says);
        let note = |id: &str, on: On, messages: Vec<Said>| Note {
            id: id.into(),
            on,
            messages,
            proposes: None,
            answers: None,
        };
        let kept = vec![
            note("N1", node("default", "price"), vec![person("there")]),
            note("N2", transition("empty"), vec![person("there")]),
            note("N3", navigate("Next"), vec![person("there")]),
        ];
        let mut notes = kept.clone();
        assert!(!join(&mut notes, &screen, &states), "nothing is gone");
        assert_eq!(notes, kept);
        let mut alone = vec![note("N1", node("empty", "total"), vec![person("cut")])];
        assert!(join(&mut alone, &screen, &states), "one gone alone is pinned to its state");
        assert_eq!(alone[0].on, whole(Shown::Fixture("empty".into())));

        let proposal = Proposal { on: "price.tap".into(), goes: Goes::To("empty".into()) };
        let mut notes = vec![
            note("N1", whole(Shown::Fixture("default".into())), vec![person("too dense")]),
            Note {
                proposes: Some(proposal),
                ..note("N2", node("empty", "total"), vec![person("cut"), agent("wrapped it")])
            },
            note("N3", node("default", "total"), vec![person("bold")]),
            note("N4", node("removed", "price"), vec![person("red")]),
            note("N5", whole(Shown::Phase("loading".into())), vec![person("slow")]),
            note("N6", transition("removed"), vec![person("why?")]),
            note("N7", navigate("Elsewhere"), vec![person("where?"), agent("there")]),
            kept[0].clone(),
        ];
        assert!(join(&mut notes, &screen, &states));
        assert_eq!(
            notes[1],
            note(
                "N2",
                whole(Shown::Fixture("empty".into())),
                vec![line("on `total`, now gone"), person("cut"), agent("wrapped it")]
            ),
            "a state with no note gets the gone one, keeping its id, proposing nothing"
        );
        assert_eq!(notes[1].stands(), Stands::Read, "read past underdraw's line");
        assert_eq!(notes.len(), 3, "the rest joined default's: {notes:#?}");
        assert_eq!(notes[2], kept[0], "what is there stays");
        assert_eq!(
            notes[0].messages,
            [
                line("on `total`, now gone"),
                person("bold"),
                line("on `price` in fixture `removed`, now gone"),
                person("red"),
                line("on the whole of phase `loading`, now gone"),
                person("slow"),
                line("on transition from default on go.tap to removed, now gone"),
                person("why?"),
                line("on navigate from Orders on go to Elsewhere, now gone"),
                person("where?"),
                agent("there"),
                person("too dense"),
            ],
            "each after a line saying where, before what still waits on the agent"
        );
        assert_eq!(notes[0].stands(), Stands::Waiting, "what waited still waits");

        let joined = notes.clone();
        assert!(!join(&mut notes, &screen, &states), "reading again moves nothing twice");
        assert_eq!(notes, joined);
        let mut none = vec![note("N1", node("default", "total"), vec![person("x")])];
        assert!(!join(&mut none, &screen, &[]), "no state to join: left as it is");
    }
}
