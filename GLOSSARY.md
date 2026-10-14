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

**Fixture** — one set of sample data. Each fixture puts the screen in one state.

**Condition** — a named yes/no decision the screen makes, such as `hasDates`.
Declared once; used by name.

**Template** — a string with placeholders that show data.

**Cases** — an ordered choice of one value, ending in an `else`.

**Assumption** — something the agent guessed and the person has not confirmed.
