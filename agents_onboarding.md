# skillview — onboarding for agents

You are an autonomous agent (Claude, Codex, Cursor, or another) that needs to
discover, filter, and reason about agent skills installed on this machine.
`skillview` is the tool for that. This doc is the fastest path from "I have
no idea what's here" to "I'm calling the right command with the right
filters". Keep it open while you work.

The CLI is JSON-by-default and self-describing. If you forget anything,
`skillview --help`, `skillview <cmd> --help`, and `skillview examples`
contain every recipe you need.

---

## 30-second mental model

- A **skill** is an `SKILL.md` (or skill-shaped `.md`) on disk with
  `name:` + `description:` frontmatter. Each skill belongs to one **agent**
  (claude / codex / cursor / agents / unknown) and one **root** (a directory
  like `~/.claude`, `~/.codex`, a project-local `.claude/`, or an `unknown`
  fallback root).
- skillview walks the filesystem, classifies every hit, computes
  exact-content hashes + MinHash signatures, and (optionally) scans Claude
  and Codex session logs to count how often each skill is mentioned.
- The output is one JSON document (the **Inventory**) plus a set of
  projection subcommands (`list`, `dups`, `usage`, `agents`, `roots`,
  `stats`) that filter and reshape it for specific questions.

---

## When you don't know what's here — discovery commands

Run these first. They're cheap, structured, and give you a map of the
landscape without parsing the full inventory.

```bash
skillview agents                  # per-agent rollup: how many skills, dups, usage
skillview roots                   # which directories were scanned, by kind + count
skillview stats --pretty          # one-shot inventory summary
skillview examples                # copy-pasteable recipe book (no scan, instant)
```

If your task is "tell the user what skills they have", start with `agents`
and `roots`. If your task is to find a specific skill, jump to `list`.

---

## Finding a specific skill — `list`

`list` returns a JSON array of skill summaries, with filters that compose
(AND semantics) and an output format that you can tune to whatever your
calling code wants to parse.

### Filters

| flag | what it does |
|---|---|
| `--agent <name>` | `claude`, `codex`, `cursor`, `agents`, `unknown` (case-insensitive) |
| `--tier primary\|secondary` | `SKILL.md` is primary; `skill.md` etc. are secondary |
| `--root-kind <kind>` | `claude-global`, `claude-project`, `codex`, `cursor`, `agents-generic`, `unknown` |
| `--name <substring>` | case-insensitive substring match on the skill name |
| `--dups-only` | only skills that participate in a duplicate cluster |
| `--dup-kind exact\|near` | restrict `--dups-only` to one cluster kind |
| `--has-usage` | shorthand for `--min-usage 1` |
| `--min-usage N` | only skills with ≥ N mentions in session logs |
| `--min-tokens N` / `--max-tokens N` | bound on description + body token count |
| `--validation-failed` | only skills whose frontmatter validation failed |

### Ordering and projection

- `--sort {agent-name|name|agent|tier|usage|tokens|sessions|path}` — default is `agent-name`. `usage`, `tokens`, `sessions` sort descending.
- `--limit N` — truncate after sorting.
- `--format {json|jsonl|tsv|ids|paths|names}` — default is `json`. `jsonl` streams one skill per line; `tsv` has a header row and tab-separated columns; `ids`/`paths`/`names` are one-per-line projections.

### Common patterns

```bash
# "Which claude skills exist?"
skillview list --agent claude --format names

# "Top-used skills in this repo"
skillview list --has-usage --sort usage --limit 10

# "All paths I might want to open in an editor"
skillview list --root-kind claude-global --format paths

# "Skills whose frontmatter is broken"
skillview list --validation-failed --format paths

# "Heavy skills" (>2k tokens of body, sorted big-first)
skillview list --min-tokens 2000 --sort tokens --format tsv

# Find a skill by partial name match, then look it up
skillview list --name browser --format ids
skillview show <id>
```

---

## Looking at one skill — `show`

`show <target>` returns the full skill record plus any duplicate-cluster
siblings. Target resolution order:

1. exact id (e.g. `s_3`)
2. exact name, case-insensitive
3. path substring, case-insensitive

If a target matches multiple skills (e.g. two `dup-recipe` rows under
different agents), all matches come back in `matched`.

```bash
skillview show agent-browser              # by name
skillview show s_3                        # by id (stable per scan against the same tree)
skillview show ~/.claude/skills/browser   # by path substring
```

**Output shape:**
```json
{
  "matched": [ { "id": "s_3", "name": "...", "agent": "...", "path": "...", "frontmatter": {...}, "tokens": {...}, "validation": {...}, "assets": [...], "usage": {...} } ],
  "cluster": null | { "cluster": { "id": "c_0", "kind": "exact|near", "similarity": 1.0, "members": ["s_3", "s_42"] }, "members": [ /* skill summaries */ ] }
}
```

---

## Duplicates — `dups`

`dups` lists clusters of skills with overlapping content. Two flavors:

- **exact** clusters: identical body bytes after normalization (similarity `1.0`).
- **near** clusters: MinHash Jaccard ≥ `--threshold` (default `0.85`).

```bash
skillview dups                            # all clusters
skillview dups --exact                    # exact-content only
skillview dups --near --min-size 3        # near-dups with 3+ members
skillview dups --agent claude --sort size --limit 5
skillview dups --threshold 0.7 --near     # looser near-dup detection
skillview dups --format tsv               # | cluster_id | kind | similarity | size | agents | members |
```

**Filters:** `--exact`, `--near`, `--min-size N`, `--agent`, `--root-kind`, `--sort {size|similarity|kind}`, `--limit N`, `--format`.

---

## Cross-agent usage — `usage`

`usage` ranks skills by how often they appear in Claude / Codex session
logs (`~/.claude/projects/**/*.jsonl`, `~/.codex/sessions/**/*.jsonl`,
`~/.codex/archived_sessions/**/*.jsonl`).

```bash
skillview usage                           # top 25 by mentions
skillview usage --top 5 --sort sessions
skillview usage --agent claude --min-mentions 10
skillview usage --sort recent --format tsv
```

**Caveat: confidence.** Only names that are reliably distinctive (≥ 6 chars
AND contain `-` or `_`) get scanned. Shorter / generic names would be
dominated by false positives, so they're marked `confidence: low` and
hidden by default. Pass `--include-low` to include them anyway (but treat
their counts as untrustworthy).

`--format paths` is intentionally not supported for `usage` — use
`list --has-usage --format paths` instead.

---

## Scan-time flags (global, work on every subcommand)

| flag | effect |
|---|---|
| `--root <path>` | what tree to scan; defaults to `$HOME` |
| `--threshold <f>` | Jaccard threshold for near-dup clustering (default 0.85) |
| `--no-similarity` | skip MinHash (still detects exact-hash dups) — faster |
| `--no-usage` | skip session-log scan — much faster on large `$HOME` |
| `--include-minhash` | include the 128-element MinHash array in output (debug) |
| `--pretty` | pretty-print JSON |

If your task doesn't need duplicate or usage data, **always pass
`--no-similarity --no-usage`**. A full scan of `$HOME` can take minutes;
the fast path is sub-second.

---

## Identifying skills across calls

- **`name`** — what users say. Not unique (two `dup-recipe` skills may
  share a name across agents). Use it for human-readable output.
- **`id`** — `s_<n>`. **Stable across runs against the same tree** (sorted
  by path before assignment), but a new file inserted lexicographically
  earlier WILL shift downstream ids. Treat ids as stable within a session,
  not as durable handles.
- **`path`** — absolute filesystem path. Globally unique. Use this if you
  need a permanent reference, or to open the file in an editor.

For "I'm passing this between two subprocess calls of skillview", `id` is
fine. For "I'm storing this in a database", use `path`.

---

## Inventory schema (top-level)

The default invocation (and `scan`) emits this shape:

```jsonc
{
  "schema_version": 2,
  "generated_at": "2026-05-13T08:34:09Z",
  "roots":    [ { "id": "r_0", "kind": "claude-global", "path": "/Users/.../.claude" }, ... ],
  "skills":   [ /* full skill records */ ],
  "clusters": [ { "id": "c_0", "kind": "exact", "similarity": 1.0, "members": ["s_3","s_17"] }, ... ],
  "stats":    { /* counts, timings */ }
}
```

A full skill record includes `id`, `tier`, `name`, `path`, `dir`, `agent`,
`root_id`, `frontmatter`, `content_hash`, optional `minhash`, `assets`,
optional `cluster_id`, `usage`, `tokens`, `validation`. See
`schema/skillview.schema.json` for the authoritative contract.

---

## Recipes by task

### "List every skill I have, grouped by agent"
```bash
skillview agents
```

### "I'm looking for a skill about <topic>"
```bash
skillview list --name <topic> --format tsv
# then `skillview show <id>` for any promising hit
```

### "Audit which skills have broken frontmatter"
```bash
skillview list --validation-failed --format paths
```

### "Show me skills nobody uses"
```bash
skillview list --max-tokens 10000 --format jsonl \
  | jq 'select(.usage_mentions == 0 and .validation_ok)'
```

### "Show me clusters of duplicate skills I should consolidate"
```bash
skillview dups --sort size --format tsv
```

### "Open the agent-browser skill in an editor"
```bash
$EDITOR "$(skillview list --name agent-browser --format paths | head -1)"
```

### "Get the top 10 most-used skills"
```bash
skillview usage --top 10
```

### "Which agent has the most skills?"
```bash
skillview agents --format tsv | sort -t$'\t' -k2 -rn | head
```

---

## Exit codes and errors

- `0` — success.
- Non-zero — bad CLI input (unknown flag / value), unparseable args, or
  `show` target with no match. Errors print to stderr; stdout stays JSON
  on success.

---

## TL;DR cheat sheet

```bash
# Discover
skillview examples
skillview agents
skillview stats --pretty

# Find
skillview list --agent claude --tier primary --sort name
skillview list --name <q> --format ids

# Inspect
skillview show <id|name|path-substring>

# Audit
skillview dups --sort size
skillview usage --top 20
skillview list --validation-failed --format paths

# Help
skillview --help
skillview <cmd> --help
```

If a flag isn't documented above, ask `skillview <cmd> --help` first; the
CLI is designed to be self-explaining so you never have to guess.
