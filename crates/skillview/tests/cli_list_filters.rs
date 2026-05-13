//! End-to-end tests for `skillview list` filter flags.
//!
//! Covers --agent, --tier, --root-kind, --name, --dups-only/--dup-kind,
//! --has-usage/--min-usage, --min-tokens/--max-tokens, --validation-failed,
//! and AND-composition of multiple filters. Sort/limit/format are owned by
//! a separate test file and are deliberately not exercised here.

mod support;

use serde_json::Value;
use support::{cmd, cmd_full, parse_json};

/// Pull `(count, skills_array)` from a `list` JSON response.
fn list_payload(out: &std::process::Output) -> (usize, Vec<Value>) {
    let inv = parse_json(out);
    let count = inv["count"].as_u64().expect("count is u64") as usize;
    let skills = inv["skills"]
        .as_array()
        .expect("skills is array")
        .clone();
    assert_eq!(
        count,
        skills.len(),
        "count field disagrees with skills.len()"
    );
    (count, skills)
}

fn names(skills: &[Value]) -> Vec<String> {
    skills
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// --agent
// ---------------------------------------------------------------------------

#[test]
fn agent_filter_claude_returns_eight_claude_skills() {
    let out = cmd().args(["list", "--agent", "claude"]).output().unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 8, "expected 8 claude skills");
    for s in &skills {
        assert_eq!(
            s["agent"].as_str(),
            Some("claude"),
            "every result should be agent=claude, got {s:?}"
        );
    }
}

#[test]
fn agent_filter_codex_returns_three_codex_skills() {
    let out = cmd().args(["list", "--agent", "codex"]).output().unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 3, "expected 3 codex skills");
    for s in &skills {
        assert_eq!(s["agent"].as_str(), Some("codex"));
    }
}

#[test]
fn agent_filter_unknown_returns_orphan_skill() {
    let out = cmd().args(["list", "--agent", "unknown"]).output().unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("orphan-skill"));
    assert_eq!(skills[0]["agent"].as_str(), Some("unknown"));
}

#[test]
fn agent_filter_is_case_insensitive() {
    let out = cmd().args(["list", "--agent", "CLAUDE"]).output().unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 8, "uppercase CLAUDE should match same skills as claude");
    for s in &skills {
        assert_eq!(s["agent"].as_str(), Some("claude"));
    }
}

#[test]
fn agent_filter_unrecognized_returns_zero() {
    let out = cmd()
        .args(["list", "--agent", "nonexistent-agent"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 0);
    assert!(skills.is_empty());
}

// ---------------------------------------------------------------------------
// --tier
// ---------------------------------------------------------------------------

#[test]
fn tier_primary_returns_thirteen() {
    let out = cmd().args(["list", "--tier", "primary"]).output().unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 13);
    for s in &skills {
        assert_eq!(s["tier"].as_str(), Some("primary"));
    }
}

#[test]
fn tier_secondary_returns_secondary_example_only() {
    let out = cmd()
        .args(["list", "--tier", "secondary"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("secondary-example"));
    assert_eq!(skills[0]["tier"].as_str(), Some("secondary"));
}

#[test]
fn tier_bogus_value_fails() {
    let out = cmd().args(["list", "--tier", "bogus"]).output().unwrap();
    assert!(
        !out.status.success(),
        "expected --tier bogus to fail clap validation"
    );
}

// ---------------------------------------------------------------------------
// --root-kind
// ---------------------------------------------------------------------------

#[test]
fn root_kind_claude_global_returns_seven() {
    let out = cmd()
        .args(["list", "--root-kind", "claude-global"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 7);
    for s in &skills {
        let path = s["path"].as_str().unwrap();
        assert!(
            path.contains("/.claude/skills/"),
            "claude-global path should live under /.claude/skills/, got {path}"
        );
    }
}

#[test]
fn root_kind_claude_project_returns_proj_skill() {
    let out = cmd()
        .args(["list", "--root-kind", "claude-project"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("proj-skill"));
}

#[test]
fn root_kind_codex_returns_three() {
    let out = cmd()
        .args(["list", "--root-kind", "codex"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 3);
    for s in &skills {
        let path = s["path"].as_str().unwrap();
        assert!(
            path.contains("/.codex/"),
            "codex root-kind path should contain /.codex/, got {path}"
        );
    }
}

#[test]
fn root_kind_cursor_returns_cursor_thing() {
    let out = cmd()
        .args(["list", "--root-kind", "cursor"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("cursor-thing"));
}

#[test]
fn root_kind_agents_generic_returns_generic_thing() {
    let out = cmd()
        .args(["list", "--root-kind", "agents-generic"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("generic-thing"));
}

#[test]
fn root_kind_unknown_returns_orphan_skill() {
    let out = cmd()
        .args(["list", "--root-kind", "unknown"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("orphan-skill"));
}

#[test]
fn root_kind_nonsense_value_fails() {
    let out = cmd()
        .args(["list", "--root-kind", "nonsense"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "expected --root-kind nonsense to fail clap validation"
    );
}

// ---------------------------------------------------------------------------
// --name
// ---------------------------------------------------------------------------

#[test]
fn name_substring_recipe_matches_four() {
    let out = cmd().args(["list", "--name", "recipe"]).output().unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 4, "should match two dup-recipe + two near-recipe");
    let mut got = names(&skills);
    got.sort();
    assert_eq!(
        got,
        vec![
            "dup-recipe".to_string(),
            "dup-recipe".to_string(),
            "near-recipe".to_string(),
            "near-recipe".to_string(),
        ]
    );
    for s in &skills {
        let name = s["name"].as_str().unwrap();
        assert!(
            name.to_lowercase().contains("recipe"),
            "every match should contain 'recipe' (case-insensitive), got {name}"
        );
    }
}

#[test]
fn name_substring_is_case_insensitive() {
    let out = cmd().args(["list", "--name", "RECIPE"]).output().unwrap();
    let (count, _) = list_payload(&out);
    assert_eq!(
        count, 4,
        "uppercase RECIPE should match the same four skills as recipe"
    );
}

#[test]
fn name_exact_match_returns_single_skill() {
    let out = cmd()
        .args(["list", "--name", "agent-browser"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("agent-browser"));
}

#[test]
fn name_no_match_returns_zero() {
    let out = cmd()
        .args(["list", "--name", "zzzz-no-match"])
        .output()
        .unwrap();
    let (count, _) = list_payload(&out);
    assert_eq!(count, 0);
}

// ---------------------------------------------------------------------------
// --dups-only + --dup-kind  (require similarity → use cmd_full)
// ---------------------------------------------------------------------------

#[test]
fn dups_only_default_threshold_returns_exact_dup_recipe_pair() {
    let out = cmd_full(&[]).args(["list", "--dups-only"]).output().unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 2);
    for s in &skills {
        assert_eq!(s["name"].as_str(), Some("dup-recipe"));
        assert!(
            !s["cluster_id"].is_null(),
            "every dups-only result must carry a non-null cluster_id, got {s:?}"
        );
    }
}

#[test]
fn dups_only_dup_kind_exact_matches_default_dups() {
    let out = cmd_full(&[])
        .args(["list", "--dups-only", "--dup-kind", "exact"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 2);
    for s in &skills {
        assert_eq!(s["name"].as_str(), Some("dup-recipe"));
    }
}

#[test]
fn dups_only_dup_kind_near_is_empty_at_default_threshold() {
    let out = cmd_full(&[])
        .args(["list", "--dups-only", "--dup-kind", "near"])
        .output()
        .unwrap();
    let (count, _) = list_payload(&out);
    assert_eq!(
        count, 0,
        "default threshold is too strict to flag near-recipe pair"
    );
}

#[test]
fn dups_only_with_lower_threshold_picks_up_near_recipes() {
    let out = cmd_full(&["--threshold", "0.6"])
        .args(["list", "--dups-only"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 4, "2 dup-recipe + 2 near-recipe at threshold 0.6");
    let mut got = names(&skills);
    got.sort();
    assert_eq!(
        got,
        vec![
            "dup-recipe".to_string(),
            "dup-recipe".to_string(),
            "near-recipe".to_string(),
            "near-recipe".to_string(),
        ]
    );
    for s in &skills {
        assert!(
            !s["cluster_id"].is_null(),
            "every dups-only result must carry a non-null cluster_id"
        );
    }
}

#[test]
fn dups_only_dup_kind_near_at_lower_threshold_returns_two_near_recipes() {
    let out = cmd_full(&["--threshold", "0.6"])
        .args(["list", "--dups-only", "--dup-kind", "near"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 2);
    for s in &skills {
        assert_eq!(s["name"].as_str(), Some("near-recipe"));
    }
}

#[test]
fn dups_only_dup_kind_exact_at_lower_threshold_still_returns_dup_recipes() {
    let out = cmd_full(&["--threshold", "0.6"])
        .args(["list", "--dups-only", "--dup-kind", "exact"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 2);
    for s in &skills {
        assert_eq!(s["name"].as_str(), Some("dup-recipe"));
    }
}

// ---------------------------------------------------------------------------
// --has-usage / --min-usage  (require usage scan → cmd_full)
// ---------------------------------------------------------------------------

#[test]
fn has_usage_returns_three_skills_with_session_mentions() {
    let out = cmd_full(&[])
        .args(["list", "--has-usage"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 3);
    let mut got = names(&skills);
    got.sort();
    assert_eq!(
        got,
        vec![
            "agent-browser".to_string(),
            "codex-helper".to_string(),
            "data-analyst".to_string(),
        ]
    );
    for s in &skills {
        let mentions = s["usage_mentions"].as_u64().unwrap_or(0);
        assert!(
            mentions > 0,
            "every --has-usage result must have usage_mentions > 0, got {s:?}"
        );
    }
}

#[test]
fn min_usage_two_returns_agent_browser_and_codex_helper() {
    let out = cmd_full(&[])
        .args(["list", "--min-usage", "2"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 2);
    let mut got = names(&skills);
    got.sort();
    assert_eq!(
        got,
        vec!["agent-browser".to_string(), "codex-helper".to_string()]
    );
    for s in &skills {
        let mentions = s["usage_mentions"].as_u64().unwrap_or(0);
        assert!(mentions >= 2, "expected usage_mentions >= 2, got {s:?}");
    }
}

#[test]
fn min_usage_five_returns_only_agent_browser() {
    let out = cmd_full(&[])
        .args(["list", "--min-usage", "5"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("agent-browser"));
    let mentions = skills[0]["usage_mentions"].as_u64().unwrap_or(0);
    assert!(mentions >= 5, "expected mentions >= 5, got {mentions}");
}

#[test]
fn min_usage_extreme_returns_zero() {
    let out = cmd_full(&[])
        .args(["list", "--min-usage", "999"])
        .output()
        .unwrap();
    let (count, _) = list_payload(&out);
    assert_eq!(count, 0);
}

// ---------------------------------------------------------------------------
// --min-tokens / --max-tokens
// ---------------------------------------------------------------------------

#[test]
fn max_tokens_50_returns_only_small_skills() {
    let out = cmd()
        .args(["list", "--max-tokens", "50"])
        .output()
        .unwrap();
    let (_count, skills) = list_payload(&out);
    // Don't assert exact count — token weights can drift slightly. But every
    // returned skill must satisfy the predicate.
    for s in &skills {
        let tokens = s["tokens_total"].as_u64().unwrap();
        assert!(tokens <= 50, "expected tokens_total <= 50, got {s:?}");
    }
}

#[test]
fn min_tokens_100_filters_out_short_skills() {
    let out = cmd()
        .args(["list", "--min-tokens", "100"])
        .output()
        .unwrap();
    let (_count, skills) = list_payload(&out);
    assert!(
        !skills.is_empty(),
        "fixture should have at least one skill >= 100 tokens"
    );
    for s in &skills {
        let tokens = s["tokens_total"].as_u64().unwrap();
        assert!(tokens >= 100, "expected tokens_total >= 100, got {s:?}");
    }
}

#[test]
fn min_tokens_extreme_returns_zero() {
    let out = cmd()
        .args(["list", "--min-tokens", "1000000"])
        .output()
        .unwrap();
    let (count, _) = list_payload(&out);
    assert_eq!(count, 0);
}

// ---------------------------------------------------------------------------
// --validation-failed
// ---------------------------------------------------------------------------

#[test]
fn validation_failed_returns_bad_skill_only() {
    let out = cmd()
        .args(["list", "--validation-failed"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 1);
    assert_eq!(skills[0]["name"].as_str(), Some("bad-skill"));
    assert_eq!(skills[0]["validation_ok"].as_bool(), Some(false));
}

#[test]
fn validation_failed_with_agent_codex_returns_zero() {
    let out = cmd()
        .args(["list", "--validation-failed", "--agent", "codex"])
        .output()
        .unwrap();
    let (count, _) = list_payload(&out);
    assert_eq!(count, 0, "no codex skill is currently invalid");
}

// ---------------------------------------------------------------------------
// Filter composition (AND semantics)
// ---------------------------------------------------------------------------

#[test]
fn agent_claude_and_tier_primary_returns_seven() {
    let out = cmd()
        .args(["list", "--agent", "claude", "--tier", "primary"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(
        count, 7,
        "claude primary = 6 claude-global primary + 1 claude-project (proj-skill)"
    );
    for s in &skills {
        assert_eq!(s["agent"].as_str(), Some("claude"));
        assert_eq!(s["tier"].as_str(), Some("primary"));
    }
}

#[test]
fn agent_claude_root_global_tier_primary_returns_six() {
    let out = cmd()
        .args([
            "list",
            "--agent",
            "claude",
            "--root-kind",
            "claude-global",
            "--tier",
            "primary",
        ])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 6, "excludes proj-skill and secondary-example");
    for s in &skills {
        assert_eq!(s["agent"].as_str(), Some("claude"));
        assert_eq!(s["tier"].as_str(), Some("primary"));
        let path = s["path"].as_str().unwrap();
        assert!(
            path.contains("/.claude/skills/")
                && !path.contains("/projects/"),
            "expected a claude-global path (not under /projects/), got {path}"
        );
    }
}

#[test]
fn name_recipe_and_agent_codex_returns_two_codex_recipes() {
    let out = cmd()
        .args(["list", "--name", "recipe", "--agent", "codex"])
        .output()
        .unwrap();
    let (count, skills) = list_payload(&out);
    assert_eq!(count, 2);
    let mut got = names(&skills);
    got.sort();
    assert_eq!(
        got,
        vec!["dup-recipe".to_string(), "near-recipe".to_string()]
    );
    for s in &skills {
        assert_eq!(s["agent"].as_str(), Some("codex"));
        let name = s["name"].as_str().unwrap();
        assert!(name.to_lowercase().contains("recipe"));
    }
}
