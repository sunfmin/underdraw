What the screen is opened with: a list naming fields of "data".

```json
"params": ["orderId"],
"data":   { "orderId": "string 1..", "order": {"type": "Order", "from": "getOrder"} }
```

Every fixture gives each param itself, or through its "$patch", even one that is nullable or has a default. A param is not view state, not filled by a query ("from") and not derived: what opens the screen gives it. Each "navigate" to the screen gives each param, of its type, in "with", and nothing else; "params": [] says it is opened with nothing, so a navigate to it gives no "with". With no "params", nothing a navigate gives it is checked. `check <root>` holds every navigate to a screen under the root to its params; a navigate to a screen not drawn yet is an assumption, its "with" unchecked.
