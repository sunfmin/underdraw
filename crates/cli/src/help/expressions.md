Every value position has a type, and is read by it, whatever its key is called. A value of the wrong type is a finding at its position, never read as a literal instead:

```
position                                  type   takes
a condition's "when"                      bool   the whole grammar ([conditions](help:conditions))
a node's "if", a cases branch's "if"      bool   a use site: !? name joined by &&
a prop a look component types bool        bool   true, false, a use site
                                                 ("!bar.canBack"), or one
                                                 placeholder ("{item.on}",
                                                 "{hasDates}")
a param a component types bool, in "with" bool   the same
a text prop (text, title, label, …)       text   a template ([templates-and-formats](help:templates-and-formats))
any other typed param or prop             its    a value of its type: one
                                          type   placeholder, a literal, cases
a prop a look component lists, untyped    any    as given: a template, a literal
a look's "if", a look branch's "if"       bool   the grammar over props ([looks](help:looks))
a look's length key (padding, gap, size,  length a number, a token, or one
  margin, width, …)                              computed: "{depth} * space.4 + 12"
a look's inks, icons, words               each   [looks](help:looks) says each key's
```

Only a look's length computes: a length times ints, and lengths added, over whole points, tokens and the look's params typed with both bounds ("depth": "int 0..4"; [looks](help:looks)). Nothing else is arithmetic: a value computed from others (a total, a count of what is left) is a derived field of the data, supplied by the fixtures ([data-and-types](help:data-and-types)). check names *, ×, + and / in a condition, in a placeholder, and in a node's prop or "with" over data ("depth": "{level} + 1"): give the look the operands as props and compute the length there. A tree line drawn by its own look, for one, indents by its depth, not by a cases table of one length per depth: "padding": [8, 12, 8, "{depth} * space.4 + 12"]. (The rows of a list nested as a tree need none of it: the list's "indent" steps them in, [trees](help:trees).)

A bool position given text that is no use site ("can go back", "n > 0") is found at "<id>.<prop>" or "<id>.with.<param>": name the test in conditions and give its name. "disabled" is one such prop, of Button ([built-in-components](help:built-in-components)), read as any other: nothing keys on its name. A prop a component lists untyped is read as given, so "!canSave" there is text, which always holds: check finds it where the look asks that prop only as a yes/no, and the fix is to type the params, {"disabled": "bool"}.
