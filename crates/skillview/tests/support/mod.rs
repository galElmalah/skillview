//! Shared helpers for skillview e2e tests.
//!
//! Each integration test file includes this module via `mod support;`.
//! All helpers run the `skillview` binary against the fixture under
//! `tests/fixtures/home/` so behavior is deterministic regardless of the
//! developer's actual `$HOME`.
//!
//! Two flavors of runner:
//!   - `run(args)`        — fast path with `--no-similarity --no-usage`,
//!                          for tests that don't care about clustering or
//!                          usage scanning.
//!   - `run_full(extra_scan, args)` — opt back into similarity and/or usage.
//!     Pass `[]` for `extra_scan` to use defaults, or e.g.
//!     `["--threshold", "0.6"]` to dial in near-dup detection.

#![allow(dead_code)]

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Absolute path to the fixture home directory bundled with the crate.
pub fn fixture_home() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("home")
}

/// Build a `Command` wired to the fixture home for HOME + --root, with usage +
/// similarity disabled by default. Callers append subcommand + args.
pub fn cmd() -> Command {
    let home = fixture_home();
    let mut c = Command::cargo_bin("skillview").expect("skillview binary builds");
    c.env("HOME", &home)
        .arg("--root")
        .arg(&home)
        .arg("--no-similarity")
        .arg("--no-usage");
    c
}

/// Like `cmd()` but with similarity + usage enabled. Pass additional global
/// scan flags via `extra_scan` (e.g. `["--threshold", "0.6"]`).
pub fn cmd_full(extra_scan: &[&str]) -> Command {
    let home = fixture_home();
    let mut c = Command::cargo_bin("skillview").expect("skillview binary builds");
    c.env("HOME", &home).arg("--root").arg(&home);
    for a in extra_scan {
        c.arg(a);
    }
    c
}

/// Run with the fast-path command (no similarity, no usage) and the given
/// subcommand+args. Returns the raw Output; callers assert on it.
pub fn run<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    cmd().args(args).output().expect("spawn skillview")
}

/// Run with similarity + usage enabled. `extra_scan` carries extra global
/// scan flags. `args` is the subcommand + its flags.
pub fn run_full<I, S>(extra_scan: &[&str], args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    cmd_full(extra_scan)
        .args(args)
        .output()
        .expect("spawn skillview")
}

pub fn assert_success(out: &Output) {
    if !out.status.success() {
        panic!(
            "command failed: exit={:?}\nstdout:\n{}\nstderr:\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

pub fn assert_failure(out: &Output) {
    if out.status.success() {
        panic!(
            "expected failure but got exit=0\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

pub fn stdout_str(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("stdout is utf-8")
}

pub fn stderr_str(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("stderr is utf-8")
}

pub fn parse_json(out: &Output) -> Value {
    assert_success(out);
    let s = stdout_str(out);
    serde_json::from_str(&s).unwrap_or_else(|e| {
        panic!(
            "stdout was not valid JSON: {e}\n--- stdout ---\n{s}\n--- stderr ---\n{}",
            stderr_str(out)
        )
    })
}

/// Parse JSONL output (one JSON object per line). Empty lines are ignored.
pub fn parse_jsonl(out: &Output) -> Vec<Value> {
    assert_success(out);
    let s = stdout_str(out);
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap_or_else(|e| {
                panic!("JSONL line was not valid JSON ({e}): {l}\n--- full stdout ---\n{s}")
            })
        })
        .collect()
}

/// Parse TSV output: returns (header, rows). Drops trailing blank rows.
pub fn parse_tsv(out: &Output) -> (Vec<String>, Vec<Vec<String>>) {
    assert_success(out);
    let s = stdout_str(out);
    let mut lines = s.lines().filter(|l| !l.is_empty());
    let header: Vec<String> = lines
        .next()
        .expect("tsv has header")
        .split('\t')
        .map(|c| c.to_string())
        .collect();
    let rows: Vec<Vec<String>> = lines
        .map(|l| l.split('\t').map(|c| c.to_string()).collect())
        .collect();
    (header, rows)
}

/// Lines of stdout, dropping trailing blank lines.
pub fn stdout_lines(out: &Output) -> Vec<String> {
    assert_success(out);
    stdout_str(out)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

/// Convenience: find the first skill in a `list`/`scan` response whose name
/// matches `name`. Panics if not present so tests fail loudly.
pub fn find_skill_by_name<'a>(skills: &'a [Value], name: &str) -> &'a Value {
    skills
        .iter()
        .find(|s| s["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no skill named {name:?} in payload"))
}
