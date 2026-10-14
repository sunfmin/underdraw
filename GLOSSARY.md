# Glossary

**Screen description** — the platform-neutral account of one screen: its data,
conditions, fixtures and node tree. The single source of truth for that screen.

**Node** — one element of the screen, with a stable **id** that becomes each
platform's test id.

**Kind** — what a node is (`Button`, `TextField`, `List`), named without any
platform's vocabulary.

**Token** — a named design value (`color.primary`, `space.4`). Nodes use
tokens, never raw values.

**Platform mapping** — how one platform realises each kind, action and token.

**View model** — the data a screen shows, as the screen needs it: the screen's
`data` and the named `types` it uses. Not an API response.

**Derived field** — a view-model field computed from other values by the app or
server, never by the screen.

**Fixture** — one sample value of the view model. Each fixture puts the screen
in one state.

**Phase** — a stage before the view model exists, such as loading or error.
Each phase has its own node tree and reads no data.

**Condition** — a named yes/no decision the screen makes, such as `hasDates`.
Declared once; used by name.

**Template** — a string with placeholders that show data.

**Cases** — an ordered choice of one value, ending in an `else`.

**Assumption** — something the agent guessed and the person has not confirmed.

**Breakpoint** — the width class a screen is laid out in (`compact`,
`regular`), chosen from the width by the tokens.

**Wireframe** — the screen drawn in grays: boxes, text and crossed-out images,
so the discussion is about structure, not styling.

**Label** — the number tagged in a wireframe on each node a person can act on,
in document order. The same number names the node in the picture and in what
`render` answers.
