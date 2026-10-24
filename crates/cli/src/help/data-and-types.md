"data" names the roots a path may start from; "types" names every shape.

```json
"data":  { "todos": "Todo[] ..200", "summary": "Summary", "plan": "Plan" },
"types": {
  "Todo":    { "id": "string", "title": "string 1..80", "done": "bool = false",
               "due": { "type": "date?", "doc": "No due date when null" } },
  "Summary": { "remaining": { "type": "int 0..", "derived": { "app": "undone todos" } } },
  "Plan":    "enum(monthly, yearly)"
}
```

A field is   type range? default?   or {"type", "doc", "derived", "from", "view"}:

- `type`: a built-in, or a named type; "[]" after it: a list; "?": nullable ("date?", "Photo[]", "Photo[]?")
- range: lo..hi, either side open: a number's value, a string's length, a list's count ("int 1..16", "string ..200", "image[] 5..")
- default: "= <JSON>": what a fixture that leaves the field out holds ("bool = false", "string = \"\""). T? defaults to null. A list has no default: write [].
- `doc`: one line for the person and the implementing agent
- `derived`: who computes it, and how: {"app": "undone todos"} for what the app works out for the screen, {"server": "the person owns it"} for what the server answers with. Fixtures supply the value; the screen never computes: no arithmetic anywhere. No operation's result reaches an app-derived field, and view state is never derived by the server.
- `from`: on a field of "data" only: the query whose result fills it, or "<query>.<field>" for one field of that result ([operations](help:operations)). A field without it is the app's or the screen's.
- `view`: true: view state, held by the screen alone (an open menu, the row a popup is about), not domain data. No operation answers with it, so keep it out of the types operations return: wrap the domain type in a row type that adds it.
  ```json
  "data":  { "docs": "DocRow[]",
             "rowMenu": { "type": "RowMenu?", "view": true } },
  "types": { "DocRow": { "doc": "Doc",
                         "menuOpen": { "type": "bool = false", "view": true } },
             "Doc": { "id": "string", "title": "string 1..60" } }
  ```

Built-ins, with how a fixture writes them:

```
string "…"   int 3   number 4.92   bool true   date "2026-11-12"
url "https://…"   image "photo-1.jpg"   icon "wifi" (a Material Symbols
name; ".fill" for the solid form)   money {"amount": 18500, "currency": "JPY"}
```

Named types start with a capital letter; every object and enum is named (no inline shapes). Only T? may be null.
