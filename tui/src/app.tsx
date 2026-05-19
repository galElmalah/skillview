import React, { useEffect, useMemo, useRef, useState } from "react";
import { useKeyboard, useRenderer, useTerminalDimensions } from "@opentui/react";
import { Tree, buildTreeNodes, filterTree, flattenVisible } from "./tree";
import { Detail } from "./detail";
import { DeleteModal } from "./delete";
import { Viewer } from "./viewer";
import { Help } from "./help";
import { ClusterView } from "./cluster";
import { CommandBar } from "./commandbar";
import {
  CachedInventory,
  deleteSkill,
  foldStream,
  loadInventory,
  loadOptionsFromEnv,
  readSkillContent,
  streamInventory,
  summarizeDir,
  chooseDeleteTarget,
  writeCache,
} from "./bridge";
import type { Inventory, Skill, StreamEvent, StreamPhase } from "./types";

const STALE_AFTER_SECONDS = 24 * 60 * 60;
const SPINNER = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

export interface AppProps {
  binary: string;
  initial: CachedInventory | null;
}

type Mode = "browse" | "search" | "delete-confirm" | "viewer" | "help" | "cluster";

export function App({ binary, initial }: AppProps) {
  const [inventory, setInventory] = useState<Inventory | null>(
    initial?.inventory ?? null,
  );
  const [cacheAgeSeconds, setCacheAgeSeconds] = useState<number | null>(
    initial?.ageSeconds ?? null,
  );
  const [scanning, setScanning] = useState(!initial);
  const [scanStartedAt, setScanStartedAt] = useState<number | null>(
    initial ? null : Date.now(),
  );
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  // Live progress driven by streaming events. `phase` indicates which stage
  // the Rust side is in; `pathsSeen` / `skillsFound` are the latest counts.
  const [streamProgress, setStreamProgress] = useState<{
    phase: StreamPhase;
    pathsSeen: number;
    skillsFound: number;
    elapsedMs: number;
  } | null>(null);

  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  // Skill nodes default to *collapsed* (showing references is opt-in via enter).
  // This set holds the IDs of skill nodes the user has expanded.
  const [expandedSkills, setExpandedSkills] = useState<Set<string>>(
    () => new Set(),
  );
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [frame, setFrame] = useState(0);

  const [mode, setMode] = useState<Mode>("browse");
  const [query, setQuery] = useState("");
  const [deletePending, setDeletePending] = useState<{
    skill: Skill;
    target: string;
    summary: { fileCount: number; totalBytes: number } | null;
  } | null>(null);

  // Content preview state (inline + full-screen viewer).
  const [preview, setPreview] = useState<{
    skillId: string;
    content: string;
  } | null>(null);
  const [viewerScroll, setViewerScroll] = useState(0);

  // Cluster view state.
  const [clusterContext, setClusterContext] = useState<{
    clusterId: string;
    memberIdx: number;
  } | null>(null);

  // Resizable split: fraction of total width given to the left (tree) pane.
  const [leftWidthPct, setLeftWidthPct] = useState<number>(0.42);

  const { width, height } = useTerminalDimensions();
  const renderer = useRenderer();
  const rescanInflight = useRef(false);

  // Tear down the renderer before exiting so we restore the alt screen,
  // raw-mode flags, and any registered signal handlers. Calling
  // process.exit() directly skips those — which is why Ctrl-C used to leave
  // garbage streaming on stdout.
  function quit(code = 0): never {
    try {
      renderer.destroy();
    } catch {
      /* ignore — best effort */
    }
    process.exit(code);
  }

  // Spinner animation while scanning.
  useEffect(() => {
    if (!scanning) return;
    const id = setInterval(() => setFrame((f) => f + 1), 90);
    return () => clearInterval(id);
  }, [scanning]);

  // Cold start: scan immediately. Stale cache: scan in background.
  useEffect(() => {
    if (!initial) {
      void rescan();
    } else if (initial.ageSeconds > STALE_AFTER_SECONDS) {
      void rescan({ silent: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function rescan(opts: { silent?: boolean } = {}) {
    if (rescanInflight.current) return;
    rescanInflight.current = true;
    setScanning(true);
    if (!opts.silent) setScanStartedAt(Date.now());
    setError(null);
    setStreamProgress(null);

    // We still spawn the streaming binary so the loader can tick a live
    // "scanned N paths · found M skills" counter while work is in flight,
    // but every non-progress event is just accumulated. The inventory only
    // hits React state once on `done` — built by folding the captured
    // events. The earlier live-merge path tried to push partial inventories
    // into state on every skill/cluster/usage event, which produced trees
    // that sometimes showed a stale subset and never recovered.
    const captured: StreamEvent[] = [];

    try {
      await streamInventory({ binary, ...loadOptionsFromEnv() }, {
        onEvent: (ev) => {
          if (ev.event === "progress") {
            setStreamProgress({
              phase: ev.phase,
              pathsSeen: ev.paths_seen,
              skillsFound: ev.skills_found,
              elapsedMs: ev.elapsed_ms,
            });
            return;
          }
          captured.push(ev);
          if (ev.event === "done") {
            const inv = foldStream(captured);
            setInventory(inv);
            setCacheAgeSeconds(0);
            void writeCache(inv);
          }
        },
      });
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setScanning(false);
      setScanStartedAt(null);
      setStreamProgress(null);
      rescanInflight.current = false;
    }
  }

  // `loadInventory` is the non-streaming entry point — kept exported for
  // tests / future CLI tools that don't need progress ticks. Silence the
  // unused-import lint without ripping it out.
  void loadInventory;

  const tree = useMemo(
    () => (inventory ? buildTreeNodes(inventory) : []),
    [inventory],
  );

  // Apply search filter: returns the tree pruned to matching skills.
  const filteredTree = useMemo(
    () => (inventory ? filterTree(tree, inventory, query) : []),
    [tree, inventory, query],
  );

  const visible = useMemo(
    () =>
      flattenVisible(
        filteredTree,
        query.trim() ? new Set() : collapsed,
        expandedSkills,
      ),
    [filteredTree, collapsed, expandedSkills, query],
  );

  // Clamp selection if it ran off the end after a filter change.
  useEffect(() => {
    if (selectedIdx >= visible.length && visible.length > 0) {
      setSelectedIdx(visible.length - 1);
    } else if (visible.length === 0) {
      setSelectedIdx(0);
    }
  }, [visible.length, selectedIdx]);

  const selected = visible[Math.min(selectedIdx, visible.length - 1)];
  // Reference rows still drive the right pane via their parent skill, so the
  // user sees consistent detail while navigating into refs.
  const selectedSkillId =
    selected?.kind === "skill" || selected?.kind === "reference"
      ? selected.skillId
      : undefined;
  const selectedSkill =
    inventory && selectedSkillId
      ? inventory.skills.find((s) => s.id === selectedSkillId)
      : undefined;

  // Lazily load (and cache) the content for the currently-selected skill so
  // the detail pane can show an inline preview.
  useEffect(() => {
    if (!selectedSkill) {
      setPreview(null);
      return;
    }
    if (preview && preview.skillId === selectedSkill.id) return;
    let cancelled = false;
    void readSkillContent(selectedSkill).then((content) => {
      if (!cancelled) setPreview({ skillId: selectedSkill.id, content });
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedSkill?.id]);

  const MIN_LEFT_PCT = 0.18;
  const MAX_LEFT_PCT = 0.82;
  const RESIZE_STEP = 0.04;
  const leftWidth = Math.max(20, Math.min(width - 20, Math.floor(width * leftWidthPct)));
  const rightWidth = width - leftWidth - 1;
  // Chrome rows: command bar (1) + stats header (1) + footer (1) = 3
  const viewportHeight = Math.max(1, height - 3);
  const pageSize = Math.max(1, viewportHeight - 2);

  function moveSelection(delta: number) {
    setSelectedIdx((i) => clamp(i + delta, 0, Math.max(0, visible.length - 1)));
  }

  function jumpTo(absoluteIdx: number) {
    setSelectedIdx(clamp(absoluteIdx, 0, Math.max(0, visible.length - 1)));
  }

  async function startDelete(skill: Skill) {
    const target = chooseDeleteTarget(skill);
    setDeletePending({ skill, target, summary: null });
    setMode("delete-confirm");
    // Compute blast radius asynchronously and update.
    try {
      const summary = await summarizeDir(target);
      setDeletePending((cur) =>
        cur && cur.skill.id === skill.id ? { ...cur, summary } : cur,
      );
    } catch {
      /* ignore */
    }
  }

  async function commitDelete() {
    if (!deletePending || !inventory) return;
    const { skill } = deletePending;
    try {
      const res = await deleteSkill(skill);
      // Locally drop the skill from inventory + persist cache.
      const next: Inventory = {
        ...inventory,
        skills: inventory.skills.filter((s) => s.id !== skill.id),
      };
      // Also drop the cluster if it's now down to a single member.
      next.clusters = inventory.clusters
        .map((c) => ({
          ...c,
          members: c.members.filter((m) => m !== skill.id),
        }))
        .filter((c) => c.members.length >= 2);
      next.stats = {
        ...next.stats,
        primary_skills: next.skills.filter((s) => s.tier === "primary").length,
        secondary_skills: next.skills.filter((s) => s.tier === "secondary")
          .length,
        duplicate_clusters: next.clusters.length,
      };
      setInventory(next);
      void writeCache(next);
      setStatusMsg(`deleted ${shortenPath(res.removed)}`);
    } catch (err) {
      setStatusMsg(`delete failed: ${(err as Error).message}`);
    } finally {
      setDeletePending(null);
      setMode("browse");
    }
  }

  const viewerLineCount = preview?.content
    ? preview.content.split("\n").length
    : 0;
  const viewerPage = Math.max(1, height - 6);

  const activeCluster =
    clusterContext && inventory
      ? inventory.clusters.find((c) => c.id === clusterContext.clusterId) ??
        null
      : null;
  const clusterMembers = activeCluster && inventory
    ? activeCluster.members
        .map((id) => inventory.skills.find((s) => s.id === id))
        .filter((s): s is Skill => !!s)
    : [];

  function focusSkillInTree(skillId: string) {
    if (!inventory) return;
    // Expand all ancestors of the target skill node and select it.
    const nodeId = `s:${skillId}`;
    const target = tree.find((n) => n.id === nodeId);
    if (!target) return;
    setCollapsed((prev) => {
      const next = new Set(prev);
      let pid: string | undefined =
        "parentId" in target ? target.parentId : undefined;
      while (pid) {
        next.delete(pid);
        const parent = tree.find((n) => n.id === pid);
        pid =
          parent && parent.kind !== "agent" && "parentId" in parent
            ? parent.parentId
            : undefined;
      }
      return next;
    });
    // Selection updates next render via the useEffect that watches `visible`.
    setTimeout(() => {
      const flat = flattenVisible(tree, collapsed, expandedSkills);
      const idx = flat.findIndex((n) => n.id === nodeId);
      if (idx >= 0) setSelectedIdx(idx);
    }, 0);
  }

  useKeyboard((key) => {
    if (mode === "help") {
      if (key.name === "escape" || key.name === "q") {
        setMode("browse");
      }
      return;
    }

    if (mode === "cluster") {
      if (!activeCluster) {
        setMode("browse");
        return;
      }
      const maxIdx = Math.max(0, clusterMembers.length - 1);
      if (key.name === "escape" || key.name === "q") {
        setMode("browse");
      } else if (key.name === "up" || key.name === "k") {
        setClusterContext((c) =>
          c ? { ...c, memberIdx: Math.max(0, c.memberIdx - 1) } : c,
        );
      } else if (key.name === "down" || key.name === "j") {
        setClusterContext((c) =>
          c ? { ...c, memberIdx: Math.min(maxIdx, c.memberIdx + 1) } : c,
        );
      } else if (key.name === "return") {
        const member = clusterMembers[clusterContext!.memberIdx];
        if (member) {
          focusSkillInTree(member.id);
          setMode("browse");
        }
      } else if (key.name === "v") {
        const member = clusterMembers[clusterContext!.memberIdx];
        if (member) {
          setViewerScroll(0);
          // Make sure the viewer renders this member's content even if it's
          // not the tree-selected one.
          focusSkillInTree(member.id);
          setMode("viewer");
        }
      } else if (key.name === "d") {
        const member = clusterMembers[clusterContext!.memberIdx];
        if (member) {
          void startDelete(member);
        }
      } else if (key.sequence === "?") {
        setMode("help");
      }
      return;
    }

    if (mode === "viewer") {
      if (key.name === "escape" || key.name === "q") {
        setMode("browse");
        return;
      }
      if (key.name === "up" || key.name === "k") {
        setViewerScroll((s) => Math.max(0, s - 1));
      } else if (key.name === "down" || key.name === "j") {
        setViewerScroll((s) =>
          Math.min(Math.max(0, viewerLineCount - 1), s + 1),
        );
      } else if (key.name === "pageup") {
        setViewerScroll((s) => Math.max(0, s - viewerPage));
      } else if (key.name === "pagedown" || key.sequence === " ") {
        setViewerScroll((s) =>
          Math.min(Math.max(0, viewerLineCount - 1), s + viewerPage),
        );
      } else if (key.name === "home" || key.sequence === "g") {
        setViewerScroll(0);
      } else if (key.name === "end" || key.sequence === "G") {
        setViewerScroll(Math.max(0, viewerLineCount - 1));
      }
      return;
    }

    if (mode === "delete-confirm") {
      if (key.name === "return" || key.name === "y") {
        void commitDelete();
      } else if (key.name === "escape" || key.name === "n" || key.name === "q") {
        setDeletePending(null);
        setMode("browse");
      }
      return;
    }

    if (mode === "search") {
      if (key.name === "escape") {
        setQuery("");
        setMode("browse");
      } else if (key.name === "return") {
        setMode("browse");
      } else if (key.name === "backspace") {
        setQuery((q) => q.slice(0, -1));
      } else if (key.name === "up") {
        moveSelection(-1);
      } else if (key.name === "down") {
        moveSelection(1);
      } else if (key.sequence && key.sequence.length === 1 && !key.ctrl && !key.meta) {
        // Printable character — append.
        const ch = key.sequence;
        if (ch >= " " && ch <= "~") {
          setQuery((q) => q + ch);
        }
      }
      return;
    }

    // browse mode
    if (key.name === "q" || (key.ctrl && key.name === "c")) {
      quit();
    }
    if (!inventory) return;
    if (key.sequence === "/") {
      setMode("search");
      setStatusMsg(null);
    } else if (key.name === "up" || key.name === "k") {
      moveSelection(-1);
    } else if (key.name === "down" || key.name === "j") {
      moveSelection(1);
    } else if (key.name === "pageup") {
      moveSelection(-pageSize);
    } else if (key.name === "pagedown" || key.sequence === " ") {
      moveSelection(pageSize);
    } else if (key.name === "home" || key.sequence === "g") {
      jumpTo(0);
    } else if (key.name === "end" || key.sequence === "G") {
      jumpTo(visible.length - 1);
    } else if (key.name === "right" || key.name === "l") {
      if (selected?.kind === "skill" && selected.refCount > 0) {
        setExpandedSkills((prev) => {
          const next = new Set(prev);
          next.add(selected.id);
          return next;
        });
      } else if (selected && selected.kind !== "skill" && selected.kind !== "reference") {
        setCollapsed((prev) => {
          const next = new Set(prev);
          next.delete(selected.id);
          return next;
        });
      }
    } else if (key.name === "left" || key.name === "h") {
      if (selected?.kind === "skill") {
        // Collapse the skill if expanded; otherwise no-op (could jump to parent
        // later if there's demand).
        setExpandedSkills((prev) => {
          if (!prev.has(selected.id)) return prev;
          const next = new Set(prev);
          next.delete(selected.id);
          return next;
        });
      } else if (selected?.kind === "reference") {
        // From a reference row, left arrow collapses the parent skill.
        setExpandedSkills((prev) => {
          const next = new Set(prev);
          next.delete(selected.parentId);
          return next;
        });
      } else if (selected) {
        // agent/root/tier
        setCollapsed((prev) => {
          const next = new Set(prev);
          next.add(selected.id);
          return next;
        });
      }
    } else if (key.name === "return") {
      if (selected?.kind === "skill") {
        if (selected.refCount === 0) {
          setStatusMsg("this skill has no referenced resources");
        } else {
          setExpandedSkills((prev) => {
            const next = new Set(prev);
            if (next.has(selected.id)) next.delete(selected.id);
            else next.add(selected.id);
            return next;
          });
        }
      } else if (selected && selected.kind !== "reference") {
        setCollapsed((prev) => {
          const next = new Set(prev);
          if (next.has(selected.id)) next.delete(selected.id);
          else next.add(selected.id);
          return next;
        });
      }
    } else if (key.name === "r") {
      void rescan();
    } else if (key.name === "d") {
      if (selected?.kind === "reference") {
        setStatusMsg("move to the parent skill row to delete");
      } else if (selectedSkill) {
        setStatusMsg(null);
        void startDelete(selectedSkill);
      }
    } else if (key.name === "v" || (key.sequence === "v")) {
      if (selectedSkill) {
        setViewerScroll(0);
        setMode("viewer");
      }
    } else if (key.name === "c" || key.sequence === "c") {
      if (selectedSkill?.cluster_id) {
        setClusterContext({ clusterId: selectedSkill.cluster_id, memberIdx: 0 });
        setMode("cluster");
        setStatusMsg(null);
      } else if (selectedSkill) {
        setStatusMsg("no duplicate cluster for this skill");
      }
    } else if (key.sequence === "?") {
      setMode("help");
    } else if (key.sequence === "[") {
      setLeftWidthPct((p) =>
        clamp(p - RESIZE_STEP, MIN_LEFT_PCT, MAX_LEFT_PCT),
      );
      setStatusMsg(null);
    } else if (key.sequence === "]") {
      setLeftWidthPct((p) =>
        clamp(p + RESIZE_STEP, MIN_LEFT_PCT, MAX_LEFT_PCT),
      );
      setStatusMsg(null);
    } else if (key.sequence === "=" || key.sequence === "0") {
      setLeftWidthPct(0.42);
      setStatusMsg("split reset");
    }
  });

  // Cold start loader.
  if (!inventory) {
    return (
      <Loader
        width={width}
        height={height}
        frame={frame}
        startedAt={scanStartedAt}
        error={error}
        progress={streamProgress}
      />
    );
  }

  // Help overlay (full-screen).
  if (mode === "help") {
    return <Help width={width} height={height} />;
  }

  // Cluster view (full-screen).
  if (mode === "cluster" && activeCluster) {
    return (
      <ClusterView
        width={width}
        height={height}
        inventory={inventory}
        cluster={activeCluster}
        selectedIdx={clusterContext?.memberIdx ?? 0}
      />
    );
  }

  // Full-screen content viewer.
  if (mode === "viewer" && selectedSkill && preview && preview.skillId === selectedSkill.id) {
    return (
      <Viewer
        width={width}
        height={height}
        skill={selectedSkill}
        content={preview.content || "(empty or unreadable file)"}
        scrollLine={viewerScroll}
      />
    );
  }

  const cacheLabel = formatCacheLabel(
    cacheAgeSeconds,
    scanning,
    frame,
    streamProgress,
  );
  const previewContent =
    preview && selectedSkill && preview.skillId === selectedSkill.id
      ? preview.content
      : null;

  return (
    <box flexDirection="column" width={width} height={height}>
      <CommandBar width={width} mode={mode} />
      <Header inventory={inventory} cacheLabel={cacheLabel} />

      <box flexDirection="row" width={width} height={viewportHeight}>
        <Tree
          width={leftWidth}
          height={viewportHeight}
          visible={visible}
          selectedIdx={Math.min(selectedIdx, visible.length - 1)}
          inventory={inventory}
          expandedSkills={expandedSkills}
        />
        <box width={1} height={viewportHeight} backgroundColor="#1a1d24" />
        <Detail
          width={rightWidth}
          height={viewportHeight}
          inventory={inventory}
          skill={selectedSkill}
          node={selected}
          error={error}
          previewContent={previewContent}
        />
      </box>

      <Footer
        mode={mode}
        query={query}
        statusMsg={statusMsg}
        visibleCount={visible.length}
        totalSkills={inventory.skills.length}
      />

      {mode === "delete-confirm" && deletePending ? (
        <DeleteModal
          width={width}
          height={height}
          skill={deletePending.skill}
          target={deletePending.target}
          summary={deletePending.summary}
        />
      ) : null}
    </box>
  );
}

function Header({
  inventory,
  cacheLabel,
}: {
  inventory: Inventory;
  cacheLabel: string;
}) {
  return (
    <box
      flexDirection="row"
      height={1}
      paddingLeft={1}
      paddingRight={1}
      backgroundColor="#1f2430"
    >
      <text fg="#a3be8c">skillview</text>
      <text fg="#5c6370">  •  </text>
      <text fg="#c0caf5">{inventory.stats.primary_skills} primary</text>
      <text fg="#5c6370">  ·  </text>
      <text fg="#c0caf5">{inventory.stats.secondary_skills} secondary</text>
      <text fg="#5c6370">  ·  </text>
      <text fg="#e5c07b">{inventory.stats.duplicate_clusters} dup clusters</text>
      <text fg="#5c6370">  ·  </text>
      <text fg="#5c6370">{cacheLabel}</text>
    </box>
  );
}

function Footer({
  mode,
  query,
  statusMsg,
  visibleCount,
  totalSkills,
}: {
  mode: Mode;
  query: string;
  statusMsg: string | null;
  visibleCount: number;
  totalSkills: number;
}) {
  if (mode === "search") {
    return (
      <box
        flexDirection="row"
        height={1}
        paddingLeft={1}
        paddingRight={1}
        backgroundColor="#2a3144"
      >
        <text fg="#7aa2f7">/ </text>
        <text fg="#ffffff">{query || " "}</text>
        <text fg="#5c6370">▌  </text>
        <text fg="#5c6370">
          {visibleCount} match · enter to select · esc to clear
        </text>
      </box>
    );
  }
  return (
    <box
      flexDirection="row"
      height={1}
      paddingLeft={1}
      paddingRight={1}
      backgroundColor="#1f2430"
    >
      <text fg="#5c6370">
        ↑/↓ · enter expand refs · ←/→ · v view · c cluster · / search · d delete · ? help · r rescan · q quit
      </text>
      {statusMsg ? (
        <>
          <text fg="#5c6370">  ·  </text>
          <text fg="#9ece6a">{statusMsg}</text>
        </>
      ) : null}
      {query ? (
        <>
          <text fg="#5c6370">  ·  </text>
          <text fg="#7aa2f7">
            /{query} ({visibleCount}/{totalSkills})
          </text>
        </>
      ) : null}
    </box>
  );
}

interface LoaderProps {
  width: number;
  height: number;
  frame: number;
  startedAt: number | null;
  error: string | null;
  progress: {
    phase: StreamPhase;
    pathsSeen: number;
    skillsFound: number;
    elapsedMs: number;
  } | null;
}

function Loader({
  width,
  height,
  frame,
  startedAt,
  error,
  progress,
}: LoaderProps) {
  const ch = SPINNER[frame % SPINNER.length];
  const elapsed = startedAt
    ? Math.max(0, Math.floor((Date.now() - startedAt) / 1000))
    : 0;
  return (
    <box
      flexDirection="column"
      width={width}
      height={height}
      alignItems="center"
      justifyContent="center"
    >
      <text fg="#7aa2f7">
        {ch}  skillview
      </text>
      <text> </text>
      {error ? (
        <>
          <text fg="#f7768e">scan failed: {error}</text>
          <text fg="#5c6370">press q to quit</text>
        </>
      ) : (
        <>
          <text fg="#c0caf5">
            scanning your home directory for skills…  ({elapsed}s)
          </text>
          <text fg="#7aa2f7">
            {progress
              ? `${formatCount(progress.pathsSeen)} paths walked · ${progress.skillsFound} skills found`
              : "starting…"}
          </text>
          <text fg="#5c6370">
            first run; future runs read from ~/.cache/skillview/inventory.json
          </text>
        </>
      )}
    </box>
  );
}

function formatCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

function formatCacheLabel(
  ageSeconds: number | null,
  scanning: boolean,
  frame: number,
  progress: {
    phase: StreamPhase;
    pathsSeen: number;
    skillsFound: number;
    elapsedMs: number;
  } | null,
): string {
  if (scanning) {
    const ch = SPINNER[frame % SPINNER.length];
    if (progress) {
      return `${ch} ${formatCount(progress.pathsSeen)} paths · ${progress.skillsFound} skills`;
    }
    return `${ch} rescanning…`;
  }
  if (ageSeconds == null) return "no cache";
  if (ageSeconds < 5) return "just scanned";
  if (ageSeconds < 60) return `cached ${Math.floor(ageSeconds)}s ago`;
  if (ageSeconds < 3600) return `cached ${Math.floor(ageSeconds / 60)}m ago`;
  if (ageSeconds < 86400) return `cached ${Math.floor(ageSeconds / 3600)}h ago`;
  return `cached ${Math.floor(ageSeconds / 86400)}d ago`;
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}

function shortenPath(p: string): string {
  const home = process.env.HOME;
  if (home && p.startsWith(home)) return "~" + p.slice(home.length);
  return p;
}
