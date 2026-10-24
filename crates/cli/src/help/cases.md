Any property's value can be chosen per state; the first true branch wins, and the last branch must be "else":

```json
"leadingIcon": { "cases": [ { "if": "item.done", "then": "check_circle.fill" },
                            { "else": "circle" } ] }
```

A slot (children, leading, …) is not a property: it holds nodes, never cases. To show one node or another, write both, each with its own "if".
