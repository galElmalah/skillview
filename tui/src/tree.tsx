import React from "react";
import type { Inventory, Skill, Root, RootKind, SkillTier } from "./types";

/**
 * Prune a tree to only branches whose leaf skills' names or descriptions
 * match the (case-insensitive) query. Returns the original tree on empty
 * query.
 */
export function filterTree(
  nodes: TreeNode[],
  inventory: Inventory,
  query: string,
): TreeNode[] {
  const q = query.trim().toLowerCase();
  if (!q) return nodes;
  const skillsById = new Map(inventory.skills.map((s) => [s.id, s]));

  // First pass: which skill nodes match?
  const keepSkill = new Set<string>();
  for (const n of nodes) {
    if (n.kind !== "skill") continue;
    const s = skillsById.get(n.skillId);
    if (!s) continue;
    const hay = (
      s.name.toLowerCase() +
      " " +
      (s.frontmatter?.description?.toLowerCase() ?? "") +
      " " +
      s.agent.toLowerCase()
    );
    if (matches(hay, q)) keepSkill.add(n.id);
  }

  // Second pass: keep ancestors of kept skills.
  const keepGroup = new Set<string>();
  for (const n of nodes) {
    if (n.kind === "skill" && keepSkill.has(n.id)) {
      // Walk parents up.
      let pid: string | undefined = n.parentId;
      while (pid) {
        keepGroup.add(pid);
        const parent = nodes.find((x) => x.id === pid);
        pid = parent && parent.kind !== "agent" && "parentId" in parent
          ? parent.parentId
          : undefined;
      }
    }
  }

  return nodes.filter((n) => {
    if (n.kind === "skill") return keepSkill.has(n.id);
    if (n.kind === "reference") return false; // search ignores references
    return keepGroup.has(n.id);
  });
}

/**
 * Case-insensitive subsequence match: every query character must appear in
 * order in the haystack. Cheaper and friendlier than exact substring.
 */
function matches(hay: string, q: string): boolean {
  if (hay.includes(q)) return true; // fast path: exact substring
  let i = 0;
  for (const ch of hay) {
    if (ch === q[i]) i++;
    if (i === q.length) return true;
  }
  return false;
}

export type TreeNode =
  | { kind: "agent"; id: string; agent: string; childCount: number }
  | { kind: "root"; id: string; parentId: string; root: Root; childCount: number }
  | { kind: "tier"; id: string; parentId: string; tier: SkillTier; childCount: number }
  | {
      kind: "skill";
      id: string;
      parentId: string;
      skillId: string;
      refCount: number;
    }
  | {
      kind: "reference";
      id: string;
      parentId: string;
      skillId: string;
      relPath: string;
      absPath: string;
      sizeBytes: number;
    };

export type VisibleNode = TreeNode & { depth: number };

export function skillNodeId(skillId: string): string {
  return `s:${skillId}`;
}

const AGENT_ORDER = ["claude", "codex", "cursor", "agents", "unknown"];
const TIER_ORDER: SkillTier[] = ["primary", "secondary"];

export function buildTreeNodes(inv: Inventory): TreeNode[] {
  const rootsById = new Map(inv.roots.map((r) => [r.id, r]));

  const byAgent = new Map<string, Skill[]>();
  for (const s of inv.skills) {
    const list = byAgent.get(s.agent) ?? [];
    list.push(s);
    byAgent.set(s.agent, list);
  }

  const agents = [...byAgent.keys()].sort(
    (a, b) =>
      indexOrLast(AGENT_ORDER, a) - indexOrLast(AGENT_ORDER, b) ||
      a.localeCompare(b),
  );

  const out: TreeNode[] = [];
  for (const agent of agents) {
    const skills = byAgent.get(agent)!;
    const agentId = `a:${agent}`;
    out.push({ kind: "agent", id: agentId, agent, childCount: skills.length });

    const byRoot = new Map<string, Skill[]>();
    for (const s of skills) {
      const list = byRoot.get(s.root_id) ?? [];
      list.push(s);
      byRoot.set(s.root_id, list);
    }

    const roots = [...byRoot.keys()].sort((a, b) => {
      const ra = rootsById.get(a);
      const rb = rootsById.get(b);
      return (ra?.path ?? "").localeCompare(rb?.path ?? "");
    });

    for (const rid of roots) {
      const r = rootsById.get(rid);
      if (!r) continue;
      const rootList = byRoot.get(rid)!;
      const rootNodeId = `r:${rid}`;
      out.push({
        kind: "root",
        id: rootNodeId,
        parentId: agentId,
        root: r,
        childCount: rootList.length,
      });

      for (const tier of TIER_ORDER) {
        const tierSkills = rootList
          .filter((s) => s.tier === tier)
          .sort((a, b) => a.name.localeCompare(b.name));
        if (tierSkills.length === 0) continue;
        const tierId = `t:${rid}:${tier}`;
        out.push({
          kind: "tier",
          id: tierId,
          parentId: rootNodeId,
          tier,
          childCount: tierSkills.length,
        });
        for (const s of tierSkills) {
          const sid = `s:${s.id}`;
          const referenced = s.assets.filter((a) => a.referenced);
          out.push({
            kind: "skill",
            id: sid,
            parentId: tierId,
            skillId: s.id,
            refCount: referenced.length,
          });
          for (const a of referenced) {
            out.push({
              kind: "reference",
              id: `${sid}:ref:${a.path}`,
              parentId: sid,
              skillId: s.id,
              relPath: a.path,
              absPath: `${s.dir}/${a.path}`,
              sizeBytes: a.size_bytes,
            });
          }
        }
      }
    }
  }
  return out;
}

export function flattenVisible(
  nodes: TreeNode[],
  collapsed: Set<string>,
  expandedSkills: Set<string>,
): VisibleNode[] {
  const out: VisibleNode[] = [];
  const hidden = new Set<string>();
  for (const n of nodes) {
    if (n.kind !== "agent" && "parentId" in n && hidden.has(n.parentId)) {
      hidden.add(n.id);
      continue;
    }
    const depth =
      n.kind === "agent"
        ? 0
        : n.kind === "root"
          ? 1
          : n.kind === "tier"
            ? 2
            : n.kind === "skill"
              ? 3
              : 4;
    out.push({ ...n, depth });
    // Agents/roots/tiers default to expanded; they hide children when in
    // `collapsed`. Skills default to collapsed; they show children only when
    // explicitly in `expandedSkills`.
    if (n.kind === "skill") {
      if (!expandedSkills.has(n.id)) hidden.add(n.id);
    } else if (n.kind !== "reference" && collapsed.has(n.id)) {
      hidden.add(n.id);
    }
  }
  return out;
}

interface TreeProps {
  width: number;
  height: number;
  visible: VisibleNode[];
  selectedIdx: number;
  inventory: Inventory;
  expandedSkills: Set<string>;
}

export function Tree({
  width,
  height,
  visible,
  selectedIdx,
  inventory,
  expandedSkills,
}: TreeProps) {
  const skillsById = React.useMemo(
    () => new Map(inventory.skills.map((s) => [s.id, s])),
    [inventory.skills],
  );

  const viewportHeight = Math.max(1, height - 2);
  const windowStart = clamp(
    selectedIdx - Math.floor(viewportHeight / 2),
    0,
    Math.max(0, visible.length - viewportHeight),
  );
  const slice = visible.slice(windowStart, windowStart + viewportHeight);

  return (
    <box flexDirection="column" width={width} height={height} paddingLeft={1}>
      <text fg="#7aa2f7"> skills</text>
      <box flexDirection="column" width={width - 2} height={viewportHeight}>
        {slice.map((node, i) => {
          const absoluteIdx = windowStart + i;
          const isSelected = absoluteIdx === selectedIdx;
          return (
            <TreeRow
              key={node.id}
              node={node}
              isSelected={isSelected}
              inventory={inventory}
              skillsById={skillsById}
              width={width - 2}
              expandedSkills={expandedSkills}
            />
          );
        })}
      </box>
    </box>
  );
}

function TreeRow({
  node,
  isSelected,
  inventory,
  skillsById,
  width,
  expandedSkills,
}: {
  node: VisibleNode;
  isSelected: boolean;
  inventory: Inventory;
  skillsById: Map<string, ReturnType<Inventory["skills"]["find"]> extends infer T ? T : Skill>;
  width: number;
  expandedSkills: Set<string>;
}) {
  const indent = "  ".repeat(node.depth);
  const bg = isSelected ? "#2a3144" : undefined;
  const fg = isSelected ? "#ffffff" : nodeColor(node);
  const label = renderLabel(node, inventory, skillsById, expandedSkills);
  const text = `${indent}${label}`;
  const truncated =
    text.length > width ? text.slice(0, Math.max(1, width - 1)) + "…" : text;
  return (
    <box flexDirection="row" width={width} backgroundColor={bg}>
      <text fg={fg}>{truncated}</text>
    </box>
  );
}

function nodeColor(node: TreeNode): string {
  switch (node.kind) {
    case "agent":
      return "#7aa2f7";
    case "root":
      return "#9ece6a";
    case "tier":
      return "#e0af68";
    case "skill":
      return "#c0caf5";
    case "reference":
      return "#5c6370";
  }
}

function renderLabel(
  node: TreeNode,
  inventory: Inventory,
  skillsById: Map<string, any>,
  expandedSkills: Set<string>,
): string {
  switch (node.kind) {
    case "agent":
      return `▾ ${node.agent}  (${node.childCount})`;
    case "root": {
      const r = node.root;
      const short = compactPath(r.path);
      return `▾ [${kindBadge(r.kind)}] ${short}  (${node.childCount})`;
    }
    case "tier":
      return `▾ ${node.tier}  (${node.childCount})`;
    case "skill": {
      const s = skillsById.get(node.skillId);
      if (!s) return "??";
      const isExpanded = expandedSkills.has(node.id);
      const refMarker =
        node.refCount > 0
          ? `${isExpanded ? "▾" : "▸"} • ${s.name}`
          : `· • ${s.name}`;
      const dupBadge = s.cluster_id
        ? (() => {
            const c = inventory.clusters.find((c) => c.id === s.cluster_id);
            if (!c) return "";
            const count = c.members.length;
            return c.kind === "exact"
              ? `  ⧉ exact (${count})`
              : `  ≈ ${(c.similarity * 100).toFixed(0)}% (${count})`;
          })()
        : "";
      const refBadge =
        node.refCount > 0 ? `  ⊕ ${node.refCount} ref${node.refCount === 1 ? "" : "s"}` : "";
      return `${refMarker}${refBadge}${usageBadge(s)}${dupBadge}`;
    }
    case "reference": {
      return `└─ ${node.relPath}  (${formatBytes(node.sizeBytes)})`;
    }
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}K`;
  return `${(n / 1024 / 1024).toFixed(1)}M`;
}

function compactPath(p: string): string {
  const home = process.env.HOME;
  if (home && p.startsWith(home)) return "~" + p.slice(home.length);
  return p;
}

function kindBadge(k: RootKind): string {
  switch (k) {
    case "claude-global":
      return "claude/global";
    case "claude-project":
      return "claude/project";
    case "codex":
      return "codex";
    case "cursor":
      return "cursor";
    case "agents-generic":
      return "agents";
    case "unknown":
      return "unknown";
  }
}

function indexOrLast(arr: string[], v: string): number {
  const i = arr.indexOf(v);
  return i === -1 ? arr.length : i;
}

function usageBadge(s: Skill): string {
  if (!s.usage || s.usage.confidence !== "high") return "";
  if (s.usage.mentions === 0) return "  ◌ unused";
  return `  ⏵ ${formatCount(s.usage.mentions)}`;
}

function formatCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10000) return `${(n / 1000).toFixed(1)}k`;
  return `${Math.floor(n / 1000)}k`;
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}
