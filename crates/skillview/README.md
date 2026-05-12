# skillview

Inventory every agent skill on your machine — `SKILL.md` files (Claude convention)
plus skill-shaped markdown under `.claude/`, `.codex/`, `.cursor/`, `.agents/`,
and project-local copies — classify by host agent, detect near-duplicates
(MinHash + Jaccard), and count cross-agent usage from Claude/Codex session logs.

## Install

```bash
cargo install skillview
```

This installs the `skillview` CLI. The optional OpenTUI frontend (`--tui`)
requires a git checkout — see
[the repo](https://github.com/galElmalah/skillview).

## Use (CLI — JSON by default, agent-friendly)

```bash
skillview                        # full inventory JSON to stdout
skillview --pretty               # pretty-print
skillview list                   # one-line summary per skill
skillview list --agent claude    # filter by agent
skillview list --dups-only       # only skills in a duplicate cluster
skillview show agent-browser     # full record for one skill
skillview dups --exact           # only exact-content duplicate clusters
```

All subcommands accept the same scan flags: `--root <path>`, `--threshold <f>`,
`--no-similarity`, `--no-usage`, `--include-minhash`, `--pretty`.

## Use (TUI — opt-in)

```bash
skillview --tui
```

Requires Bun (https://bun.sh) and the TUI sources from the repo. The binary
locates them via `$SKILLVIEW_TUI_DIR` or by walking up from the executable / CWD.

## Layout

See [DESIGN.md](https://github.com/galElmalah/skillview/blob/main/DESIGN.md)
for the full schema and architecture.
