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
    // The org-prefix rule is an opt-in example, not a built-in.
    tree_police()
        .args(["--fail-on", "none", TF_NAMING])
        .assert()
        .success()
        .stdout(predicate::str::contains("s3_bucket_missing_prefix").not());
    tree_police()
        .args([
            "--rules-dir",
            "examples/tree-police",
            "--fail-on",
            "none",
            TF_NAMING,
        ])
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

#[test]
fn repo_local_rule_replaces_builtin_with_the_same_id() {
    // Same ID as the built-in forbidden_print, but only for print("x").
    let repo = temp_repo(&[
        (
            ".tree-police/mine-py.scm",
            "((call function: (identifier) @_f arguments: (argument_list (string) @_s)) @forbidden_print\n\
             (#eq? @_f \"print\") (#eq? @_s \"\\\"x\\\"\")\n\
             (#set! severity \"error\") (#set! description \"local version\"))\n",
        ),
        ("a.py", "print(\"x\")\nprint(\"y\")\n"),
    ]);
    let findings = findings_in(&repo, &[]);
    // One finding (not two) on line 1, at the local rule's severity; line 2
    // would only match the built-in, which is now off.
    assert_eq!(
        findings,
        vec![("forbidden_print".to_string(), 1, "error".to_string())]
    );
    tree_police()
        .current_dir(repo.path())
        .args(["--list-rules", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("local version"))
        .stdout(predicate::str::contains("Use the logger instead of print()").not());
}

#[test]
fn config_disable_and_exclude() {
    let repo = temp_repo(&[
        (
            ".tree-police/config.toml",
            "exclude = [\"vendor/\"]\ndisable = [\"category:security\"]\n",
        ),
        ("src/a.py", PRINT_AND_EVAL),
        ("vendor/lib.py", PRINT_AND_EVAL),
    ]);
    let findings = findings_in(&repo, &[]);
    assert_eq!(
        findings,
        vec![("forbidden_print".to_string(), 1, "warning".to_string())]
    );
    // Excluded files aren't scanned at all, even when passed explicitly.
    tree_police()
        .current_dir(repo.path())
        .args(["--fail-on", "none", "src/a.py", "vendor/lib.py"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(scanned 1)"));
}

#[test]
fn example_config_is_valid() {
    tree_police()
        .args([
            "--rules-dir",
            "examples/tree-police",
            "--config",
            "examples/tree-police/config.example.toml",
            "--fail-on",
            "none",
            TF_NAMING,
        ])
        .assert()
        .success();
}

// --- Output formats, baseline, fixture tests --------------------------------

#[test]
fn compact_and_summary_formats() {
    tree_police()
        .args(["--format", "compact", "--fail-on", "none", PY_VIOLATIONS])
        .assert()
        .success()
        .stdout(
            predicate::str::is_match(
                r"(?m)^tests/fixtures/python/violations\.py:1:1: warning forbidden_print",
            )
            .unwrap(),
        )
        .stdout(predicate::str::contains("Summary:"));
    tree_police()
        .args(["--format", "summary", "--fail-on", "none", PY_VIOLATIONS])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"(?m)^\s+\d+\s+1\s+warning\s+forbidden_print").unwrap());
}

#[test]
fn a_later_flag_overrides_an_earlier_one() {
    // pre-commit appends `args` after the hook's own entry flags.
    tree_police()
        .args([
            "--format",
            "compact",
            "--fail-on",
            "error",
            "--format",
            "json",
            "--fail-on",
            "none",
        ])
        .arg(PY_SECURITY)
        .assert()
        .success()
        .stdout(predicate::str::starts_with("["));
}

#[test]
fn baseline_hides_known_findings_and_reports_new_ones() {
    let repo = temp_repo(&[("a.py", "print(1)\n"), ("b.py", "print(2)\n")]);
    tree_police()
        .current_dir(repo.path())
        .args(["--update-baseline", "."])
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote 2 finding(s)"));
    assert!(repo.path().join(".tree-police/baseline.json").is_file());

    // Known finding moved down a line, plus a new one: only the new one shows.
    std::fs::write(repo.path().join("a.py"), "x = 1\nprint(1)\nprint(3)\n").unwrap();
    assert_eq!(
        findings_in(&repo, &[]),
        vec![("forbidden_print".to_string(), 3, "warning".to_string())]
    );
    tree_police()
        .current_dir(repo.path())
        .arg(".")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("2 baselined"));
    assert_eq!(findings_in(&repo, &["--no-baseline"]).len(), 3);

    // Scanning a single file by path still matches its baseline entry.
    tree_police()
        .current_dir(repo.path())
        .args(["--fail-on", "none", "b.py"])
        .assert()
        .stdout(predicate::str::contains("No findings"));

    // A partial update keeps the other files' entries.
    tree_police()
        .current_dir(repo.path())
        .args(["--update-baseline", "a.py"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote 3 finding(s)"));
}

#[test]
fn fixture_tests_pass_fail_and_stay_out_of_normal_scans() {
    let rule = "((call function: (identifier) @_f) @no_foo (#eq? @_f \"foo\"))\n";
    let repo = temp_repo(&[
        (".tree-police/foo-py.scm", rule),
        (
            ".tree-police/tests/foo.py",
            "foo()\n# <- @no_foo\nbar()\nprint(1)\n",
        ),
        ("src/a.py", "x = 1\n"),
    ]);
    // print() is a built-in rule nobody asserted, so it isn't a stray.
    tree_police()
        .current_dir(repo.path())
        .arg("--test")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "1 fixture(s), 1 assertion(s), 0 failure(s)",
        ));

    // Fixtures violate rules on purpose; a normal scan never reports them,
    // even when pre-commit passes them by name.
    tree_police()
        .current_dir(repo.path())
        .args(["--hidden", ".", ".tree-police/tests/foo.py"])
        .assert()
        .success();

    std::fs::write(
        repo.path().join(".tree-police/tests/foo.py"),
        "foo()\nfoo()\n# <- @no_foo\nbar()\n# <- @no_foo\n",
    )
    .unwrap();
    tree_police()
        .current_dir(repo.path())
        .args(["--test", ".tree-police/tests"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "foo.py:1: unexpected @no_foo fired with no matching assertion",
        ))
        .stdout(predicate::str::contains(
            "foo.py:5: expected @no_foo to fire on the line above",
        ));
}

// --- --grep and --git-history ---------------------------------------------

#[test]
fn grep_searches_any_text_file_without_a_grammar() {
    let repo = temp_repo(&[
        ("notes.txt", "nothing\nthe api_key lives here\n"),
        ("a.py", "API_KEY = 1\n"),
        ("blob.bin", "api_key\0binary"),
    ]);
    tree_police()
        .current_dir(repo.path())
        .args(["--grep", "(?i)api_key", "--format", "compact"])
        .assert()
        .success()
        .stdout(predicate::str::contains("notes.txt:2:5: @grep api_key"))
        .stdout(predicate::str::contains("a.py:1:1: @grep API_KEY"))
        .stdout(predicate::str::contains("blob.bin").not());
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .expect("git should run");
    assert!(status.success(), "git {args:?} failed");
}

/// A repo whose violations only exist in history: commit 1 adds them, commit
/// 2 removes them, and an unmerged `side` branch adds one more.
fn history_repo() -> tempfile::TempDir {
    let repo = temp_repo(&[
        ("app.py", "print(\"debug\")\nx = 1\n"),
        ("unchanged.py", "y = 2\n"),
        ("notes.txt", "token: hunter2\n"),
    ]);
    let dir = repo.path();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "one"]);
    std::fs::write(dir.join("app.py"), "x = 1\n").unwrap();
    std::fs::remove_file(dir.join("notes.txt")).unwrap();
    git(dir, &["commit", "-q", "-am", "two"]);
    git(dir, &["checkout", "-q", "-b", "side"]);
    std::fs::write(dir.join("side.txt"), "hunter2 again\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "side"]);
    git(dir, &["checkout", "-q", "main"]);
    repo
}

fn history_output(repo: &tempfile::TempDir, args: &[&str]) -> String {
    let output = tree_police()
        .current_dir(repo.path())
        .arg("--git-history")
        .args(args)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

fn first_commit(repo: &tempfile::TempDir) -> String {
    let out = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["rev-list", "--max-parents=0", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn git_history_grep_finds_deleted_content_on_every_ref() {
    let repo = history_repo();
    tree_police()
        .current_dir(repo.path())
        .args(["--grep", "hunter2", "--format", "compact"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let text = history_output(&repo, &["--grep", "hunter2", "--format", "compact"]);
    let lines: Vec<&str> = text.lines().filter(|l| l.contains("@grep")).collect();
    assert_eq!(lines.len(), 2, "{text}");
    let first = &first_commit(&repo)[..7];
    assert!(
        lines[0].starts_with(&format!("notes.txt:1:8: @grep first={first} ")),
        "{text}"
    );
    assert!(lines[0].ends_with("versions=1 head=no"), "{text}");
    // Only on the unmerged branch, so not in HEAD either.
    assert!(lines[1].starts_with("side.txt:1:1: @grep"), "{text}");
    assert!(lines[1].ends_with("head=no"), "{text}");
}

#[test]
fn git_history_collapses_a_line_that_survives_edits() {
    let repo = history_repo();
    // `x = 1` is in both versions of app.py (on line 2, then line 1).
    let text = history_output(&repo, &["--grep", "x = 1", "--format", "compact"]);
    let lines: Vec<&str> = text.lines().filter(|l| l.contains("@grep")).collect();
    assert_eq!(lines, [lines[0]], "{text}");
    assert!(lines[0].starts_with("app.py:2:1: @grep"), "{text}");
    assert!(lines[0].ends_with("versions=2 head=yes"), "{text}");
    assert!(
        text.contains("1 distinct match(es) in 1 file(s), 1 still in HEAD. 2 hit(s)"),
        "{text}"
    );
}

#[test]
fn git_history_pretty_prints_a_table_per_file() {
    let repo = history_repo();
    let text = history_output(&repo, &["--grep", "hunter2"]);
    assert!(text.contains("FIRST SEEN"), "{text}");
    assert!(text.contains("IN HEAD"), "{text}");
    assert!(text.contains("│ token: hunter2"), "{text}");
    // The introducing commit's subject.
    assert!(text.contains("  one\n"), "{text}");
}

#[test]
fn git_history_json_keeps_every_hit_with_commit_fields() {
    let repo = history_repo();
    let text = history_output(&repo, &["--grep", "x = 1", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let hits = value.as_array().expect("array");
    assert_eq!(hits.len(), 2, "{text}");
    assert_eq!(hits[0]["file"], "app.py");
    assert_eq!(hits[0]["commit"], first_commit(&repo));
    assert_eq!(hits[0]["subject"], "one");
    assert_eq!(hits[0]["at_head"], false);
    assert_eq!(hits[1]["subject"], "two");
    assert_eq!(hits[1]["at_head"], true);
}

#[test]
fn git_history_revs_limit_which_refs_are_searched() {
    let repo = history_repo();
    tree_police()
        .current_dir(repo.path())
        .args([
            "--git-history",
            "main",
            "--grep",
            "hunter2",
            "--format",
            "compact",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("notes.txt"))
        .stdout(predicate::str::contains("side.txt").not());
}

#[test]
fn git_history_runs_the_ruleset_once_per_distinct_blob() {
    let repo = history_repo();
    tree_police()
        .current_dir(repo.path())
        .args(["--git-history", "--format", "compact"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "app.py:1:1: warning forbidden_print first=",
        ))
        .stdout(predicate::str::contains("head=no"))
        // app.py (2 versions) + unchanged.py (1): unchanged blobs aren't rescanned.
        .stdout(predicate::str::contains("(scanned 3 file version(s)"));
}

#[test]
fn git_history_query_reports_the_introducing_commit() {
    let repo = history_repo();
    let first = first_commit(&repo);
    let text = history_output(
        &repo,
        &[
            "--query",
            "((identifier) @fn (#eq? @fn \"print\"))",
            "--format",
            "compact",
        ],
    );
    assert!(
        text.contains(&format!("app.py:1:1: @fn first={} ", &first[..7])),
        "{text}"
    );
}

#[test]
fn git_history_outside_a_repo_is_an_internal_error() {
    let dir = tempfile::tempdir().unwrap();
    tree_police()
        .current_dir(dir.path())
        .args(["--git-history", "--grep", "x"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("inside a git repository"));
}
