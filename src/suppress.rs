//! Inline and whole-file suppression comments.
//!
//! Any comment node (any grammar node whose kind contains `comment`, so
//! `comment`, `line_comment`, `block_comment`, …) can carry one directive:
//!
//! - `tree-police: ignore` -- findings starting on the comment's own line
//!   (i.e. a trailing comment).
//! - `tree-police: ignore-next-line` -- findings starting on the line after
//!   the comment ends.
//! - `tree-police: ignore-file` -- every finding in the file, wherever the
//!   comment sits.
//!
//! Each takes an optional `[selector, …]` list (rule IDs, `category:…`,
//! `file:…`, same as the config file); without one it applies to every rule.
//! Anything after the directive (e.g. `-- reason`) is ignored. Only real
//! comment nodes count, so the same text inside a string literal does
//! nothing.

use std::path::Path;

use tree_sitter::{Node, Tree};

use crate::config::{Catalog, Selector};
use crate::scan::Finding;

const MARKER: &str = "tree-police:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// 0-based row a finding must start on.
    Line(usize),
    File,
}

#[derive(Debug)]
struct Directive {
    scope: Scope,
    /// `None` = every rule.
    selectors: Option<Vec<Selector>>,
}

impl Directive {
    fn covers(&self, finding: &Finding) -> bool {
        let in_scope = match self.scope {
            Scope::File => true,
            Scope::Line(row) => finding.line == row + 1,
        };
        in_scope
            && self
                .selectors
                .as_ref()
                .is_none_or(|sels| sels.iter().any(|s| s.matches(finding)))
    }
}

/// All suppression directives in one file.
#[derive(Debug, Default)]
pub struct Suppressions {
    directives: Vec<Directive>,
}

impl Suppressions {
    /// Collect directives from `tree`'s comment nodes. Malformed directives
    /// and unknown selectors are warned about (with `path:line`) and
    /// skipped -- an unknown selector never widens to "every rule".
    pub fn collect(path: &Path, tree: &Tree, source: &[u8], catalog: &Catalog) -> Self {
        let mut out = Suppressions::default();
        if !source.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()) {
            return out;
        }
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if node.kind().contains("comment") {
                out.add_comment(path, node, source, catalog);
                continue;
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        out
    }

    fn add_comment(&mut self, path: &Path, node: Node, source: &[u8], catalog: &Catalog) {
        let Ok(text) = node.utf8_text(source) else {
            return;
        };
        let line = node.start_position().row + 1;
        let warn = |msg: String| tracing::warn!("{}:{line}: {msg}", path.display());

        let raw = match parse_directive(text) {
            None => return,
            Some(Ok(raw)) => raw,
            Some(Err(DirectiveError::Unrecognized(other))) => {
                warn(format!(
                    "unrecognized tree-police directive {other:?} \
                     (expected ignore, ignore-next-line, or ignore-file)"
                ));
                return;
            }
            Some(Err(DirectiveError::UnclosedBracket)) => {
                warn("unclosed `[` in tree-police directive".to_string());
                return;
            }
        };
        let scope = match raw.kind {
            Kind::Ignore => Scope::Line(node.start_position().row),
            Kind::IgnoreNextLine => Scope::Line(node.end_position().row + 1),
            Kind::IgnoreFile => Scope::File,
        };
        let selectors = raw.selectors.map(|items| {
            items
                .into_iter()
                .filter_map(|item| {
                    match Selector::parse(item).and_then(|s| catalog.check(&s).map(|()| s)) {
                        Ok(sel) => Some(sel),
                        Err(err) => {
                            warn(format!("in tree-police {}: {err}", raw.kind.as_str()));
                            None
                        }
                    }
                })
                .collect()
        });
        self.directives.push(Directive { scope, selectors });
    }

    /// Drop suppressed findings; returns how many were dropped.
    pub fn apply(&self, findings: &mut Vec<Finding>) -> usize {
        if self.directives.is_empty() {
            return 0;
        }
        let before = findings.len();
        findings.retain(|f| !self.directives.iter().any(|d| d.covers(f)));
        before - findings.len()
    }
}

/// Which directive a comment carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ignore,
    IgnoreNextLine,
    IgnoreFile,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Ignore => "ignore",
            Kind::IgnoreNextLine => "ignore-next-line",
            Kind::IgnoreFile => "ignore-file",
        }
    }
}

/// A directive as written in a comment, before its selectors are checked
/// against the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDirective<'a> {
    pub kind: Kind,
    /// The `[…]` items, untrimmed, blank ones dropped; `None` = no list.
    pub selectors: Option<Vec<&'a str>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectiveError<'a> {
    /// The word after `tree-police:` isn't a known directive.
    Unrecognized(&'a str),
    UnclosedBracket,
}

/// Parse the directive in one comment's text; `None` if it has no marker.
///
/// Pure (no tree-sitter, no catalog) so it can be property-tested, fuzzed
/// (`fuzz/`), and compared against the Lean model in `formal/` (see
/// `tests/lean_oracle.rs`).
pub fn parse_directive(text: &str) -> Option<Result<RawDirective<'_>, DirectiveError<'_>>> {
    let idx = text.find(MARKER)?;
    let (word, rest) = split_word(text[idx + MARKER.len()..].trim_start());
    let kind = match word {
        "ignore" => Kind::Ignore,
        "ignore-next-line" => Kind::IgnoreNextLine,
        "ignore-file" => Kind::IgnoreFile,
        other => return Some(Err(DirectiveError::Unrecognized(other))),
    };
    let selectors = match rest.strip_prefix('[') {
        None => None,
        Some(list) => {
            let Some(end) = list.find(']') else {
                return Some(Err(DirectiveError::UnclosedBracket));
            };
            Some(
                list[..end]
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .collect(),
            )
        }
    };
    Some(Ok(RawDirective { kind, selectors }))
}

/// Split off a leading `[A-Za-z0-9-]*` word.
fn split_word(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}
