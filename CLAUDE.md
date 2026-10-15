# underdraw

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `sunfmin/underdraw`, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Tests

A test asserts the text, or the numbers, it chose: the few lines or values that show the behaviour it is about. It never compares images or pixels, and never holds a whole output to a golden file. Where the look is the point (a scrollbar shown, a zoom, a mark hidden), assert it from the state that decides it. So adding a draft, or rewording a message, fails only the tests about that thing.

## Designing a mockup with underdraw

When asked to design a mockup (a screen, a draft) with the built underdraw tool, work only from the tool itself:

- Do not read the source to learn how to use it: no `crates/`, no `docs/`, no ADRs, and no other draft's files. Learn the tool only from its help: `underdraw --help`, `underdraw <command> --help`, and the answers and findings its commands print.
- Whenever something in underdraw gets in the way while designing (the CLI, the description format, the help, render, the gallery), file a GitHub issue for it at once, before carrying on: first search the open issues, and comment on one that already covers it instead.
- If the help does not say what you need, stop designing and improve the help first: add or fix it in `crates/cli/src/help.txt` / `crates/cli/src/help/*.txt`, commit that change on its own, rebuild, and then carry on from the new help. A gap in the help is a bug in underdraw, not a reason to read the code.

## Opening the gallery

Whenever you open the gallery for the person (`underdraw gallery <root>`), also start `underdraw notes <root> --wait` in the background (Bash with `run_in_background`) and keep it running: when it answers, act on each open note, answer every one with `--answer`, then start `--wait` again. Stop it only when the person says they are done.

## Merging pull requests

In this repo, merge your own PR without waiting to be asked once you judge it OK and `./gate.sh` passes. This overrides the global rule to only open PRs. All of these must hold:

- `./gate.sh` passes locally. Nothing less counts: not `cargo test` alone, not a gate run before the last commit.
- CI, when there is any, is green: wait on `gh pr checks` with a time limit.
- The PR has no merge conflicts and no unresolved review comments.
- Nothing the PR body lists as unverified could make the change wrong.

Then run `gh pr merge --merge --delete-branch`, switch to the default branch and `git pull`. If any condition is in doubt, leave the PR open and say what is holding it.
