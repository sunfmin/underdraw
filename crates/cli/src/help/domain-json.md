The domain a project's screens share, at the project root: the nearest directory at or above a screen holding it.

```json
{
  "types":      { "Order": { "id": "string", "total": "money" } },
  "conditions": { "Order.cancellable": { "when": "status == \"pending\"", … } },
  "operations": { "cancelOrder": { "command": "Cancels an order",
                                   "params": { "id": "string" },
                                   "requires": "Order.cancellable",
                                   "fails": { "notFound": "…" } } },
  "samples":    { "twoJourneys": [ { "name": "Buy", … }, { "name": "Cancel", … } ] }
}
```

"samples" names values fixtures start from with "$sample" ([fixtures](help:fixtures)); each is any JSON value, untyped here, checked as the field it fills.

Each key is written as in screen.json ([data-and-types](help:data-and-types), [conditions](help:conditions), [operations](help:operations)), and nothing else goes there. Every command reads each screen of the project as if it declared the domain's itself: a screen uses "Order" in its "data" and calls "cancelOrder" by name, and `handoff`, `cli` and its golden outputs come out as if it had written them. A screen adds its own beside them (one only it uses, such as a type that wraps the domain's with view state, "OrderRow": { "order": "Order", "menuOpen": … }), but never one of a name the domain declares: check names the screen's at "types.Order" (or "conditions.…", "operations.…"), says it shadows the domain's, and reads the domain's in its place. A finding about the domain's file itself is at "domain.json …"; one inside a declaration it holds is at its dotted path, as if the screen held it, and is fixed in domain.json.

A type several screens use goes here once, even one only the screens hold: a row type with view state ("OrderRow" above, its "menuOpen" marked "view": true) used by two screens is declared in domain.json, not copied into each. A screen that declares one the domain holds is named as shadowing it: delete the copy, and check, render and the gallery read the screen as before.

A condition the domain shares is declared on a type ([conditions](help:conditions)), as "Order.cancellable", and each screen reads it through its own values of the type. A screen is held only to the ones it reads: one that never reads Order.cancellable may have every order pending, and check lists only those it reads, with the ones their "when" uses. One that is not on a type reads the data of each screen that uses it, so share one only when every such screen holds the fields it names; a condition over one screen's data stays that screen's.
