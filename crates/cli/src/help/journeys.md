One named, linear path a person takes across screens, told as steps, each a state of one screen; a branch is another journey. One file each, <dir>/journeys/<name>.json, under any directory of the project:

```json
{
  "$comment": "The person opens an order and cancels it before it ships.",
  "steps": [
    { "screen": "Orders", "state": "default", "on": "order-{index}.tap" },
    { "screen": "Order",  "state": "open",    "on": "cancel.tap", "outcome": "ok" },
    { "screen": "Order",  "state": "cancelled" }
  ]
}
```

- `"screen"`: the screen's name, as a navigate names it: "Orders"; or its directory, by its path from the project root (the nearest directory at or above <dir> holding domain.json; with none, <dir> itself): "orders/list". A name no description under the root declares is a finding at the step
- `"state"`: a fixture or a phase of that screen
- `"on"`: on every step but the last: "<node id>.<event>", the node acted on, its id as written, as in [transitions](help:transitions)
- `"outcome"`: for an event that calls an operation: "ok", or one of its "fails"; which transition it takes

It must be walkable: each "on" names a node its step's state shows (at either breakpoint) and an event it handles there, and that act leads to the next step: on the same screen, by a transition from this state on that event (with that outcome, when given) to the next step's state; on another screen, by a "navigate" to the screen the next step names. Data carries across screens: the values that navigate's "with" computes in its step's state are exactly the params the next step's fixture holds ([params](help:params)); a param that differs is named with both values. Steps on one screen are not held to each other. check <root> walks every journey under the root and names a broken one with the step that breaks ("steps.1.on"), so a change to a screen says which journeys it breaks. A journey is for the gallery and check only, as transitions are: never handed off.
