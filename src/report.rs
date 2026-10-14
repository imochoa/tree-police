//! Output rendering: a ripgrep-style pretty report and machine-readable
//! JSON / JSONL. Findings are written to the provided writer (stdout); the
//! caller wraps it in an `anstream` stream so ANSI colors are stripped when the
//! output is not a terminal or `NO_COLOR` is set.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use owo_colors::OwoColorize;

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
