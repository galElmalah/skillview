use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use skillview::emit::{self, BuildOptions};
use skillview::model::{
    Cluster, ClusterKind, Inventory, RootKind, Skill, SkillTier, UsageConfidence,
};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const TOP_LONG: &str = "\
skillview walks a directory (defaults to $HOME) for SKILL.md and skill-shaped
markdown, classifies each hit by host agent, detects exact + near-duplicate
clusters, and counts cross-agent usage. The CLI is the primary surface
(JSON by default, agent-friendly).

EXPLORATION TIPS (for agents)
  skillview --help                      # this screen
  skillview <subcommand> --help         # filters + flags for that subcommand
  skillview examples                    # curated recipe book
  skillview agents                      # which agents have skills here
  skillview roots                       # where on disk those skills live
  skillview stats                       # one-shot inventory overview

Pass --tui to launch the bundled OpenTUI frontend (requires Bun).";

const LIST_LONG: &str = "\
List skills with summary fields, with rich filtering and projection.

Filters compose (AND). Filters that take a value accept a single argument;
boolean filters are flags. `--sort` controls ordering; `--limit` truncates;
`--format` controls the output shape so you can pipe into other tools.

OUTPUT FORMATS
  json   default; compact unless --pretty
  jsonl  one skill object per line (good for streaming)
  tsv    tab-separated with a header row
  ids    one skill id per line (s_3)
  paths  one absolute path per line
  names  one skill name per line";

const LIST_AFTER: &str = "\
EXAMPLES
  skillview list --agent claude --tier primary
  skillview list --name browser --sort usage --limit 10
  skillview list --has-usage --format tsv
  skillview list --validation-failed --format paths
  skillview list --dups-only --dup-kind near --sort tokens
  skillview list --root-kind claude-global --format ids";

const SHOW_LONG: &str = "\
Show one skill's full record plus any duplicate-cluster siblings.

TARGET resolves in order:
  1. exact skill id (e.g. s_3)
  2. exact skill name (case-insensitive)
  3. substring of the skill's path (case-insensitive)

If a target matches more than one skill (e.g. via path substring) every match
is returned in `matched` so you can disambiguate.";

const SHOW_AFTER: &str = "\
EXAMPLES
  skillview show agent-browser
  skillview show s_3
  skillview show ~/.claude/skills/agent-browser
  skillview show code-review --pretty";

const DUPS_LONG: &str = "\
List duplicate clusters (exact-content matches + MinHash near-duplicates).

Each cluster has a kind (exact|near), a similarity score (1.0 for exact),
and a list of member skill ids. Use the filters to narrow which clusters
you care about; use --format tsv for quick scanning.";

const DUPS_AFTER: &str = "\
EXAMPLES
  skillview dups
  skillview dups --exact
  skillview dups --near --min-size 3
  skillview dups --agent claude --sort size --limit 5
  skillview dups --format tsv";

const USAGE_LONG: &str = "\
List skills ranked by how often they appear in Claude/Codex session logs.

Usage counts only attach to skills whose name is reliably distinctive
(>= 6 chars AND contains '-' or '_'). Skills with `confidence: low` are
excluded by default because the count would be dominated by false
positives — pass --include-low to see them anyway.";

const USAGE_AFTER: &str = "\
EXAMPLES
  skillview usage                       # top by mentions
  skillview usage --top 20 --sort sessions
  skillview usage --agent claude --sort recent
  skillview usage --min-mentions 5 --format tsv
  skillview usage --include-low";

const AGENTS_LONG: &str = "\
Per-agent rollup: how many skills live under each host agent, how many are
primary vs secondary, how many participate in duplicate clusters, and the
total cross-agent usage attached to each.

Use this to answer \"which agents have skills on this machine and how much
overlap is there between them?\" in a single call.";

const AGENTS_AFTER: &str = "\
EXAMPLES
  skillview agents
  skillview agents --pretty
  skillview agents --format tsv";

const ROOTS_LONG: &str = "\
List the on-disk roots skillview discovered (e.g. ~/.claude, ~/.codex,
project-local .claude directories) along with how many skills sit under
each. Useful for sanity-checking that you scanned the right tree.";

const ROOTS_AFTER: &str = "\
EXAMPLES
  skillview roots
  skillview roots --format tsv
  skillview --root ~/code/some-project roots";

const STATS_LONG: &str = "\
Inventory-wide statistics: counts, duplicate-cluster breakdown, usage
confidence breakdown, validation failures, and scan timings. JSON only —
this is meant to feed dashboards / agents, not be eyeballed.";

const STATS_AFTER: &str = "\
EXAMPLES
  skillview stats
  skillview stats --pretty";

const EXAMPLES_LONG: &str = "\
Print a curated recipe book of common skillview invocations. Use this to
discover what the CLI can do without reading the README. Each recipe is a
single line that can be copy-pasted directly into a shell.";

#[derive(Parser, Debug)]
#[command(
    name = "skillview",
    version,
    about = "Inventory agent skills across Claude, Codex, Cursor, and project-local roots.",
    long_about = TOP_LONG,
    after_help = "Run `skillview examples` for a curated recipe book, or `skillview <cmd> --help` for per-command flags."
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

    /// Emit NDJSON events on stdout as the scan progresses (`start`,
    /// `progress`, `skill`, `roots`, `clusters`, `usage`, `done`). One event
    /// per line — agents and the TUI can render results live. Suppresses the
    /// usual one-shot JSON output. Only applies to the default scan (and
    /// `scan` subcommand); filtered subcommands (`list`, `dups`, …) need the
    /// full inventory and ignore this flag.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    stream: bool,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Emit the full inventory as JSON (default when no subcommand is given).
    Scan,

    /// List skills with summary fields; supports filtering, sort, limit, format.
    #[command(long_about = LIST_LONG, after_help = LIST_AFTER)]
    List {
        /// Filter by agent (claude, codex, cursor, agents-generic, unknown).
        #[arg(long)]
        agent: Option<String>,
        /// Filter by tier (primary or secondary).
        #[arg(long, value_enum)]
        tier: Option<TierArg>,
        /// Filter by root kind (claude-global, claude-project, codex, cursor, agents-generic, unknown).
        #[arg(long = "root-kind", value_enum)]
        root_kind: Option<RootKindArg>,
        /// Substring match on skill name (case-insensitive).
        #[arg(long)]
        name: Option<String>,
        /// Only skills that belong to a duplicate cluster.
        #[arg(long, action = ArgAction::SetTrue)]
        dups_only: bool,
        /// Restrict --dups-only to a specific cluster kind.
        #[arg(long = "dup-kind", value_enum)]
        dup_kind: Option<DupKindArg>,
        /// Shorthand for `--min-usage 1`.
        #[arg(long, action = ArgAction::SetTrue)]
        has_usage: bool,
        /// Only skills with at least N usage mentions.
        #[arg(long = "min-usage")]
        min_usage: Option<u64>,
        /// Only skills with at least N total tokens (description + body).
        #[arg(long = "min-tokens")]
        min_tokens: Option<u32>,
        /// Only skills with at most N total tokens.
        #[arg(long = "max-tokens")]
        max_tokens: Option<u32>,
        /// Only skills whose frontmatter validation failed (validation.ok == false).
        #[arg(long = "validation-failed", action = ArgAction::SetTrue)]
        validation_failed: bool,
        /// Sort order for the resulting list.
        #[arg(long, value_enum, default_value_t = ListSort::AgentName)]
        sort: ListSort,
        /// Limit results to the first N entries (after sorting).
        #[arg(long)]
        limit: Option<usize>,
        /// Output format. Defaults to json.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },

    /// Show one skill's full record (name | id | path substring).
    #[command(long_about = SHOW_LONG, after_help = SHOW_AFTER)]
    Show {
        /// Skill name, id (e.g. s_3), or a substring of its path.
        target: String,
    },

    /// List duplicate clusters (exact and near).
    #[command(long_about = DUPS_LONG, after_help = DUPS_AFTER)]
    Dups {
        /// Only exact-content clusters.
        #[arg(long, action = ArgAction::SetTrue)]
        exact: bool,
        /// Only near-duplicate clusters (MinHash).
        #[arg(long, action = ArgAction::SetTrue)]
        near: bool,
        /// Only clusters with at least N members.
        #[arg(long = "min-size")]
        min_size: Option<usize>,
        /// Only clusters that include at least one skill from this agent.
        #[arg(long)]
        agent: Option<String>,
        /// Only clusters that include at least one skill from this root kind.
        #[arg(long = "root-kind", value_enum)]
        root_kind: Option<RootKindArg>,
        /// Sort order.
        #[arg(long, value_enum, default_value_t = DupsSort::Size)]
        sort: DupsSort,
        /// Limit results to the first N clusters.
        #[arg(long)]
        limit: Option<usize>,
        /// Output format. Defaults to json.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },

    /// Rank skills by cross-agent usage (Claude + Codex session logs).
    #[command(long_about = USAGE_LONG, after_help = USAGE_AFTER)]
    Usage {
        /// Filter by agent.
        #[arg(long)]
        agent: Option<String>,
        /// Only skills with at least N mentions (default: 1).
        #[arg(long = "min-mentions", default_value_t = 1)]
        min_mentions: u64,
        /// Limit results to the top N entries (after sorting).
        #[arg(long, default_value_t = 25)]
        top: usize,
        /// Sort order.
        #[arg(long, value_enum, default_value_t = UsageSort::Mentions)]
        sort: UsageSort,
        /// Include skills with confidence=low (names too generic to scan for).
        #[arg(long = "include-low", action = ArgAction::SetTrue)]
        include_low: bool,
        /// Output format. Defaults to json.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },

    /// Per-agent rollup (primary/secondary counts, dup membership, total usage).
    #[command(long_about = AGENTS_LONG, after_help = AGENTS_AFTER)]
    Agents {
        /// Output format. Defaults to json.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },

    /// List scanned roots with kind + skill counts.
    #[command(long_about = ROOTS_LONG, after_help = ROOTS_AFTER)]
    Roots {
        /// Output format. Defaults to json.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },

    /// Inventory-wide statistics (counts, dup breakdown, timings, validation).
    #[command(long_about = STATS_LONG, after_help = STATS_AFTER)]
    Stats,

    /// Print a curated recipe book of common skillview invocations.
    #[command(long_about = EXAMPLES_LONG)]
    Examples,
}

// ---------- value enums ----------

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "lowercase")]
enum TierArg {
    Primary,
    Secondary,
}

impl From<TierArg> for SkillTier {
    fn from(t: TierArg) -> Self {
        match t {
            TierArg::Primary => SkillTier::Primary,
            TierArg::Secondary => SkillTier::Secondary,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum RootKindArg {
    ClaudeGlobal,
    ClaudeProject,
    Codex,
    Cursor,
    AgentsGeneric,
    Unknown,
}

impl From<RootKindArg> for RootKind {
    fn from(r: RootKindArg) -> Self {
        match r {
            RootKindArg::ClaudeGlobal => RootKind::ClaudeGlobal,
            RootKindArg::ClaudeProject => RootKind::ClaudeProject,
            RootKindArg::Codex => RootKind::Codex,
            RootKindArg::Cursor => RootKind::Cursor,
            RootKindArg::AgentsGeneric => RootKind::AgentsGeneric,
            RootKindArg::Unknown => RootKind::Unknown,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "lowercase")]
enum DupKindArg {
    Exact,
    Near,
}

impl From<DupKindArg> for ClusterKind {
    fn from(k: DupKindArg) -> Self {
        match k {
            DupKindArg::Exact => ClusterKind::Exact,
            DupKindArg::Near => ClusterKind::Near,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum ListSort {
    /// Agent then name (default).
    AgentName,
    Name,
    Agent,
    Tier,
    /// Usage mentions, descending.
    Usage,
    /// Tokens total, descending.
    Tokens,
    /// Usage sessions, descending.
    Sessions,
    Path,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum DupsSort {
    /// Cluster member count, descending (default).
    Size,
    /// Similarity score, descending.
    Similarity,
    /// Cluster kind (exact before near), then size descending.
    Kind,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum UsageSort {
    /// Total mentions, descending (default).
    Mentions,
    /// Distinct sessions, descending.
    Sessions,
    /// Latest session timestamp, descending (missing values last).
    Recent,
    /// Skill name, ascending.
    Name,
}

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
#[clap(rename_all = "lowercase")]
enum OutputFormat {
    Json,
    Jsonl,
    Tsv,
    Ids,
    Paths,
    Names,
}

// ---------- entry ----------

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.tui {
        return launch_tui(&cli.scan);
    }

    // `examples` is a pure print — don't pay for a scan.
    if matches!(cli.cmd, Some(Cmd::Examples)) {
        print_examples();
        return Ok(());
    }

    let home = dirs::home_dir().context("could not resolve $HOME")?;
    let root = cli.scan.root.clone().unwrap_or_else(|| home.clone());

    // Streaming path: only meaningful for the default / `scan` command,
    // where the consumer wants the full inventory. Filtered subcommands need
    // the materialized inventory anyway, so we silently ignore --stream for
    // them rather than emit a half-baked stream.
    let stream_mode =
        cli.scan.stream && matches!(cli.cmd, None | Some(Cmd::Scan));
    if stream_mode {
        return run_stream(BuildOptions {
            root,
            home,
            threshold: cli.scan.threshold,
            include_minhash_in_output: cli.scan.include_minhash,
            skip_similarity: cli.scan.no_similarity,
            skip_usage: cli.scan.no_usage,
        });
    }

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
            root_kind,
            name,
            dups_only,
            dup_kind,
            has_usage,
            min_usage,
            min_tokens,
            max_tokens,
            validation_failed,
            sort,
            limit,
            format,
        } => {
            let response = list_skills(
                &inventory,
                ListFilters {
                    agent: agent.as_deref(),
                    tier: tier.map(Into::into),
                    root_kind: root_kind.map(Into::into),
                    name: name.as_deref(),
                    dups_only,
                    dup_kind: dup_kind.map(Into::into),
                    min_usage: effective_min_usage(min_usage, has_usage),
                    min_tokens,
                    max_tokens,
                    validation_failed,
                    sort,
                    limit,
                },
            )?;
            emit_list(&response, format, pretty)
        }
        Cmd::Show { target } => write_json(&show_skill(&inventory, &target)?, pretty),
        Cmd::Dups {
            exact,
            near,
            min_size,
            agent,
            root_kind,
            sort,
            limit,
            format,
        } => {
            let response = list_dups(
                &inventory,
                DupsFilters {
                    only_exact: exact,
                    only_near: near,
                    min_size,
                    agent: agent.as_deref(),
                    root_kind: root_kind.map(Into::into),
                    sort,
                    limit,
                },
            );
            emit_dups(&response, format, pretty)
        }
        Cmd::Usage {
            agent,
            min_mentions,
            top,
            sort,
            include_low,
            format,
        } => {
            let response = list_usage(
                &inventory,
                UsageFilters {
                    agent: agent.as_deref(),
                    min_mentions,
                    top,
                    sort,
                    include_low,
                },
            );
            emit_usage(&response, format, pretty)
        }
        Cmd::Agents { format } => {
            let response = list_agents(&inventory);
            emit_agents(&response, format, pretty)
        }
        Cmd::Roots { format } => {
            let response = list_roots(&inventory);
            emit_roots(&response, format, pretty)
        }
        Cmd::Stats => write_json(&build_stats(&inventory), pretty),
        Cmd::Examples => {
            // Already handled above before the scan; keep for exhaustiveness.
            print_examples();
            Ok(())
        }
    }
}

fn effective_min_usage(explicit: Option<u64>, has_usage: bool) -> Option<u64> {
    match (explicit, has_usage) {
        (Some(n), _) => Some(n),
        (None, true) => Some(1),
        (None, false) => None,
    }
}

// ---------- streaming ----------

/// Drive `emit::build_streaming` and write each event as a single NDJSON
/// line on stdout. Flushes after every event so consumers see progress live
/// rather than buffered. Errors writing to stdout terminate the stream — we
/// can't recover from a closed pipe.
fn run_stream(opts: BuildOptions) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut write_err: Option<io::Error> = None;
    let _inventory = emit::build_streaming(opts, |event| {
        if write_err.is_some() {
            return;
        }
        if let Err(e) = serde_json::to_writer(&mut out, &event) {
            write_err = Some(io::Error::other(e));
            return;
        }
        if let Err(e) = out.write_all(b"\n") {
            write_err = Some(e);
            return;
        }
        // Best-effort flush so partial output is visible even if the consumer
        // is line-buffered. Ignore failures — the next write will surface them.
        let _ = out.flush();
    })?;
    if let Some(e) = write_err {
        return Err(e.into());
    }
    Ok(())
}

// ---------- output helpers ----------

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

fn write_jsonl<T: Serialize, I: IntoIterator<Item = T>>(items: I) -> Result<()> {
    let mut stdout = io::stdout().lock();
    for item in items {
        serde_json::to_writer(&mut stdout, &item)?;
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

fn write_lines<I: IntoIterator<Item = String>>(lines: I) -> Result<()> {
    let mut stdout = io::stdout().lock();
    for line in lines {
        stdout.write_all(line.as_bytes())?;
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

fn write_tsv(header: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(header.join("\t").as_bytes())?;
    stdout.write_all(b"\n")?;
    for row in rows {
        let cleaned: Vec<String> = row.into_iter().map(sanitize_tsv).collect();
        stdout.write_all(cleaned.join("\t").as_bytes())?;
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

fn sanitize_tsv(s: String) -> String {
    s.replace('\t', " ").replace('\n', " ")
}

// ---------- list ----------

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
    tokens_total: u32,
    validation_ok: bool,
}

#[derive(Serialize)]
struct ListResponse<'a> {
    count: usize,
    skills: Vec<SkillSummary<'a>>,
}

struct ListFilters<'a> {
    agent: Option<&'a str>,
    tier: Option<SkillTier>,
    root_kind: Option<RootKind>,
    name: Option<&'a str>,
    dups_only: bool,
    dup_kind: Option<ClusterKind>,
    min_usage: Option<u64>,
    min_tokens: Option<u32>,
    max_tokens: Option<u32>,
    validation_failed: bool,
    sort: ListSort,
    limit: Option<usize>,
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
        tokens_total: s.tokens.total,
        validation_ok: s.validation.ok,
    }
}

fn root_kind_of<'a>(inv: &'a Inventory, root_id: &str) -> Option<RootKind> {
    inv.roots.iter().find(|r| r.id == root_id).map(|r| r.kind)
}

fn cluster_of<'a>(inv: &'a Inventory, skill: &Skill) -> Option<&'a Cluster> {
    let cid = skill.cluster_id.as_deref()?;
    inv.clusters.iter().find(|c| c.id == cid)
}

fn list_skills<'a>(inv: &'a Inventory, f: ListFilters<'_>) -> Result<ListResponse<'a>> {
    let name_needle = f.name.map(|s| s.to_lowercase());
    let mut skills: Vec<&Skill> = inv
        .skills
        .iter()
        .filter(|s| f.agent.map_or(true, |a| s.agent.eq_ignore_ascii_case(a)))
        .filter(|s| f.tier.map_or(true, |t| s.tier == t))
        .filter(|s| {
            f.root_kind
                .map_or(true, |rk| root_kind_of(inv, &s.root_id) == Some(rk))
        })
        .filter(|s| {
            name_needle
                .as_deref()
                .map_or(true, |n| s.name.to_lowercase().contains(n))
        })
        .filter(|s| !f.dups_only || s.cluster_id.is_some())
        .filter(|s| match f.dup_kind {
            Some(kind) => cluster_of(inv, s).map_or(false, |c| c.kind == kind),
            None => true,
        })
        .filter(|s| f.min_usage.map_or(true, |min| s.usage.mentions >= min))
        .filter(|s| f.min_tokens.map_or(true, |min| s.tokens.total >= min))
        .filter(|s| f.max_tokens.map_or(true, |max| s.tokens.total <= max))
        .filter(|s| !f.validation_failed || !s.validation.ok)
        .collect();

    sort_skills(&mut skills, f.sort);

    if let Some(limit) = f.limit {
        skills.truncate(limit);
    }

    let skills: Vec<SkillSummary<'a>> = skills.into_iter().map(summarize).collect();
    Ok(ListResponse {
        count: skills.len(),
        skills,
    })
}

fn sort_skills(skills: &mut [&Skill], sort: ListSort) {
    match sort {
        ListSort::AgentName => {
            skills.sort_by(|a, b| a.agent.cmp(&b.agent).then(a.name.cmp(&b.name)))
        }
        ListSort::Name => skills.sort_by(|a, b| a.name.cmp(&b.name)),
        ListSort::Agent => skills.sort_by(|a, b| a.agent.cmp(&b.agent).then(a.name.cmp(&b.name))),
        ListSort::Tier => {
            skills.sort_by(|a, b| tier_key(a.tier).cmp(&tier_key(b.tier)).then(a.name.cmp(&b.name)))
        }
        ListSort::Usage => {
            skills.sort_by(|a, b| b.usage.mentions.cmp(&a.usage.mentions).then(a.name.cmp(&b.name)))
        }
        ListSort::Tokens => {
            skills.sort_by(|a, b| b.tokens.total.cmp(&a.tokens.total).then(a.name.cmp(&b.name)))
        }
        ListSort::Sessions => {
            skills.sort_by(|a, b| b.usage.sessions.cmp(&a.usage.sessions).then(a.name.cmp(&b.name)))
        }
        ListSort::Path => skills.sort_by(|a, b| a.path.cmp(&b.path)),
    }
}

fn tier_key(t: SkillTier) -> u8 {
    match t {
        SkillTier::Primary => 0,
        SkillTier::Secondary => 1,
    }
}

fn emit_list(response: &ListResponse<'_>, format: OutputFormat, pretty: bool) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(response, pretty),
        OutputFormat::Jsonl => write_jsonl(response.skills.iter()),
        OutputFormat::Ids => write_lines(response.skills.iter().map(|s| s.id.to_string())),
        OutputFormat::Paths => write_lines(response.skills.iter().map(|s| s.path.to_string())),
        OutputFormat::Names => write_lines(response.skills.iter().map(|s| s.name.to_string())),
        OutputFormat::Tsv => write_tsv(
            &[
                "id",
                "agent",
                "tier",
                "name",
                "mentions",
                "sessions",
                "tokens",
                "cluster",
                "validation",
                "path",
            ],
            response.skills.iter().map(|s| {
                vec![
                    s.id.to_string(),
                    s.agent.to_string(),
                    format!("{:?}", s.tier).to_lowercase(),
                    s.name.to_string(),
                    s.usage_mentions.to_string(),
                    s.usage_sessions.to_string(),
                    s.tokens_total.to_string(),
                    s.cluster_id.unwrap_or("").to_string(),
                    if s.validation_ok { "ok" } else { "fail" }.to_string(),
                    s.path.to_string(),
                ]
            }),
        ),
    }
}

// ---------- show ----------

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

// ---------- dups ----------

#[derive(Serialize)]
struct DupsResponse<'a> {
    count: usize,
    clusters: Vec<ClusterView<'a>>,
}

struct DupsFilters<'a> {
    only_exact: bool,
    only_near: bool,
    min_size: Option<usize>,
    agent: Option<&'a str>,
    root_kind: Option<RootKind>,
    sort: DupsSort,
    limit: Option<usize>,
}

fn list_dups<'a>(inv: &'a Inventory, f: DupsFilters<'_>) -> DupsResponse<'a> {
    let mut clusters: Vec<&Cluster> = inv
        .clusters
        .iter()
        .filter(|c| match (f.only_exact, f.only_near) {
            (true, false) => c.kind == ClusterKind::Exact,
            (false, true) => c.kind == ClusterKind::Near,
            _ => true,
        })
        .filter(|c| f.min_size.map_or(true, |min| c.members.len() >= min))
        .filter(|c| match f.agent {
            Some(name) => c.members.iter().any(|mid| {
                inv.skills
                    .iter()
                    .any(|s| s.id == *mid && s.agent.eq_ignore_ascii_case(name))
            }),
            None => true,
        })
        .filter(|c| match f.root_kind {
            Some(rk) => c.members.iter().any(|mid| {
                inv.skills
                    .iter()
                    .find(|s| s.id == *mid)
                    .and_then(|s| root_kind_of(inv, &s.root_id))
                    .map_or(false, |k| k == rk)
            }),
            None => true,
        })
        .collect();

    sort_clusters(&mut clusters, f.sort);

    if let Some(limit) = f.limit {
        clusters.truncate(limit);
    }

    let clusters: Vec<ClusterView<'a>> = clusters
        .into_iter()
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

fn sort_clusters(clusters: &mut [&Cluster], sort: DupsSort) {
    match sort {
        DupsSort::Size => clusters.sort_by(|a, b| {
            b.members
                .len()
                .cmp(&a.members.len())
                .then_with(|| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal))
        }),
        DupsSort::Similarity => clusters.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.members.len().cmp(&a.members.len()))
        }),
        DupsSort::Kind => clusters.sort_by(|a, b| {
            cluster_kind_key(a.kind)
                .cmp(&cluster_kind_key(b.kind))
                .then(b.members.len().cmp(&a.members.len()))
        }),
    }
}

fn cluster_kind_key(k: ClusterKind) -> u8 {
    match k {
        ClusterKind::Exact => 0,
        ClusterKind::Near => 1,
    }
}

fn emit_dups(response: &DupsResponse<'_>, format: OutputFormat, pretty: bool) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(response, pretty),
        OutputFormat::Jsonl => write_jsonl(response.clusters.iter()),
        OutputFormat::Ids => write_lines(response.clusters.iter().map(|c| c.cluster.id.clone())),
        OutputFormat::Paths => write_lines(
            response
                .clusters
                .iter()
                .flat_map(|c| c.members.iter().map(|m| m.path.to_string())),
        ),
        OutputFormat::Names => write_lines(
            response
                .clusters
                .iter()
                .flat_map(|c| c.members.iter().map(|m| m.name.to_string())),
        ),
        OutputFormat::Tsv => write_tsv(
            &["cluster_id", "kind", "similarity", "size", "agents", "members"],
            response.clusters.iter().map(|c| {
                let agents: Vec<&str> = {
                    let mut seen: Vec<&str> = c.members.iter().map(|m| m.agent).collect();
                    seen.sort();
                    seen.dedup();
                    seen
                };
                vec![
                    c.cluster.id.clone(),
                    format!("{:?}", c.cluster.kind).to_lowercase(),
                    format!("{:.3}", c.cluster.similarity),
                    c.cluster.members.len().to_string(),
                    agents.join(","),
                    c.members
                        .iter()
                        .map(|m| format!("{}:{}", m.agent, m.name))
                        .collect::<Vec<_>>()
                        .join("|"),
                ]
            }),
        ),
    }
}

// ---------- usage ----------

#[derive(Serialize)]
struct UsageRow<'a> {
    id: &'a str,
    name: &'a str,
    agent: &'a str,
    mentions: u64,
    sessions: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_seen_at: Option<&'a str>,
    confidence: UsageConfidence,
    by_source: &'a BTreeMap<String, u64>,
}

#[derive(Serialize)]
struct UsageResponse<'a> {
    count: usize,
    skills: Vec<UsageRow<'a>>,
}

struct UsageFilters<'a> {
    agent: Option<&'a str>,
    min_mentions: u64,
    top: usize,
    sort: UsageSort,
    include_low: bool,
}

fn list_usage<'a>(inv: &'a Inventory, f: UsageFilters<'_>) -> UsageResponse<'a> {
    let mut rows: Vec<&Skill> = inv
        .skills
        .iter()
        .filter(|s| f.agent.map_or(true, |a| s.agent.eq_ignore_ascii_case(a)))
        .filter(|s| s.usage.mentions >= f.min_mentions)
        .filter(|s| f.include_low || s.usage.confidence != UsageConfidence::Low)
        .collect();

    match f.sort {
        UsageSort::Mentions => {
            rows.sort_by(|a, b| b.usage.mentions.cmp(&a.usage.mentions).then(a.name.cmp(&b.name)))
        }
        UsageSort::Sessions => {
            rows.sort_by(|a, b| b.usage.sessions.cmp(&a.usage.sessions).then(a.name.cmp(&b.name)))
        }
        UsageSort::Recent => {
            // ISO-8601 sorts lexicographically; missing values go last.
            rows.sort_by(|a, b| match (a.usage.last_seen_at.as_deref(), b.usage.last_seen_at.as_deref()) {
                (Some(x), Some(y)) => y.cmp(x),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.name.cmp(&b.name),
            });
        }
        UsageSort::Name => rows.sort_by(|a, b| a.name.cmp(&b.name)),
    }

    if f.top > 0 {
        rows.truncate(f.top);
    }

    let skills: Vec<UsageRow<'a>> = rows
        .into_iter()
        .map(|s| UsageRow {
            id: &s.id,
            name: &s.name,
            agent: &s.agent,
            mentions: s.usage.mentions,
            sessions: s.usage.sessions,
            last_seen_at: s.usage.last_seen_at.as_deref(),
            confidence: s.usage.confidence,
            by_source: &s.usage.by_source,
        })
        .collect();

    UsageResponse {
        count: skills.len(),
        skills,
    }
}

fn emit_usage(response: &UsageResponse<'_>, format: OutputFormat, pretty: bool) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(response, pretty),
        OutputFormat::Jsonl => write_jsonl(response.skills.iter()),
        OutputFormat::Ids => write_lines(response.skills.iter().map(|s| s.id.to_string())),
        OutputFormat::Names => write_lines(response.skills.iter().map(|s| s.name.to_string())),
        OutputFormat::Paths => Err(anyhow!(
            "`--format paths` is not supported for `usage` (use `list --has-usage --format paths`)"
        )),
        OutputFormat::Tsv => write_tsv(
            &[
                "id",
                "agent",
                "name",
                "mentions",
                "sessions",
                "confidence",
                "last_seen_at",
                "by_source",
            ],
            response.skills.iter().map(|s| {
                let by_source: String = s
                    .by_source
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(",");
                vec![
                    s.id.to_string(),
                    s.agent.to_string(),
                    s.name.to_string(),
                    s.mentions.to_string(),
                    s.sessions.to_string(),
                    format!("{:?}", s.confidence).to_lowercase(),
                    s.last_seen_at.unwrap_or("").to_string(),
                    by_source,
                ]
            }),
        ),
    }
}

// ---------- agents ----------

#[derive(Serialize)]
struct AgentRow {
    agent: String,
    skills: usize,
    primary: usize,
    secondary: usize,
    dup_members: usize,
    validation_failures: usize,
    usage_mentions: u64,
    usage_sessions: u64,
    roots: usize,
}

#[derive(Serialize)]
struct AgentsResponse {
    count: usize,
    agents: Vec<AgentRow>,
}

fn list_agents(inv: &Inventory) -> AgentsResponse {
    let mut by_agent: BTreeMap<String, AgentRow> = BTreeMap::new();
    let mut roots_by_agent: BTreeMap<String, std::collections::HashSet<String>> = BTreeMap::new();

    for s in &inv.skills {
        let entry = by_agent.entry(s.agent.clone()).or_insert_with(|| AgentRow {
            agent: s.agent.clone(),
            skills: 0,
            primary: 0,
            secondary: 0,
            dup_members: 0,
            validation_failures: 0,
            usage_mentions: 0,
            usage_sessions: 0,
            roots: 0,
        });
        entry.skills += 1;
        match s.tier {
            SkillTier::Primary => entry.primary += 1,
            SkillTier::Secondary => entry.secondary += 1,
        }
        if s.cluster_id.is_some() {
            entry.dup_members += 1;
        }
        if !s.validation.ok {
            entry.validation_failures += 1;
        }
        entry.usage_mentions += s.usage.mentions;
        entry.usage_sessions += s.usage.sessions;
        roots_by_agent
            .entry(s.agent.clone())
            .or_default()
            .insert(s.root_id.clone());
    }

    for (agent, row) in by_agent.iter_mut() {
        row.roots = roots_by_agent.get(agent).map(|s| s.len()).unwrap_or(0);
    }

    let mut agents: Vec<AgentRow> = by_agent.into_values().collect();
    agents.sort_by(|a, b| b.skills.cmp(&a.skills).then(a.agent.cmp(&b.agent)));

    AgentsResponse {
        count: agents.len(),
        agents,
    }
}

fn emit_agents(response: &AgentsResponse, format: OutputFormat, pretty: bool) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(response, pretty),
        OutputFormat::Jsonl => write_jsonl(response.agents.iter()),
        OutputFormat::Names | OutputFormat::Ids => {
            write_lines(response.agents.iter().map(|a| a.agent.clone()))
        }
        OutputFormat::Paths => Err(anyhow!(
            "`--format paths` is not supported for `agents` (try `skillview roots --format paths`)"
        )),
        OutputFormat::Tsv => write_tsv(
            &[
                "agent",
                "skills",
                "primary",
                "secondary",
                "dup_members",
                "validation_failures",
                "usage_mentions",
                "usage_sessions",
                "roots",
            ],
            response.agents.iter().map(|a| {
                vec![
                    a.agent.clone(),
                    a.skills.to_string(),
                    a.primary.to_string(),
                    a.secondary.to_string(),
                    a.dup_members.to_string(),
                    a.validation_failures.to_string(),
                    a.usage_mentions.to_string(),
                    a.usage_sessions.to_string(),
                    a.roots.to_string(),
                ]
            }),
        ),
    }
}

// ---------- roots ----------

#[derive(Serialize)]
struct RootRow<'a> {
    root_id: &'a str,
    kind: RootKind,
    path: &'a str,
    skills: usize,
    primary: usize,
    secondary: usize,
}

#[derive(Serialize)]
struct RootsResponse<'a> {
    count: usize,
    roots: Vec<RootRow<'a>>,
}

fn list_roots(inv: &Inventory) -> RootsResponse<'_> {
    let mut roots: Vec<RootRow> = inv
        .roots
        .iter()
        .map(|r| {
            let (primary, secondary) = inv
                .skills
                .iter()
                .filter(|s| s.root_id == r.id)
                .fold((0usize, 0usize), |(p, sec), s| match s.tier {
                    SkillTier::Primary => (p + 1, sec),
                    SkillTier::Secondary => (p, sec + 1),
                });
            RootRow {
                root_id: &r.id,
                kind: r.kind,
                path: &r.path,
                skills: primary + secondary,
                primary,
                secondary,
            }
        })
        .collect();

    roots.sort_by(|a, b| b.skills.cmp(&a.skills).then(a.path.cmp(b.path)));

    RootsResponse {
        count: roots.len(),
        roots,
    }
}

fn emit_roots(response: &RootsResponse<'_>, format: OutputFormat, pretty: bool) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(response, pretty),
        OutputFormat::Jsonl => write_jsonl(response.roots.iter()),
        OutputFormat::Ids => write_lines(response.roots.iter().map(|r| r.root_id.to_string())),
        OutputFormat::Paths => write_lines(response.roots.iter().map(|r| r.path.to_string())),
        OutputFormat::Names => write_lines(response.roots.iter().map(|r| {
            let kind = serde_json::to_string(&r.kind)
                .unwrap_or_else(|_| "\"unknown\"".to_string())
                .trim_matches('"')
                .to_string();
            kind
        })),
        OutputFormat::Tsv => write_tsv(
            &["root_id", "kind", "skills", "primary", "secondary", "path"],
            response.roots.iter().map(|r| {
                let kind = serde_json::to_string(&r.kind)
                    .unwrap_or_else(|_| "\"unknown\"".to_string())
                    .trim_matches('"')
                    .to_string();
                vec![
                    r.root_id.to_string(),
                    kind,
                    r.skills.to_string(),
                    r.primary.to_string(),
                    r.secondary.to_string(),
                    r.path.to_string(),
                ]
            }),
        ),
    }
}

// ---------- stats ----------

#[derive(Serialize)]
struct StatsResponse<'a> {
    generated_at: &'a str,
    scanned_paths: usize,
    elapsed_ms: u128,
    primary_skills: usize,
    secondary_skills: usize,
    total_skills: usize,
    duplicate_clusters: usize,
    exact_clusters: usize,
    near_clusters: usize,
    skills_in_clusters: usize,
    validation_failures: usize,
    usage_session_files: usize,
    usage_bytes_scanned: u64,
    usage_elapsed_ms: u128,
    usage_high_confidence: usize,
    usage_low_confidence: usize,
    skills_with_usage: usize,
    total_usage_mentions: u64,
    total_usage_sessions: u64,
    agents: BTreeMap<String, usize>,
    root_kinds: BTreeMap<String, usize>,
}

fn build_stats(inv: &Inventory) -> StatsResponse<'_> {
    let exact_clusters = inv.clusters.iter().filter(|c| c.kind == ClusterKind::Exact).count();
    let near_clusters = inv.clusters.iter().filter(|c| c.kind == ClusterKind::Near).count();
    let skills_in_clusters = inv.skills.iter().filter(|s| s.cluster_id.is_some()).count();
    let validation_failures = inv.skills.iter().filter(|s| !s.validation.ok).count();
    let usage_high = inv
        .skills
        .iter()
        .filter(|s| s.usage.confidence == UsageConfidence::High)
        .count();
    let usage_low = inv.skills.len() - usage_high;
    let skills_with_usage = inv.skills.iter().filter(|s| s.usage.mentions > 0).count();
    let total_mentions: u64 = inv.skills.iter().map(|s| s.usage.mentions).sum();
    let total_sessions: u64 = inv.skills.iter().map(|s| s.usage.sessions).sum();

    let mut agents: BTreeMap<String, usize> = BTreeMap::new();
    for s in &inv.skills {
        *agents.entry(s.agent.clone()).or_insert(0) += 1;
    }
    let mut root_kinds: BTreeMap<String, usize> = BTreeMap::new();
    for r in &inv.roots {
        let kind = serde_json::to_string(&r.kind)
            .unwrap_or_else(|_| "\"unknown\"".to_string())
            .trim_matches('"')
            .to_string();
        *root_kinds.entry(kind).or_insert(0) += 1;
    }

    StatsResponse {
        generated_at: &inv.generated_at,
        scanned_paths: inv.stats.scanned_paths,
        elapsed_ms: inv.stats.elapsed_ms,
        primary_skills: inv.stats.primary_skills,
        secondary_skills: inv.stats.secondary_skills,
        total_skills: inv.skills.len(),
        duplicate_clusters: inv.stats.duplicate_clusters,
        exact_clusters,
        near_clusters,
        skills_in_clusters,
        validation_failures,
        usage_session_files: inv.stats.usage_session_files,
        usage_bytes_scanned: inv.stats.usage_bytes_scanned,
        usage_elapsed_ms: inv.stats.usage_elapsed_ms,
        usage_high_confidence: usage_high,
        usage_low_confidence: usage_low,
        skills_with_usage,
        total_usage_mentions: total_mentions,
        total_usage_sessions: total_sessions,
        agents,
        root_kinds,
    }
}

// ---------- examples ----------

fn print_examples() {
    // Plain text; one recipe per line so an agent (or grep) can scan it fast.
    let text = "\
skillview examples — copy-pasteable recipes

# Discover what's on this machine
skillview agents                                    # per-agent skill counts
skillview roots                                     # scanned roots + counts
skillview stats --pretty                            # one-shot overview

# Search and filter skills
skillview list --agent claude --tier primary
skillview list --name browser --sort usage --limit 10
skillview list --root-kind claude-global --format ids
skillview list --validation-failed --format paths

# Usage signal
skillview usage --top 20
skillview usage --agent claude --sort sessions
skillview usage --min-mentions 5 --format tsv
skillview usage --include-low                       # show even untrusted counts

# Duplicates
skillview dups                                      # all clusters
skillview dups --exact                              # exact-content only
skillview dups --near --min-size 3
skillview dups --agent claude --sort size --limit 5
skillview dups --format tsv

# Inspect a single skill
skillview show agent-browser --pretty
skillview show s_3
skillview show ~/.claude/skills/agent-browser

# Pipe friendly
skillview list --has-usage --format ids       | xargs -n1 skillview show
skillview dups --near --format ids            | head
skillview list --dups-only --format paths

# Scoping
skillview --root ~/code/project list --tier primary
skillview --no-similarity stats                     # skip MinHash
skillview --no-usage list                           # skip cross-agent usage

# Per-command help
skillview list --help
skillview dups --help
skillview usage --help
";
    let _ = io::stdout().write_all(text.as_bytes());
}

// ---------- --tui launcher ----------

fn launch_tui(scan: &ScanArgs) -> Result<()> {
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
    let mut cmd = Command::new("bun");
    cmd.arg("run").arg(&entry).env("SKILLVIEW_CORE", &self_path);

    // Forward the user's scan flags into the TUI process via env vars. The
    // bridge reads these when spawning `skillview --stream` so the user's
    // `--root`, `--no-usage`, etc. actually take effect — without this they
    // were silently dropped and the TUI always scanned $HOME.
    if let Some(root) = &scan.root {
        cmd.env("SKILLVIEW_TUI_ROOT", root);
    }
    cmd.env("SKILLVIEW_TUI_THRESHOLD", scan.threshold.to_string());
    if scan.no_similarity {
        cmd.env("SKILLVIEW_TUI_NO_SIMILARITY", "1");
    }
    if scan.no_usage {
        cmd.env("SKILLVIEW_TUI_NO_USAGE", "1");
    }
    if scan.include_minhash {
        cmd.env("SKILLVIEW_TUI_INCLUDE_MINHASH", "1");
    }

    let status = cmd.status().map_err(|e| {
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
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors().take(6) {
            if let Some(found) = check_tui(anc) {
                return Some(found);
            }
        }
    }
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
