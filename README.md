# underdraw

A command-line tool for AI agents: describe a screen once, then

- **render** it as a wireframe so a person can look at it and discuss it,
- **mark** what the AI only guessed, so the person can confirm it,
- **hand off** exact guidance for building it on web, iOS and Android,
- **compare** the real UI's element tree against the description,
- **derive a CLI** from the same operations, with golden outputs to test it against.

One platform-neutral description (`screen.json`) is the single source of truth.
Design tokens live in `tokens.json`; per-platform mappings live in `platforms/`;
what each component of a node looks like is a look in its `components` (underdraw's own, plus the
screen's: see [docs/format/kinds.md](docs/format/kinds.md)).
Node ids become each platform's test id (`data-testid`, `accessibilityIdentifier`,
`testTag`), which is what makes `compare` possible.

Rendering runs on the GPU (vello on wgpu; Metal on macOS).

## Status

The description format is drafted; `underdraw check` reads it and names every
fault, and `underdraw render` draws any fixture or phase as a wireframe PNG at
any width. `underdraw handoff` gives one platform its generated view model,
how each node is built there and what every state must show. `underdraw
compare` checks a capture of the running UI against one state. `underdraw
schema` derives the view model's JSON Schema, to validate API data against.
`underdraw cli` derives a command-line interface from the screen's operations,
and `compare` holds a run of it to the golden output of one fixture.

## Build

```sh
cargo build --release          # target/release/underdraw
./gate.sh                      # what must pass before anything lands
```

`gate.sh` runs fmt, clippy, the tests and Flux. The pure crates
(`crates/format`, `crates/layout`, `crates/handoff`, `crates/compare`) must pass Flux with nothing trusted or
ignored; `verify.sh` refuses any checker other than the pinned one. The render
tests need a GPU.

## Usage

`underdraw --help` is the manual.

- `drafts/airbnb-listing/` — first draft of the format, modelled on a listing
  detail page. The content is illustrative, written from public knowledge, and
  not affiliated with Airbnb.

The format: [the screen](docs/format/screen.md), [data](docs/format/data.md),
[expressions](docs/format/expressions.md) and [platform files](docs/format/platforms.md);
the reasons are in [docs/adr](docs/adr/).
`underdraw check drafts/airbnb-listing` checks a draft against it.
`underdraw render drafts/airbnb-listing --width 1120` draws it.
`underdraw handoff drafts/airbnb-listing --platform ios` hands it to iOS.
`underdraw compare drafts/airbnb-listing --platform ios --capture c.json` checks
what was built; `underdraw compare --help` says how to write the capture.

## Open questions

- Worst-case fixtures generated from ranges (longest title, most items) to
  catch overflow.
- Localised copy. (`locale` says which language the text is in; there is one.)
- Sheets are still stubs.
