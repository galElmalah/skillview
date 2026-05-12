import { mkdir, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import type { Inventory, Skill } from "./types";

export interface LoadOptions {
  binary: string;
  root?: string;
  threshold?: number;
  noSimilarity?: boolean;
}

export interface CachedInventory {
  inventory: Inventory;
  ageSeconds: number;
  path: string;
}

export function cachePath(): string {
  const xdg =
    process.env.SKILLVIEW_CACHE_DIR ??
    process.env.XDG_CACHE_HOME ??
    join(homedir(), ".cache");
  return join(xdg, "skillview", "inventory.json");
}

/**
 * Read cached inventory, if any. Invalidates the cache when the Rust binary
 * is newer than the cache (so we never serve stale results after a rebuild).
 */
export async function readCache(binary: string): Promise<CachedInventory | null> {
  if (process.env.SKILLVIEW_NO_CACHE === "1") return null;
  const path = cachePath();
  try {
    const [cacheStat, binStat] = await Promise.all([
      stat(path),
      stat(binary).catch(() => null),
    ]);
    if (binStat && binStat.mtimeMs > cacheStat.mtimeMs) return null;
    const data = await readFile(path, "utf8");
    const inventory = JSON.parse(data) as Inventory;
    if (inventory.schema_version !== 1) return null;
    const ageSeconds = Math.max(0, (Date.now() - cacheStat.mtimeMs) / 1000);
    return { inventory, ageSeconds, path };
  } catch {
    return null;
  }
}

export async function writeCache(inventory: Inventory): Promise<void> {
  const path = cachePath();
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, JSON.stringify(inventory), "utf8");
}

export interface DeleteSummary {
  fileCount: number;
  totalBytes: number;
}

/**
 * Recursively count files + total bytes under a directory. Used to show the
 * blast radius before a destructive operation.
 */
export async function summarizeDir(dirPath: string): Promise<DeleteSummary> {
  let fileCount = 0;
  let totalBytes = 0;
  async function walk(p: string) {
    let names: string[];
    try {
      names = await readdir(p);
    } catch {
      return;
    }
    for (const name of names) {
      const full = join(p, name);
      let s;
      try {
        s = await stat(full);
      } catch {
        continue;
      }
      if (s.isDirectory()) {
        await walk(full);
      } else if (s.isFile()) {
        fileCount += 1;
        totalBytes += s.size;
      }
    }
  }
  try {
    const root = await stat(dirPath);
    if (root.isFile()) return { fileCount: 1, totalBytes: root.size };
  } catch {
    /* fall through to walk */
  }
  await walk(dirPath);
  return { fileCount, totalBytes };
}

/**
 * Delete a skill. For primary skills (a SKILL.md sitting inside its own
 * folder), we delete the whole folder. For secondary skills (a loose .md
 * under a shared folder), we delete only the file.
 */
export async function deleteSkill(skill: Skill): Promise<{ removed: string }> {
  const target = chooseDeleteTarget(skill);
  // Defense in depth: refuse to delete anything outside HOME.
  const home = homedir();
  const resolved = resolve(target);
  if (!resolved.startsWith(home + "/") && resolved !== home) {
    throw new Error(`refusing to delete path outside $HOME: ${resolved}`);
  }
  await rm(resolved, { recursive: true, force: true });
  return { removed: resolved };
}

const contentCache = new Map<string, string>();
const MAX_CACHED_CONTENTS = 64;

/**
 * Read a skill's markdown body (frontmatter stripped). Cached in-memory so
 * repeated selection of the same skill is free. Returns "" if the file
 * can't be read.
 */
export async function readSkillContent(skill: Skill): Promise<string> {
  const cached = contentCache.get(skill.id);
  if (cached !== undefined) return cached;
  let body = "";
  try {
    const raw = await readFile(skill.path, "utf8");
    body = stripFrontmatter(raw);
  } catch {
    body = "";
  }
  // Evict oldest if cache too large.
  if (contentCache.size >= MAX_CACHED_CONTENTS) {
    const first = contentCache.keys().next().value;
    if (first) contentCache.delete(first);
  }
  contentCache.set(skill.id, body);
  return body;
}

function stripFrontmatter(raw: string): string {
  const r = raw.startsWith("﻿") ? raw.slice(1) : raw;
  if (!r.startsWith("---")) return raw;
  const rest = r.slice(3).replace(/^\r?\n/, "");
  const m = rest.match(/\n---\s*(\r?\n|$)/);
  if (!m || m.index === undefined) return raw;
  return rest.slice(m.index + m[0].length);
}

export function chooseDeleteTarget(skill: Skill): string {
  // Primary tier: delete the whole skill directory (SKILL.md + assets).
  // Secondary tier: delete only the markdown file itself, since loose .md
  // files often live alongside unrelated content under skills/agents/etc.
  if (skill.tier === "primary") return skill.dir;
  return skill.path;
}

export async function loadInventory(opts: LoadOptions): Promise<Inventory> {
  const args: string[] = [];
  if (opts.root) args.push("--root", opts.root);
  if (opts.threshold != null) args.push("--threshold", String(opts.threshold));
  if (opts.noSimilarity) args.push("--no-similarity");

  const proc = Bun.spawn([opts.binary, ...args], {
    stdout: "pipe",
    stderr: "pipe",
  });

  const [stdoutText, stderrText, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);

  if (code !== 0) {
    throw new Error(
      `skillview exited ${code}: ${stderrText.trim() || "<no stderr>"}`,
    );
  }

  try {
    return JSON.parse(stdoutText) as Inventory;
  } catch (err) {
    throw new Error(
      `skillview produced invalid JSON (${(err as Error).message}). First 200 chars: ${stdoutText.slice(0, 200)}`,
    );
  }
}

export function resolveBinary(): string {
  // The Rust binary sets SKILLVIEW_CORE to its own path when launching --tui,
  // so the TUI always re-uses the same process for re-scans.
  if (process.env.SKILLVIEW_CORE) return process.env.SKILLVIEW_CORE;
  if (process.env.SKILLVIEW_BIN) return process.env.SKILLVIEW_BIN;
  // Dev fallback: walk up from this file looking for a built binary.
  const candidates = [
    "../target/release/skillview",
    "../target/debug/skillview",
    "../../target/release/skillview",
    "../../target/debug/skillview",
  ];
  const here = new URL(".", import.meta.url).pathname;
  for (const rel of candidates) {
    const path = new URL(rel, "file://" + here).pathname;
    if (Bun.file(path).size > 0) return path;
  }
  return "skillview";
}
