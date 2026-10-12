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
        let Some(idx) = text.find(MARKER) else {
            return;
        };
        let line = node.start_position().row + 1;
        let warn = |msg: String| tracing::warn!("{}:{line}: {msg}", path.display());

        let (word, rest) = split_word(text[idx + MARKER.len()..].trim_start());
        let scope = match word {
            "ignore" => Scope::Line(node.start_position().row),
            "ignore-next-line" => Scope::Line(node.end_position().row + 1),
            "ignore-file" => Scope::File,
            other => {
                warn(format!(
                    "unrecognized tree-police directive {other:?} \
                     (expected ignore, ignore-next-line, or ignore-file)"
                ));
                return;
            }
        };

        let selectors = match rest.strip_prefix('[') {
            None => None,
            Some(list) => {
                let Some(end) = list.find(']') else {
                    warn("unclosed `[` in tree-police directive".to_string());
                    return;
                };
                let mut sels = Vec::new();
                for item in list[..end].split(',').filter(|s| !s.trim().is_empty()) {
                    match Selector::parse(item).and_then(|s| catalog.check(&s).map(|()| s)) {
                        Ok(sel) => sels.push(sel),
                        Err(err) => warn(format!("in tree-police {word}: {err}")),
                    }
                }
                Some(sels)
            }
        };
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

/// Split off a leading `[a-z-]+` word.
fn split_word(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}
