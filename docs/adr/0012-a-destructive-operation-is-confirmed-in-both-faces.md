# A destructive operation is confirmed in both faces

Deleting a document was a menu item that called `deleteDoc` directly: one
stray tap, and a document was gone for the whole team. Nothing in the
description said the operation could not be undone, so nothing could hold
the screen, or the CLI derived from it
([ADR 0011](0011-a-cli-is-derived-from-the-operations.md)), to asking
first.

The decisions:

- **`"destructive": true` on the operation**, said once, beside
  `requires` and `fails`. Only a command can be destructive: a query only
  reads.
- **In the screen, only its confirmation calls it.** A confirmation is a
  popup as POPUPS already describes it — an overlay, opened by a free
  `{"open": "<id>"}` action, shown under a condition a fixture makes hold —
  whose kind's look has `"backdrop": true`, so nothing behind it can be
  acted on while it asks. `check` refuses a call to a destructive
  operation from any node outside one. The node a person first acts on
  opens it; the button inside calls the operation.
- **In the CLI, `--yes`.** A destructive command does nothing without it:
  it exits 1, as a refusal before trying, and prints
  `{"error": "needs --yes", "dryRun": {"operation", "does", "with"}}`, so
  a script or an agent sees what it was about to do. Its precondition is
  checked first: a dry run never promises what would be refused.
- **Both answers are golden outputs.** Where a fixture's node calls the
  command and its precondition holds, the run without `--yes` answers with
  the dry run and the run with it as any command does; where the
  precondition does not hold, both are refused. `compare` matches a
  capture to the one its command line asks for.

## Considered options

- **A confirmation kind in underdraw.** underdraw has no dialog kind
  ([ADR 0007](0007-kinds-are-data.md)); a backdrop in the project's look
  already says "modal", and check reads it.
- **Any overlay as the confirmation.** The row's ⋯ menu is an overlay too,
  and calling delete from it is exactly what this rules out; a popup with
  no backdrop does not stop the screen to ask.
- **Exit 0 for a dry run.** Then a script that forgot `--yes` would believe
  the document deleted. Exit 1 already means "not tried".

## Consequences

- A description with a destructive operation needs a fixture that opens
  its confirmation, as conditions must vary across fixtures.
- In the documents draft the precondition's refusal now has no golden
  output: the confirmation opens only where `canDelete` holds.
- `handoff` lists `"destructive": true` on the operation, for the platform
  to build the same confirmation.
