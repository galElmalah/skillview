# skillview

Inventory every agent skill on your machine — `SKILL.md` files (Claude convention) plus skill-shaped markdown under `.claude/`, `.codex/`, `.cursor/`, `.agents/`, and project-local copies — classify by host agent, detect near-duplicates (MinHash + Jaccard), and count cross-agent usage from Claude/Codex session logs.

The CLI is the primary surface (JSON-by-default, designed for agents). An OpenTUI frontend is opt-in via `--tui`.

```
┌ skillview · 47 primary · 12 secondary · 3 dup clusters · scanned 184k paths in 612ms ┐
│  skills                            │  agent-browser                                  │
│  ▾ claude  (32)                    │  ~/.claude/skills/agent-browser/SKILL.md        │
│    ▾ [claude/global] ~/.claude (28)│                                                 │
│      ▾ primary (24)                │  frontmatter                                    │
│        • agent-browser   ⧉ exact   │    name:        agent-browser                   │
│        • dogfood                   │    description: Browser automation CLI for AI   │
│        • frontend-design           │    agent:       claude                          │
│        ...                         │    tier:        primary                         │
│      ▾ secondary (4)               │                                                 │
│        • code-review/code-review   │  assets (6)                                     │
│  ▾ cursor  (3)                     │   ★ scripts/run.sh   (421B)                     │
│  ▾ codex   (5)                     │     templates/x.md    (88B)                     │
│  ▾ agents  (1)                     │                                                 │
│                                    │  ⧉ exact duplicate cluster (2 files)            │
│                                    │  · claude/agent-browser (~/code/.claude/skills) │
│                                    │                                                 │
│ ↑/↓ move · ←/→ collapse · r rescan · q quit                                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

## Install

### CLI (agents + scripting)

```bash
cargo install skillview
```

This installs the `skillview` binary. It's all you need for CLI use.

### TUI (humans)

```bash
git clone https://github.com/galElmalah/skillview
cd skillview
./bin/skillview --tui      # builds Rust + installs Bun deps on first run
```

Requires:
- Rust 1.75+ (`rustup install stable`)
- Bun 1.1+ (`curl -fsSL https://bun.sh/install | bash`) — TUI only

## Use

```bash
# Default: emit the full Inventory JSON.
skillview
skillview --pretty
skillview --root ~/code/project
skillview --no-similarity        # skip MinHash
skillview --threshold 0.7        # looser dup detection
skillview --no-usage             # skip cross-agent usage scan

# Discovery (use these first when you don't know the tree):
skillview examples               # copy-pasteable recipes (no scan, instant)
skillview agents                 # per-agent rollup (counts, dups, usage totals)
skillview roots                  # scanned roots + skill counts
skillview stats --pretty         # inventory-wide statistics

# List with rich filtering + sort + limit + format:
skillview list --agent claude --tier primary
skillview list --name browser --sort usage --limit 10
skillview list --root-kind claude-global --format ids
skillview list --validation-failed --format paths
skillview list --has-usage --format tsv
skillview list --dups-only --dup-kind near --min-tokens 1000

# Inspect one skill (id | name | path substring):
skillview show agent-browser
skillview show s_3
skillview show ~/.claude/skills/x

# Duplicate clusters:
skillview dups                          # all
skillview dups --exact                  # exact-content only
skillview dups --near --min-size 3 --sort size
skillview dups --agent claude --format tsv

# Cross-agent usage (session-log scan):
skillview usage --top 20
skillview usage --agent claude --sort sessions
skillview usage --min-mentions 5 --format tsv
skillview usage --include-low           # include skills with low-confidence counts

# Every subcommand has --help with examples:
skillview --help
skillview list --help
skillview dups --help
skillview usage --help

# Launch the TUI (requires Bun + the tui/ sources from a checkout):
skillview --tui
```

All subcommands share the scan flags (`--root`, `--threshold`, `--no-similarity`, `--no-usage`, `--include-minhash`, `--pretty`). Default output is compact JSON; pass `--pretty` for human reading. The `--format` flag on filtered subcommands (`list`, `dups`, `usage`, `agents`, `roots`) opts into `jsonl`, `tsv`, `ids`, `paths`, or `names` for easy piping. `--tui` is a separate mode — when set, all other flags are forwarded to the TUI's underlying scan via the spawned process.

### Filter cheatsheet

`list` filters compose (AND): `--agent`, `--tier`, `--root-kind`, `--name`, `--dups-only`, `--dup-kind`, `--has-usage`, `--min-usage N`, `--min-tokens N`, `--max-tokens N`, `--validation-failed`. Sort with `--sort {agent-name|name|agent|tier|usage|tokens|sessions|path}`. Limit with `--limit N`. Project with `--format {json|jsonl|tsv|ids|paths|names}`.

`dups` filters: `--exact`, `--near`, `--min-size N`, `--agent`, `--root-kind`. Sort with `--sort {size|similarity|kind}`.

`usage` filters: `--agent`, `--min-mentions N`, `--top N`, `--include-low`. Sort with `--sort {mentions|sessions|recent|name}`. Usage counts are only attached to skill names that are reliably distinctive (≥ 6 chars and contain `-` or `_`); names below that bar are marked `confidence: low` and hidden by default.

## Architecture

Two surfaces, one Rust binary:

- **`skillview` (Rust)** — Walks the filesystem using the [`ignore`](https://docs.rs/ignore) crate (same engine `fff` and ripgrep use), parses `SKILL.md` frontmatter, classifies each hit by agent + root kind, computes a MinHash signature over normalized body content, scans Claude/Codex session logs for cross-agent usage, and emits structured JSON.
- **`skillview-tui` (Bun + [OpenTUI](https://opentui.com))** — React app that spawns the Rust binary, parses the JSON, and renders the tree + detail view with keyboard navigation. Launched by `skillview --tui`.

The shared contract lives in [`schema/skillview.schema.json`](schema/skillview.schema.json) and the design rationale in [`DESIGN.md`](DESIGN.md).

## End-to-end smoke

`bin/smoke.sh` builds the binary, runs it against `$HOME`, validates the JSON shape, and prints every discovered skill plus duplicate clusters:

```bash
./bin/smoke.sh
```

## Keys (TUI)

| key | action |
|---|---|
| `↑` / `↓` (or `k`/`j`) | move selection |
| `→` / `←` (or `l`/`h`) | expand / collapse group |
| `enter` / `space` | toggle current group |
| `r` | re-scan |
| `q` / `Ctrl-C` | quit |

## What counts as a skill

| tier | detected when |
|---|---|
| primary | filename is exactly `SKILL.md` |
| secondary | filename is `skill.md` / `skills.md` (any case), OR any `.md` under a `.claude/.codex/.cursor/.agents/skills` ancestor with valid `name:` + `description:` frontmatter |

Anything else is ignored. The hard denylist (`Library`, `Caches`, `node_modules`, `.git`, `target`, …) lives in `crates/skillview/src/scan.rs`.

## Publishing

```bash
# From a clean checkout on main:
cargo publish --dry-run -p skillview
cargo publish -p skillview
```

The published artifact is the CLI only (the `include` list in `crates/skillview/Cargo.toml` excludes `tui/`, `bin/`, `schema/`). TUI users install via `git clone`.

## Roadmap

- v1 (here): inventory + CLI subcommands + TUI tree/detail + duplicate clusters.
- v1.1: open in `$EDITOR` (`o` key), JSON/Markdown export (`e`).
- v1.2: incremental rescans / filesystem watch.
- v2: optional embedding-based semantic similarity (`--semantic` flag).
- v3: archive / delete duplicates with strong undo guarantees.

## Layout

```
.
├── Cargo.toml                    # workspace
├── crates/skillview/             # Rust crate (lib + bin)
│   ├── Cargo.toml
│   ├── README.md                 # ships to crates.io
│   ├── LICENSE
│   └── src/{main,lib,model,scan,parse,classify,minhash,emit,usage}.rs
├── schema/skillview.schema.json  # shared contract
├── tui/                          # Bun + OpenTUI app (TUI surface)
│   ├── package.json
│   ├── tsconfig.json
│   └── src/{index,app,tree,detail,bridge,types}.{tsx,ts}
├── bin/skillview                 # dev wrapper (build + exec)
├── bin/smoke.sh                  # end-to-end smoke test
├── DESIGN.md
└── README.md
```
