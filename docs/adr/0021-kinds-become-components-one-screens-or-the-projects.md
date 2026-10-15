# Kinds become components: one screen's, or the project's

A node names the **component** it is (`"component": "OrderRow"`). What was
a kind and what was a `ref` to a screen's `components` are one thing, and
the word *kind*, the `kind` field, `kinds.json` and `ref` go. A project
needs to reuse a whole piece of a screen (a header, an order row with its
cancel button, an address sheet) across many screens, with its data and
behaviour, and two words for "what this node is" were one too many.

The decisions:

- **A component's body is one of two, never both.** A `look`
  ([ADR 0007](0007-kinds-are-data.md)): drawn from the look vocabulary,
  with no node inside that can be acted on, and built by a platform as one
  element (a `Chip`). Or `nodes`: a subtree whose nodes are each named by
  the instance's id joined to their own (`order-3-cancel`) and built one
  by one. A frame around nodes is a look component at the subtree's root.
- **Where it lives says who uses it.** One screen's own in its
  `screen.json` `"components"`; shared by the project in
  `<project>/components/<Name>.json`, with fixtures of its params, shown on
  its own in the gallery. Underdraw's built-ins are components too. Tokens
  are found the same way, in the nearest directory above a screen that has
  them.
- **Params in, nothing reached for.** A component declares its `params`
  and reads only them; a node passes them with `"with"`. It holds no view
  state: a menu's open flag is the screen's, passed in, so a state is
  still one sample of the screen's view model. It varies only through
  params and slots; a screen cannot reach inside an instance.
- Components may use components; `check` refuses a cycle.

## Considered options

- **Keep kinds, and add shared node subtrees beside them.** Two names and
  two lookup rules for what a node is, with the line between them argued
  each time.
- **A component reads the screen's data directly.** Each screen using it
  would have to happen to hold the same fields, a coupling `check` could
  only report as a missing path.
- **Screens patch an instance** (hide its second button). The component
  then has no one shape to hand off.

## Consequences

- Every draft, platform file and help text renames `kind` to `component`.
- ADR 0007's look vocabulary stands; only where looks live changes.
