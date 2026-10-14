# What changes domain data is a call; every other action is UI-only

[ADR 0009](0009-operations-live-in-the-screen-and-are-called-by-name.md)
made `call` the action for what reads or changes domain data, but left
every other action name free: only a platform file said what `toggle`,
`submit` or `reload` meant. So `"toggle": "item.done"` marked a todo done
on the server, `"submit": "PlaceOrder"` placed an order and `"reload"`
fetched the screen again, and none of them reached the operations a CLI
is derived from. Nothing told a write to the server from a menu opening.

The decisions:

- **The actions other than `call` are a fixed list, all UI-only:**
  `navigate`, `back`, `open`, `close`, `scrollTo`, `share`, `copy`, and
  `set` and `toggle` on view state. check refuses any other name. A list
  that check knows is what lets it hold the line; an open set of names
  lets `submit`, `remove` or `loadMore` slip past as UI.
- **`set` and `toggle` write only view state:** a field marked
  `"view": true`, a field inside one (`rowMenu.docId`), or an element of a
  list inside one (a reason in the cancel dialog's subject). A path to
  domain data is a finding that names the two ways out: call a command,
  or mark the field view state if only the screen holds it.
- **`submit`, `reload` and the like are operations.** Sending a form is a
  command; loading again is the screen's query, called from a phase with
  no `with` or with literals, since a phase reads no data.
- **`copy` copies a link to a screen**, named as `navigate` names it, so
  that copying a document's link needs no URL in the view model.

## Considered options

- **Keep names free and flag only `set` and `toggle` on domain data, plus
  `submit` and `reload`.** It would leave every other invented name
  (`remove`, `select`, `sortBy`) free to change data unseen; the drafts
  had six of them.
- **Treat `bind` the same.** A bound field is what the person edits
  (a search box, a note), sent later by a call; it stays as it is.

## Consequences

- What the person picks before a command takes it (dates on a calendar,
  a dish's size, a cancel reason, text being typed) is view state, and
  its field says so. The command's parameters carry it to the server.
- A platform maps `call` once, plus the UI-only actions it uses; it no
  longer maps `reload` or `submit`.
