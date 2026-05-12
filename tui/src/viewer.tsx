import React from "react";
import type { Skill } from "./types";

interface ViewerProps {
  width: number;
  height: number;
  skill: Skill;
  content: string;
  scrollLine: number;
}

export function Viewer({ width, height, skill, content, scrollLine }: ViewerProps) {
  const lines = content.split("\n");
  const viewportHeight = Math.max(1, height - 4);
  const start = clamp(scrollLine, 0, Math.max(0, lines.length - viewportHeight));
  const slice = lines.slice(start, start + viewportHeight);
  const end = Math.min(lines.length, start + viewportHeight);
  const pct = lines.length === 0
    ? 100
    : Math.round((end / lines.length) * 100);

  return (
    <box flexDirection="column" width={width} height={height}>
      <box
        flexDirection="row"
        height={1}
        paddingLeft={1}
        paddingRight={1}
        backgroundColor="#1f2430"
      >
        <text fg="#a3be8c">{skill.name}</text>
        <text fg="#5c6370">  ·  </text>
        <text fg="#5c6370">{shortenPath(skill.path)}</text>
        <text fg="#5c6370">  ·  </text>
        <text fg="#5c6370">
          line {start + 1}–{end} of {lines.length} ({pct}%)
        </text>
      </box>

      <box
        flexDirection="column"
        width={width}
        height={viewportHeight}
        paddingLeft={1}
        paddingRight={1}
      >
        {slice.map((line, i) => (
          <text key={i} fg={colorize(line)}>
            {truncate(line || " ", width - 2)}
          </text>
        ))}
      </box>

      <box
        flexDirection="row"
        height={1}
        paddingLeft={1}
        paddingRight={1}
        backgroundColor="#1f2430"
      >
        <text fg="#5c6370">
          ↑/↓ · j/k · pgup/pgdn · g/G top/bottom · esc/q close
        </text>
      </box>
    </box>
  );
}

function colorize(line: string): string {
  const t = line.trimStart();
  if (t.startsWith("# ")) return "#7aa2f7";
  if (t.startsWith("## ")) return "#9ece6a";
  if (t.startsWith("### ")) return "#e5c07b";
  if (t.startsWith("- ") || t.startsWith("* ")) return "#c0caf5";
  if (t.startsWith("```")) return "#5c6370";
  if (t.startsWith(">")) return "#5c6370";
  return "#c0caf5";
}

function truncate(s: string, w: number): string {
  if (s.length <= w) return s;
  return s.slice(0, Math.max(1, w - 1)) + "…";
}

function shortenPath(p: string): string {
  const home = process.env.HOME;
  if (home && p.startsWith(home)) return "~" + p.slice(home.length);
  return p;
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}
