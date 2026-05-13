use crate::model::SkillTier;
use ignore::{DirEntry, WalkBuilder, WalkState};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub tier: SkillTier,
}

const DENY: &[&str] = &[
    "Library",
    "Caches",
    "Movies",
    "Music",
    "Pictures",
    "Applications",
    "Public",
    "node_modules",
    ".git",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    "dist",
    "build",
    ".npm",
    ".cargo",
    ".rustup",
    ".Trash",
    ".DS_Store",
];

/// Directory names that genuinely hold skill-shaped content. A markdown file
/// under one of these is a candidate; a markdown file merely under `.claude/`
/// (e.g. `.claude/projects/.../memory/*.md`) is NOT.
const SKILL_PARENT_DIRS: &[&str] = &[
    "skills",
    "agents",
    "commands",
    "rules",
    "prompts",
    "bundled-skills",
    "optional-skills",
    "mcp-tools",
];

pub struct ScanOutcome {
    pub candidates: Vec<Candidate>,
    pub scanned_paths: usize,
}

pub fn scan(root: &Path) -> ScanOutcome {
    let candidates: Arc<Mutex<Vec<Candidate>>> = Arc::new(Mutex::new(Vec::with_capacity(256)));
    let counter = Arc::new(AtomicUsize::new(0));

    // NOTE: `standard_filters(true)` re-enables hidden-file filtering, so we
    // configure each flag explicitly and never touch it. `.claude`, `.codex`,
    // `.cursor`, `.agents` are all hidden directories — we MUST walk into them.
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .ignore(true)
        .require_git(false)
        .parents(false)
        .threads(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
        )
        .filter_entry(|e: &DirEntry| !is_denied(e))
        .build_parallel();

    walker.run(|| {
        let candidates = Arc::clone(&candidates);
        let counter = Arc::clone(&counter);
        Box::new(move |res| {
            counter.fetch_add(1, Ordering::Relaxed);
            let Ok(entry) = res else {
                return WalkState::Continue;
            };
            let Some(ft) = entry.file_type() else {
                return WalkState::Continue;
            };
            if !ft.is_file() {
                return WalkState::Continue;
            }
            if let Some(tier) = classify_candidate(entry.path()) {
                candidates.lock().unwrap().push(Candidate {
                    path: entry.path().to_path_buf(),
                    tier,
                });
            }
            WalkState::Continue
        })
    });

    let scanned_paths = counter.load(Ordering::Relaxed);
    let mut candidates = Arc::try_unwrap(candidates)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_else(|arc| arc.lock().unwrap().clone());

    // Sort by path so skill ids (`s_N`, assigned by enumerate order in emit)
    // are stable across runs against the same tree. Without this, parallel-walk
    // scheduling decides ordering and `skillview show s_3` is non-reproducible.
    candidates.sort_by(|a, b| a.path.cmp(&b.path));

    ScanOutcome {
        candidates,
        scanned_paths,
    }
}

fn is_denied(entry: &DirEntry) -> bool {
    let Some(name) = entry.file_name().to_str() else {
        return false;
    };
    DENY.iter().any(|d| *d == name)
}

fn classify_candidate(path: &Path) -> Option<SkillTier> {
    let original = path.file_name()?.to_str()?;
    if original == "SKILL.md" {
        return Some(SkillTier::Primary);
    }
    let lower = original.to_ascii_lowercase();
    if !lower.ends_with(".md") {
        return None;
    }
    let is_bare_skill_filename = matches!(lower.as_str(), "skill.md" | "skills.md");
    let under_collection = path.ancestors().skip(1).any(|a| {
        a.file_name()
            .and_then(|n| n.to_str())
            .map(|n| SKILL_PARENT_DIRS.iter().any(|d| *d == n))
            .unwrap_or(false)
    });
    if is_bare_skill_filename || under_collection {
        return Some(SkillTier::Secondary);
    }
    None
}
