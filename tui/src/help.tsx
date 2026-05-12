import React from "react";

interface HelpProps {
  width: number;
  height: number;
}

export function Help({ width, height }: HelpProps) {
  return (
    <box flexDirection="column" width={width} height={height}>
      <box
        flexDirection="row"
        height={1}
        paddingLeft={1}
        paddingRight={1}
        backgroundColor="#1f2430"
      >
        <text fg="#a3be8c">skillview · help</text>
        <text fg="#5c6370">  ·  press esc or q to close</text>
      </box>

      <box
        flexDirection="column"
        width={width}
        height={height - 2}
        paddingLeft={2}
        paddingRight={2}
      >
        <text fg="#7aa2f7">What this tool does</text>
        <text fg="#c0caf5">
          {"  "}skillview inventories every agent skill on your machine
          (SKILL.md and skill-shaped markdown
        </text>
        <text fg="#c0caf5">
          {"  "}files under .claude / .codex / .cursor / .agents / skills /
          agents / commands / rules /
        </text>
        <text fg="#c0caf5">
          {"  "}prompts / bundled-skills / optional-skills / mcp-tools),
          groups them by host agent + root,
        </text>
        <text fg="#c0caf5">
          {"  "}detects near-duplicates with MinHash, and counts how often each
          one is mentioned in
        </text>
        <text fg="#c0caf5">
          {"  "}Claude + Codex session logs.
        </text>

        <text> </text>
        <text fg="#7aa2f7">Reading the tree</text>
        <Row k="• name"          v="a skill (leaf node)" />
        <Row k="⧉ exact (N)"    v="exact-content duplicate; cluster has N members" />
        <Row k="≈ 92% (N)"      v="near-duplicate; cluster has N members, average pairwise similarity" />
        <Row k="⏵ 12k"          v="invocation count across Claude + Codex session logs" />
        <Row k="◌ unused"       v="never invoked in any scanned session (potential stale skill)" />
        <Row k="(no badge)"     v="name too generic to count reliably (e.g. ‘auth’, ‘slack’)" />

        <text> </text>
        <text fg="#7aa2f7">Seeing similar skills</text>
        <text fg="#c0caf5">
          {"  "}If a skill is part of a duplicate cluster, the tree row ends with a badge like ⧉ exact (3)
        </text>
        <text fg="#c0caf5">
          {"  "}or ≈ 92% (4). The detail pane on the right lists the other members in that cluster.
        </text>
        <text fg="#c0caf5">
          {"  "}Press [c] on any clustered skill to open the cluster view — a focused screen showing
        </text>
        <text fg="#c0caf5">
          {"  "}all members side-by-side with their paths, agents, and mention counts. From there you can:
        </text>
        <Row k="↑/↓ · j/k"  v="select a member" />
        <Row k="enter"      v="jump back to that member in the main tree" />
        <Row k="v"          v="open the full SKILL.md body of the selected member" />
        <Row k="d"          v="delete the selected member (with confirmation)" />
        <Row k="esc / q"    v="return to the main view" />

        <text> </text>
        <text fg="#7aa2f7">Navigation</text>
        <Row k="↑/↓ · k/j"        v="move selection" />
        <Row k="pgup/pgdn · space" v="move one page" />
        <Row k="home/end · g/G"    v="jump to top / bottom" />
        <Row k="←/→ · h/l · enter" v="collapse / expand a group" />
        <Row k="[ · ]"             v="shrink / grow the tree pane (middle divider)" />
        <Row k="= · 0"             v="reset split to default (42% tree)" />

        <text> </text>
        <text fg="#7aa2f7">Actions</text>
        <Row k="/" v="search (filters tree by name + description, esc clears)" />
        <Row k="v" v="open full-screen SKILL.md viewer (j/k pgup/pgdn to scroll)" />
        <Row k="c" v="open cluster view for the selected skill's duplicate group" />
        <Row k="d" v="delete the selected skill (primary → whole dir, secondary → just .md)" />
        <Row k="r" v="re-scan $HOME (cache invalidates automatically when binary changes)" />
        <Row k="?" v="this help screen" />
        <Row k="q · ctrl-c" v="quit" />

        <text> </text>
        <text fg="#7aa2f7">Cache</text>
        <text fg="#c0caf5">
          {"  "}Every scan is written to ~/.cache/skillview/inventory.json so future
          runs are instant.
        </text>
        <text fg="#c0caf5">
          {"  "}Caches older than 1 hour auto-refresh in the background.
          SKILLVIEW_NO_CACHE=1 disables.
        </text>
      </box>

      <box
        flexDirection="row"
        height={1}
        paddingLeft={1}
        paddingRight={1}
        backgroundColor="#1f2430"
      >
        <text fg="#5c6370">esc / q  close help</text>
      </box>
    </box>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <box flexDirection="row" width="100%">
      <text fg="#e5c07b">  {k.padEnd(22)}</text>
      <text fg="#c0caf5">{v}</text>
    </box>
  );
}

