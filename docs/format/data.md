# Data

`data` says what the screen shows; `types` names its shapes. Together they are
the screen's **view model** — what the screen needs, not what an API returns.
Every platform generates its model types from here (`struct`, `data class`,
`type`), and the checker types every expression and fixture against it.

```json
"data":  { "listing": "Listing", "booking": "Booking" },
"types": {
  "Listing": {
    "title":  "string 1..50",
    "rating": { "type": "number? 0..5", "doc": "New listings have no rating" },
    "photos": "image[] 5.."
  },
  "Amenity": { "label": "string 1..30", "unavailable": "bool = false" },
  "Plan":    "enum(monthly, yearly)"
}
```

## Field notation

```
field   = type range? default?
type    = name "[]"? "?"?
range   = number? ".." number?
default = "=" JSON literal
```

| Part | Means | Example |
|---|---|---|
| `name` | a built-in type, or a named type from `types` | `string`, `Listing` |
| `[]` | an array of it | `image[]` |
| `?` | may be null | `date?` |
| `a..b` | value range for numbers, length for strings, count for arrays; either side may be left open | `int 1..16`, `string ..200`, `Highlight[] ..3` |
| `= v` | default when a fixture leaves the field out | `bool = false`, `string = ""` |

A field may also be an object, to say more about it:

- `type` — the notation above.
- `doc` — one line for the person and the implementing agent.
- `derived` — this value is computed from others, by the app or the server.
  The sentence says how; fixtures supply the value. The screen never computes
  it (see [expressions](expressions.md)).

## Built-in types

| Type | Fixture value | Notes |
|---|---|---|
| `string` | `"…"` | |
| `int` | `3` | |
| `number` | `4.92` | shown with `number:N` or `number:N-M` |
| `bool` | `true` | |
| `date` | `"2026-11-12"` | ISO date; shown with `date` / `date:short` |
| `url` | `"https://…"` | |
| `image` | `"photo-1.jpg"` | a reference, not text |
| `icon` | `"wifi"` | a [Material Symbols](https://fonts.google.com/icons) name; `.fill` after it for the solid form (`favorite.fill`). Why: [ADR 0008](../adr/0008-icons-are-material-symbols.md) |
| `money` | `{"amount": 18500, "currency": "JPY"}` | shown with `money` |

## Named types

- An object type maps field names to fields. An enum type is `enum(a, b, c)`.
- Names start with a capital letter. Built-ins are lowercase.
- Every object and every enum is named — no inline shapes. A Swift `struct`,
  a Kotlin `data class` and a TypeScript `type` each need a name, and the
  same name everywhere is what lets the three implementations be compared.

## Nulls and defaults

- `T?` may be null and defaults to null. Nothing else may be null.
- A fixture may leave out a field only if it has a default.
- An array has no default; an empty list is written `[]`.
- There is no separate "optional": absent means the default, and the default of
  a nullable field is null.

## Fixtures

A fixture is one value of `data`. `"$patch": "<fixture>"` starts from another
fixture and deep-merges the rest over it. The checker fills in defaults, then
checks every field: present unless defaulted, no unknown fields, types and
ranges hold. Fixtures carry real sample values, not placeholders like
`"…24 photos"`.

## Phases

Loading and failure are not data. A screen lists them under `phases`, each with
its own node tree that reads no data:

```json
"phases": {
  "loading": { "id": "detail-loading", "component": "Screen", "children": [ … ] },
  "error":   { "id": "detail-error",   "component": "Screen", "children": [ … ] }
}
```

`root` is the screen once `data` has arrived, so inside it every non-nullable
path has a value. Why: [ADR 0003](../adr/0003-loading-and-error-are-phases-not-data.md).
A section that loads on its own later is a nullable field with a condition.
