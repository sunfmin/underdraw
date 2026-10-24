underdraw has no dialog, sheet or menu component. A popup is a state like any other: a nullable field holds what it shows, a condition says it is open, a fixture opens it, and its node floats in the screen's "overlay":

```json
"data":       { …, "cancelling": "CancelRequest?" },
"conditions": { "dialogOpen": { "when": "cancelling != null", "label": "Dialog open" } },
"fixtures":   { …, "cancel-dialog": { "$patch": "default", "cancelling": {…} } },
"root": { "id": "screen", "component": "Screen", "children": [ … ],
          "overlay": { "id": "cancel-dialog", "component": "Dialog", "if": "dialogOpen",
                       "anchor": "center", "children": [ … ] } }
```

Inside it, paths through the nullable field are safe: "if" tests it. The button that opens it says so with a UI-only action, { "open": "cancel-dialog" }; a list of choices inside it is "each" like anywhere else.

"Dialog" is the screen's own component ([components](help:components)): "params": ["width"] and a look such as { "box": "column", "backdrop": true, "width": "{width|or:88%}", "padding": 24, "radius": 16, "fill": "paper", "stroke": "line", "children": ["$children"] }, with "width": {"compact": "88%", "regular": 480} on the node.

- "anchor" places it: "center" for a dialog, "bottom" for a sheet. On a Screen, the middle and the bottom are those of the whole page, which is as tall as its content. A sheet's look leaves out "width".
- The page is only as tall as its content: an overlay taller than that runs off it and is cut. check finds it, in each state at each of the project's widths ([tokens-json](help:tokens-json)); give the fixture that opens it more content behind it.
- "backdrop": true dims the screen behind it; what is behind gets no number, since nothing there can be acted on while it is open.
