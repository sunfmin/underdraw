"on" maps an event (usually "tap") to actions, each "name": argument. What reads or changes domain data is an operation, called with "call" ([operations](help:operations)): saving, deleting, sending a form, loading again (a retry button), choosing what the server keeps (a delivery option, a filter it applies), marking a todo done. Everything else is UI-only, what the screen does by itself, and is one of these:

```
navigate   go to a screen    "navigate": "Checkout", "with": {"id": "{listing.id}"}
                             "with" gives each of its params ([params](help:params))
back       go back           "back": true
open       show a popup      "open": "cancel-dialog", "with": {"id": "{item.id}"}
close      hide it           "close": "cancel-dialog"
scrollTo   scroll to a node  "scrollTo": "calendar"
share      the system sheet  "share": "{listing.url}"
copy       a screen's link   "copy": "Document", "with": {"id": "{item.doc.id}"}
set        write view state  "set": "item.selected", "to": true
                             "set": {"booking.checkIn": null, "booking.checkOut": null}
toggle     flip view state   "toggle": "sort.menuOpen"
```

set and toggle write only view state: a field marked "view": true, or one inside such a field ([data-and-types](help:data-and-types)), an element of a view-state list included. check refuses any other action name, and set or toggle on domain data: "toggle": "item.done" on a Todo is "call": "toggleTodo". Each platform file says how to do each action it uses, "call" once.

"with" is passed along with the action. A handler may be cases:

```json
"on": { "tap": { "cases": [ { "if": "!hasDates", "then": {"scrollTo": "calendar"} },
                            { "else": {"navigate": "Checkout"} } ] } }
```

Event names, unlike action names, are free: "tap", or "change" on a node with "bind", raised once the person has edited the bound value. Its handler reads the new value through the bound path, so an edit that changes domain data calls the operation that saves it:

```json
{ "id": "qty-{index}", "component": "Stepper", "bind": "item.quantity",
  "on": { "change": { "call": "setQuantity",
                      "with": { "id": "{item.id}", "quantity": "{item.quantity}" } } } }
```

A node with "on" or "bind" is one a person can act on.
