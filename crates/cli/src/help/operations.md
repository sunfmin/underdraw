The queries and commands behind the screen, declared once and called by name. Each is a query (it only reads) or a command (it changes something):

```json
"operations": {
  "listDocs":  { "query": "The team's documents, in the order sort names",
                 "params": { "sort": "SortKey = \"updated\"" },
                 "result": "Doc[] ..50" },
  "deleteDoc": { "command": "Deletes a document for everyone",
                 "params": { "id": "string" }, "requires": "canDelete",
                 "fails": { "notFound": "the document is already gone" } }
}
```

- `"query"` or `"command"`: exactly one: what it does, in one line
- `"params"`: fields as in "data" ([data-and-types](help:data-and-types)); a named type is in "types"
- `"result"`: what it answers with: one of "types", written as a field type ("Doc", "Doc[] ..50", "Doc?"); never a built-in, and holding no view state. A query needs one; a command may have none.
- `"requires"`: a condition ([conditions](help:conditions)) that must hold before it is called: the screen's, or one on a type ("Order.cancellable")
- `"destructive"`: true: a command that cannot be undone (deleting, sending, paying). Only its confirmation may call it (below), and its CLI command does nothing without --yes (`underdraw cli --help`)
- `"bulk"`: the name of its bulk form, which does it to many items at once: "bulk": "deleteDocs" (below). Only a command has one.
- `"long"`: true: a command that takes long (an export, an import). It answers with a job, and what reads the job is derived (below)
- `"fails"`: each way it can fail: a name and when, {"offline": "…"}
- `"pages"`: "cursor", for a query that answers one page at a time ([paging](help:paging))

A query fills the fields of "data" that name it with "from":

```json
"data":  { "docs": { "type": "DocRow[] ..50", "from": "listDocs" } },
"types": { "DocRow": { "doc": "Doc",
                       "menuOpen": { "type": "bool = false", "view": true } } }
```

The field is of the query's result type, or of a type that wraps it: one field of the result's type (here doc), beside only view state and fields the app derives. "[]" and "?" match the result's. A command fills no field. "from": "<query>.<field>" fills it with one field of the result instead, of that field's type or wrapping it: "from": "listDocs.next".

A parameter with a fixed set of choices is an enum in "types", and the view model offers the same enum, so a call passes one of them on:

```
"types": { "SortKey": "enum(updated, created, title, owner)",
           "SortOption": { "id": "SortKey", "label": "string 1..8" } }
"params": { "sort": "SortKey = \"updated\"" }   default: the one the screen selects
```

The default is one of the choices; "with" gives a SortKey path ("{item.id}") or a member as a literal ("title"), and check refuses anything else.

A node calls one as an action:

```json
"on": { "tap": { "call": "deleteDoc", "with": { "id": "{item.doc.id}" } } }
```

"with" gives every parameter but those with a default, and no other. Each value is a template whose type is the parameter's: "{item.doc.id}" for a string; a nullable value only for a nullable parameter; words around a placeholder make a string. A literal works too: "title", 3, true, null. Its range lies inside the parameter's wherever the node can call it: a string's length, a number's value. A value of "string ..20" is not one of "string 1..20", since it may be "". Gate the node by a condition that narrows it, "disabled": "!hasCode" with "hasCode" when "code != \"\""; a condition's && chain narrows a value by != "" (at least one long), == a literal, and <, <=, >, >= a number. check refuses a call whose value may fall outside.

A node that calls an operation with "requires" is gated by that condition, or one whose && chain names it: "disabled": "!canDelete" on the node, or "if": "canDelete" on it or around it (a handler's cases branch counts). check refuses a call that is not. One that requires a condition on a type is gated through a value of that type, the one it acts on: "if": "item.cancellable" on a row that calls cancelOrder with { "id": "{item.id}" }, or "disabled": "!item.cancellable".

A destructive operation is called only from inside its confirmation: a popup ([popups](help:popups)) whose component's look has "backdrop": true, and that a node opens by its id with { "open": "<id>" }. The node a person first acts on opens it; only the button inside the popup calls the operation:

```
the row menu's item:  "on": { "tap": { "open": "confirm-delete",
                                       "with": { "id": "{item.doc.id}" } } }
the screen's overlay: { "id": "confirm-delete", "component": "Dialog",
                        "if": "confirmingDelete", "anchor": "center",
                        "children": [ …the question…,
  { "id": "confirm-delete-cancel", "component": "Button", "text": "取消",
    "on": { "tap": { "close": "confirm-delete" } } },
  { "id": "confirm-delete-confirm", "component": "Button", "text": "删除",
    "on": { "tap": { "call": "deleteDoc", "with": { "id": "{rowMenu.docId}" } } } } ] }
```

with "confirmingDelete": "canDelete && rowMenu.confirming" in conditions, so the popup's "if" gates the call too, and a fixture that opens it. check refuses a call to a destructive operation anywhere else, a menu without a backdrop included.

A command marked "bulk" derives a second operation, its bulk form, to act on many items the person has picked (a multi-selection):

```json
"deleteDoc": { "command": "Deletes a document for everyone",
               "params": { "id": "string" }, "requires": "canDelete",
               "destructive": true, "bulk": "deleteDocs",
               "fails": { "notFound": "the document is already gone" } }
```

derives deleteDocs, written nowhere and called by that name:

- `params`: the first parameter (the key: each item is one value of it) made a list of at least one, named with an s: "ids", string[] 1..; every other parameter as it is, the same for every item. The key has no default and is no list.
- `result`: one entry per item, in the order given: the item's key, and "error" for one not done (`underdraw cli --help`)
- `requires`: held item by item: an item it does not hold for is refused, and the others are still tried
- `fails`: each item's: one failing leaves the others to be done
- `destructive`: as the operation's: a bulk delete is confirmed too

The screen holds the selection as view state, null while nothing is picked, its list at least one long:

```json
"data":  { "selection": { "type": "Selection?", "view": true } },
"types": { "Selection": { "ids": "string[] 1..50",
                          "count": { "type": "int 1..",
                                     "derived": { "app": "ids' length" } },
                          "confirming": "bool = false" } },
"conditions": { "hasSelection": { "when": "selection != null", … },
                "confirmingDeleteSelected":
                  { "when": "hasSelection && selection.confirming", … } }
```

A row's checkbox toggles a view-state field of its row ("toggle": "item.selected"), and the app keeps selection in step. The node that acts on the selection is gated on it, here opening the confirmation:

```json
{ "id": "delete-selected", "component": "Button", "text": "删除所选",
  "disabled": "!hasSelection",
  "on": { "tap": { "open": "confirm-delete-selected" } } }
```

and the confirmation's button, under "if": "confirmingDeleteSelected", calls the bulk form with the selection:

```json
"on": { "tap": { "call": "deleteDocs",
                 "with": { "ids": "{selection.ids}" } } }
```

check refuses a call to a bulk form that is not given the selection (one path to a list in view state), or that is not gated on a non-empty one: its list may be empty, or the selection may be null where the node is (no "if" or "disabled" whose condition tests it != null). The bulk form's "requires" does not gate the node: no condition can say it of each item. Give the screen fixtures with none, some and all of the items picked. A long command has no bulk form: it answers with one job, not one per item.

A long command ("long": true) answers with a job at once, and the job runs on. Two things are derived from it, so neither is written by hand (check refuses a type or an operation of either name):

```
the job's type, its name and Job:  "ExportDocsJob": {
    "id": "string", "status": "JobStatus", "progress": "int 0..100",
    "result": "Archive?",   what the command answers with, once done
                            (left out when it answers with nothing)
    "error": "string?" }    which of its "fails" it ended in, once failed
  with "JobStatus": "enum(running, done, failed)", one for every job;
the query that reads it, its name and Job:  "exportDocsJob":
  { "query": "Reads the job exportDocs started…", "params": { "id": "string" },
    "result": "ExportDocsJob", "fails": { "notFound": "no job has this id" } }
```

The screen shows the job through the view model, as anything else: a field of "data" holds it, conditions read its status, and fixtures show it running, done and failed (check requires all three):

```
"operations": { "exportDocs": { "command": "Exports every document…",
                "result": "Archive", "long": true,
                "fails": { "tooLarge": "over 2 GB" } } },
"data":       { …, "job": "ExportDocsJob?" },      null before it is started
"conditions": { "exporting": { "when": "job != null && job.status == \"running\"", … },
                "exported":  { "when": "job != null && job.status == \"done\"
                                        && job.result != null", … },
                "exportFailed": { "when": "job != null && job.status == \"failed\"", … } },
"fixtures":   { …, "running": { "$patch": "default",
                                "job": { "id": "j42", "status": "running", "progress": 40 } },
                "done":   { …, "status": "done", "progress": 100, "result": {…} },
                "failed": { …, "status": "failed", "progress": 65, "error": "tooLarge" } }
```

A node shows the progress with "{job.progress}" under "if": "exporting". A done job holds its result; a failed one names one of the command's "fails". Loading and error stay phases: they come before the view model exists, and a job's running and failing happen inside it.

Operations live in screen.json, one screen's own, and are not shared with other screens.
