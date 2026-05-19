use crate::classify::classify_root;
use crate::minhash;
use crate::model::{
    Asset, Cluster, ClusterKind, Frontmatter, Inventory, Root, RootKind, Skill, SkillTier, Stats,
    Tokens, Usage, UsageConfidence, Validation, SCHEMA_VERSION,
};
use crate::parse::{self, ParsedSkill};
use crate::scan::{self, Candidate};
use crate::usage;
use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct BuildOptions {
    pub root: PathBuf,
    pub home: PathBuf,
    pub threshold: f64,
    pub include_minhash_in_output: bool,
    pub skip_similarity: bool,
    pub skip_usage: bool,
}

/// Phase tags for `Event::Progress`. Stable strings — consumers (CLI, TUI)
/// switch on these to update the phase indicator.
pub mod phase {
    pub const WALK: &str = "walk";
    pub const PARSE: &str = "parse";
    pub const CLUSTER: &str = "cluster";
    pub const USAGE: &str = "usage";
}

/// Streaming event emitted by `build_streaming` as work progresses. The shape
/// is deliberately flat — each variant is a self-contained NDJSON record so
/// consumers can route on the `event` tag without buffering.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum Event {
    /// Scan kicked off. Emitted before any other event.
    Start {
        root: String,
        started_at: String,
        schema_version: u32,
    },
    /// Periodic progress tick. `phase` is one of [`phase::WALK`],
    /// [`phase::PARSE`], [`phase::CLUSTER`], [`phase::USAGE`]. Skipped phases
    /// are simply absent.
    Progress {
        phase: &'static str,
        paths_seen: usize,
        skills_found: usize,
        elapsed_ms: u128,
    },
    /// A single skill record, emitted as soon as it is parsed and validated.
    /// `cluster_id` is `None` here — assignments arrive later via
    /// [`Event::Clusters`].
    Skill { skill: Skill },
    /// Final list of scanned roots, after the walk completes.
    Roots { roots: Vec<Root> },
    /// Clustering results plus skill-id → cluster-id assignments. Consumers
    /// merge `assignments` into the partial inventory they have built up
    /// from preceding `Skill` events.
    Clusters {
        clusters: Vec<Cluster>,
        assignments: BTreeMap<String, String>,
    },
    /// Per-skill usage counts plus scan stats. Consumers merge `by_skill`
    /// into the partial inventory.
    Usage {
        by_skill: BTreeMap<String, Usage>,
        session_files: usize,
        bytes_scanned: u64,
        elapsed_ms: u128,
    },
    /// Final stats; signals end of stream.
    Done {
        stats: Stats,
        generated_at: String,
    },
}

pub fn build(opts: BuildOptions) -> Result<Inventory> {
    build_streaming(opts, |_| {})
}

/// Build the full inventory while emitting progressive `Event`s through
/// `sink`. The returned `Inventory` is identical to what `build` produces;
/// streaming is purely additive (consumers may ignore events entirely).
pub fn build_streaming<F>(opts: BuildOptions, mut sink: F) -> Result<Inventory>
where
    F: FnMut(Event),
{
    let start = Instant::now();
    let started_at = now_iso8601();

    sink(Event::Start {
        root: opts.root.to_string_lossy().to_string(),
        started_at: started_at.clone(),
        schema_version: SCHEMA_VERSION,
    });

    // Walk phase — emit progress ticks while the parallel walker runs.
    let outcome = {
        let phase_start = start;
        // Inline sink call inside the ticker would borrow `sink` across the
        // closure boundary; instead capture ticks into a small buffer and
        // flush them after `scan_with_ticker` returns. Tick rate is ~120ms
        // so the buffer stays tiny even on slow filesystems.
        let mut ticks: Vec<(usize, usize, u128)> = Vec::new();
        let outcome = scan::scan_with_ticker(&opts.root, |paths, hits| {
            ticks.push((paths, hits, phase_start.elapsed().as_millis()));
        });
        for (paths, hits, elapsed_ms) in ticks {
            sink(Event::Progress {
                phase: phase::WALK,
                paths_seen: paths,
                skills_found: hits,
                elapsed_ms,
            });
        }
        outcome
    };

    let mut roots: Vec<Root> = Vec::new();
    let mut root_index: HashMap<PathBuf, String> = HashMap::new();
    let mut skills: Vec<Skill> = Vec::new();
    let mut sig_inputs: Vec<(String, String, Vec<u64>)> = Vec::new();

    // Parse phase — emit a Skill event per file as it is parsed, and a
    // throttled Progress tick so big inventories show forward motion.
    let mut last_tick_ms: u128 = 0;
    for (idx, cand) in outcome.candidates.iter().enumerate() {
        match build_skill(idx, cand, &opts, &mut roots, &mut root_index) {
            Ok(Some((skill, sig_input))) => {
                if let Some(input) = sig_input {
                    sig_inputs.push(input);
                }
                // Emit the skill event first so the consumer sees it before
                // we mutate it further (cluster assignment, usage merge).
                let mut for_event = skill.clone();
                if !opts.include_minhash_in_output {
                    for_event.minhash = None;
                }
                sink(Event::Skill { skill: for_event });
                skills.push(skill);
            }
            Ok(None) => {}
            Err(_e) => {
                // Best-effort: skip unreadable / malformed files silently.
                // (Could be exposed via a --verbose flag later.)
            }
        }
        let now_ms = start.elapsed().as_millis();
        // Tick every ~120ms during parse so the consumer can update a
        // "parsed N of M" indicator without being flooded.
        if now_ms.saturating_sub(last_tick_ms) >= 120 {
            sink(Event::Progress {
                phase: phase::PARSE,
                paths_seen: idx + 1,
                skills_found: skills.len(),
                elapsed_ms: now_ms,
            });
            last_tick_ms = now_ms;
        }
    }

    sink(Event::Roots {
        roots: roots.clone(),
    });

    // Cluster phase.
    sink(Event::Progress {
        phase: phase::CLUSTER,
        paths_seen: outcome.scanned_paths,
        skills_found: skills.len(),
        elapsed_ms: start.elapsed().as_millis(),
    });
    let mut clusters: Vec<Cluster> = Vec::new();
    let mut cluster_assignments: BTreeMap<String, String> = BTreeMap::new();
    if !opts.skip_similarity && !sig_inputs.is_empty() {
        let clustering = minhash::cluster(&sig_inputs, opts.threshold);
        for (cid, kind_str, sim, members) in &clustering.clusters {
            clusters.push(Cluster {
                id: cid.clone(),
                kind: match *kind_str {
                    "exact" => ClusterKind::Exact,
                    _ => ClusterKind::Near,
                },
                similarity: *sim,
                members: members.clone(),
            });
        }
        for skill in skills.iter_mut() {
            if let Some(cid) = clustering.assignments.get(&skill.id) {
                skill.cluster_id = Some(cid.clone());
                cluster_assignments.insert(skill.id.clone(), cid.clone());
            }
        }
    }
    sink(Event::Clusters {
        clusters: clusters.clone(),
        assignments: cluster_assignments,
    });

    if !opts.include_minhash_in_output {
        for skill in skills.iter_mut() {
            skill.minhash = None;
        }
    }

    // Usage phase.
    sink(Event::Progress {
        phase: phase::USAGE,
        paths_seen: outcome.scanned_paths,
        skills_found: skills.len(),
        elapsed_ms: start.elapsed().as_millis(),
    });
    let usage_stats = if opts.skip_usage {
        Default::default()
    } else {
        let mut reliable_indices: Vec<usize> = Vec::new();
        let mut reliable_names: Vec<String> = Vec::new();
        for (i, s) in skills.iter().enumerate() {
            if Usage::is_name_reliable(&s.name) {
                reliable_indices.push(i);
                reliable_names.push(s.name.clone());
            }
        }
        for s in skills.iter_mut() {
            s.usage.confidence = if Usage::is_name_reliable(&s.name) {
                UsageConfidence::High
            } else {
                UsageConfidence::Low
            };
        }
        match usage::scan(&opts.home, &reliable_names) {
            Ok(outcome) => {
                let mut by_skill: BTreeMap<String, Usage> = BTreeMap::new();
                for (pat_idx, u) in outcome.by_pattern.iter().enumerate() {
                    let skill_idx = reliable_indices[pat_idx];
                    let s = &mut skills[skill_idx];
                    s.usage.mentions = u.mentions;
                    s.usage.sessions = u.sessions;
                    s.usage.last_seen_at = u.last_seen_unix.map(usage::unix_to_iso8601);
                    s.usage.by_source = u.by_source.clone();
                    by_skill.insert(s.id.clone(), s.usage.clone());
                }
                sink(Event::Usage {
                    by_skill,
                    session_files: outcome.stats.session_files,
                    bytes_scanned: outcome.stats.bytes_scanned,
                    elapsed_ms: outcome.stats.elapsed_ms,
                });
                outcome.stats
            }
            Err(_) => {
                sink(Event::Usage {
                    by_skill: BTreeMap::new(),
                    session_files: 0,
                    bytes_scanned: 0,
                    elapsed_ms: 0,
                });
                Default::default()
            }
        }
    };

    let stats = Stats {
        scanned_paths: outcome.scanned_paths,
        elapsed_ms: start.elapsed().as_millis(),
        primary_skills: skills.iter().filter(|s| s.tier == SkillTier::Primary).count(),
        secondary_skills: skills
            .iter()
            .filter(|s| s.tier == SkillTier::Secondary)
            .count(),
        duplicate_clusters: clusters.len(),
        usage_session_files: usage_stats.session_files,
        usage_bytes_scanned: usage_stats.bytes_scanned,
        usage_elapsed_ms: usage_stats.elapsed_ms,
    };
    let generated_at = now_iso8601();

    sink(Event::Done {
        stats: stats.clone(),
        generated_at: generated_at.clone(),
    });

    Ok(Inventory {
        schema_version: SCHEMA_VERSION,
        generated_at,
        roots,
        skills,
        clusters,
        stats,
    })
}

#[allow(clippy::type_complexity)]
fn build_skill(
    idx: usize,
    cand: &Candidate,
    opts: &BuildOptions,
    roots: &mut Vec<Root>,
    root_index: &mut HashMap<PathBuf, String>,
) -> Result<Option<(Skill, Option<(String, String, Vec<u64>)>)>> {
    let parsed: ParsedSkill = parse::parse(&cand.path)?;
    // Secondary candidates without skill-shaped frontmatter are dropped.
    if cand.tier == SkillTier::Secondary && !looks_like_skill(&parsed.frontmatter) {
        return Ok(None);
    }

    let dir = cand
        .path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| cand.path.clone());

    let root_match = classify_root(&cand.path, &opts.home);
    let root_id = root_index
        .entry(root_match.root_dir.clone())
        .or_insert_with(|| {
            let id = format!("r_{}", roots.len());
            roots.push(Root {
                id: id.clone(),
                kind: root_match.kind,
                path: root_match.root_dir.to_string_lossy().to_string(),
            });
            id
        })
        .clone();

    let name = pick_name(&parsed.frontmatter, &dir);
    let id = format!("s_{}", idx);

    let normalized = parse::normalize_for_signature(&parsed.body);
    let content_hash = format!("blake3:{}", blake3::hash(normalized.as_bytes()).to_hex());
    let sig = minhash::signature(&normalized);

    let assets = collect_assets(&dir, &cand.path, &parsed.references);
    let tokens = compute_tokens(parsed.frontmatter.as_ref(), &parsed.body);
    let validation = validate(
        cand.tier,
        parsed.frontmatter.as_ref(),
        &parsed.body,
        &name,
        &dir,
    );

    let skill = Skill {
        id: id.clone(),
        tier: cand.tier,
        name,
        path: cand.path.to_string_lossy().to_string(),
        dir: dir.to_string_lossy().to_string(),
        agent: root_match.agent,
        root_id,
        frontmatter: parsed.frontmatter,
        content_hash: content_hash.clone(),
        minhash: Some(sig.clone()),
        assets,
        cluster_id: None,
        usage: Usage::default(),
        tokens,
        validation,
    };

    let sig_input = if opts.skip_similarity {
        None
    } else {
        Some((id, content_hash, sig))
    };

    Ok(Some((skill, sig_input)))
}

fn looks_like_skill(fm: &Option<Frontmatter>) -> bool {
    match fm {
        Some(fm) => fm.name.is_some() && fm.description.is_some(),
        None => false,
    }
}

fn pick_name(fm: &Option<Frontmatter>, dir: &Path) -> String {
    if let Some(Frontmatter {
        name: Some(name), ..
    }) = fm
    {
        return name.clone();
    }
    dir.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "(unnamed)".to_string())
}

fn collect_assets(
    dir: &Path,
    skill_path: &Path,
    references: &std::collections::BTreeSet<String>,
) -> Vec<Asset> {
    let mut out = Vec::new();
    let normalized_refs: std::collections::BTreeSet<String> = references
        .iter()
        .map(|r| r.trim_start_matches("./").to_string())
        .collect();

    let walker = ignore::WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(false)
        .ignore(false)
        .require_git(false)
        .standard_filters(false)
        .max_depth(Some(6))
        .build();

    for entry in walker.flatten() {
        let p = entry.path();
        if p == dir || p == skill_path {
            continue;
        }
        let Some(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let rel = p.strip_prefix(dir).unwrap_or(p);
        let rel_str = rel.to_string_lossy().to_string();
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let referenced = normalized_refs.contains(&rel_str)
            || normalized_refs.iter().any(|r| r.ends_with(&rel_str))
            || normalized_refs
                .iter()
                .any(|r| r == &format!("./{}", rel_str));
        out.push(Asset {
            path: rel_str,
            size_bytes,
            referenced,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Heuristic: chars / 3.7, rounded up. Empirically within ~10% of cl100k_base
/// for English markdown, which is enough resolution for "is this skill cheap
/// or expensive to load into context".
fn approx_tokens(s: &str) -> u32 {
    let chars = s.chars().count();
    if chars == 0 {
        return 0;
    }
    ((chars as f64 / 3.7).ceil() as u32).max(1)
}

fn compute_tokens(fm: Option<&Frontmatter>, body: &str) -> Tokens {
    let description = fm
        .and_then(|f| f.description.as_deref())
        .map(approx_tokens)
        .unwrap_or(0);
    let body = approx_tokens(body);
    Tokens {
        description,
        body,
        total: description.saturating_add(body),
    }
}

const DESCRIPTION_MAX_CHARS: usize = 500;

fn validate(
    tier: SkillTier,
    fm: Option<&Frontmatter>,
    body: &str,
    resolved_name: &str,
    dir: &Path,
) -> Validation {
    let mut issues = Vec::new();

    let name = fm.and_then(|f| f.name.as_deref().filter(|s| !s.trim().is_empty()));
    let desc = fm.and_then(|f| f.description.as_deref().filter(|s| !s.trim().is_empty()));

    if name.is_none() {
        issues.push("missing `name` in frontmatter".to_string());
    }
    if desc.is_none() {
        issues.push("missing `description` in frontmatter".to_string());
    }
    if let Some(d) = desc {
        let len = d.chars().count();
        if len > DESCRIPTION_MAX_CHARS {
            issues.push(format!(
                "description is {} chars (>{} recommended)",
                len, DESCRIPTION_MAX_CHARS
            ));
        }
    }
    if body.trim().is_empty() {
        issues.push("body is empty".to_string());
    }

    // Primary skills live in their own folder named after the skill — if the
    // frontmatter name and the directory name disagree, one was renamed and
    // the other wasn't. Secondary skills don't follow this convention.
    if tier == SkillTier::Primary {
        if let Some(n) = name {
            if let Some(dirname) = dir.file_name().and_then(|s| s.to_str()) {
                if !n.eq_ignore_ascii_case(dirname) {
                    issues.push(format!(
                        "frontmatter name `{}` does not match directory `{}`",
                        n, dirname
                    ));
                }
            }
        }
    }

    // resolved_name is what we'd display; not validated separately — it's
    // populated from the frontmatter or the dir already.
    let _ = resolved_name;

    Validation {
        ok: issues.is_empty(),
        issues,
    }
}

#[allow(unused_variables)]
fn now_iso8601() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[allow(dead_code)]
fn _kind_to_str(k: &RootKind) -> &'static str {
    match k {
        RootKind::ClaudeGlobal => "claude-global",
        RootKind::ClaudeProject => "claude-project",
        RootKind::Codex => "codex",
        RootKind::Cursor => "cursor",
        RootKind::AgentsGeneric => "agents-generic",
        RootKind::Unknown => "unknown",
    }
}
