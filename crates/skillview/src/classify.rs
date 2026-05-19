use crate::model::RootKind;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RootMatch {
    pub kind: RootKind,
    pub root_dir: PathBuf,
    pub agent: String,
}

pub fn classify_root(skill_path: &Path, home: &Path) -> RootMatch {
    for ancestor in skill_path.ancestors() {
        let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        match name {
            ".claude" => {
                let kind = if ancestor.parent() == Some(home) {
                    RootKind::ClaudeGlobal
                } else {
                    RootKind::ClaudeProject
                };
                return RootMatch {
                    kind,
                    root_dir: ancestor.to_path_buf(),
                    agent: "claude".into(),
                };
            }
            ".codex" => {
                return RootMatch {
                    kind: RootKind::Codex,
                    root_dir: ancestor.to_path_buf(),
                    agent: "codex".into(),
                }
            }
            ".cursor" => {
                return RootMatch {
                    kind: RootKind::Cursor,
                    root_dir: ancestor.to_path_buf(),
                    agent: "cursor".into(),
                }
            }
            ".agents" => {
                return RootMatch {
                    kind: RootKind::AgentsGeneric,
                    root_dir: ancestor.to_path_buf(),
                    agent: "agents".into(),
                }
            }
            _ => continue,
        }
    }

    // Primary fallback: derive the agent + root from the user's home-relative
    // namespace. This catches third-party agent installs (e.g. ~/.hermes,
    // ~/.accomplish), project-local skills (~/some-repo/skills/...), and
    // runtime caches (~/.cache/codex-runtimes/...) — anything that doesn't
    // wear a `.claude`/`.codex`/`.cursor`/`.agents` hat but still has a
    // stable on-disk namespace. Without this, all 500+ such skills got
    // bucketed as agent="unknown" with no way to distinguish them.
    if let Some((agent, root_dir)) = derive_namespace_root(skill_path, home) {
        return RootMatch {
            kind: RootKind::Unknown,
            root_dir,
            agent,
        };
    }

    // Secondary fallback: still no namespace match (e.g. skill outside $HOME,
    // like /opt/foo/skills/...). Walk up to the nearest skills-collection
    // directory and treat its parent as the root.
    const COLLECTION_DIRS: &[&str] = &[
        "skills",
        "bundled-skills",
        "optional-skills",
        "mcp-tools",
        "agents",
        "commands",
        "rules",
        "prompts",
    ];
    for ancestor in skill_path.ancestors() {
        if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
            if COLLECTION_DIRS.iter().any(|d| *d == name) {
                let parent = ancestor
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| ancestor.to_path_buf());
                return RootMatch {
                    kind: RootKind::Unknown,
                    root_dir: parent,
                    agent: "unknown".into(),
                };
            }
        }
    }

    RootMatch {
        kind: RootKind::Unknown,
        root_dir: skill_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default(),
        agent: "unknown".into(),
    }
}

/// Generic OS / well-known parent directories whose names are NOT meaningful
/// agent identifiers. When a SKILL lives under one of these, the real
/// namespace is one level deeper — e.g. `~/.cache/codex-runtimes/...` should
/// surface "codex-runtimes" rather than ".cache".
const NAMESPACE_NOISE: &[&str] = &[
    ".cache",
    ".config",
    ".local",
    ".npm",
    ".cargo",
    ".rustup",
    "Library",
    "Desktop",
    "Documents",
    "Downloads",
    "OrbStack",
];

/// Walk a SKILL path's components and pick the first non-noise component
/// under `$HOME` as its namespace. Returns `(agent_name, root_dir)`. The
/// agent name strips any leading '.' so `.hermes` → `hermes`. Returns
/// `None` if the path is not under `$HOME` — callers fall back to the
/// collection-dir heuristic in that case.
fn derive_namespace_root(skill_path: &Path, home: &Path) -> Option<(String, PathBuf)> {
    let rel = skill_path.strip_prefix(home).ok()?;
    let mut root = home.to_path_buf();
    let mut found_name: Option<String> = None;

    for component in rel.components() {
        let raw = component.as_os_str().to_str()?;
        root.push(raw);
        if NAMESPACE_NOISE.iter().any(|n| *n == raw) {
            // Generic system / category dir — keep walking until we hit
            // something more specific.
            continue;
        }
        found_name = Some(raw.trim_start_matches('.').to_string());
        break;
    }

    found_name.map(|name| (name, root))
}
