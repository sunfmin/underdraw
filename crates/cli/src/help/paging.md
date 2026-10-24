A query that answers one page at a time says so, and its result is one page: a type holding exactly one list, the page's rows, with the most a page holds ("..50"), and "next": "string?", where the next page starts, null on the last page.

```json
"operations": { "listDocs": { "query": "The team's documents",
                              "pages": "cursor", "result": "DocPage" } },
"types": { "DocPage": { "docs": "Doc[] ..50", "next": "string?" } },
"data":  { "docs": { "type": "DocRow[]", "from": "listDocs.docs" },
           "next": { "type": "string?", "from": "listDocs.next" } },
"conditions": { "hasMore": { "when": "next != null", "label": "More to load" } }
```

"pages" adds two parameters, never written in "params": "cursor" (string?, where the page starts) and "limit" (int 1..50 = 50: as many rows as the page's list may hold). Every call to the query gives cursor: null for the first page (a sort, a retry), or the field of "data" filled from its "next" for the page after it, offered only while that is not null ("load more", or the end of a list):

```json
{ "id": "load-more", "component": "Button", "if": "hasMore", "text": "Load more",
  "on": { "tap": { "call": "listDocs", "with": { "cursor": "{next}" } } } }
```

Give the fixtures both states: one with "next" set (more to come) and one with it null (the last page), so hasMore holds in one and not another.

check refuses a paged query whose result has no "next", a call that gives no cursor or another value, and a load more not gated by a condition that tests next != null. `underdraw cli` gives the query --cursor and --limit, and its stdout holds "next" (see cli --help).
