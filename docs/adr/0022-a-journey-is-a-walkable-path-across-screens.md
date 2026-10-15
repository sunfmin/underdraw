# A journey is a walkable path across screens, for the gallery only

A **journey** is one named, linear path a person takes across screens:
steps, each a state of one screen, and between two the node acted on, its
event and, for a call, the outcome. Transitions
([ADR 0018](0018-transitions-are-a-top-level-list-for-the-gallery.md))
stop at a screen's edge and a navigate lands on the target's first state,
so nothing could say "checkout fails to pay, and the person retries on
payment", nor which paths a change to a screen breaks.

The decisions:

- **Linear; a branch is another journey.** It tells as a story and plays
  step by step. The graph is already drawn by the flow and the overviews.
- **In `<dir>/journeys/<name>.json`**, listed under that directory in the
  gallery, naming screens by their path from the project's root.
- **`check` walks it.** Each act's node is shown in its step's state and
  leads to the next step: by a transition on the same screen, or by a
  navigate to the next step's screen. A broken journey is named, which is
  how a change to a screen says whom it affects.
- **Data carries across screens.** A screen declares its **params**, each
  a `data` field every fixture gives; a navigate gives each, of its type,
  and in a journey the values it computes in its step's state are the next
  step's fixture's params. Steps within one screen are not held to each
  other: that would mean playing out each operation.
- **For the gallery and check only**, as transitions are: a journey names
  sample data that an app's real data never matches.
