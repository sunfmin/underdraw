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

Rendering runs on the GPU (vello on wgpu; Metal on macOS).

## Status

The description format is drafted; `underdraw check` reads it and names every
fault, and `underdraw render` draws any fixture or phase as a wireframe PNG at
any width. `handoff` and `compare` are not built yet.

## Build

```sh
cargo build --release          # target/release/underdraw
./gate.sh                      # what must pass before anything lands
```

`gate.sh` runs fmt, clippy, the tests and Flux. The pure crates
(`crates/format`, `crates/layout`) must pass Flux with nothing trusted or
ignored; `verify.sh` refuses any checker other than the pinned one. The render
tests need a GPU.

## Usage

`underdraw --help` is the manual.

- `drafts/airbnb-listing/` — first draft of the format, modelled on a listing
  detail page. The content is illustrative, written from public knowledge, and
  not affiliated with Airbnb.

The format: [the screen](docs/format/screen.md), [data](docs/format/data.md) and
[expressions](docs/format/expressions.md);
the reasons are in [docs/adr](docs/adr/).
`underdraw check drafts/airbnb-listing` checks a draft against it.
`underdraw render drafts/airbnb-listing --width 1120` draws it.

## Open questions

- Worst-case fixtures generated from ranges (longest title, most items) to
  catch overflow.
- A shared icon vocabulary mapped to SF Symbols / Material Symbols / web icons.
- Localised copy. (`locale` says which language the text is in; there is one.)
- Sheets are still stubs.
