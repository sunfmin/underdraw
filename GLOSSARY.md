# underdraw

Describing a screen once, platform-neutrally, so a person can discuss it as a
wireframe and each platform can build it the same way.

## Language

### The description

**Screen description**:
The platform-neutral account of one screen: its params, data, conditions,
fixtures and node tree. The single source of truth for that screen.
_Avoid_: spec

**Project**:
One app's screen descriptions under one directory, sharing one design
language and one **domain**: a screen takes its tokens from the nearest
directory above it that has them, unless its own directory has them, and
can use every component the project shares.
_Avoid_: workspace, app

**Node**:
One element of the screen, with a stable **id** that becomes each platform's
test id, and the **component** it is.
_Avoid_: element, view

**Component**:
What a node is (`Button`, `OrderRow`), named without any platform's
vocabulary, given its data only through the params it declares and varied
only through them and its slots; it holds no view state of its own. Its
body is one of two: a **look**, which a platform builds as one element, or
nodes, each named by the instance's id joined to its own
(`order-3-cancel`). Underdraw has its own; a project's are one screen's,
or shared by every screen of the project.
_Avoid_: kind, widget, partial, fragment

**Look**:
What a component is drawn as in a wireframe, written as data: boxes,
text, `repeat` and `cases`, each key taking only what layout draws.
_Avoid_: template, style

**Overlay**:
A node that floats over its parent node rather than sitting in its flow:
a badge, a dialog, a sheet.
_Avoid_: popup layer, modal

**Anchor**:
Where an overlay floats over its node: a corner, the center, or flush
along the bottom (a sheet).

**Token**:
A named design value (`color.primary`, `space.4`) that nodes use instead of raw
values.

**Icon**:
A small picture a node shows, named as Material Symbols names it
(`arrow_back`), or with `.fill` for its solid form.
_Avoid_: glyph, symbol

**Breakpoint**:
The width class a screen is laid out in (`compact`, `regular`).

**Responsive value**:
A value written once per breakpoint (`{"compact": 1, "regular": 2}`), in
place of one value for every width.

**Assumption**:
Something the agent guessed and the person has not confirmed, such as a
screen a `navigate` names that no description under the root declares (a
screen not drawn yet).
_Avoid_: guess, TODO

**Note**:
What the person says to the agent about a design, pinned where they said it:
a node, a region, a whole state, or an arrow. The agent answers it; the
person closes it, or it closes itself, saying why, once what it is pinned to
is gone; reopened, it is pinned to the whole state. Confirming or rejecting an **Assumption** is a note on it.
_Avoid_: comment, feedback

### Data and decisions

**View model**:
The data a screen shows, shaped as the screen needs it: its `data` and the named
`types` it uses.
_Avoid_: API response, payload

**Derived field**:
A view-model field computed from other values, never by the screen, and
saying by whom: the app (for the screen alone, so never in an operation's
result) or the server (which answers with it).

**Param**:
What a screen is opened with, declared by the screen: a `data` field each
fixture gives, and each `navigate` to the screen must give, of its type.
_Avoid_: argument, route param

**Fixture**:
One sample value of the view model; each fixture puts the screen in one state.
_Avoid_: mock data, example

**View state**:
A view-model field only the screen holds, such as an open menu or the row a
popup is about, marked `"view": true`; never part of an operation's result.
_Avoid_: UI state, local state

**Phase**:
A stage before the view model exists, such as loading or error, with its own
node tree that reads no data.
_Avoid_: status

**Transition**:
Which state a person reaches when they act on a node in another state:
from a fixture or phase, on one of a node's events (and, for an event
that calls an operation, one of its outcomes), to a fixture or phase. For
the gallery only; never handed off. An event that navigates or goes back
leaves the screen and is no transition.
_Avoid_: flow, navigation, edge

**Condition**:
A named yes/no decision the screen makes, such as `hasDates`, declared once and
used by name; or one declared on a type (`Order.cancellable`) over its own
fields, used through a value of it (`item.cancellable`), as the domain shares
one.
_Avoid_: flag, predicate

**Domain**:
The types, conditions and operations a project's screens and components
share, each declared once and used by name; a screen may add its own, but
never one of the same name.
_Avoid_: model, schema

**Operation**:
A query (it only reads) or a command (it changes something) behind a screen:
named, with parameters, a result type, an optional precondition (a
condition) and named failures; a node calls it with `call`. A query names
the `data` fields its result fills (`"from"`); a parameter with a fixed set
of choices is an enum the view model offers too. A **destructive** command
cannot be undone: the screen calls it only from its confirmation, and its
CLI command only given `--yes`. A **long** command answers with a job.
_Avoid_: endpoint, API call, action

**Bulk form**:
The operation a command marked `"bulk"` derives, to act on many items at
once: its key parameter (the first) made a list of ids, its precondition
and failures held item by item, and its result one success or named
failure per item. The screen calls it with the selection; its CLI command
takes several ids.
_Avoid_: batch operation, multi-action

**Selection**:
The items a person has picked to act on together: view state, null while
none is picked, so a node that acts on it is gated on it being there.
_Avoid_: checked items, multi-select state
**Job**:
What a long command answers with at once while its work runs on: an id,
its progress, and whether it is running, done or failed, with the
command's result once done or the failure it names once failed. Its type
and the query that reads it are derived from the command, never written.
The screen shows it as view-model data, in fixtures for each status; the
CLI waits for it, or with `--no-wait` hands it back to read again.
_Avoid_: task, background operation, status

**Paged query**:
A query that answers one page at a time (`"pages": "cursor"`): its result
holds the page's rows and `next`, where the next page starts, null on the
last page. Every call gives the cursor: null for the first page, or the
`next` the last page answered with ("load more").
_Avoid_: pagination, infinite scroll

**Confirmation**:
The popup that asks before a destructive operation: an overlay with a
backdrop, opened by its id from the node a person first acts on, whose
button is the only node that calls the operation. Its CLI form is `--yes`;
without it the command prints a dry run.
_Avoid_: alert, prompt

**UI-only action**:
What a screen does by itself on an event, touching no domain data:
`navigate`, `back`, `open`, `close`, `scrollTo`, `share`, `copy`, and
`set` or `toggle` on view state. Anything else is a call to an operation.
_Avoid_: free action, local action

**Template**:
A string with placeholders that show data.

**Cases**:
An ordered choice of one value, ending in an `else`.

### Outputs

**Wireframe**:
The screen drawn in grays (boxes, text and crossed-out images) so the
discussion is about structure, not styling.
_Avoid_: mockup, screenshot

**Gallery**:
Every screen description under one directory, arranged as its directories
are, where a person picks one, to see its **overview**, and then one of its
states, to see its wireframe drawn from the description as it now stands,
and can act on its nodes to click through its transitions and navigates as
a prototype; or picks a directory, to see the **flow** between the screens
under it.
_Avoid_: mockup viewer, preview

**Overview**:
Every state of one description, small, in the order the tree lists them,
and each transition it declares an arrow from its `from` state to its `to`,
labelled with the label of the node acted on and, for a call, the outcome.
_Avoid_: state machine, storyboard

**Flow**:
The screens under one directory as boxes, and each `navigate` between them
an arrow labelled with its node, drawn from the links `check <root>`
reports; one to a screen not drawn yet ends at a dashed "?".
_Avoid_: site map, navigation graph

**Journey**:
One named path a person takes across screens, told as steps: each a
state of one screen, and between two steps the node acted on, its event
and, for a call, the outcome. A branch is another journey. It must be
walkable: each act is shown in its step's state and leads to the next
step, by a navigate or a transition, and a navigate gives the next
step's fixture exactly the params it holds. For the gallery only; never
handed off.
_Avoid_: flow, user story, scenario

**Label**:
The number tagged in a wireframe on each node a person can act on, in document
order, naming that node in conversation.

**Platform mapping**:
How one platform realises each component, action and token.

**Handoff**:
What one platform is given to build a screen: the generated view model, how
each node is built there, and the expected states.

**Expected state**:
For one fixture or phase at one breakpoint, which nodes are present, what
each shows, and which operation each calls when acted on; what a platform's
tests assert.
_Avoid_: snapshot

**Golden output**:
What a CLI derived from the operations must print for one command, and the
code it exits with: in one fixture, a query's result projected from that
fixture's data, or a command's refusal where its precondition does not
hold, or a destructive command's dry run without `--yes`, or a bulk
form's result for each item; or, for a named failure, its error (a bulk
form's: one item failing, the others done).
_Avoid_: expected output, snapshot

**Capture** — the elements of a running UI in one state, read by test id:
where each is, what it says, whether it can be used; and, for each node the
test acted on, the operations the app called. A CLI's capture is one command
run in one fixture: the command line, its exit code and what it printed.
