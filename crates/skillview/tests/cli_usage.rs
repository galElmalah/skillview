//! End-to-end tests for the `skillview usage` subcommand.
//!
//! Usage ranks skills by cross-agent mention counts harvested from Claude and
//! Codex session JSONL logs. The fixture under `tests/fixtures/home/` ships
//! two session files whose mentions are well-known (see comments below), so
//! these tests assert precise counts, ordering, filtering, and output-format
//! behavior. Every test uses `cmd_full(&[])` so that usage scanning is
//! actually performed; one sanity test uses the default `cmd()` (which sets
//! `--no-usage`) to confirm the command degrades cleanly.
//!
//! Fixture mention facts:
//!   - agent-browser → 5 mentions across 2 sessions (claude=3, codex=2), high
//!   - codex-helper  → 2 mentions across 1 session  (codex=2),           high
//!   - data-analyst  → 1 mention  in 1 session      (claude=1),          high
//!   - `short` (5-char name, no separator)          → 0 mentions, low confidence
//!   - everything else                              → 0 mentions, high confidence

mod support;

use serde_json::Value;
use support::*;

// Skills with at least one mention in the fixture session logs.
const MENTIONED: [&str; 3] = ["agent-browser", "codex-helper", "data-analyst"];

/// Total skills in the fixture; keep in sync with the other integration files.
const FIXTURE_SKILL_COUNT: usize = 14;
/// Number of low-confidence skills (currently just `short`).
const LOW_CONFIDENCE_SKILLS: usize = 1;

fn names_of(skills: &[Value]) -> Vec<String> {
    skills
        .iter()
        .map(|s| s["name"].as_str().unwrap_or("<unknown>").to_string())
        .collect()
}

// ---------- 1. default invocation ----------

#[test]
fn default_invocation_returns_three_mentioned_skills() {
    let out = cmd_full(&[]).args(["usage"]).output().expect("spawn");
    let resp = parse_json(&out);

    assert!(resp.get("count").is_some(), "missing top-level `count`");
    assert!(resp.get("skills").is_some(), "missing top-level `skills`");
    assert_eq!(resp["count"].as_u64(), Some(3), "default count drifted");

    let skills = resp["skills"].as_array().expect("skills array");
    let names = names_of(skills);
    for expected in MENTIONED {
        assert!(
            names.iter().any(|n| n == expected),
            "expected default usage output to include {expected:?}, got {names:?}"
        );
    }
    assert_eq!(skills.len(), 3, "default skills length should equal count");

    // Default sort is mentions desc, so agent-browser (5) leads.
    assert_eq!(
        skills[0]["name"].as_str(),
        Some("agent-browser"),
        "default sort should put agent-browser first"
    );

    // Default `--min-mentions 1` filter must hold.
    for s in skills {
        let m = s["mentions"].as_u64().unwrap_or(0);
        assert!(m >= 1, "skill {:?} leaked through min_mentions=1 filter", s["name"]);
    }
}

// ---------- 2-3. --top ----------

#[test]
fn top_one_keeps_only_the_leader() {
    let out = cmd_full(&[])
        .args(["usage", "--top", "1"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(1));
    let skills = resp["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0]["name"].as_str(), Some("agent-browser"));
    assert_eq!(skills[0]["mentions"].as_u64(), Some(5));
}

#[test]
fn top_zero_disables_truncation_but_min_mentions_still_filters() {
    // `top == 0` is the documented "keep all" sentinel. Default --min-mentions 1
    // still excludes the 11 zero-mention skills.
    let out = cmd_full(&[])
        .args(["usage", "--top", "0"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(3));
}

// ---------- 4-6. --min-mentions ----------

#[test]
fn min_mentions_two_drops_data_analyst() {
    let out = cmd_full(&[])
        .args(["usage", "--min-mentions", "2"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(2));
    let names = names_of(resp["skills"].as_array().unwrap());
    assert!(names.contains(&"agent-browser".to_string()));
    assert!(names.contains(&"codex-helper".to_string()));
    assert!(!names.contains(&"data-analyst".to_string()));
}

#[test]
fn min_mentions_five_keeps_only_agent_browser() {
    let out = cmd_full(&[])
        .args(["usage", "--min-mentions", "5"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(1));
    assert_eq!(
        resp["skills"][0]["name"].as_str(),
        Some("agent-browser")
    );
}

#[test]
fn min_mentions_higher_than_any_yields_empty() {
    let out = cmd_full(&[])
        .args(["usage", "--min-mentions", "99"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(0));
    assert!(resp["skills"].as_array().unwrap().is_empty());
}

// ---------- 7-9. --agent ----------

#[test]
fn agent_filter_claude_returns_only_claude_skills() {
    let out = cmd_full(&[])
        .args(["usage", "--agent", "claude"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(2));
    let names = names_of(resp["skills"].as_array().unwrap());
    assert!(names.contains(&"agent-browser".to_string()));
    assert!(names.contains(&"data-analyst".to_string()));
    for s in resp["skills"].as_array().unwrap() {
        assert_eq!(
            s["agent"].as_str(),
            Some("claude"),
            "non-claude skill leaked: {s:?}"
        );
    }
}

#[test]
fn agent_filter_codex_returns_codex_helper_only() {
    let out = cmd_full(&[])
        .args(["usage", "--agent", "codex"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(1));
    assert_eq!(
        resp["skills"][0]["name"].as_str(),
        Some("codex-helper")
    );
    assert_eq!(resp["skills"][0]["mentions"].as_u64(), Some(2));
}

#[test]
fn agent_filter_unknown_yields_empty() {
    let out = cmd_full(&[])
        .args(["usage", "--agent", "totally-not-an-agent"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(0));
    assert!(resp["skills"].as_array().unwrap().is_empty());
}

// ---------- 10-14. --sort ----------

#[test]
fn sort_mentions_is_monotonically_non_increasing() {
    let out = cmd_full(&[])
        .args(["usage", "--sort", "mentions"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    let skills = resp["skills"].as_array().unwrap();
    assert!(skills.len() >= 2);
    let m0 = skills[0]["mentions"].as_u64().unwrap();
    let m1 = skills[1]["mentions"].as_u64().unwrap();
    assert!(m0 >= m1, "sort=mentions not descending: {m0} < {m1}");
    assert_eq!(skills[0]["name"].as_str(), Some("agent-browser"));
}

#[test]
fn sort_sessions_puts_agent_browser_first() {
    let out = cmd_full(&[])
        .args(["usage", "--sort", "sessions"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    let skills = resp["skills"].as_array().unwrap();
    assert!(!skills.is_empty());
    assert_eq!(skills[0]["name"].as_str(), Some("agent-browser"));
    assert_eq!(skills[0]["sessions"].as_u64(), Some(2));
}

#[test]
fn sort_name_is_alphabetical_ascending() {
    let out = cmd_full(&[])
        .args(["usage", "--sort", "name"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    let names = names_of(resp["skills"].as_array().unwrap());
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "names not sorted ascending: {names:?}");
    assert_eq!(names.first().map(|s| s.as_str()), Some("agent-browser"));
}

#[test]
fn sort_recent_orders_by_last_seen_at_desc() {
    let out = cmd_full(&[])
        .args(["usage", "--sort", "recent"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    let skills = resp["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 3);

    // All three mentioned skills must have a populated last_seen_at.
    let last_seens: Vec<&str> = skills
        .iter()
        .map(|s| {
            s.get("last_seen_at")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("expected last_seen_at on every recent-sorted entry, missing on {:?}", s["name"]))
        })
        .collect();
    for ls in &last_seens {
        assert!(!ls.is_empty(), "last_seen_at was empty string");
        // ISO 8601-ish: e.g. "2026-05-13T08:19:32Z". A loose sanity check.
        assert!(
            ls.contains('T') && (ls.ends_with('Z') || ls.contains('+') || ls.contains('-')),
            "last_seen_at doesn't look like ISO 8601: {ls:?}"
        );
    }

    // ISO 8601 sorts lexicographically.
    assert!(
        last_seens[0] >= last_seens[1],
        "sort=recent not descending: {} < {}",
        last_seens[0],
        last_seens[1]
    );
    assert!(
        last_seens[1] >= last_seens[2],
        "sort=recent not descending: {} < {}",
        last_seens[1],
        last_seens[2]
    );
}

#[test]
fn sort_bogus_is_rejected() {
    let out = cmd_full(&[])
        .args(["usage", "--sort", "bogus-sort-key"])
        .output()
        .expect("spawn");
    assert_failure(&out);
}

// ---------- 15-16. --include-low ----------

#[test]
fn include_low_alone_still_filters_zero_mentions() {
    // `short` is the lone low-confidence skill — but it also has 0 mentions,
    // so the default --min-mentions 1 still removes it.
    let out = cmd_full(&[])
        .args(["usage", "--include-low"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(3));
    let names = names_of(resp["skills"].as_array().unwrap());
    assert!(
        !names.contains(&"short".to_string()),
        "`short` slipped through despite default --min-mentions 1"
    );
}

#[test]
fn include_low_with_min_mentions_zero_returns_every_skill() {
    let out = cmd_full(&[])
        .args(["usage", "--include-low", "--min-mentions", "0", "--top", "0"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(
        resp["count"].as_u64(),
        Some(FIXTURE_SKILL_COUNT as u64),
        "expected every skill (including `short`) to appear"
    );
    let names = names_of(resp["skills"].as_array().unwrap());
    assert!(
        names.contains(&"short".to_string()),
        "`short` should appear when --include-low + --min-mentions 0"
    );
}

#[test]
fn no_include_low_with_min_mentions_zero_excludes_short() {
    let out = cmd_full(&[])
        .args(["usage", "--min-mentions", "0", "--top", "0"])
        .output()
        .expect("spawn");
    let resp = parse_json(&out);
    let expected = (FIXTURE_SKILL_COUNT - LOW_CONFIDENCE_SKILLS) as u64;
    assert_eq!(
        resp["count"].as_u64(),
        Some(expected),
        "expected every high-confidence skill (excluding `short`)"
    );
    let names = names_of(resp["skills"].as_array().unwrap());
    assert!(!names.contains(&"short".to_string()));
}

// ---------- 17-23. --format ----------

#[test]
fn format_jsonl_emits_one_object_per_line() {
    let out = cmd_full(&[])
        .args(["usage", "--format", "jsonl"])
        .output()
        .expect("spawn");
    let rows = parse_jsonl(&out);
    assert_eq!(rows.len(), 3, "jsonl should yield 3 rows for default filters");
    for r in &rows {
        assert!(r.get("name").is_some(), "jsonl row missing `name`: {r:?}");
        assert!(r.get("mentions").is_some(), "jsonl row missing `mentions`: {r:?}");
    }
}

#[test]
fn format_tsv_header_and_agent_browser_row() {
    let out = cmd_full(&[])
        .args(["usage", "--format", "tsv"])
        .output()
        .expect("spawn");
    let (header, rows) = parse_tsv(&out);
    assert_eq!(
        header,
        vec![
            "id",
            "agent",
            "name",
            "mentions",
            "sessions",
            "confidence",
            "last_seen_at",
            "by_source",
        ],
        "tsv header drifted"
    );
    assert_eq!(rows.len(), 3, "tsv should have 3 data rows by default");

    // Locate the agent-browser row by its `name` column (index 2).
    let ab = rows
        .iter()
        .find(|r| r.get(2).map(|s| s.as_str()) == Some("agent-browser"))
        .expect("agent-browser row missing from tsv output");
    assert_eq!(ab[3], "5", "agent-browser mentions cell");
    assert_eq!(ab[4], "2", "agent-browser sessions cell");
    assert_eq!(ab[5], "high", "agent-browser confidence cell");
    let by_source = &ab[7];
    assert!(
        by_source.contains("claude=3"),
        "by_source missing claude=3: {by_source:?}"
    );
    assert!(
        by_source.contains("codex=2"),
        "by_source missing codex=2: {by_source:?}"
    );
}

#[test]
fn format_ids_emits_one_id_per_line() {
    let out = cmd_full(&[])
        .args(["usage", "--format", "ids"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 3);
    for line in &lines {
        assert!(
            line.starts_with("s_") && line[2..].chars().all(|c| c.is_ascii_digit()),
            "id line {line:?} doesn't match s_<digits>"
        );
    }
}

#[test]
fn format_names_emits_one_name_per_line() {
    let out = cmd_full(&[])
        .args(["usage", "--format", "names"])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 3);
    for expected in MENTIONED {
        assert!(
            lines.iter().any(|l| l == expected),
            "format=names missing {expected:?}, got {lines:?}"
        );
    }
}

#[test]
fn format_paths_is_rejected() {
    let out = cmd_full(&[])
        .args(["usage", "--format", "paths"])
        .output()
        .expect("spawn");
    assert_failure(&out);
    let stderr = stderr_str(&out);
    assert!(
        stderr.contains("paths"),
        "stderr should mention `paths`:\n{stderr}"
    );
    assert!(
        stderr.contains("usage") || stderr.contains("list"),
        "stderr should reference `usage` or `list` for context:\n{stderr}"
    );
}

#[test]
fn format_bogus_is_rejected() {
    let out = cmd_full(&[])
        .args(["usage", "--format", "bogus-format"])
        .output()
        .expect("spawn");
    assert_failure(&out);
}

// ---------- 24. --no-usage degrades gracefully ----------

#[test]
fn usage_subcommand_with_usage_scan_skipped_returns_zero() {
    // `cmd()` already passes `--no-usage`, so every skill has mentions=0 and
    // the default --min-mentions 1 filter eats them all.
    let out = cmd().args(["usage"]).output().expect("spawn");
    let resp = parse_json(&out);
    assert_eq!(resp["count"].as_u64(), Some(0));
}

// ---------- 25. combined flags ----------

#[test]
fn combined_agent_sort_top_format_names() {
    let out = cmd_full(&[])
        .args([
            "usage",
            "--agent",
            "claude",
            "--sort",
            "sessions",
            "--top",
            "1",
            "--format",
            "names",
        ])
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines, vec!["agent-browser".to_string()]);
}

// ---------- 26-27. by_source + confidence sanity ----------

#[test]
fn agent_browser_by_source_breaks_down_per_agent() {
    let out = cmd_full(&[]).args(["usage"]).output().expect("spawn");
    let resp = parse_json(&out);
    let ab = resp["skills"]
        .as_array()
        .expect("skills array")
        .iter()
        .find(|s| s["name"].as_str() == Some("agent-browser"))
        .expect("agent-browser missing from usage output");
    assert_eq!(ab["by_source"]["claude"].as_u64(), Some(3));
    assert_eq!(ab["by_source"]["codex"].as_u64(), Some(2));
}

#[test]
fn default_results_are_all_high_confidence() {
    let out = cmd_full(&[]).args(["usage"]).output().expect("spawn");
    let resp = parse_json(&out);
    for s in resp["skills"].as_array().expect("skills array") {
        assert_eq!(
            s["confidence"].as_str(),
            Some("high"),
            "default invocation surfaced non-high confidence skill: {s:?}"
        );
    }
}
