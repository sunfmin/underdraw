# Handoff generates the model and states the oracle

`underdraw handoff` gives the agent that builds a screen on one platform what
it needs to build it the same way every time:

- **The view model and conditions as code** in the platform's language
  (TypeScript, Swift, Kotlin), generated from the description: the types,
  and one function per condition.
- **Every node as the platform builds it**: the kind's mapping, the test id
  as written there, and each event with the action's mapping.
- **Every state's expected result**: per fixture and phase, at each
  breakpoint, which ids are present and what text and `disabled` each shows,
  and in which fixtures each condition holds.

## Considered options

- **Prose guidance only** (the description plus the platform mapping, and let
  the agent write the code). Every agent would write its own model types and
  its own reading of each condition, and nothing would show two platforms
  agree. Generated code has one reading, the description's.
- **Generate the views too.** The view code is where each platform's idioms
  and the app's existing components matter; a generator would write code
  nobody keeps. The views are left to the agent, guided by `implement`, and
  held to the description by the expected states instead.
- **Leave the expected states to `compare`.** `compare` reads a running UI;
  the expected states are what a unit or UI test can assert today, with the
  fixtures as test data. They are the same answer `render` draws from, so
  the picture the person agreed to and the test the agent writes cannot
  disagree.

## Consequences

- The conditions' `holds_in` is the generated functions' test: run each
  condition over the fixtures and compare. This was done for Swift and
  TypeScript on the airbnb draft; Kotlin has not yet been compiled.
- A gap in a platform file is a finding, not an error: the rest of the answer
  is still useful while the mapping is filled in.
- A faulty description is refused: nothing generated from it can be trusted.
