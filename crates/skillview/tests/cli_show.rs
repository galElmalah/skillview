//! End-to-end tests for `skillview show`.
//!
//! The `show` subcommand resolves a single skill target — by exact id, exact
//! name (case-insensitive), or path substring — and returns the full matched
//! skill record(s) plus any duplicate-cluster siblings.
//!
//! Response shape:
//!   { "matched": [<full skill obj>, ...],
//!     "cluster": null | { "cluster": {...}, "members": [<summary>, ...] } }

mod support;

use serde_json::Value;
use support::*;

// ---------- helpers ----------

fn matched(v: &Value) -> &Vec<Value> {
    v["matched"]
        .as_array()
        .expect("response has matched array")
}

fn matched_names(v: &Value) -> Vec<&str> {
    matched(v)
        .iter()
        .map(|m| m["name"].as_str().expect("matched.name is string"))
        .collect()
}

fn matched_agents(v: &Value) -> Vec<&str> {
    matched(v)
        .iter()
        .map(|m| m["agent"].as_str().expect("matched.agent is string"))
        .collect()
}

// ---------- 1. exact name match (single skill) ----------

#[test]
fn show_by_exact_name_returns_one_match_and_null_cluster() {
    let out = cmd().arg("show").arg("agent-browser").output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(matched(&v).len(), 1, "expected exactly one matched skill");
    assert_eq!(matched_names(&v), vec!["agent-browser"]);
    assert!(
        v["cluster"].is_null(),
        "no clustering at default --no-similarity, so cluster must be null; got {:?}",
        v["cluster"]
    );
}

// ---------- 2. case-insensitive exact name match ----------

#[test]
fn show_by_uppercase_name_matches_case_insensitively() {
    let out = cmd().arg("show").arg("AGENT-BROWSER").output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(matched(&v).len(), 1);
    assert_eq!(matched_names(&v), vec!["agent-browser"]);
}

// ---------- 3. unknown target fails ----------

#[test]
fn show_unknown_target_fails_with_helpful_stderr() {
    let out = cmd().arg("show").arg("no-such-skill").output().expect("spawn");
    assert_failure(&out);
    let err = stderr_str(&out);
    assert!(
        err.contains("no skill matched"),
        "expected stderr to say 'no skill matched', got:\n{err}"
    );
}

// ---------- 4. exact id match (chained off `list --format ids`) ----------

#[test]
fn show_by_exact_id_round_trips_through_list_ids() {
    // Step 1: discover the (non-deterministic) id for agent-browser.
    let ids_out = cmd()
        .arg("list")
        .arg("--format")
        .arg("ids")
        .arg("--name")
        .arg("agent-browser")
        .output()
        .expect("spawn list ids");
    assert_success(&ids_out);
    let id_lines = stdout_lines(&ids_out);
    assert_eq!(
        id_lines.len(),
        1,
        "expected exactly one id for `agent-browser`, got: {id_lines:?}"
    );
    let id = &id_lines[0];
    assert!(id.starts_with("s_"), "expected id like s_N, got {id:?}");

    // Step 2: feed the id back into `show` and verify we get the same skill.
    let show_out = cmd().arg("show").arg(id).output().expect("spawn show id");
    let v = parse_json(&show_out);
    assert_eq!(matched(&v).len(), 1);
    assert_eq!(matched_names(&v), vec!["agent-browser"]);
    assert_eq!(matched(&v)[0]["id"].as_str(), Some(id.as_str()));
}

// ---------- 5. path-substring match picking one ----------

#[test]
fn show_by_path_substring_picks_data_analyst() {
    let out = cmd()
        .arg("show")
        .arg("/.claude/skills/data-analyst")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(matched(&v).len(), 1);
    assert_eq!(matched_names(&v), vec!["data-analyst"]);
}

// ---------- 6. exact name match across two copies (dup-recipe) ----------

#[test]
fn show_by_name_returns_both_dup_recipe_rows() {
    // `dup-recipe` exists under both .claude/ and .codex/. Both have the same
    // `.name`, so the exact-name branch should return both rows.
    let out = cmd().arg("show").arg("dup-recipe").output().expect("spawn");
    let v = parse_json(&out);
    let names = matched_names(&v);
    let mut agents = matched_agents(&v);
    agents.sort();
    assert_eq!(names.len(), 2, "expected 2 dup-recipe matches, got {names:?}");
    assert!(
        names.iter().all(|n| *n == "dup-recipe"),
        "expected both matches to be named dup-recipe, got {names:?}"
    );
    assert_eq!(
        agents,
        vec!["claude", "codex"],
        "expected dup-recipe rows from claude + codex agents"
    );
}

// ---------- 7. path-substring picking one of the dup-recipes ----------

#[test]
fn show_path_substring_disambiguates_to_codex_dup_recipe() {
    let out = cmd()
        .arg("show")
        .arg("/.codex/skills/dup-recipe")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(matched(&v).len(), 1);
    assert_eq!(matched_names(&v), vec!["dup-recipe"]);
    assert_eq!(matched_agents(&v), vec!["codex"]);
}

// ---------- 8. cluster siblings when similarity is enabled ----------

#[test]
fn show_dup_recipe_with_similarity_attaches_exact_cluster() {
    let out = cmd_full(&[]).arg("show").arg("dup-recipe").output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(matched(&v).len(), 2);

    let cluster = &v["cluster"];
    assert!(!cluster.is_null(), "expected non-null cluster, got null");

    assert_eq!(
        cluster["cluster"]["kind"].as_str(),
        Some("exact"),
        "expected exact cluster, got {:?}",
        cluster["cluster"]["kind"]
    );

    let cluster_member_ids = cluster["cluster"]["members"]
        .as_array()
        .expect("cluster.cluster.members is array");
    assert_eq!(cluster_member_ids.len(), 2, "cluster should have 2 member ids");

    let member_summaries = cluster["members"]
        .as_array()
        .expect("cluster.members is array");
    assert_eq!(member_summaries.len(), 2);
    let summary_names: Vec<&str> = member_summaries
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert!(
        summary_names.iter().all(|n| *n == "dup-recipe"),
        "expected both summary members named dup-recipe, got {summary_names:?}"
    );
}

// ---------- 9. near-cluster surfaces at lower threshold ----------

#[test]
fn show_near_recipe_with_loose_threshold_attaches_near_cluster() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("show")
        .arg("near-recipe")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(matched(&v).len(), 2);

    let cluster = &v["cluster"];
    assert!(!cluster.is_null(), "expected near cluster, got null");
    assert_eq!(
        cluster["cluster"]["kind"].as_str(),
        Some("near"),
        "expected near cluster, got {:?}",
        cluster["cluster"]["kind"]
    );
    let sim = cluster["cluster"]["similarity"]
        .as_f64()
        .expect("similarity is a number");
    assert!(
        sim > 0.0 && sim < 1.0,
        "near similarity should be in (0, 1), got {sim}"
    );

    let member_summaries = cluster["members"]
        .as_array()
        .expect("cluster.members is array");
    assert_eq!(member_summaries.len(), 2);
}

// ---------- 10. no clustering when --no-similarity ----------

#[test]
fn show_dup_recipe_without_similarity_returns_null_cluster() {
    // With the fast-path `cmd()` (which sets `--no-similarity`), even
    // duplicates should not be clustered, so cluster must be null.
    let out = cmd().arg("show").arg("dup-recipe").output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(matched(&v).len(), 2, "should still match both rows by name");
    assert!(
        v["cluster"].is_null(),
        "with --no-similarity, cluster must be null; got {:?}",
        v["cluster"]
    );
}

// ---------- 11. --pretty produces multi-line output ----------

#[test]
fn show_pretty_flag_produces_multiline_json() {
    // Global flags like --pretty live before the subcommand.
    let pretty_out = cmd()
        .arg("--pretty")
        .arg("show")
        .arg("agent-browser")
        .output()
        .expect("spawn pretty");
    assert_success(&pretty_out);
    let pretty_stdout = stdout_str(&pretty_out);
    let pretty_lines: Vec<&str> = pretty_stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        pretty_lines.len() >= 3,
        "expected --pretty to produce at least 3 lines, got {}: {pretty_lines:?}",
        pretty_lines.len()
    );

    // Sanity: the compact form is a single line of JSON.
    let compact_out = cmd().arg("show").arg("agent-browser").output().expect("spawn compact");
    assert_success(&compact_out);
    let compact_stdout = stdout_str(&compact_out);
    let compact_lines: Vec<&str> = compact_stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        compact_lines.len(),
        1,
        "expected compact JSON on a single line, got {compact_lines:?}"
    );
}
