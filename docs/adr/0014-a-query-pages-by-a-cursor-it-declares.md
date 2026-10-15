# A query pages by a cursor it declares once

Issue #44 asked which interactions take different forms in a UI and in a
CLI. Paging is one: the screen shows "load more" or loads at the end of a
list, and a CLI takes `--limit` and `--cursor`. Left to convention, the
screen's call and the CLI's flags would each be written by hand, and
nothing would hold them to the same query. So paging is declared on the
query, and both faces derive from it.

The decisions:

- **`"pages": "cursor"` on a query.** One key, valued by the only way of
  paging there is today, so another (an offset) can be added without
  renaming it. A command never pages.
- **Paging adds the parameters `cursor` and `limit`; the description never
  writes them.** `cursor` is `string?`, where the page starts. `limit` is
  `int 1..N = N`, where N is the most the page's list holds (`Doc[] ..50`),
  so a page's size is written once, on its type, and `--limit`'s default
  comes from it.
- **The result is one page, and says where the next starts.** Its type
  holds exactly one list, the rows, and `"next": "string?"`, null on the
  last page. `check` refuses a paged query whose result has no `next`: a
  page that cannot say whether more come leaves "load more" guessing.
- **`from` may name one field of a result: `"from": "listDocs.next"`.**
  The rows are wrapped for the screen (`DocRow`) and the cursor sits
  beside them, so the page cannot land in one field of `data`. Each field
  of the result fills its own, and is checked as a whole result is: of its
  type, or wrapping it. The golden output puts the result back together
  from those fields, when every one of them is filled.
- **Every call to a paged query gives `cursor`.** Null for the first page
  (a sort, a retry), or the field filled from its `next`, and that only
  under a condition that tests it `!= null`. What makes a call "load more"
  is the cursor it passes, so `check` can hold it without a second marker:
  a call that leaves the cursor out could be either, and is refused.

## Considered options

- **Leave paging to convention, in the help.** The UI's call and the CLI's
  flags would drift, and `check` could not tell a "load more" that forgets
  the cursor, which reloads the first page forever.
- **A `"more": true` marker on the call.** It would say what the cursor
  already says, and a call without it would still be ambiguous.
- **Keep the result a list, and add `next` beside it in stdout only.**
  The screen needs `next` too, to pass on; it is part of what the server
  answers, so it is part of the result.

## Consequences

- A paged screen's fixtures show both states: one with `next` set and one
  with it null, as the condition that gates "load more" must vary.
- The view model's list holds every page loaded so far, so its type need
  not repeat the page's most (`DocRow[]`, not `DocRow[] ..50`).
- A call with a cursor appends the page's rows to the list; one with null
  replaces them. The platform's data layer does this; the description says
  only which call is which.
