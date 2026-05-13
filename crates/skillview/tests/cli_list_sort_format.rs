//! E2E tests for `skillview list` covering `--sort`, `--limit`, and `--format`
//! flags. Filter coverage lives in a sibling test file.

mod support;

use support::*;
use serde_json::Value;

// ---------- helpers ----------

/// Extract `(agent, name)` pairs in order from a default-json list response.
fn agent_name_pairs(skills: &[Value]) -> Vec<(String, String)> {
    skills
        .iter()
        .map(|s| {
            (
                s["agent"].as_str().unwrap_or("").to_string(),
                s["name"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

fn names_in_order(skills: &[Value]) -> Vec<String> {
    skills
        .iter()
        .map(|s| s["name"].as_str().unwrap_or("").to_string())
        .collect()
}

/// Run `list` with the fast harness and return its parsed JSON skills array.
fn list_skills_json(args: &[&str]) -> Vec<Value> {
    let mut c = cmd();
    c.arg("list");
    for a in args {
        c.arg(a);
    }
    let out = c.output().expect("spawn skillview");
    let v = parse_json(&out);
    v["skills"].as_array().cloned().unwrap_or_default()
}

/// Same as `list_skills_json`, but uses `cmd_full(&[])` so usage + similarity
/// data is populated.
fn list_skills_json_full(args: &[&str]) -> Vec<Value> {
    let mut c = cmd_full(&[]);
    c.arg("list");
    for a in args {
        c.arg(a);
    }
    let out = c.output().expect("spawn skillview");
    let v = parse_json(&out);
    v["skills"].as_array().cloned().unwrap_or_default()
}

// ---------- --sort ----------

#[test]
fn sort_agent_name_default_groups_by_agent_then_name() {
    // `agent-name` is the default; assert by explicit flag for clarity.
    let skills = list_skills_json(&["--sort", "agent-name"]);
    assert_eq!(skills.len(), 14);

    // Names within the `claude` agent must be ascending.
    let claude_names: Vec<String> = skills
        .iter()
        .filter(|s| s["agent"].as_str() == Some("claude"))
        .map(|s| s["name"].as_str().unwrap_or("").to_string())
        .collect();
    let mut expected = claude_names.clone();
    expected.sort();
    assert_eq!(claude_names, expected, "claude names must be sorted asc");

    // First entry should belong to the lexicographically-first agent (`agents`).
    let pairs = agent_name_pairs(&skills);
    assert_eq!(pairs.first().map(|(a, _)| a.as_str()), Some("agents"));
}

#[test]
fn sort_name_is_ascending_globally() {
    let skills = list_skills_json(&["--sort", "name"]);
    assert_eq!(skills.len(), 14);
    let names = names_in_order(&skills);
    let mut expected = names.clone();
    expected.sort();
    assert_eq!(names, expected);
    // Sanity: first asc is agent-browser, last is short.
    assert_eq!(names.first().map(String::as_str), Some("agent-browser"));
    assert_eq!(names.last().map(String::as_str), Some("short"));
}

#[test]
fn sort_agent_groups_by_agent_then_name() {
    let skills = list_skills_json(&["--sort", "agent"]);
    assert_eq!(skills.len(), 14);
    let pairs = agent_name_pairs(&skills);

    // All `agents`-agent rows come before `claude`, which comes before `codex`,
    // which comes before `cursor`, which comes before `unknown`.
    let agents: Vec<&str> = pairs.iter().map(|(a, _)| a.as_str()).collect();
    let first_agents = agents.iter().position(|a| *a != "agents").unwrap_or(agents.len());
    assert!(agents[..first_agents].iter().all(|a| *a == "agents"));

    // The sequence of distinct agents in order is sorted ascending.
    let mut seen: Vec<&str> = Vec::new();
    for a in &agents {
        if seen.last().copied() != Some(*a) {
            seen.push(*a);
        }
    }
    let mut sorted_seen = seen.clone();
    sorted_seen.sort();
    assert_eq!(seen, sorted_seen, "agents must appear in ascending order");
}

#[test]
fn sort_tier_puts_primary_first_secondary_last() {
    let skills = list_skills_json(&["--sort", "tier"]);
    assert_eq!(skills.len(), 14);

    // First 13 are primary; last is the lone secondary (`secondary-example`).
    let tiers: Vec<&str> = skills
        .iter()
        .map(|s| s["tier"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(tiers.iter().filter(|t| **t == "primary").count(), 13);
    assert_eq!(tiers.iter().filter(|t| **t == "secondary").count(), 1);
    assert_eq!(tiers.last().copied(), Some("secondary"));

    let last_name = skills.last().unwrap()["name"].as_str().unwrap_or("");
    assert_eq!(last_name, "secondary-example");
}

#[test]
fn sort_usage_descending_by_mentions() {
    let skills = list_skills_json_full(&["--sort", "usage"]);
    assert_eq!(skills.len(), 14);

    // agent-browser has 5 mentions, the most in the fixture.
    let first = &skills[0];
    assert_eq!(first["name"].as_str(), Some("agent-browser"));
    assert_eq!(first["usage_mentions"].as_u64(), Some(5));

    // Mentions are non-increasing across the full list.
    let mentions: Vec<u64> = skills
        .iter()
        .map(|s| s["usage_mentions"].as_u64().unwrap_or(0))
        .collect();
    for w in mentions.windows(2) {
        assert!(w[0] >= w[1], "mentions not monotonically non-increasing: {mentions:?}");
    }
}

#[test]
fn sort_tokens_descending_monotonic() {
    let skills = list_skills_json(&["--sort", "tokens"]);
    assert_eq!(skills.len(), 14);

    let tokens: Vec<u64> = skills
        .iter()
        .map(|s| s["tokens_total"].as_u64().unwrap_or(0))
        .collect();
    assert!(!tokens.is_empty());
    assert!(tokens.first() >= tokens.last(), "first tokens < last tokens");
    for w in tokens.windows(2) {
        assert!(w[0] >= w[1], "tokens not non-increasing: {tokens:?}");
    }
}

#[test]
fn sort_sessions_descending_by_session_count() {
    let skills = list_skills_json_full(&["--sort", "sessions"]);
    assert_eq!(skills.len(), 14);

    // agent-browser has 2 sessions, the most in the fixture.
    let first = &skills[0];
    assert_eq!(first["name"].as_str(), Some("agent-browser"));
    assert_eq!(first["usage_sessions"].as_u64(), Some(2));

    let sessions: Vec<u64> = skills
        .iter()
        .map(|s| s["usage_sessions"].as_u64().unwrap_or(0))
        .collect();
    for w in sessions.windows(2) {
        assert!(w[0] >= w[1], "sessions not non-increasing: {sessions:?}");
    }
}

#[test]
fn sort_path_ascending_monotonic() {
    let skills = list_skills_json(&["--sort", "path"]);
    assert_eq!(skills.len(), 14);

    let paths: Vec<String> = skills
        .iter()
        .map(|s| s["path"].as_str().unwrap_or("").to_string())
        .collect();
    for w in paths.windows(2) {
        assert!(w[0] <= w[1], "paths not non-decreasing:\n{paths:#?}");
    }
}

#[test]
fn sort_bogus_mode_fails() {
    let out = cmd().args(["list", "--sort", "bogus-mode"]).output().expect("spawn");
    assert_failure(&out);
}

// ---------- --limit ----------

#[test]
fn limit_truncates_to_requested_count() {
    let skills = list_skills_json(&["--limit", "5"]);
    assert_eq!(skills.len(), 5);
}

#[test]
fn limit_zero_yields_empty_list() {
    let skills = list_skills_json(&["--limit", "0"]);
    assert_eq!(skills.len(), 0);
}

#[test]
fn limit_larger_than_total_keeps_all() {
    let skills = list_skills_json(&["--limit", "999"]);
    assert_eq!(skills.len(), 14);
}

#[test]
fn limit_combines_with_sort_name() {
    let skills = list_skills_json(&["--sort", "name", "--limit", "3"]);
    let names = names_in_order(&skills);
    assert_eq!(names, vec!["agent-browser", "bad-skill", "codex-helper"]);
}

// ---------- --format ----------

#[test]
fn format_json_wraps_count_and_skills() {
    let out = cmd().args(["list", "--format", "json"]).output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(v["count"].as_u64(), Some(14));
    let skills = v["skills"].as_array().expect("skills array");
    assert_eq!(skills.len(), 14);
}

#[test]
fn format_jsonl_emits_one_object_per_line() {
    let out = cmd().args(["list", "--format", "jsonl"]).output().expect("spawn");
    let lines = parse_jsonl(&out);
    assert_eq!(lines.len(), 14);
    for line in &lines {
        // Every line must look like a SkillSummary record.
        assert!(line["name"].is_string(), "jsonl row missing name: {line}");
        assert!(line["agent"].is_string(), "jsonl row missing agent: {line}");
        assert!(line["path"].is_string(), "jsonl row missing path: {line}");
    }
}

#[test]
fn format_jsonl_respects_sort_and_limit() {
    let out = cmd()
        .args(["list", "--format", "jsonl", "--sort", "name", "--limit", "3"])
        .output()
        .expect("spawn");
    let lines = parse_jsonl(&out);
    assert_eq!(lines.len(), 3);
    let names: Vec<&str> = lines.iter().map(|l| l["name"].as_str().unwrap_or("")).collect();
    assert_eq!(names, vec!["agent-browser", "bad-skill", "codex-helper"]);
}

#[test]
fn format_tsv_has_expected_header_and_rows() {
    let out = cmd().args(["list", "--format", "tsv"]).output().expect("spawn");
    let (header, rows) = parse_tsv(&out);
    assert_eq!(
        header,
        vec![
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
        ]
    );
    assert_eq!(rows.len(), 14);

    let name_idx = header.iter().position(|h| h == "name").unwrap();
    let validation_idx = header.iter().position(|h| h == "validation").unwrap();
    let tier_idx = header.iter().position(|h| h == "tier").unwrap();

    // bad-skill validation column == "fail".
    let bad_row = rows
        .iter()
        .find(|r| r[name_idx] == "bad-skill")
        .expect("bad-skill row present");
    assert_eq!(bad_row[validation_idx], "fail");

    // agent-browser validation column == "ok".
    let good_row = rows
        .iter()
        .find(|r| r[name_idx] == "agent-browser")
        .expect("agent-browser row present");
    assert_eq!(good_row[validation_idx], "ok");

    // Every tier column is lowercase primary or secondary.
    for r in &rows {
        let t = &r[tier_idx];
        assert!(
            t == "primary" || t == "secondary",
            "unexpected tier value {t:?} in row {r:?}"
        );
    }
}

#[test]
fn format_tsv_combined_with_validation_failed_filter() {
    let out = cmd()
        .args(["list", "--format", "tsv", "--validation-failed"])
        .output()
        .expect("spawn");
    let (header, rows) = parse_tsv(&out);
    assert_eq!(rows.len(), 1);
    let name_idx = header.iter().position(|h| h == "name").unwrap();
    assert_eq!(rows[0][name_idx], "bad-skill");
}

#[test]
fn format_ids_emits_skill_id_per_line() {
    let out = cmd().args(["list", "--format", "ids"]).output().expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 14);
    for line in &lines {
        assert!(
            line.starts_with("s_") && line[2..].chars().all(|c| c.is_ascii_digit()),
            "expected s_<digits> id, got {line:?}"
        );
    }
}

#[test]
fn format_paths_emits_absolute_paths_per_line() {
    let out = cmd().args(["list", "--format", "paths"]).output().expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 14);
    for line in &lines {
        assert!(line.starts_with('/'), "path must be absolute: {line:?}");
        let lower = line.to_lowercase();
        assert!(
            lower.ends_with("skill.md"),
            "path must end with SKILL.md / skill.md: {line:?}"
        );
    }
}

#[test]
fn format_paths_with_name_filter_returns_single_known_path() {
    let out = cmd()
        .args(["list", "--format", "paths", "--name", "agent-browser"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("/.claude/skills/agent-browser/SKILL.md"),
        "unexpected path: {}",
        lines[0]
    );
}

#[test]
fn format_names_emits_name_per_line() {
    let out = cmd().args(["list", "--format", "names"]).output().expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 14);
    // Names use kebab-case alphanumerics; just sanity-check non-empty + no whitespace.
    for line in &lines {
        assert!(!line.is_empty());
        assert!(!line.chars().any(|c| c.is_whitespace()), "name has whitespace: {line:?}");
    }
}

#[test]
fn format_names_with_validation_failed_filter_returns_bad_skill_only() {
    let out = cmd()
        .args(["list", "--validation-failed", "--format", "names"])
        .output()
        .expect("spawn");
    assert_success(&out);
    assert_eq!(stdout_str(&out), "bad-skill\n");
}

#[test]
fn format_bogus_mode_fails() {
    let out = cmd()
        .args(["list", "--format", "bogus-format"])
        .output()
        .expect("spawn");
    assert_failure(&out);
}

// ---------- composition ----------

#[test]
fn compose_agent_sort_tokens_limit_names_for_claude() {
    let out = cmd()
        .args([
            "list",
            "--agent",
            "claude",
            "--sort",
            "tokens",
            "--limit",
            "3",
            "--format",
            "names",
        ])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 3);

    // Every returned name must be one of claude's 8 skills.
    let claude_names = [
        "agent-browser",
        "bad-skill",
        "data-analyst",
        "dup-recipe",
        "near-recipe",
        "proj-skill",
        "secondary-example",
        "short",
    ];
    for line in &lines {
        assert!(
            claude_names.contains(&line.as_str()),
            "name {line:?} is not one of claude's skills"
        );
    }
}

#[test]
fn compose_validation_failed_paths_yields_bad_skill_path() {
    let out = cmd()
        .args(["list", "--validation-failed", "--format", "paths"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("bad-skill/SKILL.md"),
        "expected bad-skill/SKILL.md in path, got {}",
        lines[0]
    );
}

#[test]
fn compose_sort_usage_limit_names_top_is_agent_browser() {
    let mut c = cmd_full(&[]);
    let out = c
        .args(["list", "--sort", "usage", "--limit", "3", "--format", "names"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "agent-browser");
}
