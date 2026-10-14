# underdraw

A command-line tool for AI agents: describe a screen once, then

- **render** it as a wireframe so a person can look at it and discuss it,
- **mark** what the AI only guessed, so the person can confirm it,
- **hand off** exact guidance for building it on web, iOS and Android,
- **compare** the real UI's element tree against the description.

One platform-neutral description (`screen.json`) is the single source of truth.
Design tokens live in `tokens.json`; per-platform mappings live in `platforms/`.
Node ids become each platform's test id (`data-testid`, `accessibilityIdentifier`,
`testTag`), which is what makes `compare` possible.

Rendering will run on the GPU (wgpu + vello; Metal on macOS).

## Status

Designing the description format. Nothing is built yet.

- `drafts/airbnb-listing/` — first draft of the format, modelled on a listing
  detail page. The content is illustrative, written from public knowledge, and
  not affiliated with Airbnb.

Expressions are defined in [docs/format/expressions.md](docs/format/expressions.md)
([why](docs/adr/0001-expressions-name-conditions-not-logic-in-strings.md)).
`uv run prototype/check.py drafts/airbnb-listing` checks a draft against it.

## Open questions

- A formal data schema: types, nullability, optional fields and defaults are
  still prose (`"int ≥0"`), so the checker infers types from fixtures.
- A shared icon vocabulary mapped to SF Symbols / Material Symbols / web icons.
- Localised copy.
- Sheets are still stubs.
