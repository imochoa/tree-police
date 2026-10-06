//! `tree-police` — scan source files for forbidden AST patterns using baked-in
//! tree-sitter queries, each tagged with a severity (error / warning / log).

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Parser};
use tracing_subscriber::EnvFilter;
use tree_sitter::{Parser as TsParser, Query};

use tree_police::registry;
use tree_police::report::{self, Format};
use tree_police::rules;
use tree_police::scan::{self, ScanOptions};
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

    let ad_hoc_query_source = match (&args.query, &args.query_file) {
        (Some(q), _) => Some(q.clone()),
        (None, Some(path)) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("could not read query file {}", path.display()))?,
        ),
        (None, None) => None,
    };
    if let Some(source) = ad_hoc_query_source {
        let opts = ScanOptions {
            respect_gitignore: !args.no_ignore,
            hidden: args.hidden,
        };
        return run_ad_hoc_query(
            &source,
            args.lang.as_deref(),
            &args.paths,
            args.format,
            &opts,
        );
    }

    let ruleset = rules::load_with_extra(Some(&args.rules_dir))?;
    let mut out = anstream::stdout();

    if args.list_rules {
        report::list_rules(args.format, &ruleset, &mut out)?;
        out.flush().ok();
        return Ok(0);
    }

    let opts = ScanOptions {
        respect_gitignore: !args.no_ignore,
        hidden: args.hidden,
    };
    let (mut findings, stats) = scan::scan(&args.paths, &ruleset, &opts)?;

    if let Some(min) = args.min_severity {
        findings.retain(|f| f.severity >= min);
    }
    if !args.rules.is_empty() {
        findings.retain(|f| args.rules.iter().any(|r| r == &f.rule_id));
    }
    if !args.categories.is_empty() {
        findings.retain(|f| {
            f.category
                .as_deref()
                .is_some_and(|c| args.categories.iter().any(|arg| arg == c))
        });
    }

    report::render(args.format, &findings, &stats, &mut out)?;
    out.flush().ok();

    let exit_code = match fail_threshold(args.fail_on) {
        Some(threshold) if findings.iter().any(|f| f.severity >= threshold) => 1,
        _ => 0,
    };
    Ok(exit_code)
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
    format: Format,
    opts: &ScanOptions,
) -> Result<i32> {
    let spec = match lang_name {
        Some(name) => registry::by_name(name)
            .ok_or_else(|| anyhow!("unknown language {name:?} (see --list-rules for the ones with embedded rules, or src/registry.rs for every registered language)"))?,
        None => scan::infer_language(paths, opts)?,
    };

    let query = Query::new(&spec.language(), source)
        .with_context(|| format!("failed to compile query for language {}", spec.name))?;

    let (matches, stats) = scan::run_query(paths, spec, &query, opts)?;

    let mut out = anstream::stdout();
    report::render_matches(format, &matches, &stats, &mut out)?;
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
