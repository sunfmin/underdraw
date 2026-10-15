# Expressions

A screen description holds only three kinds of dynamic value, and a look one
more: a length it computes. Nothing else in it computes anything.

| Kind | Where | Example |
|---|---|---|
| **Template** | any string | `"{listing.guests} guests · {booking.total\|money}"` |
| **Condition** | the screen's `conditions` table | `"hasDates": {"when": "booking.checkIn != null && booking.checkOut != null"}` |
| **Cases** | any property value | `{"cases": [{"if": "listing.isSaved", "then": "heart.fill"}, {"else": "heart"}]}` |

| **Computed length** | a look's length key only | `"padding": [8, 12, 8, "{depth} * space.4 + 12"]` |

Why so little: see [ADR 0001](../adr/0001-expressions-name-conditions-not-logic-in-strings.md).

## Paths

A path names a value: dot-separated identifiers.

- Roots are the keys of `data` (`listing`, `booking`); their types are in
  [data.md](data.md). A phase tree (`loading`, `error`) has no roots.
- A node with `each` repeats its `item` template once per element; inside
  `item`, two more roots exist: `item` (the element) and `index` (from 0).
  `children` of that node are drawn once, not per element, and see neither.
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
- No arithmetic (only a look computes, and only a length: see Computed
  lengths). A value computed from others (`booking.nights`,
  `booking.total`) is declared in `data` as derived and supplied by fixtures —
  the screen does not compute business values.

### Conditions on a type

A condition named `<Type>.<name>` is declared on a type ([ADR
0020](../adr/0020-a-project-shares-one-domain.md)): its `when` reads that
type's fields, and a value of the type reads it as a Bool field.

```json
"conditions": {
  "Order.cancellable": { "when": "status == \"pending\"", "label": "Not shipped yet" }
}
```

- It is used through a value of its type: `"if": "item.cancellable"`,
  `{item.cancellable}`, `order != null && order.cancellable`. Used on a value
  of another type, or named whole (`Order.cancellable`) anywhere but
  `requires`, it is a finding.
- Inside one on a type, another of that type is named short (`cancellable`);
  a condition over the screen's data cannot be named there.
- The type has no field of its name, and fixtures never give it: each value
  of the type in a fixture is given it, worked out from its own fields.
- On a screen that reads it through a value (a use site, a template, or a
  condition over the data), it must be true of one value of the type and
  false of another in the fixtures; a screen that never reads it is not
  held to it.
- An operation may `require` one; a node calling it is gated through the value
  it acts on (`"disabled": "!item.cancellable"`).
- The domain shares conditions this way, since it has no screen's data to
  start a path from. A condition over a screen's data stays that screen's.

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

## Computed lengths

A look component's length key (`gap`, `padding` or any side of it, `margin`,
`size`, `width`, `height`, `basis`, `radius`, `itemWidth`) may compute its
length: `length × int` and `length + length`, written `*` (or `×`) and `+`.

```
sum     = product ("+" product)*
product = operand ("*" operand)*
operand = whole number | token | "{" param "}"
```

- A product holds one length (a token, or else a whole number of points) and
  ints (the look's params, and any other whole number). A length times a
  length, and ints alone, are refused.
- No division, no subtraction, no fraction, no floating point.
- Each param is read whole (`{depth}`) and is typed an int with both bounds
  (`"depth": "int 0..4"`). From those bounds and the tokens' values, check
  proves the length stays from 0 to 10,000 points, and names an operand
  without bounds and a length that can pass either end. The layout crate
  holds every step to that range, which Flux proves cannot overflow.
- render draws it at what it comes to; handoff gives each platform the
  expression (`"lengths"`) to compute as written, never a number per fixture.
- A tree line a look draws on its own indents by its depth this way, not by a
  `cases` table of one length per depth. The rows of a list nested as a tree
  are stepped in by the list's `"indent"` instead.

Arithmetic anywhere else (a condition's `when`, a use site, a placeholder, a
node's prop or `with`) is a finding: give the look the operands as props.

## Checks this makes possible

- Every path exists; every format fits its type; every enum literal is a member;
  every nullable value shown is guarded; every `{…}` is a well-formed placeholder.
- No condition tests a non-Bool for truth; every `cases` ends in `else`; every
  use site names a condition or a Bool path.
- **Condition coverage:** every condition is true in some fixture and false in
  another, so every state the screen can be in can be rendered and shown.
- Implementations name the same decisions: each condition becomes one named
  computed property (`var hasDates: Bool`, `val hasDates`, `const hasDates`).
