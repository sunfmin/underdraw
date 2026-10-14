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
A view-model field computed from other values by the app or server, never by
the screen.

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

**Condition**:
A named yes/no decision the screen makes, such as `hasDates`, declared once and
used by name.
_Avoid_: flag, predicate

**Operation**:
A query (it only reads) or a command (it changes something) behind a screen:
named, with parameters, a result type, an optional precondition (a
condition) and named failures; a node calls it with `call`.
_Avoid_: endpoint, API call, action

**Template**:
A string with placeholders that show data.

**Cases**:
An ordered choice of one value, ending in an `else`.

### Outputs

**Wireframe**:
The screen drawn in grays (boxes, text and crossed-out images) so the
discussion is about structure, not styling.
_Avoid_: mockup, screenshot

**Label**:
The number tagged in a wireframe on each node a person can act on, in document
order, naming that node in conversation.

**Platform mapping**:
How one platform realises each kind, action and token.

**Handoff**:
What one platform is given to build a screen: the generated view model, how
each node is built there, and the expected states.

**Expected state**:
For one fixture or phase at one breakpoint, which nodes are present and what
each shows; what a platform's tests assert.
_Avoid_: snapshot

**Capture** — the elements of a running UI in one state, read by test id:
where each is, what it says, whether it can be used.
