---
name: skillview-cli
description: Inventory and explore agent skills installed on the local machine via the `skillview` CLI. Use when asked to discover what skills exist, list SKILL.md files across host agents (claude/codex/cursor/agents), find duplicates between roots, rank skills by cross-agent usage in session logs, or audit for broken frontmatter. JSON output by default.
---

# skillview-cli

Quick-reference card for driving the [`skillview`](https://crates.io/crates/skillview)
CLI from inside an agent loop. The companion document
[`agents_onboarding.md`](https://github.com/galElmalah/skillview/blob/main/agents_onboarding.md)
has the full agent-facing recipe book — read it once, then come back here
for the cheat sheet.

## Install

```bash
cargo install skillview
```

## Commands by task

| task | command |
|---|---|
| see what's here | `skillview agents` · `skillview roots` · `skillview stats --pretty` |
| find a skill | `skillview list --name <q> --format ids` |
| inspect one | `skillview show <id\|name\|path-substring>` |
| find duplicates | `skillview dups --sort size` |
| rank by usage | `skillview usage --top 20` |
| copy-pasteable recipes | `skillview examples` |
| per-command help | `skillview <cmd> --help` |

## Filter cheatsheet (`list`)

Filters compose with AND semantics:

```
--agent claude|codex|cursor|agents|unknown
--tier  primary|secondary
--root-kind claude-global|claude-project|codex|cursor|agents-generic|unknown
--name <substring>                  # case-insensitive
--has-usage | --min-usage N
--min-tokens N | --max-tokens N
--validation-failed
--dups-only [--dup-kind exact|near]
--sort agent-name|name|agent|tier|usage|tokens|sessions|path
--limit N
--format json|jsonl|tsv|ids|paths|names
```

`dups` accepts: `--exact`, `--near`, `--min-size N`, `--agent`,
`--root-kind`, `--sort {size|similarity|kind}`, `--limit`, `--format`.

`usage` accepts: `--agent`, `--min-mentions N`, `--top N`, `--include-low`,
`--sort {mentions|sessions|recent|name}`, `--format` (no `paths` here).

## Identifying skills across calls

- `name` — what humans say; **not unique** (two `dup-recipe` skills can share a name across agents).
- `id` (`s_<n>`) — stable across runs against the same tree (sorted by path),
  but a new file inserted lexicographically earlier will shift downstream ids.
- `path` — absolute filesystem path. Globally unique. Use this if you need a
  durable handle.

## Speed tips

A full scan of `$HOME` can take minutes. If you don't need clustering or
usage data, always pass:

```bash
skillview --no-similarity --no-usage <subcommand>
```

This drops scan time to sub-second on typical machines.

## Output shape

The default invocation (and `scan`) emits:

```jsonc
{
  "schema_version": 2,
  "generated_at": "2026-05-13T...Z",
  "roots":    [ { "id": "r_0", "kind": "claude-global", "path": "..." } ],
  "skills":   [ /* full skill records w/ frontmatter, tokens, validation, usage */ ],
  "clusters": [ { "id": "c_0", "kind": "exact|near", "similarity": 1.0, "members": [...] } ],
  "stats":    { /* counts + timings */ }
}
```

Schema: <https://github.com/galElmalah/skillview/blob/main/schema/skillview.schema.json>.

## When you're stuck

```bash
skillview --help                  # top-level overview
skillview <cmd> --help            # filters + flags for that subcommand
skillview examples                # curated recipe book (no scan, instant)
```

Everything the CLI can do is reachable from those three commands without
guessing.
