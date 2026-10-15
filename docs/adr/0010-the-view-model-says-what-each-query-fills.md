# The view model says what each query fills, who derives what, and a parameter's choices

[ADR 0009](0009-operations-live-in-the-screen-and-are-called-by-name.md)
made an operation's result one of the screen's `types` and kept view state
out of it, but left the view model shaped only as the screen needs it.
Three links between it and the operations were missing, and a CLI derived
from the operations, or golden outputs projected from fixtures, need all
three: which field a query fills, who computes a derived field, and which
values a parameter may take. This extends 0009; it supersedes nothing.

The decisions:

- **A field of `data` names the query that fills it: `"from": "listDocs"`.**
  The link sits on the field, beside its type, not in a list on the
  operation: the field is where a reader looks, and the operation's side
  (`fills` in `handoff`) is derived from it. Only a root of `data` takes
  it; a field without it is the app's or the screen's. `check` holds the
  link: it names a query (not a command), the field is not view state, and
  its type is the query's result or **wraps** it — a type with exactly one
  field of the result's type beside only view state and fields the app
  derives (`DocRow { doc: Doc, menuOpen }`), with the same `[]` and `?`.
  `handoff` lists each query's `fills` as paths (`docs`, or `docs[].doc`
  through a wrapper), which is what projecting a fixture onto a query's
  answer needs.
- **`derived` says who: `{"app": how}` or `{"server": how}`.** One key, so
  the who cannot be forgotten or written without the how; the bare string
  is refused. An app-derived field is computed for the screen
  (`summary.count`, `sort.label`), so no operation's result may reach one,
  as none may reach view state; the app adds it in the wrapper. A
  server-derived field (`canDelete`) is part of what the server answers.
  View state is the screen's own, so the server never derives it.
- **A parameter's choices are an enum in `types`, and its default is the
  field's default: `"sort": "SortKey = \"updated\""`.** The view model that
  offers the choices uses the same enum (`SortOption.id: SortKey`), so the
  choices have one home, the CLI gets an enum with the default the screen
  selects, and the existing type check of `with` is the check that a call
  passes one of them: a path of that enum, or a member written as a
  literal. `check` also holds the default to the members.

## Considered options

- **`fills` on the operation (`"fills": ["docs"]`).** Equivalent, but it
  is a side table keyed by field name: a renamed field leaves it dangling,
  and a reader of `data` cannot see where `docs` comes from.
- **A separate `"by": "app"` key beside a string `derived`.** Two keys that
  must agree, and `by` alone means nothing; the one-key object cannot be
  half written.
- **Taking a parameter's choices from a view-model path
  (`"choices": "sort.options[].id"`).** That names where the screen's
  sample values sit, not what the values are: the choices would exist only
  per fixture, and a CLI could not list them. An enum names them once, and
  both the parameter and the view model use it.

## Consequences

- Every description that wrote `derived` as a string now writes who; the
  drafts say `app` for what the screen alone needs (counts, labels, sums of
  a local cart) and `server` for what only the server knows (totals with
  fees, stock, permissions, counts across pages).
- A field the query does not fill (`summary`, `sort` in the documents
  draft) is the app's to build, from the query's answer and view state.
- The generated model and the JSON Schema say who derives each derived
  field; `handoff`'s parameters show their default (`SortKey = "updated"`).
