Every decision is named once, and used by name:

```json
"conditions": {
  "hasTodos": { "when": "summary.total > 0", "label": "Has todos" },
  "allDone":  { "when": "hasTodos && summary.remaining == 0", "label": "All done" },
  "yearly":   { "when": "plan == \"yearly\"", "label": "Yearly plan" }
}
```

Grammar of "when":

```
expr = and ("||" and)*        and = not ("&&" not)*
not  = "!" not | compare      compare = atom (op atom)?
op   = == != < <= > >=        atom = path | condition | literal | ( expr )
literal = number | "string" | true | false | null
```

- A condition is a Bool: no truthiness. Write != null, > 0, == "x".
- == and != compare equal types; null compares only with a nullable value. < <= > >= compare numbers or dates. An enum compares with a member's name as a string.
- A condition may use others by name; no cycles.
- Every condition must be true in one fixture and false in another.

A use site ("if", a bool prop or param, a cases branch's "if") is only terms joined by &&, each !? <condition or Bool path>: "hasTodos", "!canAdd", "item.done", "walks && !picked". It holds when every term does, and a != null a condition in the chain makes reads the value under it, as in a condition's own && chain. || and comparisons are a named condition.

A condition on a type is named <Type>.<name>, and its "when" reads the type's own fields, never the screen's data:

```json
"Order.cancellable": { "when": "status == \"pending\"", "label": "Not shipped yet" }
```

A value of the type reads it as a Bool field, wherever the value is: a use site ("if": "item.cancellable"), a placeholder ({item.cancellable}), or a condition of the screen ("order != null && order.cancellable").

Inside one on a type, another of that type is named short ("cancellable && !hasNote" in Order.quiet); a condition over the screen's data is not named there at all, and one on a type is never named whole elsewhere. It is no field: fixtures never give it, the type has no field of its name, and it is used only through a value of its type (check names one used on a value of another type). A screen that reads it must hold, in its fixtures, one value of the type it is true of and one it is false of; a screen that never reads it is not held to it.

Declare one in domain.json ([domain-json](help:domain-json)) to share it: the domain has no screen's data, so a condition every screen asks of an Order is one on Order. A condition over a screen's data stays that screen's.
