import React from "react";
import type { Inventory, Skill } from "./types";
import type { VisibleNode } from "./tree";

interface DetailProps {
  width: number;
  height: number;
  inventory: Inventory;
  skill?: Skill;
  node?: VisibleNode;
  error: string | null;
  previewContent: string | null;
}

export function Detail({
  width,
  height,
  inventory,
  skill,
  node,
  error,
  previewContent,
}: DetailProps) {
  if (error) {
    return (
      <box flexDirection="column" width={width} height={height} paddingLeft={1}>
        <text fg="#f7768e"> error: {error}</text>
      </box>
    );
  }

  if (!skill) {
    return (
      <box flexDirection="column" width={width} height={height} paddingLeft={1}>
        <text fg="#7aa2f7"> detail</text>
        <text fg="#5c6370">
          {node?.kind
            ? `select a skill to see frontmatter, assets and duplicates`
            : `no selection`}
        </text>
      </box>
    );
  }

  const cluster = skill.cluster_id
    ? inventory.clusters.find((c) => c.id === skill.cluster_id)
    : undefined;
  const clusterMembers =
    cluster
      ? cluster.members
          .filter((m) => m !== skill.id)
          .map((m) => inventory.skills.find((s) => s.id === m))
          .filter((s): s is Skill => !!s)
      : [];

  const fm = skill.frontmatter ?? {};
  const description = fm.description ?? "(no description)";

  return (
    <box flexDirection="column" width={width} height={height} paddingLeft={1}>
      <text fg="#7aa2f7"> {skill.name}</text>
      <text fg="#5c6370">{compactPath(skill.path)}</text>

      <text> </text>
      <text fg="#9ece6a">frontmatter</text>
      <text fg="#c0caf5">  name:        {fm.name ?? "(missing)"}</text>
      <text fg="#c0caf5">  description: {truncate(description, width - 17)}</text>
      <text fg="#c0caf5">  agent:       {skill.agent}</text>
      <text fg="#c0caf5">  tier:        {skill.tier}</text>

      <text> </text>
      <text fg="#9ece6a">validation</text>
      <ValidationBlock skill={skill} width={width} />

      <text> </text>
      <text fg="#9ece6a">tokens (approx · chars/3.7)</text>
      <TokensBlock skill={skill} />

      <text> </text>
      <text fg="#9ece6a">usage</text>
      <UsageBlock skill={skill} />

      <text> </text>
      <text fg="#9ece6a">assets ({skill.assets.length})</text>
      <AssetList assets={skill.assets} width={width} maxRows={Math.max(2, Math.floor(height / 3))} />

      {cluster ? (
        <>
          <text> </text>
          <text fg="#e5c07b">
            {cluster.kind === "exact"
              ? `⧉ exact duplicate cluster (${cluster.members.length} files)`
              : `≈ near-duplicate cluster — similarity ${(cluster.similarity * 100).toFixed(1)}%`}
          </text>
          {clusterMembers.slice(0, 4).map((m) => (
            <text key={m.id} fg="#c0caf5">
              · {m.agent}/{m.name}  ({compactPath(m.path)})
            </text>
          ))}
          {clusterMembers.length > 4 ? (
            <text fg="#5c6370">  …and {clusterMembers.length - 4} more</text>
          ) : null}
        </>
      ) : null}

      <text> </text>
      <text fg="#9ece6a">content preview (v to read full)</text>
      <ContentPreview content={previewContent} width={width} />
    </box>
  );
}

function ContentPreview({
  content,
  width,
}: {
  content: string | null;
  width: number;
}) {
  if (content == null) {
    return <text fg="#5c6370">  loading…</text>;
  }
  if (!content.trim()) {
    return <text fg="#5c6370">  (empty body)</text>;
  }
  const lines = content.split("\n").slice(0, 8);
  return (
    <>
      {lines.map((line, i) => (
        <text key={i} fg="#c0caf5">
          {truncate(line || " ", width - 2)}
        </text>
      ))}
    </>
  );
}

function ValidationBlock({ skill, width }: { skill: Skill; width: number }) {
  const v = skill.validation;
  if (!v) {
    return <text fg="#5c6370">  (not computed)</text>;
  }
  if (v.ok) {
    return <text fg="#9ece6a">  ✓ valid</text>;
  }
  const issues = v.issues ?? [];
  return (
    <>
      <text fg="#f7768e">  ✗ {issues.length} issue{issues.length === 1 ? "" : "s"}</text>
      {issues.slice(0, 5).map((it, i) => (
        <text key={i} fg="#e5c07b">{truncate("    · " + it, width - 2)}</text>
      ))}
      {issues.length > 5 ? (
        <text fg="#5c6370">    …and {issues.length - 5} more</text>
      ) : null}
    </>
  );
}

function TokensBlock({ skill }: { skill: Skill }) {
  const t = skill.tokens;
  if (!t) {
    return <text fg="#5c6370">  (not computed)</text>;
  }
  return (
    <>
      <text fg="#c0caf5">
        {"  "}description: ≈{t.description.toLocaleString().padStart(6)}  (in the index)
      </text>
      <text fg="#c0caf5">
        {"  "}body:        ≈{t.body.toLocaleString().padStart(6)}  (on activation)
      </text>
      <text fg="#c0caf5">
        {"  "}total:       ≈{t.total.toLocaleString().padStart(6)}
      </text>
    </>
  );
}

function UsageBlock({ skill }: { skill: Skill }) {
  const u = skill.usage;
  if (!u || u.confidence === "low") {
    return (
      <text fg="#5c6370">
        {"  "}name too generic to scan reliably (e.g. ‘
        {skill.name}’) — no count
      </text>
    );
  }
  if (u.mentions === 0) {
    return (
      <>
        <text fg="#e5c07b">  ◌ never invoked in Claude or Codex session logs</text>
        <text fg="#5c6370">
          {"  "}(scanned {skill.usage?.by_source ? "all" : "no"} session
          archives; could indicate a stale or unused skill)
        </text>
      </>
    );
  }
  const sources = u.by_source ?? {};
  const sourceLine = Object.entries(sources)
    .map(([k, v]) => `${k}=${v}`)
    .join(" · ");
  const last = u.last_seen_at ? prettyAge(u.last_seen_at) : "unknown";
  return (
    <>
      <text fg="#c0caf5">
        {"  "}mentions:  {u.mentions.toLocaleString()} across {u.sessions} sessions
      </text>
      <text fg="#c0caf5">
        {"  "}by source: {sourceLine || "—"}
      </text>
      <text fg="#c0caf5">
        {"  "}last seen: {last}
      </text>
    </>
  );
}

function prettyAge(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const secs = Math.max(0, (Date.now() - t) / 1000);
  if (secs < 60) return `${Math.floor(secs)}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  if (secs < 30 * 86400) return `${Math.floor(secs / 86400)}d ago`;
  return iso.slice(0, 10);
}

function AssetList({
  assets,
  width,
  maxRows,
}: {
  assets: Skill["assets"];
  width: number;
  maxRows: number;
}) {
  if (assets.length === 0) {
    return <text fg="#5c6370">  (none)</text>;
  }
  const rows = assets.slice(0, maxRows);
  return (
    <>
      {rows.map((a) => {
        const badge = a.referenced ? "★" : " ";
        const fg = a.referenced ? "#9ece6a" : "#c0caf5";
        const label = `${badge} ${a.path}  (${formatBytes(a.size_bytes)})`;
        return (
          <text key={a.path} fg={fg}>
            {truncate(label, width - 2)}
          </text>
        );
      })}
      {assets.length > maxRows ? (
        <text fg="#5c6370">  …and {assets.length - maxRows} more</text>
      ) : null}
    </>
  );
}

function truncate(s: string, w: number): string {
  if (s.length <= w) return s;
  return s.slice(0, Math.max(1, w - 1)) + "…";
}

function compactPath(p: string): string {
  const home = process.env.HOME;
  if (home && p.startsWith(home)) return "~" + p.slice(home.length);
  return p;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}K`;
  return `${(n / 1024 / 1024).toFixed(1)}M`;
}
