# Loading and error are phases, not data

A screen lists `phases` (loading, error), each with its own node tree that
reads no data, and `root` is rendered only once data is present. The first
draft had `status: enum(loading, loaded, error)` beside a nullable `listing`,
which forced a `listing != null` guard on nearly every path and could describe
"loaded, but no listing", a state that cannot happen.

## Considered options

- **A status field and nullable data.** Keeps one tree, but forces guards on
  nearly every path and represents impossible states.
- **Union types (`loading | error | loaded(Listing)`).** All three platforms
  have them (Swift enums with payloads, Kotlin sealed classes, TypeScript
  discriminated unions), but conditions and templates would then need type
  narrowing — a large addition to the expression rules for one common case.

## Consequences

- Shared elements (the back button) are reused across phase trees as a component (`ref` until ADR 0021).
- A section that loads separately is still modelled as a nullable field plus a
  condition. If that becomes common, revisit union types.
