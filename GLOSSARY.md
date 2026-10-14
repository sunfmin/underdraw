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

**Token**:
A named design value (`color.primary`, `space.4`) that nodes use instead of raw
values.

**Breakpoint**:
The width class a screen is laid out in (`compact`, `regular`).

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

**Phase**:
A stage before the view model exists, such as loading or error, with its own
node tree that reads no data.
_Avoid_: status

**Condition**:
A named yes/no decision the screen makes, such as `hasDates`, declared once and
used by name.
_Avoid_: flag, predicate

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
