//! E2E tests for the `agents`, `roots`, and `stats` subcommands of skillview.
//!
//! These rollup commands all derive their numbers from the same scan that
//! powers `list`/`scan`, so the assertions focus on the per-agent and
//! per-root aggregations as well as the cross-cutting `stats` snapshot.

mod support;

use serde_json::Value;
use support::*;

// ---------- helpers ----------

/// Pull the `agents` array out of an `agents` JSON response.
fn agents_array(v: &Value) -> Vec<Value> {
    v["agents"].as_array().cloned().unwrap_or_default()
}

/// Pull the `roots` array out of a `roots` JSON response.
fn roots_array(v: &Value) -> Vec<Value> {
    v["roots"].as_array().cloned().unwrap_or_default()
}

/// Find the agent row by name (the list is sorted by skills desc, ties broken
/// by name asc; we look up by field to keep tests robust).
fn agent_row<'a>(agents: &'a [Value], name: &str) -> &'a Value {
    agents
        .iter()
        .find(|a| a["agent"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no agent row for {name:?}"))
}

/// Find the root row by kind. When multiple roots share a kind we return the
/// one with the most skills (so e.g. the larger claude-* root for that kind).
fn root_row_by_kind<'a>(roots: &'a [Value], kind: &str) -> &'a Value {
    roots
        .iter()
        .filter(|r| r["kind"].as_str() == Some(kind))
        .max_by_key(|r| r["skills"].as_u64().unwrap_or(0))
        .unwrap_or_else(|| panic!("no root with kind {kind:?}"))
}

// ====================================================================
// agents
// ====================================================================

#[test]
fn agents_default_json_shape_and_counts() {
    let out = cmd().args(["agents"]).output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(v["count"].as_u64(), Some(5), "expected 5 agents");

    let agents = agents_array(&v);
    assert_eq!(agents.len(), 5);

    // claude row.
    let claude = agent_row(&agents, "claude");
    assert_eq!(claude["skills"].as_u64(), Some(8));
    assert_eq!(claude["primary"].as_u64(), Some(7));
    assert_eq!(claude["secondary"].as_u64(), Some(1));
    assert_eq!(claude["validation_failures"].as_u64(), Some(1));
    assert_eq!(claude["roots"].as_u64(), Some(2));

    // codex row.
    let codex = agent_row(&agents, "codex");
    assert_eq!(codex["skills"].as_u64(), Some(3));
    assert_eq!(codex["primary"].as_u64(), Some(3));
    assert_eq!(codex["secondary"].as_u64(), Some(0));

    // cursor / agents / orphan each have skills=1. (The fixture's orphan
    // skill at `~/orphan/skills/orphan-skill/SKILL.md` gets agent="orphan"
    // from the namespace-derivation fallback — the top-level dir under
    // $HOME is the agent name.)
    for name in &["cursor", "agents", "orphan"] {
        let row = agent_row(&agents, name);
        assert_eq!(
            row["skills"].as_u64(),
            Some(1),
            "{name} should have skills=1"
        );
    }

    // Sorted by skills desc — first row should have >= every other's skills.
    let first_skills = agents[0]["skills"].as_u64().unwrap();
    for a in &agents[1..] {
        assert!(
            first_skills >= a["skills"].as_u64().unwrap(),
            "agents not sorted by skills desc: {agents:?}"
        );
    }
}

#[test]
fn agents_with_similarity_default_threshold_dup_members() {
    let out = cmd_full(&[]).args(["agents"]).output().expect("spawn");
    let v = parse_json(&out);
    let agents = agents_array(&v);

    assert_eq!(agent_row(&agents, "claude")["dup_members"].as_u64(), Some(1));
    assert_eq!(agent_row(&agents, "codex")["dup_members"].as_u64(), Some(1));
    for name in &["cursor", "agents", "orphan"] {
        assert_eq!(
            agent_row(&agents, name)["dup_members"].as_u64(),
            Some(0),
            "{name} should have 0 dup_members"
        );
    }
}

#[test]
fn agents_with_similarity_threshold_06_catches_near_dups() {
    let out = cmd_full(&["--threshold", "0.6"])
        .args(["agents"])
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    let agents = agents_array(&v);

    assert_eq!(agent_row(&agents, "claude")["dup_members"].as_u64(), Some(2));
    assert_eq!(agent_row(&agents, "codex")["dup_members"].as_u64(), Some(2));
}

#[test]
fn agents_with_usage_aggregates_per_agent() {
    let out = cmd_full(&[]).args(["agents"]).output().expect("spawn");
    let v = parse_json(&out);
    let agents = agents_array(&v);

    let claude = agent_row(&agents, "claude");
    assert_eq!(claude["usage_mentions"].as_u64(), Some(6));
    assert_eq!(claude["usage_sessions"].as_u64(), Some(3));

    let codex = agent_row(&agents, "codex");
    assert_eq!(codex["usage_mentions"].as_u64(), Some(2));
    assert_eq!(codex["usage_sessions"].as_u64(), Some(1));
}

#[test]
fn agents_format_tsv_header_and_row_count() {
    let out = cmd()
        .args(["agents", "--format", "tsv"])
        .output()
        .expect("spawn");
    let (header, rows) = parse_tsv(&out);
    assert_eq!(
        header,
        vec![
            "agent",
            "skills",
            "primary",
            "secondary",
            "dup_members",
            "validation_failures",
            "usage_mentions",
            "usage_sessions",
            "roots",
        ]
    );
    assert_eq!(rows.len(), 5);
}

#[test]
fn agents_format_jsonl_one_per_line() {
    let out = cmd()
        .args(["agents", "--format", "jsonl"])
        .output()
        .expect("spawn");
    let lines = parse_jsonl(&out);
    assert_eq!(lines.len(), 5);
    for line in &lines {
        assert!(line["agent"].is_string(), "jsonl row missing agent: {line}");
        assert!(line["skills"].is_u64(), "jsonl row missing skills: {line}");
    }
}

#[test]
fn agents_format_names_emits_agent_names() {
    let out = cmd()
        .args(["agents", "--format", "names"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 5);
    let expected: std::collections::HashSet<&str> =
        ["claude", "codex", "cursor", "agents", "orphan"]
            .iter()
            .copied()
            .collect();
    let got: std::collections::HashSet<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(got, expected, "names output: {lines:?}");
}

#[test]
fn agents_format_ids_emits_one_agent_per_line() {
    // `ids` for agents prints the same content as `names` since agents don't
    // have a separate id field; just sanity-check the row count.
    let out = cmd()
        .args(["agents", "--format", "ids"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 5);
}

#[test]
fn agents_format_paths_fails_with_helpful_stderr() {
    let out = cmd()
        .args(["agents", "--format", "paths"])
        .output()
        .expect("spawn");
    assert_failure(&out);
    let err = stderr_str(&out);
    assert!(
        err.contains("paths"),
        "stderr should mention 'paths': {err}"
    );
    assert!(
        err.contains("roots"),
        "stderr should suggest `roots`: {err}"
    );
}

#[test]
fn agents_format_bogus_fails() {
    let out = cmd()
        .args(["agents", "--format", "bogus-format"])
        .output()
        .expect("spawn");
    assert_failure(&out);
}

// ====================================================================
// roots
// ====================================================================

#[test]
fn roots_default_json_count_and_largest() {
    let out = cmd().args(["roots"]).output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(v["count"].as_u64(), Some(6), "expected 6 roots");

    let roots = roots_array(&v);
    assert_eq!(roots.len(), 6);

    // Sum of skills == 14.
    let total: u64 = roots
        .iter()
        .map(|r| r["skills"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(total, 14, "sum of root skills must equal total skill count");

    // Largest root: 7 skills, kind "claude-global".
    let largest = &roots[0];
    assert_eq!(largest["skills"].as_u64(), Some(7));
    assert_eq!(largest["kind"].as_str(), Some("claude-global"));
}

#[test]
fn roots_per_kind_skill_counts() {
    let out = cmd().args(["roots"]).output().expect("spawn");
    let v = parse_json(&out);
    let roots = roots_array(&v);

    assert_eq!(
        root_row_by_kind(&roots, "claude-project")["skills"].as_u64(),
        Some(1)
    );
    assert_eq!(root_row_by_kind(&roots, "codex")["skills"].as_u64(), Some(3));
    assert_eq!(root_row_by_kind(&roots, "cursor")["skills"].as_u64(), Some(1));
    assert_eq!(
        root_row_by_kind(&roots, "agents-generic")["skills"].as_u64(),
        Some(1)
    );
    assert_eq!(
        root_row_by_kind(&roots, "unknown")["skills"].as_u64(),
        Some(1)
    );
}

#[test]
fn roots_primary_secondary_totals() {
    let out = cmd().args(["roots"]).output().expect("spawn");
    let v = parse_json(&out);
    let roots = roots_array(&v);

    let primary: u64 = roots
        .iter()
        .map(|r| r["primary"].as_u64().unwrap_or(0))
        .sum();
    let secondary: u64 = roots
        .iter()
        .map(|r| r["secondary"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(primary, 13);
    assert_eq!(secondary, 1);
}

#[test]
fn roots_paths_are_absolute_under_fixture() {
    let out = cmd().args(["roots"]).output().expect("spawn");
    let v = parse_json(&out);
    let roots = roots_array(&v);

    for r in &roots {
        let path = r["path"].as_str().expect("path string");
        assert!(path.starts_with('/'), "root path not absolute: {path}");
        let matches_known = path.contains(".claude")
            || path.contains(".codex")
            || path.contains(".cursor")
            || path.contains(".agents")
            || path.contains("projects/proj-x")
            || path.contains("orphan");
        assert!(matches_known, "unrecognized root path: {path}");
    }
}

#[test]
fn roots_format_tsv_header_and_rows() {
    let out = cmd()
        .args(["roots", "--format", "tsv"])
        .output()
        .expect("spawn");
    let (header, rows) = parse_tsv(&out);
    assert_eq!(
        header,
        vec!["root_id", "kind", "skills", "primary", "secondary", "path"]
    );
    assert_eq!(rows.len(), 6);
}

#[test]
fn roots_format_ids_emits_six_root_ids() {
    let out = cmd()
        .args(["roots", "--format", "ids"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 6);
    for line in &lines {
        assert!(
            line.starts_with("r_"),
            "expected r_<n> id, got {line:?}"
        );
    }
}

#[test]
fn roots_format_paths_emits_six_absolute_paths() {
    let out = cmd()
        .args(["roots", "--format", "paths"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 6);
    for line in &lines {
        assert!(line.starts_with('/'), "path not absolute: {line}");
    }
}

#[test]
fn roots_format_names_emits_kind_per_line() {
    let out = cmd()
        .args(["roots", "--format", "names"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 6);
    let allowed: std::collections::HashSet<&str> = [
        "claude-global",
        "claude-project",
        "codex",
        "cursor",
        "agents-generic",
        "unknown",
    ]
    .iter()
    .copied()
    .collect();
    for line in &lines {
        assert!(
            allowed.contains(line.as_str()),
            "unexpected name line {line:?}"
        );
    }
}

#[test]
fn roots_format_jsonl_has_required_keys() {
    let out = cmd()
        .args(["roots", "--format", "jsonl"])
        .output()
        .expect("spawn");
    let lines = parse_jsonl(&out);
    assert_eq!(lines.len(), 6);
    for line in &lines {
        assert!(line["root_id"].is_string(), "missing root_id: {line}");
        assert!(line["kind"].is_string(), "missing kind: {line}");
        assert!(line["path"].is_string(), "missing path: {line}");
    }
}

#[test]
fn roots_format_bogus_fails() {
    let out = cmd()
        .args(["roots", "--format", "bogus-format"])
        .output()
        .expect("spawn");
    assert_failure(&out);
}

// ====================================================================
// stats
// ====================================================================

#[test]
fn stats_default_json_has_all_keys_and_baseline_counts() {
    let out = cmd().args(["stats"]).output().expect("spawn");
    let v = parse_json(&out);

    // Required keys all present.
    let expected_keys = [
        "generated_at",
        "scanned_paths",
        "elapsed_ms",
        "primary_skills",
        "secondary_skills",
        "total_skills",
        "duplicate_clusters",
        "exact_clusters",
        "near_clusters",
        "skills_in_clusters",
        "validation_failures",
        "usage_session_files",
        "usage_bytes_scanned",
        "usage_elapsed_ms",
        "usage_high_confidence",
        "usage_low_confidence",
        "skills_with_usage",
        "total_usage_mentions",
        "total_usage_sessions",
        "agents",
        "root_kinds",
    ];
    for key in &expected_keys {
        assert!(v.get(*key).is_some(), "stats response missing key {key:?}");
    }

    // Skills.
    assert_eq!(v["total_skills"].as_u64(), Some(14));
    assert_eq!(v["primary_skills"].as_u64(), Some(13));
    assert_eq!(v["secondary_skills"].as_u64(), Some(1));

    // Similarity skipped → zero clusters.
    assert_eq!(v["duplicate_clusters"].as_u64(), Some(0));
    assert_eq!(v["exact_clusters"].as_u64(), Some(0));
    assert_eq!(v["near_clusters"].as_u64(), Some(0));
    assert_eq!(v["skills_in_clusters"].as_u64(), Some(0));

    // Validation.
    assert_eq!(v["validation_failures"].as_u64(), Some(1));

    // Usage skipped → no session files, no skills with usage.
    assert_eq!(v["usage_session_files"].as_u64(), Some(0));
    assert_eq!(v["skills_with_usage"].as_u64(), Some(0));

    // Agents map covers all five agents.
    let agents = v["agents"].as_object().expect("agents map");
    assert_eq!(agents.get("claude").and_then(Value::as_u64), Some(8));
    assert_eq!(agents.get("codex").and_then(Value::as_u64), Some(3));
    assert_eq!(agents.get("cursor").and_then(Value::as_u64), Some(1));
    assert_eq!(agents.get("agents").and_then(Value::as_u64), Some(1));
    // Was "unknown" pre-namespace-classifier; orphan/ now derives its own
    // agent name. Root kind is still "unknown" (see root_kinds map below).
    assert_eq!(agents.get("orphan").and_then(Value::as_u64), Some(1));

    // root_kinds map covers all six kinds.
    let root_kinds = v["root_kinds"].as_object().expect("root_kinds map");
    for kind in &[
        "claude-global",
        "claude-project",
        "codex",
        "cursor",
        "agents-generic",
        "unknown",
    ] {
        let n = root_kinds.get(*kind).and_then(Value::as_u64).unwrap_or(0);
        assert!(n >= 1, "expected at least 1 root of kind {kind}, got {n}");
    }
}

#[test]
fn stats_with_similarity_default_threshold_one_cluster() {
    let out = cmd_full(&[]).args(["stats"]).output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(v["duplicate_clusters"].as_u64(), Some(1));
    assert_eq!(v["exact_clusters"].as_u64(), Some(1));
    assert_eq!(v["near_clusters"].as_u64(), Some(0));
    assert_eq!(v["skills_in_clusters"].as_u64(), Some(2));
}

#[test]
fn stats_with_similarity_threshold_06_picks_up_near_cluster() {
    let out = cmd_full(&["--threshold", "0.6"])
        .args(["stats"])
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(v["duplicate_clusters"].as_u64(), Some(2));
    assert_eq!(v["exact_clusters"].as_u64(), Some(1));
    assert_eq!(v["near_clusters"].as_u64(), Some(1));
    assert_eq!(v["skills_in_clusters"].as_u64(), Some(4));
}

#[test]
fn stats_with_usage_populates_session_and_confidence_fields() {
    let out = cmd_full(&[]).args(["stats"]).output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(v["usage_session_files"].as_u64(), Some(2));
    assert_eq!(v["skills_with_usage"].as_u64(), Some(3));
    assert_eq!(v["total_usage_mentions"].as_u64(), Some(8));
    assert_eq!(v["total_usage_sessions"].as_u64(), Some(4));
    assert_eq!(v["usage_high_confidence"].as_u64(), Some(13));
    assert_eq!(v["usage_low_confidence"].as_u64(), Some(1));
}

#[test]
fn stats_pretty_indents_output() {
    let out = cmd().args(["stats", "--pretty"]).output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    // Multiple lines, each indented (or at least the inner fields are).
    let line_count = s.lines().count();
    assert!(
        line_count > 5,
        "expected multi-line pretty output, got {line_count} lines:\n{s}"
    );
    assert!(
        s.lines().any(|l| l.starts_with("  ")),
        "expected indented lines in pretty output:\n{s}"
    );
}

#[test]
fn stats_rejects_format_flag() {
    // `stats` has no `--format` flag; clap should bail out as unknown.
    let out = cmd()
        .args(["stats", "--format", "json"])
        .output()
        .expect("spawn");
    assert_failure(&out);
}
