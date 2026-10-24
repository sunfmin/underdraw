A node is an object:

- `"id"`: unique in every state: "todo-{index}" inside an item template, "cell-{row}-{index}" inside one inside another ("index" below); check names an id two nodes share. Required on a node with "on", "bind" or "assumed", and on an instance of a component holding one; any other may leave it out and is given one, the same on every run and unchanged by an unrelated node added: the id of the node it is in, its component in kebab case, and the first three words of its text, title, label or icon ("page-text-hello-name" for {"component": "Text", "text": "Hello {name}"} in "page"; four hex digits for text with no ASCII word), "-{index}" after an item's (inside a repeat inside one, the outer's {index} in the id it is in said by the outer's "index" name), "-2", "-3"… after one taken. A component's own nodes are given theirs without its root's. check, render, handoff and the gallery all name it so
- `"component"`: what the node is: a built-in component ([built-in-components](help:built-in-components)) or one of the screen's own ([components](help:components)); check refuses "kind", its old name
- `"if"`: a use site: the node and all inside it show only when it holds
- `"only"`: "compact" or "regular": shown at that breakpoint only
- `"each"`: a list path: repeat "item" once per element
- `"item"`: the node repeated; it sees item (the element) and index (0..)
- `"index"`: a name for this repeat's index, beside "each": "row". A repeat inside one rebinds item and index, so a node there reaches this one's index only by its name:
  ```json
  { "id": "rows", "component": "List", "each": "rows", "index": "row",
    "item": { "id": "row-{index}", "component": "List", "each": "item.cells",
      "item": { "id": "cell-{row}-{index}", "component": "Button", … } } }
  ```
  gives cell-0-0, cell-0-1, cell-1-0…; a transition or a journey names it as written, "cell-{row}-{index}.tap". A word no data root, enclosing repeat, item, index or n takes
- `"limit"`: at most this many elements: 5, or {"compact": 6, "regular": 9}
- `"nest", "fold", "indent"`: beside "each": its rows are a tree ([trees](help:trees))
- `"bind"`: a path (or a list of paths) the person edits; its value is given to the component as "value" ("bind": "composer.text"; a date range: ["booking.checkIn", "booking.checkOut"])
- `"disabled"`: a bool prop of Button, so a use site: "!canAdd" ([expressions](help:expressions))
- `"on"`: actions ([actions](help:actions))
- `"with"`: the params of a screen's component, one value each ([components](help:components))
- `"assumed"`: what you guessed there and what else it could be, in a sentence ([assumed](help:assumed)): drawn orange until the person agrees
- `"platform"`: {"web": {"hidden": true, "$why": "…"}} or {"ios": {"<prop>": v}}: one platform leaves it out, or builds a prop differently
- any other key: a prop of the component, such as "text", "variant", "icon"

Slots hold child nodes (one node or a list; never cases: use "if"):

- `children`: the body, in order
- `leading`: before the body; `trailing`: pushed to the far end (in a Grid or Columns these two do not fill its cells in turn: give the cells as children, in order)
- `action`: in the body after children (EmptyState's button)
- `overlay`: floats over the node; its "anchor" is "bottom-trailing" (default), "top-leading", "top-trailing", "bottom-leading" (16 in from the corner), "center" (the middle of the node: a dialog) or "bottom" (flush along the bottom edge, as wide as the node: a sheet)
- `foot`: stays at the foot of the node while what is above it scrolls, out of its siblings' order: a chat's box to write in, a side panel's buttons. Drawn after everything else, on the bottom edge of a column (Stack, Card): a pane in a Columns is as tall as its row. `compare` treats it as pinned. StickyBar does this for a whole screen:
  ```json
  { "id": "side", "component": "Stack", "children": [ …cards ],
    "foot": { "id": "composer", "component": "Row", … } }
  ```

A component draws its slots only if its look places "$children"; check refuses a node in a slot of one whose look does not (IconButton, Badge).

"each" with no "item" passes the elements to the component as "items" (photos):

```json
{ "id": "photos", "component": "ImageCarousel", "each": "listing.photos" }
```

"item" sees item and index; "children" of the same node are drawn once and see neither.
