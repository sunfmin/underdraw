tokens.json "breakpoint" splits widths into compact and regular; `render --width` picks one. Two tools:

```
"only": "regular"                        keep the node at one breakpoint
"columns": {"compact": ["1fr"], "regular": ["58%", "1fr"]}
                                         any value, one per breakpoint
```

A responsive value names both breakpoints; a value the same at both is written once.
