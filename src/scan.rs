//! The scan engine: walk the filesystem, parse each file once, run every
//! query for its language, and collect [`Finding`]s.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use rayon::prelude::*;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, QueryCursor};

use crate::registry;
use crate::rules::Ruleset;
use crate::severity::Severity;

/// One rule match at a source location.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Finding {
    pub file: PathBuf,
    pub language: &'static str,
    pub rule_id: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// 1-based line of the match start.
    pub line: usize,
    /// 1-based column (byte offset within the line) of the match start.
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    /// Query file the rule came from, e.g. `rules-py.scm`.
    pub query_file: String,
    /// The source line containing the match (trailing whitespace trimmed).
    pub snippet: String,
}

/// Aggregate counters for the summary line.
#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub files_scanned: usize,
    pub files_with_findings: usize,
}

/// Filesystem-walk options.
pub struct ScanOptions {
    pub respect_gitignore: bool,
    pub hidden: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            hidden: false,
        }
    }
}

/// Scan `paths` and return findings (sorted by file, line, column) plus stats.
pub fn scan(
    paths: &[PathBuf],
    ruleset: &Ruleset,
    opts: &ScanOptions,
) -> Result<(Vec<Finding>, ScanStats)> {
    if paths.is_empty() {
        return Ok((Vec::new(), ScanStats::default()));
    }

    let files = collect_files(paths, ruleset, opts);

    let per_file: Vec<Vec<Finding>> = files
        .par_iter()
        .map_init(Parser::new, |parser, path| scan_file(parser, path, ruleset))
        .collect();

    let mut findings = Vec::new();
    let mut files_with_findings = 0;
    for file_findings in per_file {
        if !file_findings.is_empty() {
            files_with_findings += 1;
        }
        findings.extend(file_findings);
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });

    let stats = ScanStats {
        files_scanned: files.len(),
        files_with_findings,
    };
    Ok((findings, stats))
}

fn collect_files(paths: &[PathBuf], ruleset: &Ruleset, opts: &ScanOptions) -> Vec<PathBuf> {
    let mut builder = WalkBuilder::new(&paths[0]);
    for p in &paths[1..] {
        builder.add(p);
    }
    builder
        .hidden(!opts.hidden)
        .git_ignore(opts.respect_gitignore)
        .git_global(opts.respect_gitignore)
        .git_exclude(opts.respect_gitignore)
        .ignore(opts.respect_gitignore)
        .parents(opts.respect_gitignore)
        // Never descend into the archived grammar clones or Terraform state.
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            name != "grammars-old" && name != ".terraform"
        });

    let mut files = Vec::new();
    for result in builder.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                tracing::warn!("walk error: {err}");
                continue;
            }
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if let Some(spec) = registry::for_extension(ext) {
            if ruleset.language(spec.name).is_some() {
                files.push(path.to_path_buf());
            }
        }
    }
    files
}

fn scan_file(parser: &mut Parser, path: &Path, ruleset: &Ruleset) -> Vec<Finding> {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return Vec::new();
    };
    let Some(spec) = registry::for_extension(ext) else {
        return Vec::new();
    };
    let Some(lang_rules) = ruleset.language(spec.name) else {
        return Vec::new();
    };

    let source = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("skipping {}: {err}", path.display());
            return Vec::new();
        }
    };

    if parser.set_language(&spec.language()).is_err() {
        tracing::error!("could not load grammar for {}", spec.name);
        return Vec::new();
    }
    let Some(tree) = parser.parse(&source, None) else {
        tracing::warn!("failed to parse {}", path.display());
        return Vec::new();
    };
    let root = tree.root_node();

    let mut findings = Vec::new();
    let mut cursor = QueryCursor::new();
    for cq in &lang_rules.queries {
        let mut matches = cursor.matches(&cq.query, root, source.as_slice());
        while let Some(m) = matches.next() {
            // The one non-helper capture is the finding.
            let Some(capture) = m.captures.iter().find(|c| !cq.is_helper(c.index)) else {
                continue;
            };
            let node = capture.node;
            let meta = cq.pattern_meta(m.pattern_index);
            let start = node.start_position();
            let end = node.end_position();
            findings.push(Finding {
                file: path.to_path_buf(),
                language: spec.name,
                rule_id: cq.capture_name(capture.index).to_string(),
                severity: meta.severity,
                description: meta.description.clone(),
                category: meta.category.clone(),
                line: start.row + 1,
                column: start.column + 1,
                end_line: end.row + 1,
                end_column: end.column + 1,
                query_file: cq.source_path.clone(),
                snippet: line_text(&source, node.start_byte()),
            });
        }
    }
    findings
}

/// Extract the source line containing `byte`, trailing whitespace trimmed.
fn line_text(source: &[u8], byte: usize) -> String {
    let byte = byte.min(source.len());
    let start = source[..byte]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |p| p + 1);
    let end = source[byte..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(source.len(), |p| byte + p);
    String::from_utf8_lossy(&source[start..end])
        .trim_end()
        .to_string()
}
