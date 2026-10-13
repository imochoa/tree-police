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

#[test]
fn show_tree_prints_an_indented_ast() {
    tree_police()
        .args(["--show-tree", PY_VIOLATIONS])
        .assert()
        .success()
        .stdout(predicate::str::contains("module 1:1"))
        .stdout(predicate::str::contains("call 1:1"))
        .stdout(predicate::str::contains(": print"));
}

#[test]
fn show_tree_rejects_an_unregistered_extension() {
    tree_police()
        .args(["--show-tree", "Cargo.toml"])
        .assert()
        .code(2);
}

#[test]
fn ad_hoc_query_infers_language_from_a_single_file() {
    tree_police()
        .args([
            "--query",
            "(call function: (identifier) @fn)",
            PY_VIOLATIONS,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("@fn"))
        .stdout(predicate::str::contains("print(\"debug\")"));
}

#[test]
fn ad_hoc_query_reports_every_non_underscore_capture() {
    // Two captures in one pattern: both should show up, unlike the
    // embedded ruleset's one-rule-id-per-pattern convention.
    tree_police()
        .args([
            "--format",
            "json",
            "--query",
            r#"(call function: (identifier) @fn (#eq? @fn "print") arguments: (argument_list (string) @arg))"#,
            PY_VIOLATIONS,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"capture\": \"fn\""))
        .stdout(predicate::str::contains("\"capture\": \"arg\""));
}

#[test]
fn ad_hoc_query_drops_underscore_captures() {
    tree_police()
        .args([
            "--query",
            r#"(call function: (identifier) @_fn (#eq? @_fn "print")) @whole"#,
            PY_VIOLATIONS,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("@whole"))
        .stdout(predicate::str::contains("@_fn").not());
}

#[test]
fn ad_hoc_query_requires_lang_for_an_ambiguous_directory() {
    tree_police()
        .args(["--query", "(identifier)", "tests/fixtures"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("pass --lang"));
}

#[test]
fn ad_hoc_query_respects_explicit_lang() {
    tree_police()
        .args([
            "--lang",
            "python",
            "--query",
            "(call function: (identifier) @fn)",
            "tests/fixtures/python",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("@fn"));
}

#[test]
fn ad_hoc_query_always_exits_zero_regardless_of_matches() {
    // Ad-hoc query mode is a search, not a lint gate -- --fail-on doesn't
    // apply. Exit 0 whether there are matches (violations.py) or none
    // (clean.py), unlike embedded-ruleset scanning's --fail-on gating.
    tree_police()
        .args([
            "--query",
            "(call function: (identifier) @fn)",
            PY_VIOLATIONS,
        ])
        .assert()
        .success();
    tree_police()
        .args(["--query", "(call function: (identifier) @fn)", PY_CLEAN])
        .assert()
        .success()
        .stdout(predicate::str::contains("No matches"));
}

// --- Suppression comments and the config file ------------------------------
//
// These build a throwaway repo in a temp dir and run from inside it, since
// both features are about paths and per-file content.

fn temp_repo(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    for (rel, contents) in files {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    dir
}

/// `(rule_id, line, severity)` for every finding, scanning `.` from `dir`.
fn findings_in(dir: &tempfile::TempDir, extra: &[&str]) -> Vec<(String, u64, String)> {
    let output = tree_police()
        .current_dir(dir.path())
        .args(["--format", "json", "--fail-on", "none"])
        .args(extra)
        .arg(".")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["rule_id"].as_str().unwrap().to_string(),
                f["line"].as_u64().unwrap(),
                f["severity"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn rule_lines(findings: &[(String, u64, String)], rule: &str) -> Vec<u64> {
    findings
        .iter()
        .filter(|(r, _, _)| r == rule)
        .map(|(_, l, _)| *l)
        .collect()
}

#[test]
fn inline_suppressions_cover_only_their_line_and_selectors() {
    let repo = temp_repo(&[(
        "a.py",
        "print(1)  # tree-police: ignore\n\
         print(2)  # tree-police: ignore[forbidden_print] -- intentional\n\
         print(3)  # tree-police: ignore[category:forbidden]\n\
         print(4)  # tree-police: ignore[eval_exec_usage]\n\
         # tree-police: ignore-next-line[forbidden_print]\n\
         print(6)\n\
         print(7)\n\
         x = \"tree-police: ignore\"; print(8)\n",
    )]);
    let findings = findings_in(&repo, &[]);
    // 4: selector names a different rule; 7: next-line only reaches line 6;
    // 8: the marker is in a string literal, not a comment.
    assert_eq!(rule_lines(&findings, "forbidden_print"), vec![4, 7, 8]);
}

#[test]
fn file_suppression_applies_anywhere_in_the_file() {
    let repo = temp_repo(&[
        (
            "a.py",
            "print(1)\n# tree-police: ignore-file[forbidden_print]\nprint(3)\n",
        ),
        ("b.py", "print(1)\n"),
        (
            "main.tf",
            "# tree-police: ignore-file\nresource \"aws_s3_bucket\" \"x\" {\n  bucket = \"MyBucket\"\n}\n",
        ),
    ]);
    let findings = findings_in(&repo, &[]);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(rule_lines(&findings, "forbidden_print"), vec![1]);
}

#[test]
fn unknown_suppression_selector_warns_and_does_not_suppress() {
    let repo = temp_repo(&[("a.py", "print(1)  # tree-police: ignore[no_such_rule]\n")]);
    tree_police()
        .current_dir(repo.path())
        .arg(".")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("forbidden_print"))
        .stderr(predicate::str::contains("unknown rule ID \"no_such_rule\""));
}

#[test]
fn pretty_summary_reports_suppressed_count() {
    let repo = temp_repo(&[("a.py", "print(1)  # tree-police: ignore\n")]);
    tree_police()
        .current_dir(repo.path())
        .arg(".")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 suppressed"));
}

const SCOPED_CONFIG: &str = r#"
[[scope]]
rules = ["forbidden_print"]
exclude = ["tests/**"]

[[scope]]
rules = ["category:security"]
paths = ["src/"]

[[override]]
rules = ["forbidden_print"]
paths = ["legacy/**"]
severity = "log"
"#;

const PRINT_AND_EVAL: &str = "print(1)\neval(\"x\")\n";

#[test]
fn config_scopes_rules_and_overrides_severity_by_path() {
    let repo = temp_repo(&[
        (".tree-police/config.toml", SCOPED_CONFIG),
        ("src/a.py", PRINT_AND_EVAL),
        ("tests/test_a.py", PRINT_AND_EVAL),
        ("legacy/old.py", PRINT_AND_EVAL),
    ]);
    let output = tree_police()
        .current_dir(repo.path())
        .args(["--format", "json", "--fail-on", "none", "."])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let mut got: Vec<(String, String, String)> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["file"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches("./")
                    .to_string(),
                f["rule_id"].as_str().unwrap().to_string(),
                f["severity"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    got.sort();
    let expected = [
        ("legacy/old.py", "forbidden_print", "log"),
        ("src/a.py", "eval_exec_usage", "error"),
        ("src/a.py", "forbidden_print", "warning"),
    ];
    let expected: Vec<_> = expected
        .iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn config_globs_are_relative_to_repo_root_not_cwd() {
    let repo = temp_repo(&[
        (".tree-police/config.toml", SCOPED_CONFIG),
        ("tests/test_a.py", "print(1)\n"),
    ]);
    // Scanning from the root by a path deeper than the glob still matches.
    tree_police()
        .current_dir(repo.path())
        .arg("tests/test_a.py")
        .assert()
        .success();
    // Explicit --config: globs are relative to that file's own directory.
    let repo = temp_repo(&[
        ("conf/tp.toml", SCOPED_CONFIG),
        ("conf/tests/test_a.py", "print(1)\n"),
        ("tests/test_a.py", "print(1)\n"),
    ]);
    let findings = tree_police()
        .current_dir(repo.path())
        .args(["--config", "conf/tp.toml", "--fail-on", "none", "."])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(findings).unwrap();
    assert!(!text.contains("conf/tests/test_a.py"), "{text}");
    assert!(text.contains("tests/test_a.py"), "{text}");
}

#[test]
fn config_with_unknown_rule_is_an_internal_error() {
    let repo = temp_repo(&[
        (
            ".tree-police/config.toml",
            "[[scope]]\nrules = [\"no_such_rule\"]\nexclude = [\"x\"]\n",
        ),
        ("a.py", "print(1)\n"),
    ]);
    tree_police()
        .current_dir(repo.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unknown rule ID \"no_such_rule\""));
}

#[test]
fn explicit_missing_config_is_an_error() {
    tree_police()
        .args(["--config", "does-not-exist.toml", PY_CLEAN])
        .assert()
        .code(2);
}

#[test]
fn list_rules_lists_a_multi_pattern_rule_once() {
    let repo = temp_repo(&[(
        ".tree-police/two-py.scm",
        "((call function: (identifier) @_f) @two_shapes (#eq? @_f \"a\"))\n\
         ((call function: (identifier) @_f) @two_shapes (#eq? @_f \"b\"))\n",
    )]);
    let output = tree_police()
        .current_dir(repo.path())
        .args(["--list-rules", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let count = value
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["rule_id"] == "two_shapes")
        .count();
    assert_eq!(count, 1);
    tree_police()
        .current_dir(repo.path())
        .arg("--list-rules")
        .assert()
        .success()
        .stdout(predicate::str::contains("two_shapes").count(1));
}

#[test]
fn top_level_predicate_is_rejected_instead_of_ignored() {
    tree_police()
        .args([
            "--query",
            r#"(identifier) @x (#eq? @x "print")"#,
            PY_VIOLATIONS,
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("top level"));
    let repo = temp_repo(&[
        (
            ".tree-police/bad-py.scm",
            "(identifier) @bad (#eq? @bad \"x\")\n",
        ),
        ("a.py", "x = 1\n"),
    ]);
    tree_police()
        .current_dir(repo.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("bad-py.scm"));
}
