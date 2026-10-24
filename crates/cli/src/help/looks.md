What a look component ([components](help:components)) is drawn as. A component is never a guess: write its whole look; check refuses "assumed" on one (a node may still carry one).

```json
"components": { "Chip": {
    "doc": "A filter as a pill: filled when on.",
    "params": { "selected": "bool = false", "text": "string" },
    "look": { "box": "row", "gap": 6, "padding": [6, 12], "radius": 999,
              "fill":  { "cases": [ { "if": "selected", "then": "strong" }, { "else": null } ] },
              "stroke": { "cases": [ { "if": "selected", "then": null }, { "else": "line" } ] },
              "children": [ { "text": "{text}", "font": "font.caption",
                              "ink": { "cases": [ { "if": "selected", "then": "paper" },
                                                  { "else": "text" } ] } } ] } } }
```

Beside "params", "look" and "doc", a component may say how a platform treats it, which `compare` reads (each true or false):

- `"scrolls": true`: its content scrolls or pages, so what is in it may lie outside its frame (Screen, Scroll, Carousel)
- `"pinned":  true`: pinned to the screen, out of its siblings' order (StickyBar)

A node then writes { "id": "chip-direct", "component": "Chip", "text": "Direct", "selected": "{filters.direct}" }, or "selected": "{hasDates}", a condition of the screen's, which holds where it does. A look's templates read the node's props (after the node's own templates are filled), not data, and only those its params declare. A prop is a path, a literal or, for a bool, a condition's name in braces or, typed bool, a use site ("!bar.canBack"); typed params hold each to its type ([components](help:components), [expressions](help:expressions)).

A look is one of:

```
{"box": "row"|"column"|"grid" or none, …box keys}    a box
{"text": "…", …text keys}                            text
{"component": "Glyph", "icon": "{leadingIcon}", …props}   another component drawn inside
{"repeat": 3 | "{list}" | ["50%", "35%"], "each": look}
                   repeated; inside: n (from 1), index (from 0), item
{"cases": [{"if": …, "then": look}, {"else": look}]}
"$children"        the node's slots: leading, body, trailing, foot, overlays
[look, …]          each in turn
```

Any look may carry "if"; it is left out when that does not hold.

Box keys:

- `gap padding margin`: a length (number or token); padding and margin also [v, h] or [top, right, bottom, left]
- `width height size`: a length or "70%" of the parent; size sets both
- `aspect`: "4:3" or a number
- `grow shrink basis`: flex sharing in the parent row or column
- `self`: "start": as wide as its content; in a row, centred across
- `items`: "start" | "center"; justify "center" | "between" | "end" check refuses a word a key does not take
- `columns rows span`: grid tracks: a count or ["1fr", "1fr"]; span covers cells a track is "2fr", "40%" or a number of points: ["3fr", "1fr", 48]; check refuses anything else
- `fill stroke radius dashed`: outline and fill in an ink
- `cross topLine`: a crossed placeholder, a line on top, in an ink
- `wireframe`: {"screen": "{screen}", "state": "{state}"}: that state of another description drawn in the box (Wireframe)
- `dividers`: a line between the cells, in an ink (true: line)
- `clip`: cut children at this radius (false: never)
- `overflowX`: "clip": a row that runs off the edge
- `pin`: {"x": "end", "y": "end", "inset": 12}: floats over the parent; each axis start | center | end
- `backdrop`: true, on an overlay: a see-through dark over the node it floats over, and nothing there can be acted on. Only the whole look of a component put in "overlay", or a box with "pin", floats: check refuses it elsewhere
- `itemWidth`: every child this wide, not shrinking
- `frame`: true: the node's padding, gap, radius, border and elevation override the look's (containers)
- `children`: looks inside it

Text keys: text, font (token or a size number), ink, weight, maxLines, align (start|center|end), underline, strike; also self and pin.

Inks: text, muted, line, fill, strong, paper, or a colour token, drawn in the ink it reads as: color.primary strong, color.textSecondary muted, color.border line, a colour near white in tokens.json paper, else text. None is a hue: say a colour in "$comment" ([tokens-json](help:tokens-json)).

A length key (gap, padding or any side of it, margin, size, width, height, basis, radius, itemWidth) may compute its length: a length multiplied by ints, and lengths added, over whole points, tokens and the look's own int params in braces. A tree line its look draws (one shown on its own, apart from a list nested as a tree, which [trees](help:trees) indents) indents by its depth:

```json
"TreeLine": { "params": { "depth": "int 0..4", "title": "string" },
              "look": { "box": "row", "padding": [8, 12, 8, "{depth} * space.4 + 12"],
                        "children": [ { "text": "{title}" } ] } }
{ "id": "line-{index}", "component": "TreeLine", "depth": "{item.depth}",
  "title": "{item.title}" }
```

`*` multiplies (× too) and `+` adds, `*` first; a term of one product holds one length (a token, or else a number of points) and ints. There is no division, no subtraction and no fraction, and a param is read whole: {depth}, no path and no filter. Each param it reads is an int with both bounds, typed in "params" ("int 0..4"), so check proves from them and from tokens.json that the length stays from 0 to {const:LENGTH_MOST} points; it names an operand without bounds, a length times a length, a term of ints alone, a token tokens.json does not name, and a length that can pass either end. render draws it at what it comes to; handoff gives a platform the expression ("lengths"), to compute as written.

Look placeholders take filters, left to right: or:v (when missing), count, first, last, head, tail (first word / the rest), slashes (2026-11-12 → 2026/11/12), glyph (an icon's character), atLeast:n, atMost:n.

```
"columns": "{columns|or:2}"     "repeat": "{items|count|atMost:5}"
```

A look's "if" uses the condition grammar over props written without braces: "value && value != \"\"", "size == \"small\"", "value|first"; check refuses one that is not the grammar, and a filter that is none.

A prop holds when given and not false; "" counts as given.

Any key named icon or ending in Icon holds a Material Symbols name.
