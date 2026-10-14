# Compare checks relations, not pixels

`underdraw compare` checks a running UI against one state of the description
by what is there, what it says, and relative order and nesting — not pixels,
sizes or spacing — because the wireframe's pixels and sizes are guesses,
while order and nesting are what the person agreed to.

It reads a **capture**: the UI's elements by test id, each with its frame
and, when the platform's tool reads them, its text and whether it is enabled,
written in one form for every platform. It checks:

- **what is there**: every node the state shows has an element, once, and
  nothing the state hides does;
- **what it says**: the words, and enabled or disabled, against the
  handoff's expected state;
- **where it is, relatively**: each node below, or right of, the sibling
  before it where the wireframe puts it, and inside the nearest element
  holding it;
- **what it calls**: for each node the test acted on, the operation calls
  the app made, against the ones the expected state says that event makes
  ([ADR 0009](0009-operations-live-in-the-screen-and-are-called-by-name.md)):
  none missing, none extra, each with the same arguments. The test records
  the calls at the app's data layer, where operations are implemented once,
  so the check does not depend on the platform's network or storage.

## Considered options

- **Compare screenshots with the wireframe.** The wireframe is gray boxes
  and the real UI is not, so every pixel differs; a threshold loose enough to
  pass a right UI passes a wrong one.
- **Compare sizes and spacing too.** The wireframe's sizes come from the
  catalog's guesses and a stand-in font, not the platform's components. A
  check against them fails a correct UI.
- **Read each platform's own dump** (XCUITest's tree, a uiautomator XML,
  the DOM). Three parsers in underdraw, each tied to a tool's output format.
  Instead the agent writes the capture from its UI test, where the tool is
  already at hand, and underdraw reads one format.

## Consequences

- A check needs what it reads: without frames, no order or nesting; without
  text, no words. Skipped checks are counted in `checked`, so a capture that
  checks nothing shows it.
- Lazy containers build only what is on screen, so the capture must be made
  with the content built (or assembled from scrolled parts).
- Pinned bars and overlays are left out of order, and nothing is checked for
  nesting inside a node that scrolls: where they sit depends on the scroll
  position.
