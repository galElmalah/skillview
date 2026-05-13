# skillview

Inventory every agent skill on your machine — `SKILL.md` files (Claude convention)
plus skill-shaped markdown under `.claude/`, `.codex/`, `.cursor/`, `.agents/`,
and project-local copies — classify by host agent, detect near-duplicates
(MinHash + Jaccard), and count cross-agent usage from Claude/Codex session logs.

> **Driving this from an agent loop?** Read
> [`agents_onboarding.md`](https://github.com/galElmalah/skillview/blob/main/agents_onboarding.md)
> in the repo first — it's the fastest path from "I have no idea what's here"
> to "I'm calling the right subcommand with the right filters".

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
skillview examples               # curated recipe book (no scan, instant)

# Discovery
skillview agents                 # per-agent rollup
skillview roots                  # scanned roots + skill counts
skillview stats --pretty         # inventory-wide statistics

# Filter and project
skillview list --agent claude --tier primary
skillview list --name browser --sort usage --limit 10
skillview list --root-kind claude-global --format ids
skillview list --validation-failed --format paths
skillview list --has-usage --format tsv

# Duplicates
skillview dups                   # all clusters
skillview dups --exact           # exact-content only
skillview dups --near --min-size 3 --sort size
skillview dups --agent claude --format tsv

# Usage signal (cross-agent session log scan)
skillview usage --top 20
skillview usage --agent claude --sort sessions
skillview usage --min-mentions 5 --format tsv

# Inspect a single skill
skillview show agent-browser
skillview show s_3
skillview show ~/.claude/skills/agent-browser
```

Every subcommand has rich `--help`:

```bash
skillview --help                 # top-level overview + exploration tips
skillview list --help            # filters, sort options, output formats
skillview dups --help
skillview usage --help
skillview agents --help
skillview roots --help
skillview stats --help
```

### Filters (`list`)

| flag | effect |
|---|---|
| `--agent <name>` | only this agent (`claude`, `codex`, `cursor`, `agents-generic`, `unknown`) |
| `--tier primary\|secondary` | filter by skill tier |
| `--root-kind <kind>` | filter by discovered root (`claude-global`, `claude-project`, `codex`, `cursor`, `agents-generic`, `unknown`) |
| `--name <substring>` | case-insensitive substring match on the skill name |
| `--dups-only` | only skills that belong to a duplicate cluster |
| `--dup-kind exact\|near` | restrict `--dups-only` to one cluster kind |
| `--has-usage` | shorthand for `--min-usage 1` |
| `--min-usage N` | only skills with ≥ N mentions in session logs |
| `--min-tokens N` / `--max-tokens N` | bound on description+body token count |
| `--validation-failed` | only skills whose frontmatter validation failed |
| `--sort` | `agent-name` (default), `name`, `agent`, `tier`, `usage`, `tokens`, `sessions`, `path` |
| `--limit N` | truncate after sorting |
| `--format` | `json` (default), `jsonl`, `tsv`, `ids`, `paths`, `names` |

### Filters (`dups`)

| flag | effect |
|---|---|
| `--exact` / `--near` | restrict to one cluster kind |
| `--min-size N` | only clusters with ≥ N members |
| `--agent <name>` | only clusters that include at least one skill from this agent |
| `--root-kind <kind>` | same, by root kind |
| `--sort` | `size` (default), `similarity`, `kind` |
| `--limit N` / `--format` | as above |

### Filters (`usage`)

| flag | effect |
|---|---|
| `--agent <name>` | only this agent |
| `--min-mentions N` | drop skills below the threshold (default 1) |
| `--top N` | keep the top N after sorting (default 25) |
| `--sort` | `mentions` (default), `sessions`, `recent`, `name` |
| `--include-low` | include `confidence=low` skills (names too generic to scan reliably) |
| `--format` | `json`, `jsonl`, `tsv`, `ids`, `names` |

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
