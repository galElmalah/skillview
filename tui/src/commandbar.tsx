import React from "react";

type Mode = "browse" | "search" | "delete-confirm" | "viewer" | "help" | "cluster";

interface CommandBarProps {
  width: number;
  mode: Mode;
}

interface Cmd {
  key: string;
  label: string;
  modes: Mode[];
}

const COMMANDS: Cmd[] = [
  { key: "/", label: "search", modes: ["browse"] },
  { key: "v", label: "view", modes: ["browse"] },
  { key: "c", label: "cluster", modes: ["browse"] },
  { key: "d", label: "delete", modes: ["browse", "cluster"] },
  { key: "[ ]", label: "resize", modes: ["browse"] },
  { key: "r", label: "rescan", modes: ["browse"] },
  { key: "?", label: "help", modes: ["browse", "cluster", "viewer"] },
  { key: "q", label: "quit", modes: ["browse", "cluster", "viewer", "help"] },
  { key: "esc", label: "back", modes: ["search", "cluster", "viewer", "help", "delete-confirm"] },
];

export function CommandBar({ width, mode }: CommandBarProps) {
  const visible = COMMANDS.filter((c) => c.modes.includes(mode));
  return (
    <box
      flexDirection="row"
      width={width}
      height={1}
      paddingLeft={1}
      paddingRight={1}
      backgroundColor="#0f1117"
    >
      {visible.map((c, i) => (
        <React.Fragment key={c.key}>
          {i > 0 ? <text fg="#3b4252">  │  </text> : null}
          <text fg="#e5c07b">[{c.key}]</text>
          <text fg="#c0caf5"> {c.label}</text>
        </React.Fragment>
      ))}
    </box>
  );
}
