//! Black-box CLI tests: run the compiled `tree-police` binary against the repo's
//! fixtures and assert on findings, severities, and exit codes.

use assert_cmd::Command;
use predicates::prelude::*;

fn tree_police() -> Command {
    Command::cargo_bin("tree-police").expect("tree-police binary should build")
}

const PY_VIOLATIONS: &str = "tests/fixtures/python/violations.py";
const PY_SECURITY: &str = "tests/fixtures/python/security_violations.py";
const PY_CLEAN: &str = "tests/fixtures/python/clean.py";
const TF_NAMING: &str = "tests/fixtures/terraform/naming_violations.tf";
const TF_CLEAN: &str = "tests/fixtures/terraform/clean.tf";

#[test]
fn clean_files_produce_no_findings_and_exit_zero() {
    tree_police()
        .args([PY_CLEAN, TF_CLEAN])
        .assert()
        .success()
        .stdout(predicate::str::contains("No findings"));
}

#[test]
fn python_forbidden_patterns_are_reported() {
    tree_police()
        .args(["--fail-on", "none", PY_VIOLATIONS])
        .assert()
        .success()
        .stdout(predicate::str::contains("forbidden_print"))
        .stdout(predicate::str::contains("todo"));
}

#[test]
fn json_output_carries_rule_and_severity() {
    let output = tree_police()
        .args(["--format", "json", "--fail-on", "none", PY_SECURITY])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON array");
    let findings = value.as_array().expect("array");
    assert!(findings
        .iter()
        .any(|f| { f["rule_id"] == "eval_exec_usage" && f["severity"] == "error" }));
}

#[test]
fn terraform_naming_rule_fires() {
    tree_police()
        .args(["--fail-on", "none", TF_NAMING])
        .assert()
        .success()
        .stdout(predicate::str::contains("s3_bucket_missing_prefix"));
}

#[test]
fn fail_on_error_ignores_warnings_but_catches_errors() {
    // violations.py has warning + log only -> exit 0 under --fail-on error.
    tree_police()
        .args(["--fail-on", "error", PY_VIOLATIONS])
        .assert()
        .success();
    // security_violations.py has an error -> exit 1.
    tree_police()
        .args(["--fail-on", "error", PY_SECURITY])
        .assert()
        .code(1);
}

#[test]
fn min_severity_filters_lower_levels() {
    // Only errors requested: the warning/log from violations.py drop out.
    tree_police()
        .args([
            "--min-severity",
            "error",
            "--fail-on",
            "none",
            PY_VIOLATIONS,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No findings"));
}

#[test]
fn list_rules_prints_catalog() {
    tree_police()
        .arg("--list-rules")
        .assert()
        .success()
        .stdout(predicate::str::contains("forbidden_print"))
        .stdout(predicate::str::contains("eval_exec_usage"));
}
