import React from "react";
import type { Skill } from "./types";

interface DeleteModalProps {
  width: number;
  height: number;
  skill: Skill;
  target: string;
  summary: { fileCount: number; totalBytes: number } | null;
}

export function DeleteModal({
  width,
  height,
  skill,
  target,
  summary,
}: DeleteModalProps) {
  const modalW = Math.min(80, width - 8);
  const modalH = 12;
  return (
    <box
      position="absolute"
      left={Math.max(0, Math.floor((width - modalW) / 2))}
      top={Math.max(0, Math.floor((height - modalH) / 2))}
      width={modalW}
      height={modalH}
      flexDirection="column"
      backgroundColor="#1a1d24"
      border
      borderStyle="double"
      borderColor="#f7768e"
      paddingLeft={2}
      paddingRight={2}
      paddingTop={1}
    >
      <text fg="#f7768e">delete this skill?</text>
      <text> </text>
      <text fg="#c0caf5">name:   {skill.name}</text>
      <text fg="#c0caf5">agent:  {skill.agent}</text>
      <text fg="#c0caf5">tier:   {skill.tier}</text>
      <text fg="#c0caf5">target: {shortenPath(target)}</text>
      {skill.tier === "primary" ? (
        <text fg="#5c6370">
          (whole skill directory will be removed{" "}
          {summary
            ? `— ${summary.fileCount} files, ${formatBytes(summary.totalBytes)}`
            : "— computing size…"})
        </text>
      ) : (
        <text fg="#5c6370">(only this .md file will be removed)</text>
      )}
      <text> </text>
      <text fg="#e5c07b">
        [enter / y] confirm    [esc / n] cancel
      </text>
    </box>
  );
}

function shortenPath(p: string): string {
  const home = process.env.HOME;
  if (home && p.startsWith(home)) return "~" + p.slice(home.length);
  return p;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}K`;
  return `${(n / 1024 / 1024).toFixed(1)}M`;
}
