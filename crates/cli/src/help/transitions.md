Which state a person reaches when they act on a node in another state. They are for the gallery, to lay the states out and click through them; they are not handed off, and need not name every event.

```json
"transitions": [
  { "from": "default",       "on": "order-{index}-cancel.tap",            "to": "cancel-dialog" },
  { "from": "cancel-dialog", "on": "cd-keep.tap",                         "to": "default" },
  { "from": "cancel-ready",  "on": "cd-confirm.tap", "outcome": "ok",      "to": "default" },
  { "from": "cancel-ready",  "on": "cd-confirm.tap", "outcome": "shipped", "to": "cancel-ready" }
]
```

- `"from"`, `"to"`: a state: a fixture or a phase
- `"on"`: "<node id>.<event>": the node's "id" as written (a template inside an item, "order-{index}-cancel"), and an event its "on" handles. The node is shown in "from", at either breakpoint.
- `"outcome"`: given exactly when the event calls an operation: "ok", or one of the operation's "fails"

Opening and closing a popup are transitions like any other. An event that navigates or goes back leaves the screen: it is not a transition. check names a transition whose state, node, event or outcome is not one.
