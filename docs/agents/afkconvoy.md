# afkconvoy

The config afkconvoy reads for this repo: the first yaml block below. Labels follow
`docs/agents/triage-labels.md` — the fleet takes `ready-for-agent` issues and hands
what it cannot finish to `ready-for-human` (both afkconvoy's defaults).

```yaml
base_branch: main
gate_command: ./gate.sh
gate_seconds: 3600
concurrency: 8
```
