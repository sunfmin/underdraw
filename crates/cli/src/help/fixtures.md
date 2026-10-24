Each is one whole value of "data", by name, in the order they are shown.

```json
"fixtures": {
  "default": { "todos": [{"id": "1", "title": "Milk", "done": true}],
               "summary": {"remaining": 0}, "plan": "monthly" },
  "empty":   { "$patch": "default", "todos": [], "summary": {"remaining": 0} }
}
```

"$patch": "<fixture written above>" starts from it and deep-merges the rest over it: objects merge key by key; lists and scalars replace.

{"$sample": "<name>"}, in place of any value in a fixture (the fixture itself, a field, an item), starts it from that sample in domain.json's "samples" ([domain-json](help:domain-json)), so screens showing the same data (a tree's lines, two journeys) write it once; any other key beside "$sample" merges over it as "$patch" does (only over an object sample):

```json
"journeys": { "$sample": "twoJourneys" },
"first":    { "$sample": "buy", "name": "Buy again" }
```

It reads, checks and renders exactly as the value written out. A sample is checked where a fixture uses it, as that field's type: one of the wrong type is a finding at the field naming the sample ("… : the fixture starts it from sample buy"); a name no sample has is a finding too.

A fixture may leave out only fields with a default; no unknown fields; types and ranges hold. Use real sample values. The first fixture is the default for `render`.
