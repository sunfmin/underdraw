# Field types are compact strings, not JSON Schema

The first draft described data in prose (`"int ≥1"`, `"number 0–5, 可为空"`), so
nothing could check a fixture or an expression, and whether a missing field
meant `false` or "unknown" was undefined.

We write each field as a one-line type — `"string 1..50"`, `"number? 0..5"`,
`"bool = false"` — with every object and enum named in `types`. A field becomes
an object only to add `doc` or `derived`.

## Considered options

- **JSON Schema.** Agents know it well, and it can be exported later. But it is
  four to six lines per field, and it can say far more than three platforms'
  model types can (`oneOf`, `patternProperties`, conditional schemas) — every
  such feature is one more thing each implementing agent must translate.
- **TypeScript, Protobuf or TypeSpec files.** Real languages with tooling, but
  a second file in a second language beside `screen.json`, and none of them
  carries the ranges the renderer needs to produce worst-case fixtures.

## Consequences

- One field, one line: the shape of the view model fits on one screen.
- The notation maps one-to-one onto Swift, Kotlin and TypeScript types.
- Ranges give the renderer the extremes to try (longest title, most photos).
- It is our grammar to define, document and parse; JSON Schema can be derived
  from it if a consumer needs one.
