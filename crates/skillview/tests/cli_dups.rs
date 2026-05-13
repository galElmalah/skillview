//! End-to-end tests for `skillview dups`.
//!
//! `dups` lists duplicate clusters (exact + near) discovered during scan, with
//! filters for cluster kind, size, agent, root-kind, sort + limit, and across
//! six output formats.
//!
//! Default JSON shape:
//!   { "count": N,
//!     "clusters": [
//!       { "cluster": { "id", "kind", "similarity", "members": [skill_id, ...] },
//!         "members": [<summary>, ...] } ] }

mod support;

use serde_json::Value;
use support::*;

// ---------- helpers ----------

fn count(v: &Value) -> usize {
    v["count"].as_u64().expect("count is u64") as usize
}

fn clusters(v: &Value) -> &Vec<Value> {
    v["clusters"].as_array().expect("clusters is array")
}

fn cluster_kind(c: &Value) -> &str {
    c["cluster"]["kind"].as_str().expect("kind is string")
}

fn cluster_similarity(c: &Value) -> f64 {
    c["cluster"]["similarity"]
        .as_f64()
        .expect("similarity is a number")
}

fn member_names(c: &Value) -> Vec<&str> {
    c["members"]
        .as_array()
        .expect("members is array")
        .iter()
        .map(|m| m["name"].as_str().expect("name is string"))
        .collect()
}

// ---------- 12. default: one exact cluster ----------

#[test]
fn dups_default_has_one_exact_cluster_with_two_dup_recipes() {
    let out = cmd_full(&[]).arg("dups").output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 1, "expected exactly one cluster at default threshold");
    let c = &clusters(&v)[0];
    assert_eq!(cluster_kind(c), "exact");
    assert_eq!(cluster_similarity(c), 1.0);
    let names = member_names(c);
    assert_eq!(names.len(), 2);
    assert!(
        names.iter().all(|n| *n == "dup-recipe"),
        "expected both cluster members to be dup-recipe, got {names:?}"
    );
}

// ---------- 13. --exact filter ----------

#[test]
fn dups_exact_only_returns_the_exact_cluster() {
    let out = cmd_full(&[]).arg("dups").arg("--exact").output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 1);
    let c = &clusters(&v)[0];
    assert_eq!(cluster_kind(c), "exact");
    assert_eq!(member_names(c).len(), 2);
}

// ---------- 14. --near filter at default threshold ----------

#[test]
fn dups_near_only_default_threshold_is_empty() {
    // At the default similarity threshold (0.85), `near-recipe` (sim ~0.74)
    // does not cluster, so `--near` should be empty.
    let out = cmd_full(&[]).arg("dups").arg("--near").output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 0);
}

// ---------- 15. --near at threshold 0.6 surfaces the near cluster ----------

#[test]
fn dups_near_only_loose_threshold_surfaces_near_recipe() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--near")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 1);
    let c = &clusters(&v)[0];
    assert_eq!(cluster_kind(c), "near");
    let sim = cluster_similarity(c);
    assert!(sim > 0.0 && sim < 1.0, "near similarity should be in (0,1), got {sim}");
    let names = member_names(c);
    assert!(
        names.iter().all(|n| *n == "near-recipe"),
        "expected both members named near-recipe, got {names:?}"
    );
}

// ---------- 16. --min-size 3 prunes everything ----------

#[test]
fn dups_min_size_three_is_empty() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--min-size")
        .arg("3")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 0, "no cluster has >=3 members in the fixture");
}

// ---------- 17. --min-size 2 keeps both clusters ----------

#[test]
fn dups_min_size_two_keeps_both_clusters() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--min-size")
        .arg("2")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 2);
}

// ---------- 18. --agent claude (claude is in both clusters) ----------

#[test]
fn dups_agent_claude_matches_both_clusters() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--agent")
        .arg("claude")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 2, "claude participates in both dup clusters");
}

// ---------- 19. --agent agents (not in any dup cluster) ----------

#[test]
fn dups_agent_agents_is_empty() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--agent")
        .arg("agents")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 0);
}

// ---------- 20. --root-kind codex ----------

#[test]
fn dups_root_kind_codex_threshold_06_returns_both_clusters() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--root-kind")
        .arg("codex")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 2, "both clusters include a codex member");
}

#[test]
fn dups_root_kind_codex_default_threshold_returns_one_cluster() {
    let out = cmd_full(&[])
        .arg("dups")
        .arg("--root-kind")
        .arg("codex")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 1, "only the exact cluster survives the default threshold");
}

// ---------- 21. --root-kind cursor (no cursor member in any dup) ----------

#[test]
fn dups_root_kind_cursor_is_empty_at_any_threshold() {
    let out_default = cmd_full(&[])
        .arg("dups")
        .arg("--root-kind")
        .arg("cursor")
        .output()
        .expect("spawn default");
    let v_default = parse_json(&out_default);
    assert_eq!(count(&v_default), 0);

    let out_loose = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--root-kind")
        .arg("cursor")
        .output()
        .expect("spawn loose");
    let v_loose = parse_json(&out_loose);
    assert_eq!(count(&v_loose), 0);
}

// ---------- 22. --sort size (default) ----------

#[test]
fn dups_sort_size_returns_two_clusters_at_threshold_06() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--sort")
        .arg("size")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 2);
}

// ---------- 23. --sort similarity puts exact (1.0) ahead of near ----------

#[test]
fn dups_sort_similarity_orders_exact_before_near() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--sort")
        .arg("similarity")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 2);
    let cs = clusters(&v);
    let s0 = cluster_similarity(&cs[0]);
    let s1 = cluster_similarity(&cs[1]);
    assert!(
        s0 >= s1,
        "with --sort similarity, similarities should be non-increasing; got [{s0}, {s1}]"
    );
    assert_eq!(s0, 1.0, "first cluster (highest similarity) should be exact (1.0)");
    assert!(
        s1 > 0.0 && s1 < 1.0,
        "second cluster should be a near cluster with sim in (0,1), got {s1}"
    );
}

// ---------- 24. --sort kind puts exact before near ----------

#[test]
fn dups_sort_kind_puts_exact_before_near() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--sort")
        .arg("kind")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 2);
    let cs = clusters(&v);
    assert_eq!(cluster_kind(&cs[0]), "exact");
    assert_eq!(cluster_kind(&cs[1]), "near");
}

// ---------- 25. --sort bogus fails ----------

#[test]
fn dups_sort_bogus_fails() {
    let out = cmd_full(&[])
        .arg("dups")
        .arg("--sort")
        .arg("bogus")
        .output()
        .expect("spawn");
    assert_failure(&out);
}

// ---------- 26. --limit 1 ----------

#[test]
fn dups_limit_one_truncates_to_one_cluster() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--limit")
        .arg("1")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 1);
}

// ---------- 27. --limit 0 ----------

#[test]
fn dups_limit_zero_returns_empty() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--limit")
        .arg("0")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 0);
}

// ---------- 28. --format jsonl ----------

#[test]
fn dups_format_jsonl_emits_one_object_per_cluster() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--format")
        .arg("jsonl")
        .output()
        .expect("spawn");
    let lines = parse_jsonl(&out);
    assert_eq!(lines.len(), 2, "expected one JSON object per cluster");
    // Each line should look like a ClusterView (has "cluster" + "members").
    for (i, line) in lines.iter().enumerate() {
        assert!(
            line["cluster"].is_object(),
            "jsonl line {i} missing `cluster` object: {line}"
        );
        assert!(
            line["members"].is_array(),
            "jsonl line {i} missing `members` array: {line}"
        );
    }
}

// ---------- 29. --format tsv ----------

#[test]
fn dups_format_tsv_has_expected_header_and_rows() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--format")
        .arg("tsv")
        .output()
        .expect("spawn");
    let (header, rows) = parse_tsv(&out);
    assert_eq!(
        header,
        vec![
            "cluster_id".to_string(),
            "kind".to_string(),
            "similarity".to_string(),
            "size".to_string(),
            "agents".to_string(),
            "members".to_string(),
        ]
    );
    assert_eq!(rows.len(), 2, "expected 2 data rows, got {}", rows.len());

    // Find the exact row; in default `--sort size` ordering with equal sizes
    // the tie-breaker is similarity desc, so the exact row comes first, but
    // we don't rely on that — locate it by `kind`.
    let exact_row = rows
        .iter()
        .find(|r| r.get(1).map(String::as_str) == Some("exact"))
        .expect("expected an exact row in tsv");

    let agents_col = &exact_row[4];
    assert!(
        agents_col.contains("claude") && agents_col.contains("codex"),
        "agents column should mention both claude and codex, got {agents_col:?}"
    );
    // Per emit code: agents are sorted + deduped.
    assert_eq!(agents_col, "claude,codex");

    let members_col = &exact_row[5];
    assert!(
        members_col.contains("claude:dup-recipe") && members_col.contains("codex:dup-recipe"),
        "members column should contain claude:dup-recipe and codex:dup-recipe, got {members_col:?}"
    );
    assert!(
        members_col.contains('|'),
        "members column should be `|`-joined, got {members_col:?}"
    );
}

// ---------- 30. --format ids ----------

#[test]
fn dups_format_ids_emits_one_cluster_id_per_line() {
    let out = cmd_full(&[])
        .arg("dups")
        .arg("--format")
        .arg("ids")
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 1, "default threshold → 1 cluster id");
    let id = &lines[0];
    assert!(
        id.starts_with("c_") && id[2..].chars().all(|c| c.is_ascii_digit()),
        "expected cluster id like c_<digit>, got {id:?}"
    );
}

// ---------- 31. --format paths ----------

#[test]
fn dups_format_paths_emits_absolute_skill_paths() {
    let out = cmd_full(&["--threshold", "0.6"])
        .arg("dups")
        .arg("--format")
        .arg("paths")
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(
        lines.len(),
        4,
        "expected 2 clusters x 2 members = 4 paths, got {lines:?}"
    );
    for path in &lines {
        assert!(
            path.starts_with('/'),
            "expected absolute path, got {path:?}"
        );
        assert!(
            path.ends_with("SKILL.md"),
            "expected path ending in SKILL.md, got {path:?}"
        );
    }
}

// ---------- 32. --format names ----------

#[test]
fn dups_format_names_emits_one_member_name_per_line() {
    let out = cmd_full(&[])
        .arg("dups")
        .arg("--format")
        .arg("names")
        .output()
        .expect("spawn");
    let lines = stdout_lines(&out);
    assert_eq!(lines.len(), 2, "default threshold → 1 cluster x 2 members");
    assert!(
        lines.iter().all(|l| l == "dup-recipe"),
        "expected both names to be dup-recipe, got {lines:?}"
    );
}

// ---------- 33. --format bogus ----------

#[test]
fn dups_format_bogus_fails() {
    let out = cmd_full(&[])
        .arg("dups")
        .arg("--format")
        .arg("bogus")
        .output()
        .expect("spawn");
    assert_failure(&out);
}

// ---------- 34. --exact and --near together fall back to "all" ----------

#[test]
fn dups_exact_and_near_together_returns_all() {
    // When both flags are set, the code's `(true, true)` branch falls through
    // to `_ => true`, returning the full set. At default threshold that's
    // just the one exact cluster.
    let out = cmd_full(&[])
        .arg("dups")
        .arg("--exact")
        .arg("--near")
        .output()
        .expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 1);
    let c = &clusters(&v)[0];
    assert_eq!(cluster_kind(c), "exact");
}

// ---------- 35. --no-similarity skips clustering entirely ----------

#[test]
fn dups_with_no_similarity_returns_zero_clusters() {
    // `cmd()` sets `--no-similarity`, so no clusters are computed at all.
    let out = cmd().arg("dups").output().expect("spawn");
    let v = parse_json(&out);
    assert_eq!(count(&v), 0);
    assert!(
        clusters(&v).is_empty(),
        "expected clusters array to be empty, got {:?}",
        clusters(&v)
    );
}
