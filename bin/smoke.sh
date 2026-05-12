#!/usr/bin/env bash
# smoke.sh — build the Rust core, run it against $HOME, validate the JSON
# shape, and print every discovered skill + duplicate cluster.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }
warn() { printf '\033[33m%s\033[0m\n' "$*" >&2; }
fail() { printf '\033[31m%s\033[0m\n' "$*" >&2; exit 1; }

bold "▶ building skillview (release)"
cargo build --release -p skillview

bin="$here/target/release/skillview"
[[ -x "$bin" ]] || fail "build did not produce $bin"

out="$here/tmp/inventory.json"
mkdir -p "$here/tmp"

bold "▶ scanning \$HOME ($HOME)"
"$bin" --pretty > "$out"

bytes=$(wc -c < "$out" | tr -d ' ')
dim "  wrote $bytes bytes to $out"

if ! command -v jq >/dev/null 2>&1; then
  warn "jq not found — falling back to python for validation"
  python3 - "$out" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    inv = json.load(f)
assert inv["schema_version"] == 1, "schema_version must be 1"
assert isinstance(inv["roots"], list), "roots must be a list"
assert isinstance(inv["skills"], list), "skills must be a list"
assert isinstance(inv["clusters"], list), "clusters must be a list"
print(f"  ok: {len(inv['skills'])} skills · {len(inv['roots'])} roots · {len(inv['clusters'])} clusters")
PY
else
  schema_version=$(jq -r '.schema_version' "$out")
  [[ "$schema_version" == "1" ]] || fail "schema_version mismatch: $schema_version"
  roots=$(jq '.roots | length' "$out")
  skills=$(jq '.skills | length' "$out")
  clusters=$(jq '.clusters | length' "$out")
  dim "  ok: $skills skills · $roots roots · $clusters clusters"
fi

echo
bold "▶ summary"
jq -r '
  "stats:",
  "  scanned_paths:      \(.stats.scanned_paths)",
  "  elapsed_ms:         \(.stats.elapsed_ms)",
  "  primary_skills:     \(.stats.primary_skills)",
  "  secondary_skills:   \(.stats.secondary_skills)",
  "  duplicate_clusters: \(.stats.duplicate_clusters)"
' "$out"

echo
bold "▶ roots"
jq -r '.roots[] | "  [\(.kind)] \(.path)"' "$out"

echo
bold "▶ skills (tier · agent · name · path)"
jq -r '
  .skills
  | sort_by(.tier, .agent, .name)
  | .[]
  | "  \(.tier|.[0:1]|ascii_upcase) · \(.agent|.[0:6]) · \(.name) — \(.path)"
' "$out"

echo
bold "▶ duplicate clusters"
clusters_count=$(jq '.clusters | length' "$out")
if [[ "$clusters_count" == "0" ]]; then
  dim "  (none)"
else
  jq -r '
    .clusters as $cs
    | .skills as $ss
    | $cs[]
    | "  cluster \(.id) — \(.kind) — similarity \((.similarity*100)|floor)%",
      (
        .members[] as $m
        | "    · " + (
            ($ss[] | select(.id == $m) | "\(.agent)/\(.name)  (\(.path))")
          )
      )
  ' "$out"
fi

echo
bold "▶ done"
dim "  full inventory at $out"
