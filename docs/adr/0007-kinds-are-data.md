# Kinds are data; underdraw only interprets their looks

Superseded in part: kinds and `kinds.json` became components by [ADR 0021](0021-kinds-become-components-one-screens-or-the-projects.md); the look vocabulary stands.

What each kind of node looks like in a wireframe is declared as data in
`kinds.json`: a project's file beside `tokens.json`, over the defaults
underdraw ships in the same form. Underdraw's code only interprets a
**look**, built from a fixed vocabulary — a box, text, `repeat` and
`cases` — because a project's own components (a `TodoRow`, a design
system's `PriceTag`) should not need a release of underdraw, and because a
kind's facts were split between data (`platforms/*.json`) and Rust code
(the catalog).

## Considered options

- **Keep the catalog in code.** Every built-in kind is drawn with care and
  checked by Flux, but the list is closed. A kind that is not in it can only
  be drawn as a box with its name, and adding one means changing underdraw.
- **Plugins or scripts** (WASM, Lua, JavaScript). They can draw anything, but
  the extension runs as code: it is outside Flux, slow to start, and something
  every project must build. A look written as JSON can be checked, diffed and
  written by an agent.
- **Sizes and alignment in `screen.json`** (`width`, `grow`, `align` on a
  node). Quick for one screen, but the rule then lives on every node that
  needs it rather than once on the kind, and `compare` checks no sizes, so
  nothing would hold an implementation to them.
- **Keep complex kinds** (calendar, map, carousel) **as code primitives.**
  Each one turned out to be boxes, text and a repeat, so they are data like
  the rest.

## Consequences

- Layout options (`grow`, `justify`, `pin`, grid tracks) can be written in a
  look and nowhere else. A screen still describes structure only.
- `check` knows every kind: a name in `screen.json` that no kind defines is a
  fault, caught before `render`. A kind is never a guess: `check` refuses
  `assumed` on a kind, so the agent writes the kind's whole look instead.
  (This replaces an earlier rule that let a kind carry `assumed` and be
  drawn orange. A node may still carry `assumed`: an open product decision,
  not a missing kind.)
- The defaults must render the drafts exactly as the code catalog did. The
  layout tests and a pixel comparison of every state are the proof.
- A box with a kind's name still covers a kind with no look, so layout stays
  total, though `check` stops such a screen first.
