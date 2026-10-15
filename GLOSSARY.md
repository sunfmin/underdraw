# underdraw

Describing a screen once, platform-neutrally, so a person can discuss it as a
wireframe and each platform can build it the same way.

## Language

### The description

**Screen description**:
The platform-neutral account of one screen: its data, conditions, fixtures and
node tree. The single source of truth for that screen.
_Avoid_: spec

**Node**:
One element of the screen, with a stable **id** that becomes each platform's
test id.
_Avoid_: element, component, view

**Kind**:
What a node is (`Button`, `TextField`, `List`), named without any platform's
vocabulary.
_Avoid_: widget, component type

**Look**:
What a kind is drawn as in a wireframe, written as data in `kinds.json`:
boxes, text, `repeat` and `cases`, each key taking only what layout draws.
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
Something the agent guessed and the person has not confirmed.
_Avoid_: guess, TODO

### Data and decisions

**View model**:
The data a screen shows, shaped as the screen needs it: its `data` and the named
`types` it uses.
_Avoid_: API response, payload

**Derived field**:
A view-model field computed from other values, never by the screen, and
saying by whom: the app (for the screen alone, so never in an operation's
result) or the server (which answers with it).

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
used by name.
_Avoid_: flag, predicate

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
are, where a person picks one, and then one of its states, to see its
wireframe drawn from the description as it now stands.
_Avoid_: mockup viewer, preview

**Label**:
The number tagged in a wireframe on each node a person can act on, in document
order, naming that node in conversation.

**Platform mapping**:
How one platform realises each kind, action and token.

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
