//! `tree-police` — scan source files for forbidden AST patterns using baked-in
//! tree-sitter queries, each tagged with a severity (error / warning / log).

use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::{ArgAction, Parser};
use tracing_subscriber::EnvFilter;

use tree_police::report::{self, Format};
use tree_police::rules;
use tree_police::scan::{self, ScanOptions};
use tree_police::severity::Severity;

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
    #[arg(long)]
    list_rules: bool,

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
