//! Annotated rule fixtures: `<- @rule_id` comments name the rule expected to
//! fire on the line directly above.
//!
//! ```python
//! print("debug")
//! # <- @forbidden_print
//! ```
//!
//! Checked in both directions: every assertion must be hit, and a checked
//! rule that fires on a line with no matching assertion is a failure too, so
//! one fixture gives "should match" and "should not match" coverage. Any
//! line comment works (`#`, `//`, `--`, `;`, `%`), so fixtures can be written
//! in every registered language.
//!
//! Used by `tree-police --test` (repo-local rule fixtures) and by this
//! crate's own `tests/query_cases.rs`.

use std::collections::HashSet;

/// A `<- @name` assertion: the rule expected on the line above it.
#[derive(Debug, Clone)]
pub struct Assertion {
    /// 0-based row of the line being asserted about (the comment's row - 1).
    pub target_row: usize,
    pub expected: String,
    /// 0-based row of the assertion comment itself.
    pub comment_row: usize,
}

/// A rule match to check against the assertions (0-based rows, inclusive).
#[derive(Debug, Clone)]
pub struct Hit {
    pub rule_id: String,
    pub start_row: usize,
    pub end_row: usize,
}

const COMMENT_PREFIXES: [&str; 5] = ["#", "//", "--", ";", "%"];

/// Every `<- @name` assertion in `source`.
pub fn parse_assertions(source: &str) -> Vec<Assertion> {
    let mut out = Vec::new();
    for (row, line) in source.lines().enumerate() {
        let line = line.trim_start();
        let Some(rest) = COMMENT_PREFIXES
            .iter()
            .find_map(|p| line.strip_prefix(p))
            .map(|r| r.trim_start_matches([';', '/', '-']))
        else {
            continue;
        };
        let Some(name) = rest.trim().strip_prefix("<- @") else {
            continue;
        };
        let name = name.split_whitespace().next().unwrap_or_default();
        if row == 0 || name.is_empty() {
            continue;
        }
        out.push(Assertion {
            target_row: row - 1,
            expected: name.to_string(),
            comment_row: row,
        });
    }
    out
}

/// Failures for one fixture, as `"<line>: message"` (1-based lines).
///
/// `checked` says which rules count as strays when they fire unannotated;
/// rules asserted somewhere in the fixture are always checked. A hit that
/// overlaps an assertion comment is ignored for the stray check: comment
/// rules (e.g. a TODO matcher) can legitimately fire on the annotation itself.
pub fn check(
    assertions: &[Assertion],
    hits: &[Hit],
    checked: impl Fn(&str) -> bool,
) -> Vec<(usize, String)> {
    let comment_rows: HashSet<usize> = assertions.iter().map(|a| a.comment_row).collect();
    let asserted: HashSet<&str> = assertions.iter().map(|a| a.expected.as_str()).collect();
    let mut failures = Vec::new();

    for a in assertions {
        let hit = hits
            .iter()
            .any(|h| h.rule_id == a.expected && (h.start_row..=h.end_row).contains(&a.target_row));
        if !hit {
            failures.push((
                a.comment_row + 1,
                format!(
                    "expected @{} to fire on the line above, but it did not",
                    a.expected
                ),
            ));
        }
    }

    for h in hits {
        if !asserted.contains(h.rule_id.as_str()) && !checked(&h.rule_id) {
            continue;
        }
        if (h.start_row..=h.end_row).any(|row| comment_rows.contains(&row)) {
            continue;
        }
        let covered = assertions
            .iter()
            .any(|a| a.expected == h.rule_id && (h.start_row..=h.end_row).contains(&a.target_row));
        if !covered {
            failures.push((
                h.start_row + 1,
                format!("unexpected @{} fired with no matching assertion", h.rule_id),
            ));
        }
    }

    failures.sort();
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(rule: &str, row: usize) -> Hit {
        Hit {
            rule_id: rule.to_string(),
            start_row: row,
            end_row: row,
        }
    }

    #[test]
    fn parses_assertions_in_any_line_comment_syntax() {
        let src = "a\n# <- @one\nb\n// <- @two\nc\n  -- <- @three trailing words\nd\n;; <- @four\n";
        let got: Vec<_> = parse_assertions(src)
            .into_iter()
            .map(|a| (a.target_row, a.expected))
            .collect();
        assert_eq!(
            got,
            vec![
                (0, "one".to_string()),
                (2, "two".to_string()),
                (4, "three".to_string()),
                (6, "four".to_string()),
            ]
        );
    }

    #[test]
    fn reports_misses_and_checked_strays_only() {
        let assertions = parse_assertions("x\n# <- @a\ny\n# <- @a\n");
        let hits = vec![hit("a", 0), hit("a", 4), hit("builtin", 4), hit("mine", 4)];
        let failures = check(&assertions, &hits, |r| r == "mine");
        assert_eq!(
            failures,
            vec![
                (
                    4,
                    "expected @a to fire on the line above, but it did not".to_string()
                ),
                (
                    5,
                    "unexpected @a fired with no matching assertion".to_string()
                ),
                (
                    5,
                    "unexpected @mine fired with no matching assertion".to_string()
                ),
            ]
        );
    }
}
