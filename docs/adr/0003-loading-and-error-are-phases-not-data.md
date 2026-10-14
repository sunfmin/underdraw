# Loading and error are phases, not data

The first draft had `status: enum(loading, loaded, error)` beside `listing`. In
the loading fixture there was no listing, so `listing` had to be nullable, and
every `listing.…` on the screen would have needed a `listing != null` guard —
or the description could say "loaded, but no listing", a state that cannot
happen.

We moved loading and error out of `data`. A screen lists `phases`, each with
its own node tree that reads no data; `root` is rendered only once data is
present.

## Considered options

- **A status field and nullable data.** Keeps one tree, but forces guards on
  nearly every path and represents impossible states.
- **Union types (`loading | error | loaded(Listing)`).** All three platforms
  have them (Swift enums with payloads, Kotlin sealed classes, TypeScript
  discriminated unions), but conditions and templates would then need type
  narrowing — a large addition to the expression rules for one common case.

## Consequences

- Nearly every data-driven screen has these phases, so this special case covers
  the common shape without union types.
- Shared elements (the back button) are reused across phase trees with `ref`.
- A section that loads separately is still modelled as a nullable field plus a
  condition. If that becomes common, revisit union types.
