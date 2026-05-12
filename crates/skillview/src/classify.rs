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
