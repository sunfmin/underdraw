# underdraw

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `sunfmin/underdraw`, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Designing a mockup with underdraw

When asked to design a mockup (a screen, a draft) with the built underdraw tool, work only from the tool itself:

- Do not read the source to learn how to use it: no `crates/`, no `docs/`, no ADRs, and no other draft's files. Learn the tool only from its help: `underdraw --help`, `underdraw <command> --help`, and the answers and findings its commands print.
- If the help does not say what you need, stop designing and improve the help first: add or fix it in `crates/cli/src/help.txt` / `crates/cli/src/help/*.txt`, commit that change on its own, rebuild, and then carry on from the new help. A gap in the help is a bug in underdraw, not a reason to read the code.
