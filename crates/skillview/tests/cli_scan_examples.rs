//! Default invocation, explicit `scan`, `--pretty`, `--include-minhash`, and
//! the `examples` recipe-book subcommand.
//!
//! Default-and-`scan` should be identical shapes; `examples` is special
//! because it MUST short-circuit before any scan happens (it's the only
//! subcommand that never touches disk).

mod support;

use std::time::Instant;

use support::{assert_success, cmd, parse_json, stdout_lines, stdout_str};

/// Total skill count in the fixture home. If you change the fixture, update
/// this and `sanity.rs` together.
const FIXTURE_SKILL_COUNT: usize = 14;

// ---------- default invocation (no subcommand) ----------

#[test]
fn default_invocation_emits_json() {
    let out = cmd().output().expect("spawn");
    let _ = parse_json(&out); // parse_json asserts success + valid JSON
}

#[test]
fn default_invocation_has_expected_top_level_keys() {
    let out = cmd().output().expect("spawn");
    let inv = parse_json(&out);
    for key in ["schema_version", "generated_at", "roots", "skills", "clusters", "stats"] {
        assert!(
            inv.get(key).is_some(),
            "missing top-level key {key:?} in default output: {inv:#?}"
        );
    }
}

#[test]
fn default_invocation_uses_schema_version_2() {
    let out = cmd().output().expect("spawn");
    let inv = parse_json(&out);
    assert_eq!(
        inv["schema_version"].as_u64(),
        Some(2),
        "schema_version drifted: {:?}",
        inv["schema_version"]
    );
}

#[test]
fn default_invocation_returns_all_fixture_skills() {
    let out = cmd().output().expect("spawn");
    let inv = parse_json(&out);
    let skills = inv["skills"].as_array().expect("skills must be array");
    assert_eq!(
        skills.len(),
        FIXTURE_SKILL_COUNT,
        "skill count drifted from fixture"
    );
}

// ---------- explicit `scan` subcommand ----------

#[test]
fn scan_subcommand_emits_json_with_same_shape_as_default() {
    let out = cmd().arg("scan").output().expect("spawn");
    let inv = parse_json(&out);
    for key in ["schema_version", "generated_at", "roots", "skills", "clusters", "stats"] {
        assert!(
            inv.get(key).is_some(),
            "missing top-level key {key:?} in `scan` output"
        );
    }
}

#[test]
fn scan_subcommand_returns_same_skill_count_as_default() {
    let out = cmd().arg("scan").output().expect("spawn");
    let inv = parse_json(&out);
    let skills = inv["skills"].as_array().expect("skills must be array");
    assert_eq!(skills.len(), FIXTURE_SKILL_COUNT);
}

// ---------- `--pretty` ----------

#[test]
fn non_pretty_output_is_a_single_json_line() {
    let out = cmd().output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    // compact serializer + one trailing newline -> one non-empty line.
    let non_empty: Vec<&str> = s.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        non_empty.len(),
        1,
        "expected compact output to be one line; got {} lines:\n{s}",
        non_empty.len()
    );
}

#[test]
fn pretty_output_spans_many_lines() {
    let out = cmd().arg("--pretty").output().expect("spawn");
    let _ = parse_json(&out); // must still be valid JSON
    let lines = stdout_lines(&out);
    assert!(
        lines.len() >= 5,
        "expected pretty output to span >=5 lines, got {} line(s):\n{}",
        lines.len(),
        stdout_str(&out)
    );
}

#[test]
fn pretty_output_has_indentation() {
    let out = cmd().arg("--pretty").output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    // serde_json::to_writer_pretty uses two-space indent and newlines.
    assert!(
        s.contains("\n  \"") || s.contains("\n    \""),
        "expected pretty output to contain indented keys, got:\n{s}"
    );
}

// ---------- `--include-minhash` ----------

#[test]
fn minhash_is_stripped_by_default() {
    let out = cmd().output().expect("spawn");
    let inv = parse_json(&out);
    let skills = inv["skills"].as_array().expect("skills array");
    let leaked: Vec<&str> = skills
        .iter()
        .filter(|s| !s.get("minhash").map(|v| v.is_null()).unwrap_or(true))
        .map(|s| s["name"].as_str().unwrap_or("<unknown>"))
        .collect();
    assert!(
        leaked.is_empty(),
        "expected NO skill to ship a non-null minhash without --include-minhash, but found: {leaked:?}"
    );
}

#[test]
fn include_minhash_attaches_a_128_element_signature() {
    let out = cmd().arg("--include-minhash").output().expect("spawn");
    let inv = parse_json(&out);
    let skills = inv["skills"].as_array().expect("skills array");

    let with_minhash: Vec<&serde_json::Value> = skills
        .iter()
        .filter(|s| s.get("minhash").and_then(|v| v.as_array()).is_some())
        .collect();
    assert!(
        !with_minhash.is_empty(),
        "expected at least one skill to expose a non-null minhash with --include-minhash"
    );

    for s in &with_minhash {
        let arr = s["minhash"].as_array().expect("checked non-null");
        assert_eq!(
            arr.len(),
            128,
            "expected minhash to have 128 elements for skill {:?}, got {}",
            s["name"],
            arr.len()
        );
        // Every element must be a u64.
        assert!(
            arr.iter().all(|v| v.as_u64().is_some()),
            "expected every minhash element to be a u64 for skill {:?}",
            s["name"]
        );
    }
}

// ---------- `examples` subcommand ----------

#[test]
fn examples_subcommand_exits_zero() {
    let out = cmd().arg("examples").output().expect("spawn");
    assert_success(&out);
}

#[test]
fn examples_output_contains_section_markers() {
    let out = cmd().arg("examples").output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    for marker in [
        "# Discover what's on this machine",
        "# Search and filter skills",
        "# Duplicates",
    ] {
        assert!(
            s.contains(marker),
            "examples output missing section marker {marker:?}:\n{s}"
        );
    }
}

#[test]
fn examples_output_references_per_command_help() {
    let out = cmd().arg("examples").output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    assert!(
        s.contains("skillview list --help"),
        "examples output should point users at `skillview list --help`:\n{s}"
    );
}

#[test]
fn examples_subcommand_is_fast_because_it_skips_the_scan() {
    let start = Instant::now();
    let out = cmd().arg("examples").output().expect("spawn");
    let elapsed = start.elapsed();
    assert_success(&out);
    assert!(
        elapsed.as_secs_f32() < 1.0,
        "`skillview examples` took {:?}; it must be near-instant since it bypasses the scan",
        elapsed
    );
}

#[test]
fn examples_does_not_require_a_real_root() {
    // `examples` short-circuits before the scan, so even a bogus --root
    // must not break it. We deliberately bypass `cmd()` here so we can
    // point --root at a nonexistent path without the fixture interfering.
    use assert_cmd::Command;
    let out = Command::cargo_bin("skillview")
        .expect("skillview binary builds")
        .arg("--root")
        .arg("/nonexistent/path-that-cannot-exist-skillview-test")
        .arg("--no-similarity")
        .arg("--no-usage")
        .arg("examples")
        .output()
        .expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    assert!(
        s.contains("# Discover what's on this machine"),
        "examples output looked truncated with a bogus --root:\n{s}"
    );
}
