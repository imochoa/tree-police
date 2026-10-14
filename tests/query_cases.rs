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

use streaming_iterator::StreamingIterator;
use tree_police::casetest::{self, Hit};
use tree_police::registry;
use tree_police::rules;
use tree_sitter::{Parser, QueryCursor};

/// Run `queries/<query_stem>-<code>.scm` (where `<code>` is `lang`'s
/// registry code) against `tests/query-cases/<lang>/<fixture_stem>.<ext>` and
/// check every assertion fires, and nothing else does. `query_stem` and
/// `fixture_stem` are separate because a language's rules live in one file
/// (`rules-<code>.scm`) while its fixtures stay split by topic for
/// readability. The assertion syntax and checking live in
/// `tree_police::casetest`, shared with `tree-police --test`.
fn run_query_cases(lang: &str, query_stem: &str, fixture_stem: &str, ext: &str) {
    let fixture_path = format!("tests/query-cases/{lang}/{fixture_stem}.{ext}");
    let source = fs::read_to_string(&fixture_path)
        .unwrap_or_else(|err| panic!("reading {fixture_path}: {err}"));

    let assertions = casetest::parse_assertions(&source);
    assert!(
        !assertions.is_empty(),
        "{fixture_path} has no `# <- @name` assertions -- nothing to check"
    );

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
    let mut hits = Vec::new();
    while let Some(m) = matches.next() {
        if let Some(capture) = m.captures.iter().find(|c| !cq.is_helper(c.index)) {
            let node = capture.node;
            hits.push(Hit {
                rule_id: cq.capture_name(capture.index).to_string(),
                start_row: node.start_position().row,
                end_row: node.end_position().row,
            });
        }
    }

    let failures: Vec<String> = casetest::check(&assertions, &hits, |_| true)
        .into_iter()
        .map(|(line, message)| format!("{fixture_path}:{line}: {message}"))
        .collect();
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

#[test]
fn terraform_variables_and_naming() {
    run_query_cases("terraform", "rules", "variables-and-naming", "tf");
}
