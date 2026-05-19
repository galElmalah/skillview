use crate::model::SkillTier;
use ignore::{DirEntry, WalkBuilder, WalkState};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
    scan_with_ticker(root, |_, _| {})
}

/// Walk `root` and collect skill candidates, calling `ticker(paths_seen,
/// candidates_so_far)` periodically (~every 120ms) from a background thread
/// while the walk is in flight. The ticker is called from the **calling**
/// thread, so the closure does not need to be `Send`.
///
/// Used by streaming callers (e.g. `--stream`, the TUI) to surface "we're
/// still walking, here's the count" UI before the walk completes. The walker
/// itself runs on a worker thread so this function can drain the progress
/// channel.
pub fn scan_with_ticker<F: FnMut(usize, usize)>(root: &Path, mut ticker: F) -> ScanOutcome {
    let counter = Arc::new(AtomicUsize::new(0));
    let hits = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));

    let walk_counter = Arc::clone(&counter);
    let walk_hits = Arc::clone(&hits);
    let walk_done = Arc::clone(&done);
    let root_buf = root.to_path_buf();
    let walk_handle = thread::spawn(move || {
        let candidates = run_walk(&root_buf, walk_counter, walk_hits);
        walk_done.store(true, Ordering::Relaxed);
        candidates
    });

    // Drive ticker from this thread until the walker finishes. Sleeping a
    // little less than the human-noticeable boundary (~150ms) so the UI feels
    // alive without flooding stdout.
    let tick = Duration::from_millis(120);
    while !done.load(Ordering::Relaxed) {
        thread::sleep(tick);
        if done.load(Ordering::Relaxed) {
            break;
        }
        ticker(
            counter.load(Ordering::Relaxed),
            hits.load(Ordering::Relaxed),
        );
    }

    let mut candidates = walk_handle.join().unwrap_or_default();
    // Sort by path so skill ids (`s_N`, assigned by enumerate order in emit)
    // are stable across runs against the same tree. Without this, parallel-walk
    // scheduling decides ordering and `skillview show s_3` is non-reproducible.
    candidates.sort_by(|a, b| a.path.cmp(&b.path));

    let scanned_paths = counter.load(Ordering::Relaxed);
    // One final tick with the post-sort totals so the consumer sees the
    // closing number even if the walk finished faster than the tick interval.
    ticker(scanned_paths, candidates.len());

    ScanOutcome {
        candidates,
        scanned_paths,
    }
}

fn run_walk(
    root: &Path,
    counter: Arc<AtomicUsize>,
    hits: Arc<AtomicUsize>,
) -> Vec<Candidate> {
    let candidates: Arc<Mutex<Vec<Candidate>>> = Arc::new(Mutex::new(Vec::with_capacity(256)));

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
        let hits = Arc::clone(&hits);
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
                hits.fetch_add(1, Ordering::Relaxed);
            }
            WalkState::Continue
        })
    });

    Arc::try_unwrap(candidates)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_else(|arc| arc.lock().unwrap().clone())
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
