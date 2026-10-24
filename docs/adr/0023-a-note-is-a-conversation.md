# A note is a conversation

ADR 0019 gave the person notes: one thing said, pinned where it was said,
answered by the agent in one line, then closed or reopened by the person,
and written by holding N and clicking or dragging. In use the person wants
to talk the design through: pick something, say what is wrong, see the
agent change it, and say the next thing, as in a chat. Writing a note took
a mode, closing every answered one was a chore, and one line of reply left
the agent no room to ask back.

The decisions:

- **A note is a conversation**, pinned to a node, a whole state, or an
  arrow: the person's messages and the agent's in the order said. Each has
  at most one. Picking a thing shows its note in the sidebar, under the
  thing's card, with a box at the bottom to write in and a button to send
  (Enter sends, ⇧Enter breaks the line); with nothing picked, the note is
  the whole state's. There is no mode to write one: N is gone.
- **No regions.** Dragging over a picture no longer picks a region; what
  one region covered is said in the state's note instead.
- **A note is never closed.** Who spoke last says where it stands: the
  person, and it waits on the agent; the agent, and it is answered, unread
  until the person has seen it. `--wait` marks a message taken when it
  hands it to the agent, so the window tells "waiting for the agent" from
  "the agent is changing it". The picture marks only what waits and what
  is unread.
- **The agent answers at any length and may ask back**, but still never
  starts a note: what it wants to ask unprompted is an assumption.
- **A note whose node or arrow is gone joins its state's note**, saying
  where it was, so nothing said is lost and the conversation goes on.
- **Confirming (✓) or rejecting (✗) an assumption, and proposing where an
  act should go, are messages** in the node's note, marked as such; ✗ must
  say what is right.
- **Old notes are not converted.** The notes written before are deleted;
  `check` reports a `notes.json` in the old form rather than reading it.

This supersedes ADR 0019's lifecycle (open, answered, closed by the person,
reopened on the whole state, closing itself once its node is gone), its
one-line reply, region notes, and the N mode. What 0019 decided otherwise
stands: the gallery never writes a description, notes live in `notes.json`
beside `screen.json` in git, the agent learns of them through `--wait`
whatever agent it is, and ⌘C copies a reference.

## Considered options

- **The window starts the agent** (say, `claude -p`) on each message. No
  agent need be waiting, but underdraw would be tied to one agent and
  learn how to run it, which 0019 refused for good reason.
- **Several notes per thing**, each closed when done. Closer to 0019, but
  the person would pick a note before speaking, and the chat would be
  split.
- **The agent moves a note when it renames a node.** More exact, but it
  must remember every time; a note joining its state's needs no one to
  remember.
- **Converting old notes.** Kept them readable, but there were few, and
  a second format to read forever costs more than they are worth.
- **Keep regions, without notes.** A region was only ever picked to be
  spoken of, so without its note it has no use.

## Consequences

- The sidebar holds a scrolling conversation and a text box at its foot;
  it needs a real text editor (#151) and a scrollbar (#156).
- A message the agent took but never answered shows how long ago it was
  taken, so a stopped agent is noticed rather than waited on.
- Notes pile up instead of closing; the picture keeps quiet about those
  answered and read.
