# Operations live in the screen, and are called by name

Superseded in part: operations, types and conditions are shared in the project's domain by [ADR 0020](0020-a-project-shares-one-domain.md).

A screen description declares its **operations** — the queries and
commands behind it — in `screen.json`, beside the view model and conditions
they are written in, and a node calls one with the `call` action. Before
this, "delete this document" and "sort by name" were free action names
(`{"open": "confirm-delete"}`, `{"set": "sort"}`) that only each platform
file explained, so nothing said what a screen asks of the server, what it
must hold before asking, or what can go wrong. The same operations are
what a command-line interface can later be derived from: the screen and
the CLI are two faces of one domain layer.

The decisions:

- **In `screen.json`, under `"operations"`.** An operation's parameters
  are view-model fields (`"id": "string"`), its result is one of the
  screen's `types`, and its precondition is one of its `conditions`. All
  three live in `screen.json`; a second file would refer back into it for
  every name.
- **One screen's own; not shared.** Types and conditions are per screen
  today, and an operation is written in them. Two screens that call the
  same operation each declare it. When a CLI is derived from many screens,
  the operations it gathers by name must agree; a shared file is the
  answer to revisit then, together with shared types.
- **`call` is one action beside the free ones.** What reads or changes
  domain data is an operation, called by name with `"with"` for its
  parameters, and check holds the call to the declaration: the operation
  exists, `with` gives exactly its parameters, each of a fitting type, and
  a call to an operation that `requires` a condition is gated by that
  condition (`disabled` / `if`). What the screen does by itself — navigate,
  toggle, open, set view state — stays a free action that each platform
  file maps, as before. A platform maps `call` once, not each operation.
  ([ADR 0012](0012-what-changes-domain-data-is-a-call.md) closes that
  list: only UI-only actions, with `set` and `toggle` on view state.)
- **View state is marked on the field (`"view": true`)**, and no
  operation's result may reach it. An open menu or a popup's subject is
  the screen's alone; a result that held it would make the server answer
  for the UI. A row that needs view state beside domain data wraps the
  domain type (`DocRow { doc: Doc, menuOpen }`).

## Considered options

- **A separate `operations.json`, shared by every screen of a project.**
  The natural home for a CLI, but it would need the types it names, and
  types are per screen; sharing both is a larger change than this one.
- **Keep free action names, and map each in the platform files.** Nothing
  could check a parameter, a precondition or a result, and the domain
  layer would be written once per platform instead of once.
- **An operation's result as a new inline shape.** Every object is named
  in `types` ([ADR 0002](0002-field-types-are-compact-strings-not-json-schema.md));
  a result is one more use of a named type.

## Consequences

- `handoff` lists the operations, and names on each event the operation
  it calls; the platform's agent implements them in the app's data layer.
- A precondition is a condition over the view model, so it says what the
  screen knows (`rowMenu.canDelete`), not what the server enforces; the
  server's refusal is one of `fails`.
- Gating accepts the same condition, or one whose top-level `&&` chain
  names it, as conditions guard nulls already.
- A fitting type includes the range: a value given to `string 1..20`
  lies inside it wherever the node can call it. A typed `string ..20`
  passes only on a node gated by a condition that narrows it
  (`code != ""`), so the CLI derived from the operations never has to
  accept an argument its own parameter refuses, and it refuses one
  outside the range.
- [ADR 0010](0010-the-view-model-says-what-each-query-fills.md) extends
  this: which `data` field a query fills, who derives a derived field, and
  a parameter's choices as an enum.
