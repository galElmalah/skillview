use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Args, Parser, Subcommand};
use serde::Serialize;
use skillview::emit::{self, BuildOptions};
use skillview::model::{Cluster, Inventory, Skill, SkillTier};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(
    name = "skillview",
    version,
    about = "Inventory agent skills across Claude, Codex, Cursor, and project-local roots.",
    long_about = "skillview walks a directory (defaults to $HOME) for SKILL.md and \
                  skill-shaped markdown, classifies each hit by host agent, detects \
                  exact + near-duplicate clusters, and counts cross-agent usage. \
                  The CLI is the primary surface (JSON by default, agent-friendly). \
                  Pass --tui to launch the bundled OpenTUI frontend (requires Bun)."
)]
struct Cli {
    /// Launch the OpenTUI frontend instead of the CLI. Requires `bun` on PATH
    /// and the TUI sources (set $SKILLVIEW_TUI_DIR, or run from a checkout).
    #[arg(long, global = true)]
    tui: bool,

    #[command(flatten)]
    scan: ScanArgs,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Args, Debug, Clone)]
struct ScanArgs {
    /// Root to scan. Defaults to $HOME.
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    /// Jaccard similarity threshold for near-duplicate clustering.
    #[arg(long, global = true, default_value_t = 0.85)]
    threshold: f64,

    /// Skip MinHash similarity (still detects exact-hash duplicates).
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    no_similarity: bool,

    /// Skip cross-agent usage scan (parses Claude/Codex session JSONLs).
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    no_usage: bool,

    /// Include 128-element MinHash arrays in JSON output (debug).
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    include_minhash: bool,

    /// Pretty-print JSON output.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    pretty: bool,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Emit the full inventory as JSON (default).
    Scan,
    /// List skills with summary fields; supports filtering.
    List {
        /// Filter by agent (e.g. claude, codex, cursor, agents-generic, unknown).
        #[arg(long)]
        agent: Option<String>,
        /// Filter by tier (primary or secondary).
        #[arg(long)]
        tier: Option<String>,
        /// Substring match on skill name (case-insensitive).
        #[arg(long)]
        name: Option<String>,
        /// Only list skills that belong to a duplicate cluster.
        #[arg(long, action = ArgAction::SetTrue)]
        dups_only: bool,
    },
    /// Show one skill's full record (name | id | path suffix match).
    Show {
        /// Skill name, id (e.g. s_3), or a substring of its path.
        target: String,
    },
    /// List duplicate clusters (exact and near).
    Dups {
        /// Only exact-content clusters.
        #[arg(long, action = ArgAction::SetTrue)]
        exact: bool,
        /// Only near-duplicate clusters (MinHash).
        #[arg(long, action = ArgAction::SetTrue)]
        near: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.tui {
        return launch_tui();
    }

    let home = dirs::home_dir().context("could not resolve $HOME")?;
    let root = cli.scan.root.clone().unwrap_or_else(|| home.clone());

    let inventory = emit::build(BuildOptions {
        root,
        home,
        threshold: cli.scan.threshold,
        include_minhash_in_output: cli.scan.include_minhash,
        skip_similarity: cli.scan.no_similarity,
        skip_usage: cli.scan.no_usage,
    })?;

    let pretty = cli.scan.pretty;
    match cli.cmd.unwrap_or(Cmd::Scan) {
        Cmd::Scan => write_json(&inventory, pretty),
        Cmd::List {
            agent,
            tier,
            name,
            dups_only,
        } => write_json(
            &list_skills(&inventory, agent.as_deref(), tier.as_deref(), name.as_deref(), dups_only)?,
            pretty,
        ),
        Cmd::Show { target } => write_json(&show_skill(&inventory, &target)?, pretty),
        Cmd::Dups { exact, near } => write_json(&list_dups(&inventory, exact, near), pretty),
    }
}

fn write_json<T: Serialize>(value: &T, pretty: bool) -> Result<()> {
    let mut stdout = io::stdout().lock();
    if pretty {
        serde_json::to_writer_pretty(&mut stdout, value)?;
    } else {
        serde_json::to_writer(&mut stdout, value)?;
    }
    stdout.write_all(b"\n")?;
    Ok(())
}

// ---------- list / show / dups projections ----------

#[derive(Serialize)]
struct SkillSummary<'a> {
    id: &'a str,
    name: &'a str,
    tier: SkillTier,
    agent: &'a str,
    root_id: &'a str,
    path: &'a str,
    dir: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cluster_id: Option<&'a str>,
    usage_mentions: u64,
    usage_sessions: u64,
}

#[derive(Serialize)]
struct ListResponse<'a> {
    count: usize,
    skills: Vec<SkillSummary<'a>>,
}

fn summarize(s: &Skill) -> SkillSummary<'_> {
    SkillSummary {
        id: &s.id,
        name: &s.name,
        tier: s.tier,
        agent: &s.agent,
        root_id: &s.root_id,
        path: &s.path,
        dir: &s.dir,
        cluster_id: s.cluster_id.as_deref(),
        usage_mentions: s.usage.mentions,
        usage_sessions: s.usage.sessions,
    }
}

fn list_skills<'a>(
    inv: &'a Inventory,
    agent: Option<&str>,
    tier: Option<&str>,
    name: Option<&str>,
    dups_only: bool,
) -> Result<ListResponse<'a>> {
    let tier_filter = match tier {
        Some(t) => Some(parse_tier(t)?),
        None => None,
    };
    let name_needle = name.map(|s| s.to_lowercase());
    let mut skills: Vec<SkillSummary<'a>> = inv
        .skills
        .iter()
        .filter(|s| agent.map_or(true, |a| s.agent.eq_ignore_ascii_case(a)))
        .filter(|s| tier_filter.map_or(true, |t| s.tier == t))
        .filter(|s| {
            name_needle
                .as_deref()
                .map_or(true, |n| s.name.to_lowercase().contains(n))
        })
        .filter(|s| !dups_only || s.cluster_id.is_some())
        .map(summarize)
        .collect();
    skills.sort_by(|a, b| a.agent.cmp(b.agent).then(a.name.cmp(b.name)));
    Ok(ListResponse {
        count: skills.len(),
        skills,
    })
}

fn parse_tier(s: &str) -> Result<SkillTier> {
    match s.to_lowercase().as_str() {
        "primary" => Ok(SkillTier::Primary),
        "secondary" => Ok(SkillTier::Secondary),
        other => Err(anyhow!("unknown tier {other:?} (expected primary|secondary)")),
    }
}

#[derive(Serialize)]
struct ShowResponse<'a> {
    matched: Vec<&'a Skill>,
    cluster: Option<ClusterView<'a>>,
}

#[derive(Serialize)]
struct ClusterView<'a> {
    cluster: &'a Cluster,
    members: Vec<SkillSummary<'a>>,
}

fn show_skill<'a>(inv: &'a Inventory, target: &str) -> Result<ShowResponse<'a>> {
    let lower = target.to_lowercase();
    let matched: Vec<&Skill> = inv
        .skills
        .iter()
        .filter(|s| {
            s.id == target
                || s.name.eq_ignore_ascii_case(target)
                || s.path.to_lowercase().contains(&lower)
        })
        .collect();
    if matched.is_empty() {
        return Err(anyhow!(
            "no skill matched {target:?} (try `skillview list --name {target}`)"
        ));
    }
    let cluster = matched
        .iter()
        .find_map(|s| s.cluster_id.as_deref())
        .and_then(|cid| inv.clusters.iter().find(|c| c.id == cid))
        .map(|c| ClusterView {
            cluster: c,
            members: c
                .members
                .iter()
                .filter_map(|mid| inv.skills.iter().find(|s| &s.id == mid))
                .map(summarize)
                .collect(),
        });
    Ok(ShowResponse { matched, cluster })
}

#[derive(Serialize)]
struct DupsResponse<'a> {
    count: usize,
    clusters: Vec<ClusterView<'a>>,
}

fn list_dups<'a>(inv: &'a Inventory, only_exact: bool, only_near: bool) -> DupsResponse<'a> {
    let clusters: Vec<ClusterView<'a>> = inv
        .clusters
        .iter()
        .filter(|c| match (only_exact, only_near) {
            (true, false) => matches!(c.kind, skillview::model::ClusterKind::Exact),
            (false, true) => matches!(c.kind, skillview::model::ClusterKind::Near),
            _ => true,
        })
        .map(|c| ClusterView {
            cluster: c,
            members: c
                .members
                .iter()
                .filter_map(|mid| inv.skills.iter().find(|s| &s.id == mid))
                .map(summarize)
                .collect(),
        })
        .collect();
    DupsResponse {
        count: clusters.len(),
        clusters,
    }
}

// ---------- --tui launcher ----------

fn launch_tui() -> Result<()> {
    let tui_dir = find_tui_dir().ok_or_else(|| {
        anyhow!(
            "could not locate the OpenTUI sources. Set $SKILLVIEW_TUI_DIR to the \
             `tui/` directory of a skillview checkout, or run this binary from \
             a tree that contains one. The crates.io release ships the CLI only; \
             the TUI lives in the repo at https://github.com/galElmalah/skillview."
        )
    })?;
    let entry = tui_dir.join("src").join("index.tsx");
    if !entry.exists() {
        return Err(anyhow!(
            "found a tui/ dir at {} but it has no src/index.tsx",
            tui_dir.display()
        ));
    }
    // Tell the TUI which Rust binary to spawn — itself.
    let self_path = std::env::current_exe().context("could not resolve own exe path")?;
    let status = Command::new("bun")
        .arg("run")
        .arg(&entry)
        .env("SKILLVIEW_CORE", &self_path)
        .status()
        .map_err(|e| {
            anyhow!(
                "could not exec `bun run {}`: {e}. Install Bun: https://bun.sh",
                entry.display()
            )
        })?;
    std::process::exit(status.code().unwrap_or(1));
}

fn find_tui_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SKILLVIEW_TUI_DIR") {
        let path = PathBuf::from(p);
        if path.join("src").join("index.tsx").exists() {
            return Some(path);
        }
    }
    // Look near the binary (dev builds: target/release/skillview → ../../tui).
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors().take(6) {
            if let Some(found) = check_tui(anc) {
                return Some(found);
            }
        }
    }
    // Look up from CWD (running from a checkout).
    if let Ok(cwd) = std::env::current_dir() {
        for anc in cwd.ancestors().take(6) {
            if let Some(found) = check_tui(anc) {
                return Some(found);
            }
        }
    }
    None
}

fn check_tui(dir: &Path) -> Option<PathBuf> {
    let candidate = dir.join("tui");
    if candidate.join("src").join("index.tsx").exists() {
        Some(candidate)
    } else {
        None
    }
}
