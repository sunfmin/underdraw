# Components drawn as a look

Every node names the `component` it is (`Button`, `ListItem`, `TodoRow`). A
component whose body is a **look** says what it is drawn as in a wireframe,
and declares the `params` its look reads; a node gives them as props.
underdraw ships its components in
[`crates/format/components.json`](../../crates/format/components.json). A
screen's own are in its `screen.json` `"components"`, beside those whose body
is nodes, and replace one of underdraw's by name. One the project shares is
in `components/<Name>.json` at the project root, its file holding what
`"components"` would hold under its name, either body; every screen of the
project uses it as its own, and a screen's own of the same name is a
finding. A shared one carries `fixtures` of its params, and is shown on its
own: `components/<Name>` is the description of one instance of it, its
fixtures the states, which `check` and the gallery take as they take a
screen's directory. A component has a look or nodes, never both. There is no `kinds.json`. Why:
[ADR 0021](../adr/0021-kinds-become-components-one-screens-or-the-projects.md),
and [ADR 0007](../adr/0007-kinds-are-data.md) for the look vocabulary.

```json
{
  "components": {
    "SearchField": {
      "doc": "A one-line search box: fills what its row leaves.",
      "params": ["value", "placeholder"],
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

`check` refuses a node whose component neither the screen nor underdraw
defines, a component with both a look and nodes or neither, and every fault in
a screen's own looks, among them a name a look reads that its `params` leave
out: a placeholder's first step, a name in an `if` or a `cases` branch, and,
for a box with `"frame": true`, `padding`, `gap`, `radius`, `border` and
`elevation`. What a `repeat` binds (`n`, `index`, `item`) is no param. A
component is never a guess: `check` refuses `"assumed"` on one (a node may
still carry one).

Beside `params`, `look` and `doc`, a component may say how a platform treats it, which
`compare` reads: `"scrolls": true` when its content scrolls or pages, so what
is in it may lie outside its frame (`Screen`, `Carousel`), and
`"pinned": true` when it is pinned to the screen, out of its siblings' order
(`StickyBar`).

A layout rule (grow, alignment, sizes) belongs in the component's look, written
once, and never on a node in `screen.json`. A screen describes structure.

## Looks

A look is one of these:

| Look | Makes |
|---|---|
| `{ "box": …, … }` | A box: see below. `box` is `"row"` (items centred across), `"column"`, `"grid"`, or left out (a plain box). |
| `{ "text": …, … }` | Text: see below. |
| `{ "component": "Glyph", "icon": "{leadingIcon}", … }` | Another component's look drawn inside this one, with the other keys as its props. It belongs to this node and is not a node of its own. |
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
| `items`, `justify` | Alignment of its children: `items` `"start"` or `"center"`; `justify` `"center"`, `"between"` or `"end"`. `check` refuses any other word, here and for `box`, `align`, `self`, `overflowX` and a `pin`'s edges. |
| `columns`, `rows`, `span` | Grid tracks: a count, or a list such as `["58%", "1fr"]`. `span` makes a cell cover that many columns and rows. |
| `fill`, `stroke`, `radius`, `dashed` | Its outline and fill, in an ink (below). |
| `cross`, `topLine` | A crossed placeholder, or a line along its top, in an ink. |
| `dividers` | A line between its cells, in an ink (`true`: the line ink), drawn over them, halfway across the gap. |
| `clip`, `overflowX` | `clip`: children are cut to the box, at this corner radius (`false`: never). `overflowX: "clip"`: a row that runs off the edge. |
| `pin` | `{"x": "end", "y": "end", "inset": 12}`: floats over its parent. Each axis is `start`, `center` or `end`. |
| `itemWidth` | Gives every child this width and stops them shrinking (carousel cards). |
| `frame` | `true`: the node's own `padding`, `gap`, `radius`, `border` and `elevation` override the look's. Containers set this, so a description can still pad a `Stack`. |
| `children` | Looks inside it. |

### Text keys

`text` (a template), `font` (a token, `font.body` when not set; or a
number, the size of the body font, as tall as its line: how `Glyph` makes
an icon as big as its square), `ink`,
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
