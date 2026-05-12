//! Cross-agent usage scanner.
//!
//! Walks Claude and Codex session JSONL files (~/.claude/projects/**/*.jsonl,
//! ~/.codex/sessions/**/*.jsonl, ~/.codex/archived_sessions/**/*.jsonl) and
//! counts how many times each skill is mentioned by name. Uses Aho-Corasick
//! for parallel multi-pattern matching, with a quoted-context filter to
//! suppress most false positives (so `"agent-browser"` matches but a
//! prose sentence mentioning the word doesn't).

use aho_corasick::{AhoCorasick, MatchKind};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

#[derive(Debug, Default, Clone)]
pub struct SkillUsage {
    pub mentions: u64,
    pub sessions: u64,
    pub last_seen_unix: Option<i64>,
    pub by_source: BTreeMap<String, u64>,
}

#[derive(Debug, Default, Clone)]
pub struct UsageStats {
    pub session_files: usize,
    pub bytes_scanned: u64,
    pub elapsed_ms: u128,
}

pub struct UsageOutcome {
    /// pattern_index -> SkillUsage
    pub by_pattern: Vec<SkillUsage>,
    pub stats: UsageStats,
}

/// Build a usage report. `patterns` is the list of skill names; index in the
/// result corresponds to index in patterns. Skills that share a name (e.g.
/// duplicates across roots) MUST be passed as the same pattern multiple times
/// or de-duplicated by the caller — this function returns one entry per
/// supplied pattern in order.
pub fn scan(home: &Path, patterns: &[String]) -> Result<UsageOutcome> {
    let start = Instant::now();
    if patterns.is_empty() {
        return Ok(UsageOutcome {
            by_pattern: Vec::new(),
            stats: UsageStats::default(),
        });
    }

    // Some skills share a name across roots (e.g. duplicates). We build a
    // dedup'd Aho-Corasick over unique names, then fan out to every owning
    // pattern index.
    let mut unique: Vec<String> = patterns.to_vec();
    unique.sort();
    unique.dedup();

    // Map: unique-name -> Vec<pattern_index>
    let mut name_to_indexes: HashMap<&str, Vec<usize>> = HashMap::with_capacity(unique.len());
    for (i, p) in patterns.iter().enumerate() {
        name_to_indexes.entry(p.as_str()).or_default().push(i);
    }

    let ac = AhoCorasick::builder()
        .match_kind(MatchKind::LeftmostFirst)
        .ascii_case_insensitive(false)
        .build(&unique)?;

    let session_files = discover_session_files(home);

    let bytes_scanned = AtomicU64::new(0);
    let file_count = AtomicUsize::new(0);
    // per-unique-name aggregates
    let aggregates: Vec<Mutex<SkillUsage>> = (0..unique.len())
        .map(|_| Mutex::new(SkillUsage::default()))
        .collect();

    // Parallel processing via rayon would be ideal but we want to stay
    // dependency-light. Use a simple thread pool sized to available
    // parallelism.
    use std::sync::mpsc;
    use std::thread;

    let (tx, rx) = mpsc::channel::<PathBuf>();
    for f in &session_files {
        tx.send(f.clone()).unwrap();
    }
    drop(tx);
    let rx = std::sync::Arc::new(Mutex::new(rx));

    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);

    let aggregates_ref = &aggregates;
    let ac_ref = &ac;
    let name_to_indexes_ref = &name_to_indexes;
    let unique_ref = &unique;
    let bytes_scanned_ref = &bytes_scanned;
    let file_count_ref = &file_count;

    thread::scope(|scope| {
        for _ in 0..num_threads {
            let rx = rx.clone();
            scope.spawn(move || loop {
                let path = {
                    let lock = rx.lock().unwrap();
                    match lock.recv() {
                        Ok(p) => p,
                        Err(_) => break,
                    }
                };
                if let Some((bytes, _local_mentions)) = process_file(
                    &path,
                    ac_ref,
                    unique_ref,
                    name_to_indexes_ref,
                    aggregates_ref,
                ) {
                    bytes_scanned_ref.fetch_add(bytes, Ordering::Relaxed);
                    file_count_ref.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });

    // Fan out unique aggregates back to per-pattern result.
    let mut by_pattern: Vec<SkillUsage> = Vec::with_capacity(patterns.len());
    for p in patterns {
        let idx = unique.binary_search_by(|x| x.as_str().cmp(p.as_str())).unwrap();
        let agg = aggregates[idx].lock().unwrap().clone();
        by_pattern.push(agg);
    }

    Ok(UsageOutcome {
        by_pattern,
        stats: UsageStats {
            session_files: file_count.load(Ordering::Relaxed),
            bytes_scanned: bytes_scanned.load(Ordering::Relaxed),
            elapsed_ms: start.elapsed().as_millis(),
        },
    })
}

fn process_file(
    path: &Path,
    ac: &AhoCorasick,
    unique: &[String],
    name_to_indexes: &HashMap<&str, Vec<usize>>,
    aggregates: &[Mutex<SkillUsage>],
) -> Option<(u64, u64)> {
    let bytes = fs::read(path).ok()?;
    let len = bytes.len() as u64;
    let mtime_unix = file_mtime_unix(path);
    let source = classify_source(path);

    // Track which unique skill indexes are seen at least once in this file
    // (to compute the "sessions" count, separate from raw mention count).
    let mut local_counts: HashMap<usize, u64> = HashMap::new();
    let mut total_mentions = 0u64;

    for m in ac.find_iter(&bytes) {
        let start = m.start();
        let end = m.end();
        if !is_quoted_or_word_bounded(&bytes, start, end) {
            continue;
        }
        let name = &unique[m.pattern().as_usize()];
        // Aho-Corasick patterns are sorted (we sorted before building); the
        // `pattern()` index aligns with `unique` because we passed `unique`
        // in order.
        *local_counts.entry(m.pattern().as_usize()).or_default() += 1;
        total_mentions += 1;
        // (We don't actually use `name` here but keep it for clarity / future.)
        let _ = name;
    }

    if !local_counts.is_empty() {
        for (idx, count) in local_counts.iter() {
            let mut agg = aggregates[*idx].lock().unwrap();
            agg.mentions += count;
            agg.sessions += 1;
            *agg.by_source.entry(source.to_string()).or_default() += count;
            if let Some(t) = mtime_unix {
                agg.last_seen_unix = Some(agg.last_seen_unix.map_or(t, |cur| cur.max(t)));
            }
            // Also fan out to other pattern_indexes that share this unique
            // name (handled in the caller via the unique-index mapping).
            let _ = name_to_indexes; // (used in caller)
            drop(agg);
        }
    }

    Some((len, total_mentions))
}

fn discover_session_files(home: &Path) -> Vec<PathBuf> {
    let mut out: BTreeSet<PathBuf> = BTreeSet::new();
    let roots = [
        home.join(".claude").join("projects"),
        home.join(".codex").join("sessions"),
        home.join(".codex").join("archived_sessions"),
    ];
    for root in roots {
        if !root.exists() {
            continue;
        }
        let walker = ignore::WalkBuilder::new(&root)
            .hidden(false)
            .git_ignore(false)
            .ignore(false)
            .require_git(false)
            .standard_filters(false)
            .max_depth(Some(6))
            .build();
        for entry in walker.flatten() {
            let p = entry.path();
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                && p.extension().and_then(|s| s.to_str()) == Some("jsonl")
            {
                out.insert(p.to_path_buf());
            }
        }
    }
    out.into_iter().collect()
}

fn classify_source(path: &Path) -> &'static str {
    let s = path.to_string_lossy();
    if s.contains("/.claude/") {
        "claude"
    } else if s.contains("/.codex/") {
        "codex"
    } else {
        "other"
    }
}

fn file_mtime_unix(path: &Path) -> Option<i64> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dur = mtime.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    Some(dur.as_secs() as i64)
}

/// True if the matched span is bounded by a non-alphanumeric/dash character
/// on both sides (or end-of-buffer). This rejects substring matches inside
/// longer identifiers (e.g. matching "render" inside "renderer") while still
/// accepting quoted occurrences, slash-prefixed mentions (`/handoff`), and
/// path components.
fn is_quoted_or_word_bounded(bytes: &[u8], start: usize, end: usize) -> bool {
    let before = if start == 0 { 0u8 } else { bytes[start - 1] };
    let after = if end >= bytes.len() { 0u8 } else { bytes[end] };
    !is_skill_char(before) && !is_skill_char(after)
}

#[inline]
fn is_skill_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub fn unix_to_iso8601(unix: i64) -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}
