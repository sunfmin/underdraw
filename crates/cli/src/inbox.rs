//! The inbox: what waits on the person in one description. Each assumption
//! not yet answered, and each note the agent has answered, for the person
//! to close or reopen, are theirs; the notes still open wait on the agent,
//! and are kept apart. Worked out from the description's assumptions, as
//! its states lay them out, and its notes as notes.json now holds them, so
//! an item answered or closed leaves it as the files change.

use crate::draw::Shown;
use crate::notes::{Note, Status};
use crate::reference::On;

/// An assumption on a node, where it is first seen: the state, in the order
/// the tree lists them, that shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Guessed {
    pub state: Shown,
    /// The id the node is given there, to pick it by.
    pub id: String,
    /// Its id as the description writes it: a note answering it names it so.
    pub written: String,
    /// What was assumed.
    pub assumed: String,
}

/// One thing that waits on the person.
#[derive(Debug, PartialEq)]
pub enum Item<'a> {
    /// An assumption no note answers yet.
    Guess(&'a Guessed),
    /// A note the agent has answered: the person closes or reopens it.
    Answered(&'a Note),
}

/// The inbox of one description.
#[derive(Debug, PartialEq)]
pub struct Inbox<'a> {
    /// What waits on the person: each assumption not yet answered, as
    /// `guessed` lists them, then each note answered, in the order written.
    pub yours: Vec<Item<'a>>,
    /// The notes still open, waiting on the agent, in the order written.
    pub agents: Vec<&'a Note>,
}

impl<'a> Inbox<'a> {
    /// The inbox of a description whose assumptions are `guessed`, each
    /// once, and whose notes are `notes`. An assumption is answered while a
    /// note on its node that answers it is not closed: one closed, the
    /// assumption still there, waits on the person again.
    pub fn of(guessed: &'a [Guessed], notes: &'a [Note]) -> Inbox<'a> {
        let answers = |guess: &Guessed| {
            notes.iter().any(|note| {
                let on = match &note.on {
                    On::Node { node, .. } => node.origin.written == guess.written,
                    _ => false,
                };
                on && note.answers.is_some() && note.status != Status::Closed
            })
        };
        let guesses = guessed.iter().filter(|guess| !answers(guess)).map(Item::Guess);
        let answered = notes.iter().filter(|note| note.status == Status::Answered).map(Item::Answered);
        Inbox {
            yours: guesses.chain(answered).collect(),
            agents: notes.iter().filter(|note| note.status == Status::Open).collect(),
        }
    }

    /// Whether nothing waits, on the person or on the agent.
    pub fn is_empty(&self) -> bool {
        self.yours.is_empty() && self.agents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn guess(written: &str) -> Guessed {
        Guessed {
            state: Shown::Fixture("default".into()),
            id: written.to_string(),
            written: written.to_string(),
            assumed: format!("{written} is guessed; it could be another"),
        }
    }

    /// A note on node `written` of the given status, answering its
    /// assumption when `answers`.
    fn note(id: &str, written: &str, status: &str, answers: bool) -> Note {
        let mut note = json!({
            "id": id,
            "status": status,
            "state": { "fixture": "default" },
            "node": { "id": written, "at": "/root", "instance": [], "label": null, "width": 390 },
            "thread": [{ "by": "person", "says": "this" }],
        });
        if answers {
            note["answers"] = json!({ "assumed": "a guess; or not", "guess": "author", "answer": "wrong" });
        }
        serde_json::from_value(note).unwrap()
    }

    #[test]
    fn two_unanswered_assumptions_and_one_answered_note_are_three_items() {
        let guessed = [guess("card"), guess("nav-export")];
        let notes = [note("N1", "title", "answered", false), note("N2", "title", "open", false)];
        let inbox = Inbox::of(&guessed, &notes);
        assert_eq!(
            inbox.yours,
            [Item::Guess(&guessed[0]), Item::Guess(&guessed[1]), Item::Answered(&notes[0])]
        );
        assert_eq!(inbox.agents, [&notes[1]], "the open note waits on the agent");
    }

    #[test]
    fn an_assumption_leaves_while_a_note_answers_it_and_comes_back_once_that_is_closed() {
        let guessed = [guess("card")];
        let answering = [note("N1", "card", "open", true)];
        assert!(Inbox::of(&guessed, &answering).yours.is_empty(), "answered, waiting on the agent");
        let answered = [note("N1", "card", "answered", true)];
        assert_eq!(Inbox::of(&guessed, &answered).yours, [Item::Answered(&answered[0])]);
        let closed = [note("N1", "card", "closed", true)];
        let inbox = Inbox::of(&guessed, &closed);
        assert_eq!(inbox.yours, [Item::Guess(&guessed[0])], "closed, the guess still there");
        let plain = [note("N1", "card", "open", false)];
        assert_eq!(
            Inbox::of(&guessed, &plain).yours,
            [Item::Guess(&guessed[0])],
            "a note that answers nothing"
        );
    }

    #[test]
    fn a_closed_note_waits_on_no_one() {
        let notes = [note("N1", "title", "closed", false)];
        assert!(Inbox::of(&[], &notes).is_empty());
    }
}
