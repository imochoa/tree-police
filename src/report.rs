//! Output rendering: a ripgrep-style pretty report and machine-readable
//! JSON / JSONL. Findings are written to the provided writer (stdout); the
//! caller wraps it in an `anstream` stream so ANSI colors are stripped when the
//! output is not a terminal or `NO_COLOR` is set.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::history::{self, History, Hit, Occurrence, Reportable};
use crate::rules::Ruleset;
use crate::scan::{Finding, Match, ScanStats};
use crate::severity::Severity;

/// Output format for findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum Format {
    /// Colored, grouped-by-file terminal report with a summary.
    Pretty,
    /// A single pretty-printed JSON array of findings.
    Json,
    /// One JSON finding object per line.
    Jsonl,
    /// One line per finding (`path:line:col: severity rule  description`)
    /// plus the summary; suits pre-commit and editor problem matchers.
    Compact,
    /// Counts per rule (findings and files), most frequent first. For
    /// sizing up a rollout rather than fixing individual findings.
    Summary,
}

/// Render findings in the requested format.
pub fn render(
    format: Format,
    findings: &[Finding],
    stats: &ScanStats,
    w: &mut impl Write,
) -> Result<()> {
    match format {
        Format::Pretty => render_pretty(findings, stats, w),
        Format::Compact => {
            for f in findings {
                let description = f
                    .description
                    .as_deref()
                    .map(|d| format!("  {d}"))
                    .unwrap_or_default();
                writeln!(
                    w,
                    "{}:{}:{}: {} {}{description}",
                    f.file.display(),
                    f.line,
                    f.column,
                    colorize(f.severity, f.severity.as_str()),
                    f.rule_id.bold(),
                )?;
            }
            render_summary(findings, stats, w)
        }
        Format::Summary => render_rule_counts(findings, stats, w),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *w, findings)?;
            writeln!(w)?;
            Ok(())
        }
        Format::Jsonl => {
            for finding in findings {
                serde_json::to_writer(&mut *w, finding)?;
                writeln!(w)?;
            }
            Ok(())
        }
    }
}

fn render_pretty(findings: &[Finding], stats: &ScanStats, w: &mut impl Write) -> Result<()> {
    let mut current: Option<&Path> = None;
    for finding in findings {
        if current != Some(finding.file.as_path()) {
            if current.is_some() {
                writeln!(w)?;
            }
            writeln!(w, "{}", finding.file.display().bold().underline())?;
            current = Some(finding.file.as_path());
        }

        let location = format!("{}:{}", finding.line, finding.column);
        let description = finding
            .description
            .as_deref()
            .map(|d| format!("  {d}"))
            .unwrap_or_default();
        writeln!(
            w,
            "  {location}  {badge}  {rule}{description}",
            location = location.dimmed(),
            badge = severity_badge(finding.severity),
            rule = finding.rule_id.bold(),
        )?;

        let gutter = format!("{:>4} | ", finding.line);
        writeln!(w, "  {}{}", gutter.dimmed(), finding.snippet)?;

        let indent = " ".repeat(gutter.len());
        let pad = " ".repeat(finding.column.saturating_sub(1));
        let width = caret_width(
            finding.line,
            finding.column,
            finding.end_line,
            finding.end_column,
            &finding.snippet,
        );
        let caret = "^".repeat(width);
        writeln!(w, "  {indent}{pad}{}", colorize(finding.severity, &caret))?;
    }

    render_summary(findings, stats, w)
}

/// One row per rule: severity, finding count, file count; most findings
/// first, ties by rule ID.
fn render_rule_counts(findings: &[Finding], stats: &ScanStats, w: &mut impl Write) -> Result<()> {
    struct Row<'a> {
        rule: &'a str,
        severity: Severity,
        findings: usize,
        files: HashSet<&'a Path>,
    }
    let mut rows: Vec<Row> = Vec::new();
    for f in findings {
        let row = match rows.iter_mut().position(|r| r.rule == f.rule_id) {
            Some(i) => &mut rows[i],
            None => {
                rows.push(Row {
                    rule: &f.rule_id,
                    severity: f.severity,
                    findings: 0,
                    files: HashSet::new(),
                });
                rows.last_mut().unwrap()
            }
        };
        row.findings += 1;
        row.severity = row.severity.max(f.severity);
        row.files.insert(f.file.as_path());
    }
    rows.sort_by(|a, b| b.findings.cmp(&a.findings).then(a.rule.cmp(b.rule)));

    if !rows.is_empty() {
        writeln!(
            w,
            "{}",
            format!("{:>8}  {:>6}  {:<7}  rule", "findings", "files", "severity").dimmed()
        )?;
        for r in &rows {
            writeln!(
                w,
                "{:>8}  {:>6}  {}  {}",
                r.findings,
                r.files.len(),
                severity_badge(r.severity),
                r.rule,
            )?;
        }
    }
    render_summary(findings, stats, w)
}

fn render_summary(findings: &[Finding], stats: &ScanStats, w: &mut impl Write) -> Result<()> {
    writeln!(w)?;
    let mut suppressed = match stats.suppressed {
        0 => String::new(),
        n => format!(" {n} suppressed by tree-police: ignore comments."),
    };
    if stats.baselined > 0 {
        suppressed.push_str(&format!(" {} baselined.", stats.baselined));
    }
    if findings.is_empty() {
        writeln!(
            w,
            "{}{}",
            format!("No findings. Scanned {} file(s).", stats.files_scanned).green(),
            suppressed.dimmed(),
        )?;
        return Ok(());
    }

    let mut errors = 0;
    let mut warnings = 0;
    let mut logs = 0;
    for finding in findings {
        match finding.severity {
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
            Severity::Log => logs += 1,
        }
    }

    writeln!(
        w,
        "{} {} error(s), {} warning(s), {} log(s) across {} file(s) (scanned {}).{}",
        "Summary:".bold(),
        errors.to_string().red().bold(),
        warnings.to_string().yellow().bold(),
        logs.to_string().cyan().bold(),
        stats.files_with_findings,
        stats.files_scanned,
        suppressed.dimmed(),
    )?;
    Ok(())
}

/// Render ad-hoc `--query`/`--query-file` matches (see `scan::run_query`)
/// in the requested format. Unlike `render`, there's no severity/category
/// to show -- just where each capture landed and what it caught.
pub fn render_matches(
    format: Format,
    matches: &[Match],
    stats: &ScanStats,
    w: &mut impl Write,
) -> Result<()> {
    match format {
        Format::Pretty | Format::Summary => render_matches_pretty(matches, stats, w),
        Format::Compact => {
            for m in matches {
                writeln!(
                    w,
                    "{}:{}:{}: @{} {}",
                    m.file.display(),
                    m.line,
                    m.column,
                    m.capture.bold(),
                    m.text.lines().next().unwrap_or_default(),
                )?;
            }
            Ok(())
        }
        Format::Json => {
            serde_json::to_writer_pretty(&mut *w, matches)?;
            writeln!(w)?;
            Ok(())
        }
        Format::Jsonl => {
            for m in matches {
                serde_json::to_writer(&mut *w, m)?;
                writeln!(w)?;
            }
            Ok(())
        }
    }
}

fn render_matches_pretty(matches: &[Match], stats: &ScanStats, w: &mut impl Write) -> Result<()> {
    let mut current: Option<&Path> = None;
    for m in matches {
        if current != Some(m.file.as_path()) {
            if current.is_some() {
                writeln!(w)?;
            }
            writeln!(w, "{}", m.file.display().bold().underline())?;
            current = Some(m.file.as_path());
        }

        let location = format!("{}:{}", m.line, m.column);
        writeln!(
            w,
            "  {location}  @{capture}",
            location = location.dimmed(),
            capture = m.capture.bold(),
        )?;

        let gutter = format!("{:>4} | ", m.line);
        writeln!(w, "  {}{}", gutter.dimmed(), m.snippet)?;

        let indent = " ".repeat(gutter.len());
        let pad = " ".repeat(m.column.saturating_sub(1));
        let width = caret_width(m.line, m.column, m.end_line, m.end_column, &m.snippet);
        writeln!(w, "  {indent}{pad}{}", "^".repeat(width).cyan())?;
    }

    writeln!(w)?;
    if matches.is_empty() {
        writeln!(
            w,
            "{}",
            format!("No matches. Scanned {} file(s).", stats.files_scanned).green()
        )?;
    } else {
        writeln!(
            w,
            "{} {} match(es) in {} file(s) (scanned {}).",
            "Summary:".bold(),
            matches.len().to_string().bold(),
            stats.files_with_findings,
            stats.files_scanned,
        )?;
    }
    Ok(())
}

/// Render `--git-history` results. The human formats (`pretty`, `compact`,
/// `summary`) collapse the same hit across file versions into one
/// [`Occurrence`] row with when it first appeared, the newest version that
/// still has it, and whether `HEAD` does. `json`/`jsonl` keep every hit,
/// flat, with its commit's metadata alongside, for `jq` and friends.
pub fn render_history<T: Reportable>(
    format: Format,
    history: &History,
    hits: &[Hit<T>],
    stats: &ScanStats,
    w: &mut impl Write,
) -> Result<()> {
    let records = || hits.iter().map(|hit| HistoryRecord::new(history, hit));
    match format {
        Format::Json => {
            serde_json::to_writer_pretty(&mut *w, &records().collect::<Vec<_>>())?;
            writeln!(w)?;
            Ok(())
        }
        Format::Jsonl => {
            for record in records() {
                serde_json::to_writer(&mut *w, &record)?;
                writeln!(w)?;
            }
            Ok(())
        }
        Format::Pretty => {
            let rows = history::occurrences(history, hits);
            render_history_table(history, &rows, w)?;
            render_history_summary(history, &rows, hits, stats, w)
        }
        Format::Compact => {
            let rows = history::occurrences(history, hits);
            for row in &rows {
                let item = &row.first.item;
                let (first, last) = (history.commit_of(row.first), history.commit_of(row.last));
                writeln!(
                    w,
                    "{}:{}:{}: {} first={} {} last={} {} versions={} head={}",
                    item.file().display(),
                    item.line(),
                    item.column(),
                    label_cell(item.severity(), &severity_and_label(item)),
                    first.short(),
                    first.date,
                    last.short(),
                    last.date,
                    row.versions,
                    if row.at_head { "yes" } else { "no" },
                )?;
            }
            render_history_summary(history, &rows, hits, stats, w)
        }
        Format::Summary => {
            let rows = history::occurrences(history, hits);
            render_history_counts(&rows, w)?;
            render_history_summary(history, &rows, hits, stats, w)
        }
    }
}

/// One `--git-history` hit for `json`/`jsonl`: the finding or match itself
/// (with `file` relative to the cwd), plus where in history it was found.
#[derive(serde::Serialize)]
struct HistoryRecord<'a, T> {
    #[serde(flatten)]
    item: &'a T,
    /// Full id of the oldest commit containing this file version.
    commit: &'a str,
    commit_date: &'a str,
    author: &'a str,
    subject: &'a str,
    /// Object id of the file version (`git cat-file -p <blob>` shows it).
    blob: &'a str,
    /// Whether `HEAD` still contains this exact file version.
    at_head: bool,
}

impl<'a, T> HistoryRecord<'a, T> {
    fn new(history: &'a History, hit: &'a Hit<T>) -> Self {
        let blob = &history.blobs[hit.blob];
        let commit = history.commit_of(hit);
        Self {
            item: &hit.item,
            commit: &commit.id,
            commit_date: &commit.date,
            author: &commit.author,
            subject: &commit.subject,
            blob: &blob.id,
            at_head: blob.at_head,
        }
    }
}

/// One table per file, columns aligned across all of them, each row
/// followed by the matching source line (from the oldest version).
fn render_history_table<T: Reportable>(
    history: &History,
    rows: &[Occurrence<T>],
    w: &mut impl Write,
) -> Result<()> {
    const HEADERS: [&str; 8] = [
        "LINE",
        "MATCH",
        "FIRST SEEN",
        "AUTHOR",
        "LAST SEEN",
        "VERSIONS",
        "IN HEAD",
        "COMMIT",
    ];
    let cells: Vec<[String; 8]> = rows
        .iter()
        .map(|row| {
            let item = &row.first.item;
            let (first, last) = (history.commit_of(row.first), history.commit_of(row.last));
            [
                format!("{}:{}", item.line(), item.column()),
                item.label(),
                format!("{} {}", first.short(), first.date),
                truncate(&first.author, 20),
                format!("{} {}", last.short(), last.date),
                row.versions.to_string(),
                if row.at_head { "yes" } else { "no" }.to_string(),
                truncate(&first.subject, 50),
            ]
        })
        .collect();
    let mut widths = HEADERS.map(|h| h.chars().count());
    for row in &cells {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let pad = |text: &str, col: usize| format!("{text:<width$}", width = widths[col]);

    let mut current: Option<&Path> = None;
    for (row, cells) in rows.iter().zip(&cells) {
        let item = &row.first.item;
        if current != Some(item.file()) {
            if current.is_some() {
                writeln!(w)?;
            }
            writeln!(w, "{}", item.file().display().bold().underline())?;
            let header: Vec<String> = HEADERS.iter().enumerate().map(|(i, h)| pad(h, i)).collect();
            writeln!(w, "  {}", header.join("  ").trim_end().dimmed())?;
            current = Some(item.file());
        }
        let line = [
            pad(&cells[0], 0).dimmed().to_string(),
            label_cell(item.severity(), &pad(&cells[1], 1)),
            pad(&cells[2], 2),
            pad(&cells[3], 3),
            pad(&cells[4], 4),
            format!("{:>width$}", cells[5], width = widths[5]),
            if row.at_head {
                pad(&cells[6], 6).bold().to_string()
            } else {
                pad(&cells[6], 6).dimmed().to_string()
            },
            cells[7].dimmed().to_string(),
        ];
        writeln!(w, "  {}", line.join("  "))?;
        let indent = " ".repeat(widths[0]);
        writeln!(w, "  {indent}  {} {}", "│".dimmed(), item.snippet().trim())?;
    }
    Ok(())
}

/// Distinct occurrences per rule/capture, most frequent first.
fn render_history_counts<T: Reportable>(rows: &[Occurrence<T>], w: &mut impl Write) -> Result<()> {
    struct Count<'a> {
        label: String,
        severity: Option<Severity>,
        rows: usize,
        files: HashSet<&'a Path>,
        at_head: usize,
    }
    let mut counts: Vec<Count> = Vec::new();
    for row in rows {
        let item = &row.first.item;
        let label = item.label();
        let count = match counts.iter().position(|c| c.label == label) {
            Some(i) => &mut counts[i],
            None => {
                counts.push(Count {
                    label,
                    severity: item.severity(),
                    rows: 0,
                    files: HashSet::new(),
                    at_head: 0,
                });
                counts.last_mut().expect("just pushed")
            }
        };
        count.rows += 1;
        count.files.insert(item.file());
        count.at_head += usize::from(row.at_head);
    }
    counts.sort_by(|a, b| b.rows.cmp(&a.rows).then_with(|| a.label.cmp(&b.label)));
    writeln!(
        w,
        "{}",
        format!("{:>8}  {:>6}  {:>7}  MATCH", "DISTINCT", "FILES", "IN HEAD").dimmed()
    )?;
    for c in &counts {
        writeln!(
            w,
            "{:>8}  {:>6}  {:>7}  {}",
            c.rows,
            c.files.len(),
            c.at_head,
            label_cell(c.severity, &c.label),
        )?;
    }
    Ok(())
}

fn render_history_summary<T: Reportable>(
    history: &History,
    rows: &[Occurrence<T>],
    hits: &[Hit<T>],
    stats: &ScanStats,
    w: &mut impl Write,
) -> Result<()> {
    writeln!(w)?;
    let scanned = format!(
        "{} file version(s) introduced by {} commit(s)",
        stats.files_scanned,
        history.commits.len()
    );
    if rows.is_empty() {
        writeln!(w, "{}", format!("No matches. Scanned {scanned}.").green())?;
        return Ok(());
    }
    let files: HashSet<&Path> = rows.iter().map(|r| r.first.item.file()).collect();
    let at_head = rows.iter().filter(|r| r.at_head).count();
    writeln!(
        w,
        "{} {} distinct match(es) in {} file(s), {} still in HEAD. {} hit(s) across {} file version(s) (scanned {scanned}).",
        "Summary:".bold(),
        rows.len().to_string().bold(),
        files.len(),
        at_head.to_string().bold(),
        hits.len(),
        stats.files_with_findings,
    )?;
    Ok(())
}

fn severity_and_label<T: Reportable>(item: &T) -> String {
    match item.severity() {
        Some(severity) => format!("{} {}", severity.as_str(), item.label()),
        None => item.label(),
    }
}

/// `text` colored by a finding's severity, or bold for an ad-hoc match.
fn label_cell(severity: Option<Severity>, text: &str) -> String {
    match severity {
        Some(severity) => colorize(severity, text),
        None => text.bold().to_string(),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max - 1).collect();
        format!("{cut}…")
    }
}

/// One rule in the embedded catalog, flattened for export.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuleInfo {
    pub language: &'static str,
    pub query_file: String,
    pub rule_id: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Flatten the embedded ruleset into a list, skipping anonymous (helper-only)
/// patterns that have no reportable capture. A rule written as several
/// patterns (alternative shapes sharing one capture name) is listed once,
/// with the first pattern's metadata.
fn rule_catalog(ruleset: &Ruleset) -> Vec<RuleInfo> {
    let mut rules = Vec::new();
    for lang in ruleset.languages() {
        let mut seen = HashSet::new();
        for query in &lang.queries {
            for pattern in query.patterns() {
                let Some(rule_id) = pattern.rule_id.clone() else {
                    continue;
                };
                if pattern.replaced || !seen.insert(rule_id.clone()) {
                    continue;
                }
                rules.push(RuleInfo {
                    language: lang.spec.name,
                    query_file: query.source_path.clone(),
                    rule_id,
                    severity: pattern.severity,
                    description: pattern.description.clone(),
                    category: pattern.category.clone(),
                });
            }
        }
    }
    rules
}

/// Print the embedded rule catalog in the requested format.
pub fn list_rules(format: Format, ruleset: &Ruleset, w: &mut impl Write) -> Result<()> {
    match format {
        Format::Pretty | Format::Compact | Format::Summary => list_rules_pretty(ruleset, w),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *w, &rule_catalog(ruleset))?;
            writeln!(w)?;
            Ok(())
        }
        Format::Jsonl => {
            for rule in rule_catalog(ruleset) {
                serde_json::to_writer(&mut *w, &rule)?;
                writeln!(w)?;
            }
            Ok(())
        }
    }
}

fn list_rules_pretty(ruleset: &Ruleset, w: &mut impl Write) -> Result<()> {
    for lang in ruleset.languages() {
        writeln!(
            w,
            "{} ({})",
            lang.spec.name.bold(),
            lang.spec.extensions.join(", ").dimmed()
        )?;

        // Group by category, preserving first-seen order, rather than by
        // query file -- a language's rules now live in one file, with
        // `category` doing the classification a file split used to.
        let mut groups: Vec<(Option<&str>, Vec<&crate::rules::PatternMeta>)> = Vec::new();
        let mut seen = HashSet::new();
        for query in &lang.queries {
            for pattern in query.patterns().iter().filter(|p| !p.replaced) {
                if let Some(rule_id) = pattern.rule_id.as_deref() {
                    if !seen.insert(rule_id) {
                        continue;
                    }
                }
                let category = pattern.category.as_deref();
                match groups.iter_mut().find(|(c, _)| *c == category) {
                    Some((_, patterns)) => patterns.push(pattern),
                    None => groups.push((category, vec![pattern])),
                }
            }
        }

        for (category, patterns) in groups {
            writeln!(w, "  {}", category.unwrap_or("(uncategorized)").dimmed())?;
            for pattern in patterns {
                let rule = pattern.rule_id.as_deref().unwrap_or("<anonymous>");
                let description = pattern
                    .description
                    .as_deref()
                    .map(|d| format!("  — {d}"))
                    .unwrap_or_default();
                writeln!(
                    w,
                    "    {badge}  {rule}{description}",
                    badge = severity_badge(pattern.severity),
                    rule = rule.bold(),
                )?;
            }
        }
        writeln!(w)?;
    }
    Ok(())
}

fn caret_width(
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    snippet: &str,
) -> usize {
    if end_line == line {
        end_column.saturating_sub(column).max(1)
    } else {
        // Multi-line node: underline to the end of the first line.
        snippet
            .len()
            .saturating_sub(column.saturating_sub(1))
            .max(1)
    }
}

fn severity_badge(severity: Severity) -> String {
    let label = format!("{:<7}", severity.as_str());
    colorize(severity, &label)
}

fn colorize(severity: Severity, text: &str) -> String {
    match severity {
        Severity::Error => text.red().bold().to_string(),
        Severity::Warning => text.yellow().bold().to_string(),
        Severity::Log => text.cyan().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample() -> Finding {
        Finding {
            file: PathBuf::from("a.py"),
            language: "python",
            rule_id: "forbidden_print".to_string(),
            severity: Severity::Warning,
            description: None,
            category: None,
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 14,
            query_file: "rules-py.scm".to_string(),
            snippet: "print(\"hi\")".to_string(),
        }
    }

    #[test]
    fn json_roundtrips_key_fields() {
        let findings = vec![sample()];
        let mut buf = Vec::new();
        render(Format::Json, &findings, &ScanStats::default(), &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("\"rule_id\": \"forbidden_print\""));
        assert!(text.contains("\"severity\": \"warning\""));
    }

    #[test]
    fn compact_is_one_line_per_finding_plus_summary() {
        let findings = vec![sample(), sample()];
        let mut buf = Vec::new();
        render(Format::Compact, &findings, &ScanStats::default(), &mut buf).unwrap();
        let text = strip(&buf);
        assert!(
            text.starts_with("a.py:1:1: warning forbidden_print\n"),
            "{text}"
        );
        assert!(text.contains("Summary: 0 error(s), 2 warning(s)"), "{text}");
    }

    #[test]
    fn summary_counts_findings_and_files_per_rule() {
        let mut other = sample();
        other.file = PathBuf::from("b.py");
        let mut eval = sample();
        eval.rule_id = "eval_exec_usage".to_string();
        eval.severity = Severity::Error;
        let findings = vec![sample(), sample(), other, eval];
        let mut buf = Vec::new();
        render(Format::Summary, &findings, &ScanStats::default(), &mut buf).unwrap();
        let text = strip(&buf);
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines[1]
                .split_whitespace()
                .eq(["3", "2", "warning", "forbidden_print"]),
            "{text}"
        );
        assert!(
            lines[2]
                .split_whitespace()
                .eq(["1", "1", "error", "eval_exec_usage"]),
            "{text}"
        );
    }

    fn strip(buf: &[u8]) -> String {
        let mut out = Vec::new();
        let mut stream = anstream::StripStream::new(&mut out);
        stream.write_all(buf).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn jsonl_emits_one_line_per_finding() {
        let findings = vec![sample(), sample()];
        let mut buf = Vec::new();
        render(Format::Jsonl, &findings, &ScanStats::default(), &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert_eq!(text.lines().count(), 2);
    }
}
