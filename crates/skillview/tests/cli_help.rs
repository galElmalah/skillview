//! Top-level help, version, per-subcommand `--help`, and top-level error paths.
//!
//! These tests exercise clap's argument parsing surface. clap short-circuits
//! before any scan logic, so they're cheap and don't depend on the fixture
//! contents — but using the shared `cmd()` keeps the test setup uniform.

mod support;

use support::{assert_failure, assert_success, cmd, stderr_str, stdout_str};

/// The subcommands that must appear in `skillview --help` AND that must each
/// respond to `--help` themselves.
const SUBCOMMANDS: &[&str] = &[
    "scan", "list", "show", "dups", "usage", "agents", "roots", "stats", "examples",
];

#[test]
fn top_help_exits_zero() {
    let out = cmd().arg("--help").output().expect("spawn");
    assert_success(&out);
}

#[test]
fn top_help_lists_all_subcommands() {
    let out = cmd().arg("--help").output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    for sub in SUBCOMMANDS {
        assert!(
            s.contains(sub),
            "subcommand {sub:?} missing from --help output:\n{s}"
        );
    }
}

#[test]
fn top_help_contains_exploration_tips() {
    let out = cmd().arg("--help").output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    assert!(
        s.contains("EXPLORATION TIPS"),
        "expected 'EXPLORATION TIPS' marker in help, got:\n{s}"
    );
}

#[test]
fn top_help_mentions_examples_recipe_book() {
    let out = cmd().arg("--help").output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    assert!(
        s.contains("skillview examples"),
        "expected help to point users at `skillview examples`, got:\n{s}"
    );
}

#[test]
fn short_version_flag_prints_version() {
    let out = cmd().arg("-V").output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    assert!(s.contains("skillview"), "version output missing name: {s}");
    assert!(
        s.chars().any(|c| c.is_ascii_digit()),
        "version output has no digits: {s}"
    );
}

#[test]
fn long_version_flag_prints_version() {
    let out = cmd().arg("--version").output().expect("spawn");
    assert_success(&out);
    let s = stdout_str(&out);
    assert!(s.contains("skillview"), "version output missing name: {s}");
    assert!(
        s.chars().any(|c| c.is_ascii_digit()),
        "version output has no digits: {s}"
    );
}

#[test]
fn scan_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("scan").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "scan --help produced empty stdout"
    );
}

#[test]
fn list_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("list").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "list --help produced empty stdout"
    );
}

#[test]
fn show_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("show").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "show --help produced empty stdout"
    );
}

#[test]
fn dups_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("dups").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "dups --help produced empty stdout"
    );
}

#[test]
fn usage_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("usage").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "usage --help produced empty stdout"
    );
}

#[test]
fn agents_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("agents").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "agents --help produced empty stdout"
    );
}

#[test]
fn roots_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("roots").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "roots --help produced empty stdout"
    );
}

#[test]
fn stats_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("stats").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "stats --help produced empty stdout"
    );
}

#[test]
fn examples_subcommand_help_exits_zero_and_has_output() {
    let out = cmd().arg("examples").arg("--help").output().expect("spawn");
    assert_success(&out);
    assert!(
        !stdout_str(&out).trim().is_empty(),
        "examples --help produced empty stdout"
    );
}

#[test]
fn bogus_subcommand_fails_with_helpful_stderr() {
    let out = cmd().arg("bogus-cmd").output().expect("spawn");
    assert_failure(&out);
    let err = stderr_str(&out).to_lowercase();
    assert!(
        err.contains("unrecognized") || err.contains("invalid") || err.contains("unexpected"),
        "expected stderr to flag unrecognized/invalid subcommand, got:\n{err}"
    );
}

#[test]
fn list_with_invalid_tier_fails() {
    // `--tier garbage` is not in the value_enum, so clap should reject it.
    let out = cmd()
        .arg("list")
        .arg("--tier")
        .arg("garbage")
        .output()
        .expect("spawn");
    assert_failure(&out);
}
