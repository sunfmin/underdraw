# Transitions are a top-level list, for the gallery only

A screen's fixtures and phases are its states, but nothing said how a
person gets from one to another: that tapping a row's cancel button opens
the cancel dialog, or that confirming it lands on the list again — or, if
the order shipped meanwhile, stays on the dialog. The gallery needs that
to lay the states out as an overview and to click through them as a
prototype, and the person settling a design needs it to see a flow, not
a heap of pictures.

The decisions:

- **A top-level `transitions` list** in screen.json, each
  `{"from", "on", "outcome"?, "to"}`. `from` and `to` are states, a
  fixture or a phase; `on` is a node's id as written and one of its
  events, `"cd-confirm.tap"`.
- **An outcome exactly when the event calls an operation**: `ok`, or one
  of the operation's `fails`. One event leads to as many states as its
  operation can answer.
- **Opening and closing an overlay are transitions like any other.** An
  event that navigates or goes back leaves the screen, and is not one.
- **For the gallery only.** `handoff` and `compare` ignore transitions: a
  platform builds what a node does from its handler and the states from
  the fixtures, and nothing in a transition changes either.
- **check names every fault**: an unknown state or node, a node not shown
  in its `from` state, an event the node does not handle, a navigate or
  back event, and an outcome missing on a call, given on a non-call, or
  not one of the operation's.

## Considered options

- **On the fixture** (`"$next": {…}` in each). A fixture is one sample of
  the view model; what follows from it is not data, and a phase has no
  fixture to hold it.
- **On the operation**, each failure naming the state it leads to. Only
  calls would have one: opening a dialog calls nothing, and one operation
  called from two states leads to different ones.
- **On the node's handler**, beside its actions. The handler is what the
  app does, and a platform builds it; a state is a sample the person
  looks at. Mixing them would make `handoff` hand over pictures as if
  they were behaviour, and a repeated node would need a state per row.
- **Also in `handoff`**, as the expected state after each action. A
  platform's tests would assert it, but a transition names sample data
  ("this fixture"), which an app's real data never matches; the expected
  states already say what each state shows and calls.

## Consequences

- A transition names a node by its written id, so a row's button inside
  an item is named once (`order-{index}-cancel`), not once per row.
- Whether a node is shown in a state is known only by building that
  state, so check names a node missing from its `from` state only once
  the description has no other fault.
- Transitions need not be complete: a state no transition reaches is
  still drawn in the gallery.
