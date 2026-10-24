A hierarchy (a file tree, nested comments, categories) is one list whose elements hold a list of their own type, never a flat list with a depth number. Beside "each", "nest" names that field: each row is followed by its own rows, one level further in, at any depth. "indent" is how far in each level is drawn, written once here (a space token or points; 16 when left out): every row under the tree follows it, whatever component draws it, so no look pads by depth. "fold" names a bool field of the row in view state: while it is true the row's own rows are not drawn. A fixture folds a row by setting it, and lists every row still; a node toggles it.

```json
"types": { "Entry": { "name": "string 1..", "children": "Entry[]",
                      "folded": { "type": "bool = false", "view": true } } },
"fixtures": { "default": { "files": [
    { "name": "src/", "children": [
        { "name": "main.rs", "children": [] },
        { "name": "lib/", "folded": true, "children": [
            { "name": "a.rs", "children": [] } ] } ] },
    { "name": "README.md", "children": [] } ] } },
{ "id": "files", "component": "List", "each": "files",
  "nest": "children", "fold": "folded", "indent": "space.4",
  "item": { "id": "file-{index}", "component": "Row", "children": [
    { "id": "file-{index}-fold", "component": "IconButton", "icon": "chevron_right",
      "on": { "tap": { "toggle": "item.folded" } } },
    { "id": "file-{index}-name", "component": "Text", "text": "{item.name}" } ] } }
```

draws src/, main.rs and lib/ one step in, and README.md; a.rs is not drawn, as lib/ is folded. "index" counts every row in that order, folded away or not (a.rs is file-3, README.md file-4), so a row's id stays the same whatever is folded above it. Rows of different kinds share the tree by one row type: give it a field per kind and "if" each node in "item". "limit" counts only the rows at the top. check names a "nest" that is not a list of the element's own type, a "fold" that is not a bool in view state, and either one, or "indent", without "nest".
