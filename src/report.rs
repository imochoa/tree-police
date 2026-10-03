//! Output rendering: a ripgrep-style pretty report and machine-readable
//! JSON / JSONL. Findings are written to the provided writer (stdout); the
//! caller wraps it in an `anstream` stream so ANSI colors are stripped when the
//! output is not a terminal or `NO_COLOR` is set.

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::rules::Ruleset;
use crate::scan::{Finding, ScanStats};
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
        let width = caret_width(finding);
        let caret = "^".repeat(width);
        writeln!(w, "  {indent}{pad}{}", colorize(finding.severity, &caret))?;
    }

    render_summary(findings, stats, w)
}

fn render_summary(findings: &[Finding], stats: &ScanStats, w: &mut impl Write) -> Result<()> {
    writeln!(w)?;
    if findings.is_empty() {
        writeln!(
            w,
            "{}",
            format!("No findings. Scanned {} file(s).", stats.files_scanned).green()
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
        "{} {} error(s), {} warning(s), {} log(s) across {} file(s) (scanned {}).",
        "Summary:".bold(),
        errors.to_string().red().bold(),
        warnings.to_string().yellow().bold(),
        logs.to_string().cyan().bold(),
        stats.files_with_findings,
        stats.files_scanned,
    )?;
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
/// patterns that have no reportable capture.
fn rule_catalog(ruleset: &Ruleset) -> Vec<RuleInfo> {
    let mut rules = Vec::new();
    for lang in ruleset.languages() {
        for query in &lang.queries {
            for pattern in query.patterns() {
                let Some(rule_id) = pattern.rule_id.clone() else {
                    continue;
                };
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
        Format::Pretty => list_rules_pretty(ruleset, w),
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
        for query in &lang.queries {
            for pattern in query.patterns() {
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

fn caret_width(finding: &Finding) -> usize {
    if finding.end_line == finding.line {
        finding.end_column.saturating_sub(finding.column).max(1)
    } else {
        // Multi-line node: underline to the end of the first line.
        finding
            .snippet
            .len()
            .saturating_sub(finding.column.saturating_sub(1))
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
    fn jsonl_emits_one_line_per_finding() {
        let findings = vec![sample(), sample()];
        let mut buf = Vec::new();
        render(Format::Jsonl, &findings, &ScanStats::default(), &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert_eq!(text.lines().count(), 2);
    }
}
