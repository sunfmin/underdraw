# Kinds

Every node has a `kind` (`Button`, `ListItem`, `TodoRow`), and `kinds.json`
says what each kind looks like in a wireframe. underdraw ships the kinds in
[`crates/format/kinds.json`](../../crates/format/kinds.json). A project's own
`kinds.json`, beside `screen.json`, adds kinds and replaces a default by
name. Why: [ADR 0007](../adr/0007-kinds-are-data.md).

```json
{
  "kinds": {
    "TextField": {
      "doc": "A one-line input: fills what its row leaves.",
      "look": {
        "box": "row", "grow": 1, "padding": [10, 12], "stroke": "line", "radius": "radius.sm",
        "children": [ { "cases": [
          { "if": "value && value != \"\"", "then": { "text": "{value}" } },
          { "else": { "text": "{placeholder}", "ink": "muted" } } ] } ]
      }
    }
  }
}
```

`check` refuses a node whose kind no file defines, and every fault in a
project's looks. A kind still waiting for the person's approval carries
`"assumed": "why it is a guess"`. `render` draws it in orange until the line
is removed.

A layout rule (grow, alignment, sizes) belongs in the kind's look, written
once, and never on a node in `screen.json`. A screen describes structure.

## Looks

A look is one of these:

| Look | Makes |
|---|---|
| `{ "box": …, … }` | A box: see below. `box` is `"row"` (items centred across), `"column"`, `"grid"`, or left out (a plain box). |
| `{ "text": …, … }` | Text: see below. |
| `{ "kind": "Glyph", "icon": "{leadingIcon}", … }` | Another kind's look drawn inside this one, with the other keys as its props. It belongs to this node and is not a node of its own. |
| `{ "repeat": 30, "each": look }` | `each` repeated: a count, or a list (`"{shape}"`, `["50%", "35%"]`). Inside `each`, `n` counts from 1, `index` counts from 0, and `item` is the element. At most 1000. |
| `{ "cases": [ { "if": …, "then": look }, { "else": look } ] }` | The first branch whose `if` holds. |
| `"$children"` | The node's children: `leading`, the body, then `trailing` pushed to the far end. `overlay` children float at their `anchor`. |
| a list | Each in turn. |

Any look may carry `"if"`, and it is left out when the condition fails.

### Box keys

| Key | Value |
|---|---|
| `gap`, `padding`, `margin` | A length (a number, or a token such as `space.4`). `padding` and `margin` also take `[vertical, horizontal]` or `[top, right, bottom, left]`, as CSS orders them. |
| `width`, `height`, `size` | A length, or a share of the parent such as `"70%"`. `size` sets both. |
| `aspect` | `"4:3"`, or a number. |
| `grow`, `shrink`, `basis` | How the box takes up spare room in its row or column, or gives it back. |
| `self` | `"start"`: as wide as its content, not its column. |
| `items`, `justify` | Alignment of its children: `items` `"start"` or `"center"`; `justify` `"center"` or `"between"`. |
| `columns`, `rows`, `span` | Grid tracks: a count, or a list such as `["58%", "1fr"]`. `span` makes a cell cover that many columns and rows. |
| `fill`, `stroke`, `radius`, `dashed` | Its outline and fill, in an ink (below). |
| `cross`, `topLine` | A crossed placeholder, or a line along its top, in an ink. |
| `clip`, `overflowX` | `clip`: children are cut to the box, at this corner radius (`false`: never). `overflowX: "clip"`: a row that runs off the edge. |
| `pin` | `{"x": "end", "y": "end", "inset": 12}`: floats over its parent. Each axis is `start`, `center` or `end`. |
| `itemWidth` | Gives every child this width and stops them shrinking (carousel cards). |
| `frame` | `true`: the node's own `padding`, `gap`, `radius`, `border` and `elevation` override the look's. Containers set this, so a description can still pad a `Stack`. |
| `children` | Looks inside it. |

### Text keys

`text` (a template), `font` (a token: `font.body` when not set), `ink`,
`weight`, `maxLines`, `align` (`start`, `center` or `end`), `underline`,
`strike`. It also takes `self` and `pin`.

### Inks

A wireframe is drawn in grays: `text`, `muted`, `line`, `fill`, `strong`
and `paper`. A colour token such as `color.textSecondary` is read as the
nearest of these.

## Reading the node: templates and conditions

A string in a look is a template over the node's props, as in
[expressions](expressions.md). When the whole string is a single
placeholder (`"{columns|or:2}"`), it gives the value itself, not text. A
placeholder may pass the value through filters, applied left to right:

| Filter | Gives |
|---|---|
| `or:v` | `v` when the value is missing (`or:16`, `or:font.body`, `or:4:3`) |
| `count` | the length of a list |
| `first`, `last` | the first or last element of a list |
| `head`, `tail` | the first word, and the rest (`"image 4:3"` → `image`, `4:3`) |
| `slashes` | `2026-11-12` → `2026/11/12` |
| `glyph` | the character an icon is drawn as (below) |
| `atLeast:n`, `atMost:n` | the number held to a bound |

An `if` uses a screen condition's grammar (`&&`, `||`, `!`, `==`, `!=`,
parentheses, `"strings"`, numbers), applied to placeholders written without
braces: `"value|first && value|last"`, `"size == \"small\""`. A prop on its
own holds when it is given and is not false. An empty string counts as
given, so write `value != ""` to test for it.

## Icons

An icon is named as [Material Symbols](https://fonts.google.com/icons) names
it (`arrow_back`, `favorite`), with `.fill` after the name for the solid form
(`favorite.fill`). `render` draws the icon itself, from Material Symbols' own
font: `{icon|glyph}` gives the character it is drawn as, and a name that is
no icon is drawn as `?`. `check` refuses such a name: in a node, in a fixture,
and in a look. Why: [ADR 0008](../adr/0008-icons-are-material-symbols.md).
Any key named `icon` or ending in `Icon` (`leadingIcon`) holds an icon's
name.
