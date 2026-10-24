"components" holds the screen's own components, each by its name. A component's body is one of two, never both; check refuses a component with both, or with neither:

- a look: drawn as one element, and built by a platform as one: `"Chip": { "params": { "selected": "bool", "text": "string" }, "look": … }` ([looks](help:looks)). A node is one by its name, and gives it each param as a prop: { "id": "chip-direct", "component": "Chip", "text": "Direct", "selected": "{filters.direct}" }.
- nodes: a subtree written once and used more than once, with the params it reads, given in "with" (below).

Either body declares its params one way: typed, as fields of data ([data-and-types](help:data-and-types)), { "selected": "bool", "count": "int = 0" }. A look's may instead be a list of the names, ["selected", "text"], each then taking any value. A typed prop is held to its type, as a param in "with" is: check refuses one missing (unless it has a default or is nullable) and one ill-typed, at "<id>.<prop>". A bool prop, typed or listed, may be given a condition of the screen's, "{hasDates}", and holds where it does, in render, the gallery and handoff; a typed one also takes a use site, "!bar.canBack" or "a && !b" ([expressions](help:expressions)).

A look component declares in "params" every name its look reads, and its look reads nothing else: the first step of each placeholder ("{text}", "{label.short}" reads label), each name in an "if" or a branch of "cases", and, for a box with "frame": true, padding, gap, radius, border and elevation. What a repeat binds (n, index, item) is no param. check names each name a look reads that its params leave out, at "components.<Name>.look", and refuses "params" that is neither a list of names nor an object of types.

underdraw's own components ([built-in-components](help:built-in-components)) are look components of the same form; a screen's own of the same name replaces one whole.

There is no kinds.json: check refuses one left beside screen.json. Move each of its components into "components", with the params its look reads, and delete the file.

A component whose body is nodes is a piece of the screen used more than once (a nav bar, an order row with its cancel button), named with a capital and not as one of underdraw's own:

```json
"components": { "OrderRow": {
  "params": { "order": "Order", "menuOpen": "bool" },
  "nodes": { "component": "Card", "on": { "tap": { "navigate": "OrderDetail", "with": { "id": "{order.id}" } } },
             "children": [
    { "id": "title",  "component": "Text", "text": "#{order.number}" },
    { "id": "cancel", "component": "IconButton", "icon": "cancel", "if": "order.cancellable",
      "on": { "tap": { "open": "cancel-dialog" } } },
    { "id": "more",   "component": "IconButton", "icon": "more_vert", "on": { "tap": { "toggle": "menuOpen" } } },
    { "slot": "children" } ] } } }
```

and used where a node goes, phases included, by its name:

```json
{ "id": "order-{index}", "component": "OrderRow",
  "with": { "order": "{item}", "menuOpen": "{rowMenuOpen}" },
  "children": { "id": "order-{index}-note", "component": "Text", "text": "{item.note}" } }
```

- params: what it reads, typed as fields of data ([data-and-types](help:data-and-types)). It reads nothing else: a path that reaches past them, or a condition of the screen's, is refused. Each is given in "with", of its type, unless it has a default ("bool = false") or is nullable; check refuses one missing, one ill-typed and one it does not declare.
- "with" gives each a template read where the instance is: "{item}" for a value, "#{item.number}" for text, a literal; a bool param may be given a condition of the screen's, "{hasDates}", or a use site, "!hasDates" ([expressions](help:expressions)), and holds where it does.
- nodes: one node, the root of its subtree. The root has no id: it is named by the instance's. Every node inside is named by the instance's id joined to its own: "cancel" in "order-3" is "order-3-cancel", and in states, handoff, captures, transitions and notes it is "order-{index}-cancel" as written. An id inside need only be unique in the component.
- It holds no view state: check refuses a param marked "view". A flag such as a menu's open state is the screen's, in data marked "view": true, passed in whole ("{rowMenuOpen}"); a "set" or "toggle" on the param acts on what was passed, and check refuses it where a screen passes domain data. Shown on its own, a shared component holds its params itself, so its set and toggle on them are checked as view state's.
- It varies only through its params and slots: the instance holds only id, component, with, if, only, assumed and the slots it fills; any other key reaches inside, and is refused. { "slot": "children" } in its nodes stands for what an instance gives in that slot (children, leading, trailing, action, overlay or foot): the screen's own nodes, read where the instance is, under their own ids. check refuses a slot the component places nowhere.
- A component may use another; check refuses one that uses itself, at any depth: a node of OrderRow that is an OrderRow, or a look that nests the component it draws.
- check refuses "ref", the old way to repeat a node: use the component.

A component used by many screens is the project's: written once in components/<Name>.json at the project root (the directory holding domain.json; [files](help:files)), its file holding what "components" would hold under its name, either body:

```
components/OrderRow.json     { "params": { "order": "Order" }, "nodes": { … } }
components/OrderStatus.json  { "params": ["status"], "look": { … } }
```

Every screen of the project uses it by name as if it declared it in its own "components": the domain's types type its params, and its nodes call the domain's operations, so the same OrderRow, cancel button and all, is on the order list and on one order's page, and an edit to the file changes both. Shared components may use each other and underdraw's; one that uses a screen's own is defined only on the screens declaring it, so share that one too.

A screen may add its own beside them, never one of a name the project shares: check names the screen's at "components.<Name>", says it shadows the project's, and uses the project's in its place. A finding inside a shared component is at "components/<Name>.json …" and fixed in the file; a node inside it is named as any instance's (summary-cancel), and the gallery's ⌘C names it "at /nodes/… in components/<Name>.json".

A shared component is shown on its own, so it can be discussed without a screen: its file carries "fixtures", each a value of its params, as a screen's carry values of its data ($patch included):

```
components/OrderRow.json     "fixtures": { "pending": { "order": { "id": "A12", … } },
                                           "shipped": { "$patch": "pending", "order": { "status": "shipped" } } }
components/OrderStatus.json  "fixtures": { "pending": { "status": "pending" } }
```

Each fixture gives every param, of its type, and nothing else: a look's params, listed, take text, a number or true or false, the type the first fixture to give one gives. The description that shows it is components/<Name> (no file to write: the directory holds only its notes.json): `underdraw check <root>/components/OrderRow` checks the fixtures, a finding at "fixtures.<name>…", and `check <root>` and the gallery list it among the descriptions. It is one instance, named as the component is in kebab case (order-row, so its nodes are order-row-cancel), drawn in each fixture; it declares no screen, so no navigate reaches it. One that declares no params has one state to show and needs no "fixtures": without them it is drawn in one, named default. One with params and no fixtures is a finding there, never on a screen using it; a screen's own component takes none.
