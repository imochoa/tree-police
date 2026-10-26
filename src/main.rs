//! `tree-police` — scan source files for forbidden AST patterns using baked-in
//! tree-sitter queries, each tagged with a severity (error / warning / log).

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Parser};
use owo_colors::OwoColorize;
use regex::bytes::Regex;
use tracing_subscriber::EnvFilter;
use tree_sitter::{Parser as TsParser, Query};

use tree_police::baseline::Baseline;
use tree_police::casetest;
use tree_police::config::{Catalog, Config};
use tree_police::history::History;
use tree_police::registry;
use tree_police::report::{self, Format};
use tree_police::rules;
use tree_police::scan::{self, Finding, ScanOptions};
use tree_police::severity::Severity;
use tree_police::tree_view;

/// Severity threshold that makes the process exit non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "lowercase")]
enum FailOn {
    /// Never fail on findings (exit 0 unless an internal error occurs).
    None,
    Log,
    Warning,
    Error,
}

#[derive(Debug, Parser)]
#[command(
    name = "tree-police",
    version,
    // A later flag wins over an earlier one, so pre-commit `args` can
    // override the hook entry's own `--format`/`--fail-on`.
    args_override_self = true,
    about = "Ripgrep-style AST scanner with baked-in tree-sitter queries and severity levels.",
    long_about = "Scans .py / .tf (and other registered) files against tree-sitter queries \
compiled into the binary. Each rule declares a severity (error / warning / log) via a \
`(#set! severity ...)` directive in its .scm file.\n\nExit codes: 0 = no findings at or \
above --fail-on; 1 = findings at or above the threshold; 2 = an internal error."
)]
struct Args {
    /// Files or directories to scan.
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t = Format::Pretty)]
    format: Format,

    /// Only report findings at or above this severity.
    #[arg(long, value_enum)]
    min_severity: Option<Severity>,

    /// Only report findings from this rule ID (repeatable). See --list-rules
    /// for valid IDs.
    #[arg(long = "rule")]
    rules: Vec<String>,

    /// Only report findings tagged with this category (repeatable). See
    /// --list-rules for valid categories.
    #[arg(long = "category")]
    categories: Vec<String>,

    /// Exit non-zero when a finding at or above this severity is present.
    #[arg(long, value_enum, default_value_t = FailOn::Warning)]
    fail_on: FailOn,

    /// List the embedded rules (with severities) and exit.
    #[arg(long, conflicts_with_all = ["query", "query_file"])]
    list_rules: bool,

    /// Print an indented AST for FILE and exit (language inferred from its
    /// extension, same registry as scanning). Useful for discovering node
    /// names while writing a query -- see docs/writing-queries.md.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["query", "query_file", "list_rules"])]
    show_tree: Option<PathBuf>,

    /// Ad-hoc query string to run against `paths` instead of the embedded
    /// ruleset, reporting every non-`_` capture (not one rule-id capture
    /// per pattern like the embedded ruleset) -- see docs/writing-queries.md.
    #[arg(long, conflicts_with_all = ["query_file", "show_tree", "list_rules"])]
    query: Option<String>,

    /// Same as --query, read from a file instead of the command line.
    #[arg(long, conflicts_with_all = ["query", "show_tree", "list_rules"])]
    query_file: Option<PathBuf>,

    /// Plain regex to search for, line by line like ripgrep, instead of
    /// running tree-sitter queries. Searches every text file whatever its
    /// extension, so no grammar or --lang is involved. Prefix the pattern
    /// with `(?i)` for a case-insensitive search. Like --query, a search
    /// rather than a lint gate: always exits 0.
    #[arg(
        long,
        value_name = "REGEX",
        conflicts_with_all = ["query", "query_file", "show_tree", "list_rules", "test", "update_baseline", "lang"]
    )]
    grep: Option<String>,

    /// Scan every file version in the git history instead of the working
    /// tree: all refs by default, or the given revisions (space-separated,
    /// e.g. `main` or `v0.2.0..HEAD`). `paths` become git pathspecs. Each
    /// distinct file version is scanned once and reported as
    /// `<commit>:<path>` at the oldest commit that introduced it, which
    /// `git show` accepts as-is. Works with the ruleset, --query and --grep.
    #[arg(
        long,
        value_name = "REVS",
        num_args = 0..=1,
        default_missing_value = "",
        conflicts_with_all = ["show_tree", "list_rules", "test", "update_baseline", "baseline"]
    )]
    git_history: Option<String>,

    /// Language for --query/--query-file. Inferred automatically when
    /// `paths` resolve to files of exactly one registered language;
    /// required if that's ambiguous (e.g. a mixed-language directory).
    #[arg(long)]
    lang: Option<String>,

    /// Do not respect .gitignore / .ignore files.
    #[arg(long)]
    no_ignore: bool,

    /// Include hidden files and directories.
    #[arg(long)]
    hidden: bool,

    /// Number of scanning threads (default: available parallelism).
    #[arg(short = 'j', long)]
    threads: Option<usize>,

    /// Directory of repo-local `<label>-<code>.scm` rule files, merged with
    /// the embedded ruleset. Not an error if it doesn't exist.
    #[arg(long, default_value = ".tree-police")]
    rules_dir: PathBuf,

    /// Config file scoping rules to paths and overriding severities (see
    /// README "Config file"). Defaults to `<rules-dir>/config.toml` if that
    /// exists; globs there are relative to the rules dir's parent (the repo
    /// root for `.tree-police/`), and to this file's own directory when it
    /// is passed explicitly.
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Baseline file of known findings to hide (see README "Baseline").
    /// Defaults to `<rules-dir>/baseline.json` if that exists.
    #[arg(long, value_name = "FILE", conflicts_with = "no_baseline")]
    baseline: Option<PathBuf>,

    /// Ignore any baseline file and report every finding.
    #[arg(long)]
    no_baseline: bool,

    /// Write the current findings to the baseline file (default
    /// `<rules-dir>/baseline.json`) instead of reporting them, and exit 0.
    /// Entries for files this run didn't scan are kept.
    #[arg(long, conflicts_with_all = ["no_baseline", "query", "query_file", "list_rules", "test"])]
    update_baseline: bool,

    /// Check annotated rule fixtures (`# <- @rule_id` under the line that
    /// should match) in DIR, default `<rules-dir>/tests`, and exit 1 on any
    /// failure. See README "Testing repo-local rules".
    #[arg(
        long,
        value_name = "DIR",
        num_args = 0..=1,
        conflicts_with_all = ["query", "query_file", "list_rules", "show_tree"]
    )]
    test: Option<Option<PathBuf>>,

    /// Increase logging verbosity (-v = info, -vv = debug, -vvv = trace).
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,
}

fn main() {
    let args = Args::parse();
    if let Err(err) = init_tracing(args.verbose) {
        eprintln!("error: {err}");
        std::process::exit(2);
    }
    match run(args) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            tracing::error!("{err:#}");
            std::process::exit(2);
        }
    }
}

fn run(args: Args) -> Result<i32> {
    if let Some(threads) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .map_err(|err| anyhow!("could not configure thread pool: {err}"))?;
    }

    if let Some(path) = &args.show_tree {
        show_tree(path)?;
        return Ok(0);
    }

    // `--git-history`'s blobs, listed once up front for whichever mode runs.
    let history = match &args.git_history {
        Some(revs) => {
            let revs: Vec<String> = revs.split_whitespace().map(String::from).collect();
            Some(History::load(&revs, &args.paths)?)
        }
        None => None,
    };
    let walk_opts = ScanOptions {
        respect_gitignore: !args.no_ignore,
        hidden: args.hidden,
        skip_dirs: Vec::new(),
    };

    if let Some(pattern) = &args.grep {
        let regex =
            Regex::new(pattern).with_context(|| format!("invalid --grep regex {pattern:?}"))?;
        let mut out = anstream::stdout();
        match &history {
            Some(history) => {
                let (hits, stats) = history.grep(&regex)?;
                report::render_history(args.format, history, &hits, &stats, &mut out)?;
            }
            None => {
                let (matches, stats) = scan::grep(&args.paths, &regex, &walk_opts)?;
                report::render_matches(args.format, &matches, &stats, &mut out)?;
            }
        }
        out.flush().ok();
        return Ok(0);
    }

    let ad_hoc_query_source = match (&args.query, &args.query_file) {
        (Some(q), _) => Some(q.clone()),
        (None, Some(path)) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("could not read query file {}", path.display()))?,
        ),
        (None, None) => None,
    };
    if let Some(source) = ad_hoc_query_source {
        return run_ad_hoc_query(
            &source,
            args.lang.as_deref(),
            &args.paths,
            history.as_ref(),
            args.format,
            &walk_opts,
        );
    }

    let ruleset = rules::load_with_extra(Some(&args.rules_dir))?;
    let mut out = anstream::stdout();

    if args.list_rules {
        report::list_rules(args.format, &ruleset, &mut out)?;
        out.flush().ok();
        return Ok(0);
    }

    if let Some(dir) = &args.test {
        let dir = dir.clone().unwrap_or_else(|| args.rules_dir.join("tests"));
        return run_fixture_tests(&dir, &ruleset, &walk_opts, &mut out);
    }

    let opts = ScanOptions {
        respect_gitignore: !args.no_ignore,
        hidden: args.hidden,
        skip_dirs: vec![std::path::absolute(args.rules_dir.join("tests"))?],
    };
    let catalog = Catalog::from_ruleset(&ruleset);
    let config = match &args.config {
        Some(path) => Config::load_explicit(path, &catalog)?,
        None => Config::load_default(&args.rules_dir, &catalog)?,
    };
    let keep = |f: &Finding| {
        args.min_severity.is_none_or(|min| f.severity >= min)
            && (args.rules.is_empty() || args.rules.iter().any(|r| r == &f.rule_id))
            && (args.categories.is_empty()
                || f.category
                    .as_deref()
                    .is_some_and(|c| args.categories.iter().any(|arg| arg == c)))
    };

    if let Some(history) = &history {
        // No baseline here: it records working-tree paths, which history
        // findings never match.
        let (mut hits, stats) = history.scan(&ruleset, &config)?;
        hits.retain(|hit| keep(&hit.item));
        report::render_history(args.format, history, &hits, &stats, &mut out)?;
        out.flush().ok();
        return Ok(exit_code(args.fail_on, hits.iter().map(|hit| &hit.item)));
    }

    let (mut findings, mut stats) = scan::scan(&args.paths, &ruleset, &config, &opts)?;

    if args.update_baseline {
        let (path, root) = baseline_location(&args)?;
        let existing = if path.is_file() {
            Baseline::load(path, root)?
        } else {
            Baseline::empty(path, root)
        };
        let shown = existing.path().display().to_string();
        let total = existing.update(&stats.files, &findings)?;
        writeln!(out, "Wrote {total} finding(s) to {shown}.")?;
        out.flush().ok();
        return Ok(0);
    }
    if !args.no_baseline {
        let (path, root) = baseline_location(&args)?;
        if args.baseline.is_some() || path.is_file() {
            stats.baselined = Baseline::load(path, root)?.apply(&mut findings);
        }
    }

    findings.retain(keep);

    report::render(args.format, &findings, &stats, &mut out)?;
    out.flush().ok();
    Ok(exit_code(args.fail_on, &findings))
}

/// 1 if any finding is at or above `--fail-on`, else 0.
fn exit_code<'a>(fail_on: FailOn, findings: impl IntoIterator<Item = &'a Finding>) -> i32 {
    match fail_threshold(fail_on) {
        Some(threshold) if findings.into_iter().any(|f| f.severity >= threshold) => 1,
        _ => 0,
    }
}

/// The baseline file and the root its paths are relative to: an explicit
/// `--baseline FILE` is relative to its own directory, the default
/// `<rules-dir>/baseline.json` to the rules dir's parent (the repo root).
fn baseline_location(args: &Args) -> Result<(PathBuf, PathBuf)> {
    let (path, anchor) = match &args.baseline {
        Some(path) => (path.clone(), std::path::absolute(path)?),
        None => (
            args.rules_dir.join("baseline.json"),
            std::path::absolute(&args.rules_dir)?,
        ),
    };
    let root = anchor.parent().unwrap_or(&anchor).to_path_buf();
    Ok((path, root))
}

/// `--test`: run the full ruleset over annotated fixtures under `dir` and
/// check the `<- @rule` assertions (see `tree_police::casetest`). Strays are
/// checked for repo-local rules and for any rule a fixture asserts.
fn run_fixture_tests(
    dir: &Path,
    ruleset: &rules::Ruleset,
    opts: &ScanOptions,
    out: &mut impl Write,
) -> Result<i32> {
    if !dir.is_dir() {
        return Err(anyhow!(
            "fixture directory {} does not exist",
            dir.display()
        ));
    }
    let (findings, stats) = scan::scan(&[dir.to_path_buf()], ruleset, &Config::default(), opts)?;
    if stats.files.is_empty() {
        return Err(anyhow!(
            "no fixtures with a registered language extension under {}",
            dir.display()
        ));
    }
    let local: HashSet<&str> = ruleset
        .languages()
        .iter()
        .flat_map(|l| l.queries.iter().filter(|q| !q.embedded))
        .flat_map(|q| q.patterns().iter().filter_map(|p| p.rule_id.as_deref()))
        .collect();

    let mut assertion_count = 0;
    let mut failure_count = 0;
    for file in &stats.files {
        let source = std::fs::read_to_string(file)
            .with_context(|| format!("could not read {}", file.display()))?;
        let assertions = casetest::parse_assertions(&source);
        assertion_count += assertions.len();
        let hits: Vec<casetest::Hit> = findings
            .iter()
            .filter(|f| &f.file == file)
            .map(|f| casetest::Hit {
                rule_id: f.rule_id.clone(),
                start_row: f.line - 1,
                end_row: f.end_line - 1,
            })
            .collect();
        for (line, message) in casetest::check(&assertions, &hits, |r| local.contains(r)) {
            failure_count += 1;
            writeln!(out, "{}:{line}: {message}", file.display())?;
        }
    }

    let summary = format!(
        "{} fixture(s), {assertion_count} assertion(s), {failure_count} failure(s).",
        stats.files.len()
    );
    if failure_count == 0 {
        writeln!(out, "{}", summary.green())?;
        Ok(0)
    } else {
        writeln!(out, "{}", summary.red().bold())?;
        Ok(1)
    }
}

fn show_tree(path: &Path) -> Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).ok_or_else(|| {
        anyhow!(
            "{}: no file extension, can't infer a language",
            path.display()
        )
    })?;
    let spec = registry::for_extension(ext).ok_or_else(|| {
        anyhow!(
            "{}: no grammar registered for extension \".{ext}\" (see --list-rules or src/registry.rs for supported languages)",
            path.display()
        )
    })?;

    let source =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;

    let mut parser = TsParser::new();
    parser
        .set_language(&spec.language())
        .map_err(|err| anyhow!("could not load grammar for {}: {err}", spec.name))?;
    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("{}: failed to parse", path.display()))?;

    let mut out = anstream::stdout();
    tree_view::print_tree(&tree, &source, &mut out)?;
    out.flush().ok();
    Ok(())
}

/// Run an ad-hoc `--query`/`--query-file` and print every non-`_` capture
/// (tree-grepper's model, see docs/writing-queries.md). Unlike the embedded
/// ruleset, this is a search, not a lint gate: always exits 0 regardless of
/// match count, and ignores --fail-on/--min-severity/--rule/--category,
/// none of which have meaning without a severity/rule-id/category.
fn run_ad_hoc_query(
    source: &str,
    lang_name: Option<&str>,
    paths: &[PathBuf],
    history: Option<&History>,
    format: Format,
    opts: &ScanOptions,
) -> Result<i32> {
    let spec = match (lang_name, history) {
        (Some(name), _) => registry::by_name(name)
            .ok_or_else(|| anyhow!("unknown language {name:?} (see --list-rules for the ones with embedded rules, or src/registry.rs for every registered language)"))?,
        (None, Some(history)) => history.infer_language()?,
        (None, None) => scan::infer_language(paths, opts)?,
    };

    rules::check_top_level_predicates(source)?;
    let query = Query::new(&spec.language(), source)
        .with_context(|| format!("failed to compile query for language {}", spec.name))?;

    let mut out = anstream::stdout();
    match history {
        Some(history) => {
            let (hits, stats) = history.run_query(spec, &query)?;
            report::render_history(format, history, &hits, &stats, &mut out)?;
        }
        None => {
            let (matches, stats) = scan::run_query(paths, spec, &query, opts)?;
            report::render_matches(format, &matches, &stats, &mut out)?;
        }
    }
    out.flush().ok();
    Ok(0)
}

fn fail_threshold(fail_on: FailOn) -> Option<Severity> {
    match fail_on {
        FailOn::None => None,
        FailOn::Log => Some(Severity::Log),
        FailOn::Warning => Some(Severity::Warning),
        FailOn::Error => Some(Severity::Error),
    }
}

fn init_tracing(verbose: u8) -> Result<()> {
    let fallback = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(fallback));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|err| anyhow!("could not initialize diagnostics: {err}"))
}
