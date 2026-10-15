# The person speaks through notes; the gallery never writes a description

The gallery shows a person the design, but what they think of it reached
the agent only as words typed into a conversation: "the third row, the
price, on the phone", which the agent then had to find in `screen.json`.
An assumption could be seen but not answered, and a missing transition
could be found but not asked for. The person needs a handle in the window
itself, and the agent needs what they say pinned to the exact place in the
description.

The decisions:

- **The gallery never writes a description.** Confirming an assumption,
  asking for a transition or changing a word could each be done by the
  window editing `screen.json`, but then two writers change one file, and
  the window must learn to edit templated ids, cases and repeats as the
  agent does. Instead the window writes only **notes**; the agent makes
  every change, and `check` holds it as before.
- **A note is pinned where it was said**: a node, a region, a whole state,
  or an arrow (a transition, or a navigate between screens). It names the
  node as written in the description (`order-{index}`, its JSON path), with
  the instance the person pointed at (`[2]`, its label, the width) as
  context: the agent edits the template, and the instance says which data
  showed the fault.
- **⌘C copies the same reference** as one readable line, for the person
  who would rather say it in the conversation now.
- **The agent answers; the person closes.** A note is open, then answered
  by the agent with one line of reply, then closed or reopened by the
  person. The agent cannot create, close or reopen a note: what it wants
  to ask the person is an **assumption**, the channel that already runs
  that way. A note closes itself, saying why, when what it is pinned to is
  gone, and when an assumption the person confirmed has been taken out;
  the window says so once, and keeps it among the closed, to reopen.
- **An assumption is answered with a note.** ✓ says the guess was right;
  ✗ says it was wrong and must say what is right. The panel shows what was
  guessed, what else it could be, and what each answer means, so the
  person can answer it: `check` refuses an `assumed` too short to say
  that, and the assumptions underdraw makes itself (a screen not drawn
  yet, a kind it does not know) come as whole sentences, in the language
  the agent asks for.
- **Notes live in `notes.json` beside `screen.json`, in git.** The agent
  reads them with `underdraw notes <root>`, waits for a new one with
  `--wait`, and answers one with `--answer`; nothing is kept in a process.
  A note is not a fault: `check` counts the open and answered, and its
  exit code does not change.

## Considered options

- **The window edits the description directly.** Fastest for the person,
  but two writers, and the window becomes a second author of the format.
- **Only copying a reference.** No new file and no lifecycle, but the
  person says one thing at a time, nothing is kept, and nothing says
  whether the agent dealt with it.
- **Notes outside the repository** (ignored, or in a hidden directory).
  A clean tree, but the reason a design changed is lost with them.
- **A button that sends a note to Claude.** It would tie underdraw to one
  agent; `--wait` lets any agent learn of a note from the files.
- **The agent closes what it answers.** One step fewer, but nothing then
  says the person saw the change.

## Consequences

- The window writes a file for the first time; it writes only
  `notes.json`, and only the person's acts.
- A note pinned to a node that the agent renames closes itself, saying the
  node is gone; the person reopens it, on the whole state, if it still
  holds.
- Drawing on the picture, choosing between two versions, and speaking a
  note are left for later.
