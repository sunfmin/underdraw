# A bulk operation is derived from the single one

Deleting several documents meant opening each row's ⋯ menu and confirming
each delete in turn. A screen that acts on a multi-selection needs a
command that takes many items, and the CLI derived from the operations
([ADR 0011](0011-a-cli-is-derived-from-the-operations.md)) needs one that
takes many ids. Writing that command by hand beside the single one would
say its parameters, precondition, failures and destructiveness twice, and
let them drift.

The decisions:

- **`"bulk": "<name>"` on a command derives its bulk form.** The
  operation names it (`deleteDoc` → `"bulk": "deleteDocs"`), so the screen
  calls it by name like any other ([ADR 0009](0009-operations-live-in-the-screen-and-are-called-by-name.md)),
  and no name is guessed from English plurals. Its key parameter is the
  operation's first, made a list of at least one and named with an `s`
  (`id` → `ids`); every other parameter carries over, the same for every
  item. Its failures and its `destructive` are the operation's. A query
  has none: one that reads many is a query of its own.
- **The precondition holds item by item.** No condition of the screen can
  say of each picked item that it holds, so the bulk form's call is not
  gated by `requires`. The server, and the CLI before calling it, asks it
  of each item; one it does not hold for is refused, and the others are
  still tried.
- **The result is one entry per item:** `{"results": [{"id": "c"},
  {"id": "e", "error": "notFound"}]}`, in the order given, so a person or
  a script knows which were not done, and why.
- **The selection is view state, null while empty.** A call to a bulk form
  gives its list one path to a list in view state at least one long
  (`"string[] 1.."`), inside a nullable field; `check` holds the node to
  a condition testing that field `!= null`, the same guard it already asks
  of any path that may be null. So "gated on a non-empty selection" is
  said with what the format had, not with a new count operator.
- **In the CLI, its own command and exit codes.** The bulk form is
  `delete-docs <ids>…`, its whole name in kebab case, leaving `delete` as
  it was. It exits 0 when every item was done and 2 when any was not;
  1 still means nothing was tried (no `--yes`). Its golden outputs are
  full success in each fixture whose node calls it, and, once for each
  way an item can fail, a partial failure: the first item failing, the
  others done.

## Considered options

- **`"bulk": true`, with the name derived** (`deleteDoc` → `deleteDocs`).
  Plurals are English and irregular; a name written once is no burden.
- **One command for both** (`delete <id>…`). The single and the bulk
  forms answer differently (`{}` against `{"results": …}`), and a script
  would have to guess which it gets.
- **A selection gated by its count** (`selection.count > 0`). Conditions
  have no array length, and nothing would tie a count to the list the
  call is given; a nullable selection with a non-empty list is checked
  with the null guards `check` already has.
- **Exit 1 for a refused item.** Exit 1 says nothing was tried; in a bulk
  run the others were.

## Consequences

- A draft with a bulk form needs fixtures with none, some and all picked,
  as `hasSelection` must vary across fixtures.
- A bulk form's full-success golden output takes the fixture's selection
  as items its precondition holds for: the draft picks only documents the
  person owns.
- `handoff` lists the bulk form beside its operation, with `"bulkOf"`; the
  operation says `"bulk"`.
- A long command ([ADR 0015](0015-a-long-command-answers-with-a-job.md))
  has no bulk form: it answers with one job, and a bulk form would need
  one per item. `check` refuses `"long"` and `"bulk"` together until a
  screen needs both.
