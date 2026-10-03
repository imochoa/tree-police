//! Per-rule query-case tests: each fixture under `tests/query-cases/<lang>/`
//! is annotated with `# <- @capture_name` comments naming the capture
//! expected to fire on the line directly above.
//!
//! Unlike `tree-sitter query --test` (which pairs assertions to matches in a
//! way that gets confused once a query file defines more than one distinct
//! capture name -- verified empirically, not a guess), this checks every
//! assertion against the actual capture spans directly, and -- the other
//! direction -- flags any capture that fires on a line with no matching
//! assertion. That gives both "should match" and "should not match"
//! coverage from one annotated file, which plain whole-file clean fixtures
//! don't: a rule firing on the wrong line of a big fixture still passes a
//! file-level `assert_clean`/`assert_match` check.

use std::fs;

use tree_police::registry;
use tree_police::rules;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, QueryCursor};

/// A `# <- @name` assertion: the capture expected on the line above it.
struct Assertion {
    /// 0-indexed row of the line being asserted about (the comment's row - 1).
    target_row: usize,
    expected: String,
    /// 1-indexed line of the assertion comment itself, for error messages.
    comment_line: usize,
}

fn parse_assertions(source: &str) -> Vec<Assertion> {
    let mut out = Vec::new();
    for (row, line) in source.lines().enumerate() {
        let Some(name) = line.trim().strip_prefix("# <- @") else {
            continue;
        };
        if row == 0 || name.is_empty() {
            continue;
        }
        out.push(Assertion {
            target_row: row - 1,
            expected: name.trim().to_string(),
            comment_line: row + 1,
        });
    }
    out
}

/// A reported (non-helper) capture from running the query over the fixture.
struct ActualCapture {
    name: String,
    start_row: usize,
    end_row: usize,
}

/// Run `queries/<query_stem>-<code>.scm` (where `<code>` is `lang`'s
/// registry code) against `tests/query-cases/<lang>/<fixture_stem>.<ext>` and
/// check every assertion fires, and nothing else does. `query_stem` and
/// `fixture_stem` are separate because a language's rules live in one file
/// (`rules-<code>.scm`) while its fixtures stay split by topic for
/// readability.
fn run_query_cases(lang: &str, query_stem: &str, fixture_stem: &str, ext: &str) {
    let fixture_path = format!("tests/query-cases/{lang}/{fixture_stem}.{ext}");
    let source = fs::read_to_string(&fixture_path)
        .unwrap_or_else(|err| panic!("reading {fixture_path}: {err}"));

    let assertions = parse_assertions(&source);
    assert!(
        !assertions.is_empty(),
        "{fixture_path} has no `# <- @name` assertions -- nothing to check"
    );
    // Assertion comments are themselves source lines, so a rule that matches
    // on comment *content* (e.g. `todo`, scanning for the literal word
    // "todo") can legitimately fire on the `# <- @todo` line right below the
    // one it's annotating. Exclude assertion-comment rows from the stray
    // check below -- they're test metadata, not fixture content.
    let assertion_rows: std::collections::HashSet<usize> =
        assertions.iter().map(|a| a.comment_line - 1).collect();

    let spec = registry::by_name(lang).expect("language is registered");

    let ruleset = rules::load().expect("embedded queries should compile");
    let lang_rules = ruleset
        .language(lang)
        .unwrap_or_else(|| panic!("no rules registered for language {lang}"));
    let query_name = format!("{query_stem}-{}.scm", spec.code);
    let cq = lang_rules
        .queries
        .iter()
        .find(|q| q.source_path == query_name)
        .unwrap_or_else(|| panic!("no compiled query found for {query_name}"));
    let mut parser = Parser::new();
    parser
        .set_language(&spec.language())
        .expect("grammar should load");
    let tree = parser
        .parse(source.as_bytes(), None)
        .expect("fixture should parse");

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&cq.query, tree.root_node(), source.as_bytes());
    let mut actual = Vec::new();
    while let Some(m) = matches.next() {
        if let Some(capture) = m.captures.iter().find(|c| !cq.is_helper(c.index)) {
            let node = capture.node;
            actual.push(ActualCapture {
                name: cq.capture_name(capture.index).to_string(),
                start_row: node.start_position().row,
                end_row: node.end_position().row,
            });
        }
    }

    let mut failures = Vec::new();

    for a in &assertions {
        let hit = actual
            .iter()
            .any(|c| c.name == a.expected && (c.start_row..=c.end_row).contains(&a.target_row));
        if !hit {
            failures.push(format!(
                "{fixture_path}:{}: expected @{} to fire on the line above, but it did not",
                a.comment_line, a.expected
            ));
        }
    }

    for c in &actual {
        if (c.start_row..=c.end_row).any(|row| assertion_rows.contains(&row)) {
            continue;
        }
        let covered = assertions
            .iter()
            .any(|a| a.expected == c.name && (c.start_row..=c.end_row).contains(&a.target_row));
        if !covered {
            failures.push(format!(
                "{fixture_path}:{}: unexpected @{} fired with no matching assertion",
                c.start_row + 1,
                c.name
            ));
        }
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn python_forbidden_patterns() {
    run_query_cases("python", "rules", "forbidden-patterns", "py");
}

#[test]
fn python_security_patterns() {
    run_query_cases("python", "rules", "security-patterns", "py");
}

#[test]
fn python_temporal_patterns() {
    run_query_cases("python", "rules", "temporal-patterns", "py");
}

#[test]
fn python_web_patterns() {
    run_query_cases("python", "rules", "web-patterns", "py");
}
