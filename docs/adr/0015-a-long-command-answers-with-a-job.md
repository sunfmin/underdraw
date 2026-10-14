# A long command answers with a job, and what reads it is derived

Exporting a team's documents takes minutes. Nothing in the description
could say so: an operation answered with its result or failed, so the
screen had no way to show progress, and the CLI derived from it
([ADR 0011](0011-a-cli-is-derived-from-the-operations.md)) could only
block in silence. Each platform would invent its own job shape and its
own way of asking how far along it is.

The decisions:

- **`"long": true` on the command**, said once, beside `destructive`.
  Only a command can be long: a query only reads, and answers at once.
- **It answers with a job, whose type is derived**: `exportDocs` gives
  `ExportDocsJob { id, status: JobStatus, progress: int 0..100,
  result: <its result>?, error: string? }`, with one
  `JobStatus = enum(running, done, failed)` for every job. `result` is
  there only when the command answers with something.
- **So is the query that reads it**: `exportDocsJob(id) → ExportDocsJob`,
  failing `notFound`. Its CLI command, its whole name in kebab case as
  every command's, is the long command's name and `-job`
  (`export-docs-job <id>`).
- **Derived means not written.** The description is expanded with both
  before anything reads it, so check, the fixtures, handoff and the CLI
  see one description; a type or an operation of a derived name written
  by hand is a finding.
- **The screen shows the job as view-model data.** A field of `data`
  holds it, conditions read its `status`, and `check` requires fixtures
  where it is running, done and failed. A done job holds its result, and
  a failed one names one of the command's `fails`. Loading and error
  stay phases: they come before the view model exists
  ([ADR 0003](0003-loading-and-error-are-phases-not-data.md)), and a
  job's progress happens inside it.
- **The CLI waits by default.** It prints the job's progress on stderr,
  one JSON object a line, and then the result on stdout, or exits 2 with
  the failure the job names. `--no-wait` prints the job instead, exit 0.
- **Golden outputs come from the job each fixture holds.** Waiting: the
  result where it is done, the failure where it failed, none where it is
  still running. `--no-wait` and the `-job` command: the job as it stands.
  A fixture that holds no job has none, rather than an invented one.

## Considered options

- **Write the job type and the poll query by hand.** Every long command
  would repeat the same three fields, and nothing would hold the poll
  query to the command it reads.
- **Running and failed as phases.** A phase reads no data, so the screen
  could not show how far along the job is, or the result beside it.
- **A progress stream in place of polling.** Platforms differ in how they
  stream; an id and a query to read the job work on every one, and the
  CLI's `--no-wait` needs the id anyway.
- **Progress on stdout while waiting.** stdout is one JSON object for every
  command; progress lines there would break every script reading it.

## Consequences

- `handoff` lists `"job": {"type", "query"}` on a long command and
  `"jobOf"` on the derived query, for the platform to start, show and
  poll the job the same way.
- `compare` matches a CLI capture with `--no-wait` or without it, as the
  run was. It reads stdout and the exit code, not the progress on stderr.
