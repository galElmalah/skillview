import React from "react";
import type { Cluster, Inventory, Skill } from "./types";

interface ClusterViewProps {
  width: number;
  height: number;
  inventory: Inventory;
  cluster: Cluster;
  selectedIdx: number;
}

export function ClusterView({
  width,
  height,
  inventory,
  cluster,
  selectedIdx,
}: ClusterViewProps) {
  const members = cluster.members
    .map((id) => inventory.skills.find((s) => s.id === id))
    .filter((s): s is Skill => !!s);

  const safeIdx = Math.min(Math.max(0, selectedIdx), Math.max(0, members.length - 1));
  const selected = members[safeIdx];

  const headerColor = cluster.kind === "exact" ? "#f7768e" : "#e5c07b";
  const headerLabel =
    cluster.kind === "exact"
      ? `⧉ exact-content cluster — ${members.length} identical copies`
      : `≈ near-duplicate cluster — ${members.length} members at ~${Math.round(cluster.similarity * 100)}% similarity`;

  return (
    <box flexDirection="column" width={width} height={height}>
      <box
        flexDirection="row"
        height={1}
        paddingLeft={1}
        paddingRight={1}
        backgroundColor="#1f2430"
      >
        <text fg={headerColor}>{headerLabel}</text>
        <text fg="#5c6370">  ·  </text>
        <text fg="#5c6370">cluster {cluster.id}</text>
      </box>

      <box flexDirection="row" width={width} height={height - 3}>
        <box
          flexDirection="column"
          width={Math.max(40, Math.floor(width * 0.55))}
          height={height - 3}
          paddingLeft={1}
          paddingRight={1}
        >
          <text fg="#7aa2f7"> members</text>
          {members.map((m, i) => (
            <ClusterRow
              key={m.id}
              skill={m}
              isSelected={i === safeIdx}
              width={Math.max(40, Math.floor(width * 0.55)) - 2}
            />
          ))}
        </box>

        <box
          width={1}
          height={height - 3}
          backgroundColor="#1a1d24"
        />

        <box
          flexDirection="column"
          width={width - Math.max(40, Math.floor(width * 0.55)) - 1}
          height={height - 3}
          paddingLeft={1}
          paddingRight={1}
        >
          {selected ? <MemberDetail skill={selected} /> : null}
        </box>
      </box>

      <box
        flexDirection="row"
        height={1}
        paddingLeft={1}
        paddingRight={1}
        backgroundColor="#1f2430"
      >
        <text fg="#5c6370">
          ↑/↓ select · enter jump to tree · v view content · d delete · esc back
        </text>
      </box>

      <box height={1} backgroundColor="#0f1117" />
    </box>
  );
}

function ClusterRow({
  skill,
  isSelected,
  width,
}: {
  skill: Skill;
  isSelected: boolean;
  width: number;
}) {
  const bg = isSelected ? "#2a3144" : undefined;
  const fg = isSelected ? "#ffffff" : "#c0caf5";
  const mark = isSelected ? "▸" : " ";
  const usage =
    skill.usage?.confidence === "high"
      ? skill.usage.mentions === 0
        ? "  ◌"
        : `  ⏵ ${shortCount(skill.usage.mentions)}`
      : "";
  const label = `${mark} ${skill.agent.padEnd(7)} · ${skill.name}${usage}`;
  const pathLabel = `    ${compactPath(skill.path)}`;
  return (
    <>
      <box width={width} backgroundColor={bg}>
        <text fg={fg}>{truncate(label, width)}</text>
      </box>
      <box width={width} backgroundColor={bg}>
        <text fg={isSelected ? "#a3be8c" : "#5c6370"}>
          {truncate(pathLabel, width)}
        </text>
      </box>
    </>
  );
}

function MemberDetail({ skill }: { skill: Skill }) {
  const u = skill.usage;
  return (
    <>
      <text fg="#7aa2f7"> {skill.name}</text>
      <text fg="#5c6370">{compactPath(skill.path)}</text>
      <text> </text>
      <text fg="#9ece6a">frontmatter</text>
      <text fg="#c0caf5">
        {"  "}description: {skill.frontmatter?.description ?? "(none)"}
      </text>
      <text fg="#c0caf5">
        {"  "}agent:       {skill.agent}
      </text>
      <text fg="#c0caf5">
        {"  "}tier:        {skill.tier}
      </text>
      <text fg="#c0caf5">
        {"  "}root:        {skill.root_id}
      </text>
      <text> </text>
      <text fg="#9ece6a">usage</text>
      {u && u.confidence === "high" && u.mentions > 0 ? (
        <>
          <text fg="#c0caf5">
            {"  "}mentions:  {u.mentions} across {u.sessions} sessions
          </text>
          {u.by_source ? (
            <text fg="#c0caf5">
              {"  "}sources:   {Object.entries(u.by_source).map(([k, v]) => `${k}=${v}`).join(" · ")}
            </text>
          ) : null}
          {u.last_seen_at ? (
            <text fg="#c0caf5">
              {"  "}last seen: {u.last_seen_at}
            </text>
          ) : null}
        </>
      ) : u && u.confidence === "high" ? (
        <text fg="#e5c07b">  ◌ never invoked in scanned session logs</text>
      ) : (
        <text fg="#5c6370">  (name too generic to scan reliably)</text>
      )}
      <text> </text>
      <text fg="#9ece6a">assets ({skill.assets.length})</text>
      {skill.assets.slice(0, 6).map((a) => (
        <text key={a.path} fg="#c0caf5">
          {"  "}{a.referenced ? "★" : " "} {a.path}
        </text>
      ))}
      {skill.assets.length > 6 ? (
        <text fg="#5c6370">  …and {skill.assets.length - 6} more</text>
      ) : null}
    </>
  );
}

function compactPath(p: string): string {
  const home = process.env.HOME;
  if (home && p.startsWith(home)) return "~" + p.slice(home.length);
  return p;
}

function truncate(s: string, w: number): string {
  if (s.length <= w) return s;
  return s.slice(0, Math.max(1, w - 1)) + "…";
}

function shortCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10000) return `${(n / 1000).toFixed(1)}k`;
  return `${Math.floor(n / 1000)}k`;
}
