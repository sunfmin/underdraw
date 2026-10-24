```
{
  "screen":     "TodoList",          the name code is generated under
  "title":      "My todos",          what a person calls it
  "locale":     "zh-CN",             optional BCP 47 tag: glyph forms, line breaks
  "tokens":     "tokens.json",
  "params":     ["orderId"],         what it is opened with: fields of data ([params](help:params))
  "data":       {…}, "types": {…},   the view model
  "fixtures":   {…},                 sample view models, one state each
  "conditions": {…},                 every yes/no the screen decides, named
  "operations": {…},                 the queries and commands it calls ([operations](help:operations), [paging](help:paging))
  "components": {"Name": component, …}
                                     the screen's own components, beside the
                                     project's components/ ([components](help:components))
  "root":       node,                the screen once data has arrived
  "phases":     {"loading": node, "error": node}
                                     trees shown before data; they read no data
  "transitions": [{"from", "on", "outcome"?, "to"}, …]
                                     which state acting on a node leads to ([transitions](help:transitions))
}
```

"$comment", "$note", "$why" may be put on anything: read by people only.
