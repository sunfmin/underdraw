# A CLI is derived from the operations, and tested against golden outputs

[ADR 0009](0009-operations-live-in-the-screen-and-are-called-by-name.md)
called the screen and a CLI two faces of one domain layer, and
[ADR 0010](0010-the-view-model-says-what-each-query-fills.md) gave the
operations what a CLI needs: which field a query fills, and a parameter's
choices. `underdraw cli` derives the CLI from them, so neither face can
drift from the other, and `compare` holds a running CLI to it as it holds a
running UI to the screen.

The decisions:

- **One command per operation, named by its whole name in kebab case**
  (`listDocs` is `list-docs`, `setQuantity` is `set-quantity`), so the
  command says what it acts on. Derived, not written: a `"cli"` name on the
  operation would be a second name to keep in step.
- **A parameter without a default is positional; any other is a flag.**
  An enum parameter takes its choices, and its default carries over, so
  `--sort` defaults to what the screen's dropdown selects.
- **stdout is `{"result": …}`, or `{}`**: one JSON object, as every
  underdraw command prints, with one key for every result alike, whether a
  list, an object or nullable. Its JSON Schema holds the result's type and
  only the types it reaches, so none that holds view state.
- **Exit codes keep underdraw's 0/1/2.** 1 when the precondition does not
  hold: the command is refused before it is tried, and the answer names
  why (`{"error": "requires canDelete"}`). 2 when it was tried and failed
  in a named way (`{"error": "notFound"}`): it could not do its work.
- **Golden outputs come from the fixtures, only where they say it.** A
  query that fills `data` answers, in each fixture, with that fixture's
  data projected through what it fills (`docs[].doc`), given its defaults.
  A command a node calls in a fixture is refused where its precondition
  does not hold there, and answers `{}` where it holds and the command
  answers with nothing. A command whose result the fixture does not hold
  has no golden output, rather than an invented one.
- **`compare` takes a CLI's capture** (`command`, `exit`, `stdout`, and
  optionally the named `failure` the test made happen), reads the command
  line as the derived CLI takes it, defaults filled in, and diffs exit code
  and stdout value by value against the matching golden output.

## Considered options

- **The first word alone** (`listDocs` is `list`), the whole name only when
  two operations share that word. This was the rule at first; on a screen
  with several nouns it lost what a command acts on (`list` for
  `listDeliveryOptions`, `set` for `setQuantity`), and whether a command
  got its whole name hung on an unrelated operation sharing its verb
  (`remove-item` beside `choose`).
- **A noun-verb tree** (`delivery list`, `promo apply`). It would need the
  noun told apart from the verb in a name, which the operation does not
  say.

- **Wrap a list result in a key named for it** (`{"docs": [...]}`). Nicer
  to read, but the name would come from the field a query fills, which a
  command does not have: two rules instead of one.
- **Exit 2 for a refused precondition too.** Then 1 would mean nothing in
  a derived CLI, and a script could not tell "not allowed" from "broke".
- **Golden outputs for every argument** (`list-docs --sort title`). The
  fixtures hold one order of the documents; any other would be invented.

## Consequences

- A CLI's tests need a fake of the server that holds a fixture's data,
  as a UI's need one of the operations.
- A precondition is a condition over the view model, so the CLI must
  answer the same question the screen asks before calling the command.
