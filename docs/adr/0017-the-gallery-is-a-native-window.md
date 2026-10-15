# The gallery is a native window, one per directory, redrawn as the files change

`render` writes one PNG of one state, so the person settling a design had
a heap of files under each `renders/` and nothing to see them by: which
screen, which state, and whether the picture is still the description as
it now stands. A gallery shows every screen description under a directory,
arranged as its directories are, and draws the state the person picks.

The decisions:

- **A native window**, drawn with vello and parley as the PNGs are, with
  winit for the window. Its tree and panels are drawn by hand rather than
  by a UI toolkit.
- **The command still answers at once.** `underdraw gallery <root>` starts
  the window in a process of its own and exits, printing one JSON object:
  the descriptions it found, which have faults, and whether the window
  opened. Where there is no display it exits 2 and says so. Only the
  window keeps running; the CLI keeps no state.
- **One window per root.** Running it again brings that window forward
  rather than opening a second, so an agent can run it after every round.
- **Drawn from the files as they now stand.** The window watches the root
  and redraws when a description changes, appears or goes; nothing is
  read from `renders/`.
- **A tree of directories, descriptions and states.** A state, a fixture
  or a phase, is drawn at 390 and 1120 side by side, with its labels. A
  description with faults is listed, marked, and shows its faults instead
  of a picture.
- **A node can be picked in the picture**, the innermost at that point,
  overlays first, to read its id, kind, label, box, what the agent
  assumed there, and the operation it calls.

## Considered options

- **A static HTML page of pre-rendered states.** No process stays behind,
  but every round of edits means rendering every state again, and the
  page is stale the moment a description changes.
- **A local server opened in the browser.** Live, but a port to own and a
  server process for what one window does.
- **A UI toolkit (egui).** A ready tree and scrolling, at the price of a
  second renderer whose wgpu may not be vello's.
- **The gallery written as a screen description.** It would draw itself
  with underdraw, but would push the format to scrolling and expanding
  trees it has no need of otherwise.

## Consequences

- `render --help` no longer says to show the PNG to the person: the
  gallery is what a person looks at, and `render` is for the agent reading
  node boxes.
- The window runs on macOS, Linux and Windows by construction; only macOS
  is checked locally.
