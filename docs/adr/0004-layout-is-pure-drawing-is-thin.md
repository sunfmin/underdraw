# Layout is pure; drawing is thin

`underdraw render` lays a screen out in pure, Flux-checked code and leaves
only text shaping and pixels to the binary, because layout is where most of
the decisions are — what each kind looks like, how breakpoints change it —
and they should be testable without fonts, a GPU or looking at pictures.

The steps:

1. **Instantiate** (`underdraw-format`): the description in one state (a
   fixture or a phase) at one breakpoint becomes a plain tree, with
   conditions decided, `cases` chosen, templates filled and `each` expanded.
2. **Lay out** (`underdraw-layout`): a catalog says what each kind looks like
   as boxes, taffy places them, and the answer is a list of things to draw in
   a handful of grays, plus where every node went. Text is measured through a
   `Measure` trait; tests use a stand-in that gives every character half its
   font size, so the geometry is exact.
3. **Draw** (the binary): parley implements `Measure` and sets the text in the
   system's fonts, vello draws the list on the GPU (Metal on macOS), and the
   pixels go to a PNG.

## Considered options

- **Lay out in the binary, with the fonts at hand.** Simpler, but the layout
  decisions would be testable only by looking at pictures.
- **Draw with a browser (HTML and a headless Chrome).** Real CSS layout for
  free, but a browser to install and drive, seconds per picture, and a
  rendering no platform's native UI matches anyway. A wireframe needs boxes and
  text, not CSS.
- **A styled mockup in the brand's colours.** It invites discussion of
  colours and polish before the structure is agreed. The wireframe is gray on
  purpose; colour appears only where the agent guessed (orange), so that is
  what the eye goes to.

## Consequences

- Overlays are positioned by `underdraw-layout` itself, not by taffy's
  insets: taffy 0.15 resolves `bottom` against the wrong box when the page's
  height is open (every full-page render). Revisit when taffy fixes it.
- Rendering needs a GPU. `check` does not, and `doctor` says whether one is
  there.
