# Expressions

A screen description holds only three kinds of dynamic value. Nothing else in
it computes anything.

| Kind | Where | Example |
|---|---|---|
| **Template** | any string | `"{listing.guests} guests · {booking.total\|money}"` |
| **Condition** | the screen's `conditions` table | `"hasDates": {"when": "booking.checkIn != null && booking.checkOut != null"}` |
| **Cases** | any property value | `{"cases": [{"if": "listing.isSaved", "then": "heart.fill"}, {"else": "heart"}]}` |

Why so little: see [ADR 0001](../adr/0001-expressions-name-conditions-not-logic-in-strings.md).

## Paths

A path names a value: dot-separated identifiers.

- Roots are the keys of `data` (`listing`, `booking`); their types are in
  [data.md](data.md). A phase tree (`loading`, `error`) has no roots.
- Inside `each`, two more roots exist: `item` (the element) and `index` (from 0).
- `each` iterates arrays only. Data that should be iterated is an array.

## Templates

Text with placeholders: `{path}` or `{path|format}`. A placeholder holds a path
and nothing else — no operators, no `?:`, no nesting. `{{` and `}}` are literal
braces.

The receiving property decides what a placeholder yields:

- A **text property** (`text`, `title`, `subtitle`, `label`, `detail`) always
  receives text, so every placeholder in it follows the format rules below.
- Any other property given a string that is exactly one placeholder
  (`"value": "{listing.rating}"`, `"center": "{listing.location}"`) receives the
  value itself with its type. That is how a node gets a number, an image or an
  object.

### Formats

The set is closed. A value whose type needs a format and has none is an error:
the description says how it is shown, so no platform guesses.

| Format | Takes | Shows |
|---|---|---|
| *(none)* | string, int | as is |
| `money` | Money | `¥18,500` per the locale |
| `date` | date | medium date per the locale |
| `date:short` | date | short date per the locale |
| `number:N` | number | exactly N decimals (`number:1`: `5` → `5.0`) |
| `number:N-M` | number | N to M decimals (`number:0-1`: `1` → `1`, `1.5` → `1.5`) |
| `count` | array | its length |

A nullable value — or one reached through a nullable value — may appear in a
template only where an enclosing `if` or `cases` branch tests that path
`!= null`, directly or through a named condition. Only `!= null` in the
condition's top-level `&&` chain counts; a negated condition guarantees nothing.

A text property cannot show a Bool, an enum, an image or an object: choose the
words with `cases`.

## Conditions

Every decision the screen makes has a name, declared once:

```json
"conditions": {
  "hasDates":   { "when": "booking.checkIn != null && booking.checkOut != null", "label": "Dates chosen" },
  "canReserve": { "when": "hasDates && booking.available && booking.total != null", "label": "Can reserve" }
}
```

- `label` is the human name: what a person sees when a mockup is shown per state.
- A condition may use other conditions by name. No cycles.

Grammar of `when`:

```
expr    = and ("||" and)*
and     = not ("&&" not)*
not     = "!" not | compare
compare = atom (("==" | "!=" | "<" | "<=" | ">" | ">=") atom)?
atom    = path | conditionName | literal | "(" expr ")"
literal = number | "string" | true | false | null
```

Types are strict, the same on every platform:

- `when` is a Bool. There is no truthiness: a date, a string or a number is not
  a condition; write `!= null`, `> 0`, `== "monthly"`.
- `==` / `!=` compare equal types; `null` compares with any nullable value.
- `<` `<=` `>` `>=` compare numbers or dates.
- A string literal compared with an enum must be one of its members.
- Comparing a value that cannot be null with `null` is an error: it is constant.
- No arithmetic. A value computed from others (`booking.nights`,
  `booking.total`) is declared in `data` as derived and supplied by fixtures —
  the screen does not compute business values.

## Use sites

Where a node needs a yes/no (`if`, `disabled`, a `cases` branch), it names one:

```
use = "!"? (conditionName | path)
```

A path here must be a Bool. Anything more complex is a named condition.

## Cases

Chooses one value for any property — text, icon, variant, action:

```json
"text": { "cases": [
  { "if": "!hasDates",         "then": "Check availability" },
  { "if": "booking.available", "then": "Reserve" },
  { "else": "Dates unavailable" }
] }
```

- Branches are tried in order; the first true one wins.
- The last branch must be `else`. Every state has a defined value.

## Checks this makes possible

- Every path exists; every format fits its type; every enum literal is a member;
  every nullable value shown is guarded; every `{…}` is a well-formed placeholder.
- No condition tests a non-Bool for truth; every `cases` ends in `else`; every
  use site names a condition or a Bool path.
- **Condition coverage:** every condition is true in some fixture and false in
  another, so every state the screen can be in can be rendered and shown.
- Implementations name the same decisions: each condition becomes one named
  computed property (`var hasDates: Bool`, `val hasDates`, `const hasDates`).
