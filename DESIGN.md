# skillview — design

A two-process tool that inventories every agent skill on your machine, lays them out as a tree grouped by host agent, and flags near-duplicates so you can see what's drifted across `~/.claude`, `~/.codex`, `~/.cursor`, `~/.agents`, and project-local copies.

## Architecture

```
+---------------------+        JSON over stdout        +---------------------+
|  skillview (bin)    |  ───────────────────────────▶  |  skillview-tui      |
|  (Rust binary)      |                                |  (Bun + OpenTUI)    |
|                     |                                |                     |
|  - ignore-crate walk|                                |  - React/JSX        |
|  - frontmatter parse|                                |  - tree pane (left) |
|  - MinHash clustering                                |  - detail pane(right|
|  - emit Inventory   |                                |  - useKeyboard nav  |
+---------------------+                                +---------------------+
```

Two binaries, one shared schema (`schema/skillview.schema.json`). The Bun side spawns the Rust binary, reads stdout, parses JSON, renders. Re-scan = re-spawn. No long-lived IPC.

## Decisions

| Question | Choice | Why |
|---|---|---|
| Language split | Rust core + Bun TUI over stdio/JSON | Keeps OpenTUI native; keeps walking & MinHash native in Rust |
| File walker | `ignore` crate directly | Same engine `fff` and ripgrep use; parallel, .gitignore-aware |
| Scan scope | Full `$HOME` + denylist | Catches skills in unusual locations |
| Skill detection | Tiered: primary `SKILL.md`, secondary `skill.md` / `skills.md` / `.md` under known agent dirs | Honors Claude convention without missing ad-hoc layouts |
| Assets | All sibling files (recursive) + parsed link references, files marked `referenced=true` if linked | Surfaces orphans without hiding them |
| Similarity | MinHash signatures + Jaccard ≥ 0.85 (configurable), union-find clustering | Fast, no model dependency, catches drifted copies |
| TUI scope (v1) | View-only: tree + detail | Safest baseline; mutations later |

## JSON contract

See `schema/skillview.schema.json`. Top-level shape:

```jsonc
{
  "schema_version": 1,
  "generated_at": "2026-05-12T12:34:56Z",
  "roots": [
    { "id": "r_0", "kind": "claude-global", "path": "/Users/gal/.claude" }
  ],
  "skills": [
    {
      "id": "s_0",
      "tier": "primary",
      "name": "agent-browser",
      "path": ".../skills/agent-browser/SKILL.md",
      "dir":  ".../skills/agent-browser",
      "agent": "claude",
      "root_id": "r_0",
      "frontmatter": { "name": "agent-browser", "description": "..." },
      "content_hash": "blake3:...",
      "minhash": [/* 128 u64 */],
      "assets": [
        { "path": "scripts/run.sh", "size_bytes": 421, "referenced": true },
        { "path": "templates/x.md", "size_bytes": 88,  "referenced": false }
      ],
      "cluster_id": "c_0"
    }
  ],
  "clusters": [
    { "id": "c_0", "kind": "near", "similarity": 0.92, "members": ["s_0","s_7"] }
  ],
  "stats": {
    "scanned_paths": 184213,
    "elapsed_ms": 612,
    "primary_skills": 47,
    "secondary_skills": 12,
    "duplicate_clusters": 3
  }
}
```

`minhash` is included in the JSON to keep the contract debuggable; the TUI ignores it.

## Rust core modules

```
crates/skillview/
├── Cargo.toml
└── src/
    ├── main.rs        # CLI entry, top-level orchestration
    ├── lib.rs         # re-exports
    ├── model.rs       # serde types matching the schema
    ├── scan.rs        # ignore-crate parallel walk + candidate filter
    ├── parse.rs       # frontmatter split + markdown link extraction
    ├── classify.rs    # agent + root-kind inference from path
    ├── minhash.rs     # MinHash signature + union-find clustering
    └── emit.rs        # assemble Inventory and serialize
```

### Scan filter

Accept as a candidate any file where:
1. Filename is literally `SKILL.md` → **primary**, or
2. Filename is `skill.md`/`skills.md` (case-insensitive) → **secondary**, or
3. Any `.md` whose ancestor path includes a segment named `.claude`, `.codex`, `.cursor`, `.agents`, or `skills` → **secondary** (re-validated by frontmatter in parse phase; dropped if no `name:` + `description:`).

Hard denylist of directory names (skipped wholesale): `Library`, `Caches`, `Movies`, `Music`, `Pictures`, `Applications`, `Public`, `node_modules`, `.git`, `target`, `.venv`, `venv`, `__pycache__`, `.next`, `dist`, `build`, `.npm`, `.cargo`, `.rustup`.

### Agent + root inference (`classify.rs`)

Walk ancestors of each skill path:
- First `.claude` found: if its parent is `$HOME` → `claude-global`, else `claude-project`. Agent = `claude`.
- First `.codex` → `codex`.
- First `.cursor` → `cursor`.
- First `.agents` → `agents-generic`.
- Fallback: nearest `skills/` ancestor's parent is the root, agent = `unknown`.

The `root_id` is assigned by interning unique `root_dir`s.

### MinHash

128 permutations using XxHash64 with k different seeds. Shingle size = 3 over lowercased alphanumeric tokens of the **body** (frontmatter stripped, links replaced with their text). Two skills are in the same cluster iff their estimated Jaccard ≥ `--threshold` (default 0.85). Clusters are connected components via union-find. Exact duplicates are detected first via Blake3 of normalized body and grouped as `kind: "exact"` clusters; the remaining skills go through MinHash to form `kind: "near"` clusters.

## TUI structure

```
tui/
├── package.json
├── tsconfig.json
└── src/
    ├── index.tsx     # createCliRenderer + createRoot
    ├── app.tsx       # state, keyboard routing, layout
    ├── tree.tsx      # left pane (agent → root → skill)
    ├── detail.tsx    # right pane (frontmatter, assets, cluster banner)
    ├── bridge.ts     # spawn the Rust binary, parse JSON
    └── types.ts      # mirror of the schema
```

Keybindings:
- `↑/↓` move selection
- `→/←` expand/collapse group
- `q` quit
- `r` re-scan (re-spawn core)
- `?` toggle help overlay (later)

## Non-goals (v1)

- Mutations (delete/archive duplicates) — would need explicit safety design.
- Incremental rescans / file watching.
- Embedding-based semantic similarity.
- Remote skill catalogs.

## Running

```
# build the Rust core once
cargo build --release -p skillview

# install the Bun deps
cd tui && bun install && cd ..

# run end-to-end via the wrapper
./bin/skillview
```
