//! `--git-history`: run a scan over every file version in a git
//! repository's history rather than the working tree.
//!
//! Each distinct blob is read and parsed exactly once, however many commits
//! contain it, and is reported at the oldest commit that introduced it. A
//! file that never changed across 10k commits costs one parse, not 10k.
//! Everything goes through the `git` binary (`git log --raw` to list the
//! blobs, one `git cat-file --batch` to stream them), so there's no libgit2
//! dependency and anything `git` can read works, shallow clones included.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{anyhow, bail, Context, Result};
use rayon::iter::{ParallelBridge, ParallelIterator};
use regex::bytes::Regex;
use tree_sitter::{Parser, Query};

use crate::config::{Catalog, Config};
use crate::registry::{self, LangSpec};
use crate::rules::Ruleset;
use crate::scan::{self, Finding, Match, ScanStats};

/// One distinct file version, at the oldest commit that introduced it.
#[derive(Debug, Clone)]
pub struct Blob {
    /// Object id of the file contents.
    pub id: String,
    /// Full id of the oldest commit that added this content.
    pub commit: String,
    /// Absolute path the file had in that commit (what config scoping and
    /// the language lookup see).
    pub path: PathBuf,
    /// `<short commit>:<path relative to the cwd>` -- the reported `file`,
    /// which `git show` accepts as-is.
    pub display: PathBuf,
}

impl Blob {
    fn language(&self) -> Option<&'static LangSpec> {
        let ext = self.path.extension()?.to_str()?;
        registry::for_extension(ext)
    }
}

/// List every distinct blob reachable from `revs` (all refs when empty),
/// limited to `pathspecs` (interpreted by git, relative to the cwd), oldest
/// commit first.
pub fn list_blobs(revs: &[String], pathspecs: &[PathBuf]) -> Result<Vec<Blob>> {
    let toplevel = git_output(&["rev-parse", "--show-toplevel"])
        .context("--git-history must run inside a git repository")?;
    let toplevel = PathBuf::from(String::from_utf8_lossy(&toplevel).trim());
    let cwd = std::env::current_dir()?;

    let mut cmd = Command::new("git");
    // --topo-order with --reverse visits parents before children, so a
    // blob's first sighting is its oldest commit. Merges diff against their
    // first parent only: content coming in from the other side was already
    // seen on that side's own commits, so only conflict resolutions are new.
    cmd.args([
        "log",
        "--reverse",
        "--topo-order",
        "--full-history",
        "--diff-merges=first-parent",
        "--raw",
        "--no-abbrev",
        "--no-renames",
        "-z",
        "--format=%x01%H",
    ]);
    if revs.is_empty() {
        cmd.arg("--all");
    } else {
        cmd.args(revs);
    }
    cmd.arg("--").args(pathspecs);
    let raw = run(&mut cmd)?;

    let mut blobs = Vec::new();
    let mut seen = HashSet::new();
    let mut commit = String::new();
    let mut fields = raw.split(|&b| b == 0).map(|f| f.trim_ascii_start());
    while let Some(field) = fields.next() {
        if let Some(id) = field.strip_prefix(b"\x01") {
            commit = String::from_utf8_lossy(id).into_owned();
            continue;
        }
        // `:<old mode> <new mode> <old id> <new id> <status>`, then the path.
        let Some(header) = field.strip_prefix(b":") else {
            continue;
        };
        let path = fields.next().unwrap_or_default();
        let header = String::from_utf8_lossy(header);
        let parts: Vec<&str> = header.split(' ').collect();
        let [_, new_mode, _, new_id, _status] = parts[..] else {
            tracing::warn!("unexpected `git log --raw` line: {header}");
            continue;
        };
        // Regular files only: deletions (mode 000000), symlinks (120000) and
        // submodule commits (160000) have no source to scan.
        if !matches!(new_mode, "100644" | "100755") || !seen.insert(new_id.to_string()) {
            continue;
        }
        let path = toplevel.join(String::from_utf8_lossy(path).as_ref());
        let relative = path.strip_prefix(&cwd).unwrap_or(&path);
        let short = &commit[..commit.len().min(12)];
        blobs.push(Blob {
            id: new_id.to_string(),
            commit: commit.clone(),
            display: PathBuf::from(format!("{short}:{}", relative.display())),
            path,
        });
    }
    Ok(blobs)
}

/// History counterpart of [`scan::scan`]: the ruleset over every blob of a
/// language it has rules for. Findings come out oldest commit first.
pub fn scan(
    blobs: &[Blob],
    ruleset: &Ruleset,
    config: &Config,
) -> Result<(Vec<Finding>, ScanStats)> {
    let catalog = Catalog::from_ruleset(ruleset);
    let selected: Vec<(&Blob, &'static LangSpec)> = blobs
        .iter()
        .filter(|b| !config.excludes(&b.path))
        .filter_map(|b| Some((b, b.language()?)))
        .filter(|(_, spec)| ruleset.language(spec.name).is_some())
        .collect();

    let per_blob = for_each_blob(&selected, |parser, &(blob, spec), source| {
        let (mut findings, suppressed) =
            scan::scan_source(parser, &blob.path, spec, source, ruleset, config, &catalog);
        for f in &mut findings {
            f.file = blob.display.clone();
        }
        (findings, suppressed)
    })?;

    let mut findings = Vec::new();
    let mut stats = stats_for(&selected, |(b, _)| b);
    for (file_findings, suppressed) in per_blob {
        stats.suppressed += suppressed;
        stats.files_with_findings += usize::from(!file_findings.is_empty());
        findings.extend(file_findings);
    }
    Ok((findings, stats))
}

/// History counterpart of [`scan::run_query`].
pub fn run_query(
    blobs: &[Blob],
    spec: &'static LangSpec,
    query: &Query,
) -> Result<(Vec<Match>, ScanStats)> {
    let selected: Vec<&Blob> = blobs
        .iter()
        .filter(|b| b.language().is_some_and(|s| s.name == spec.name))
        .collect();
    let per_blob = for_each_blob(&selected, |parser, blob, source| {
        relabel(
            scan::query_source(parser, &blob.path, spec, source, query),
            blob,
        )
    })?;
    Ok(collect_matches(per_blob, stats_for(&selected, |b| b)))
}

/// History counterpart of [`scan::grep`]: every blob, any extension.
pub fn grep(blobs: &[Blob], regex: &Regex) -> Result<(Vec<Match>, ScanStats)> {
    let selected: Vec<&Blob> = blobs.iter().collect();
    let per_blob = for_each_blob(&selected, |_, blob, source| {
        relabel(scan::grep_source(&blob.path, source, regex), blob)
    })?;
    Ok(collect_matches(per_blob, stats_for(&selected, |b| b)))
}

/// [`scan::infer_language`] for history: the single registered language
/// among the blobs, or an error asking for `--lang`.
pub fn infer_language(blobs: &[Blob]) -> Result<&'static LangSpec> {
    let mut found: Vec<&'static LangSpec> = Vec::new();
    for spec in blobs.iter().filter_map(Blob::language) {
        if !found.iter().any(|s| s.name == spec.name) {
            found.push(spec);
        }
    }
    match found[..] {
        [] => bail!(
            "no file with a registered language extension in the history; pass --lang explicitly"
        ),
        [spec] => Ok(spec),
        _ => {
            let names: Vec<_> = found.iter().map(|s| s.name).collect();
            bail!(
                "multiple languages found in the history ({}); pass --lang to pick one",
                names.join(", ")
            )
        }
    }
}

fn relabel(mut matches: Vec<Match>, blob: &Blob) -> Vec<Match> {
    for m in &mut matches {
        m.file = blob.display.clone();
    }
    matches
}

fn collect_matches(per_blob: Vec<Vec<Match>>, mut stats: ScanStats) -> (Vec<Match>, ScanStats) {
    stats.files_with_findings = per_blob.iter().filter(|m| !m.is_empty()).count();
    (per_blob.into_iter().flatten().collect(), stats)
}

fn stats_for<T>(selected: &[T], blob: impl Fn(&T) -> &Blob) -> ScanStats {
    ScanStats {
        files_scanned: selected.len(),
        files: selected.iter().map(|s| blob(s).display.clone()).collect(),
        ..ScanStats::default()
    }
}

/// Stream the contents of `items`' blobs through one `git cat-file --batch`
/// and run `f` on each in parallel (one tree-sitter parser per thread).
/// Results come back in `items` order, i.e. oldest commit first.
fn for_each_blob<T, R, F>(items: &[T], f: F) -> Result<Vec<R>>
where
    T: Sync + AsRef<Blob>,
    R: Send,
    F: Fn(&mut Parser, &T, &[u8]) -> R + Sync,
{
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("could not run `git cat-file`")?;

    // Feed ids from a separate thread so neither pipe can fill up and stall.
    let mut stdin = child.stdin.take().expect("piped stdin");
    let ids: Vec<String> = items.iter().map(|i| i.as_ref().id.clone()).collect();
    let writer = thread::spawn(move || -> std::io::Result<()> {
        for id in ids {
            writeln!(stdin, "{id}")?;
        }
        Ok(())
    });

    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout"));
    let contents = items
        .iter()
        .enumerate()
        .map(move |(index, item)| (index, item, read_object(&mut reader, &item.as_ref().id)));
    let results: Result<Vec<(usize, R)>> = contents
        .par_bridge()
        .map_init(Parser::new, |parser, (index, item, source)| {
            source.map(|source| (index, f(parser, item, &source)))
        })
        .collect();
    if results.is_err() {
        // Unblock the writer thread before joining it.
        child.kill().ok();
    }
    let written = writer
        .join()
        .map_err(|_| anyhow!("`git cat-file` writer thread panicked"))?;
    child.wait()?;
    let mut results = results?;
    written.context("could not write to `git cat-file`")?;
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, r)| r).collect())
}

impl AsRef<Blob> for Blob {
    fn as_ref(&self) -> &Blob {
        self
    }
}

impl AsRef<Blob> for (&Blob, &'static LangSpec) {
    fn as_ref(&self) -> &Blob {
        self.0
    }
}

/// Read one `<id> blob <size>\n<contents>\n` record from `git cat-file --batch`.
fn read_object(reader: &mut impl BufRead, id: &str) -> Result<Vec<u8>> {
    let mut header = String::new();
    reader.read_line(&mut header)?;
    let size = match header.split_whitespace().collect::<Vec<_>>()[..] {
        [_, "blob", size] => size.parse::<usize>()?,
        _ => bail!("`git cat-file` could not read blob {id}: {}", header.trim()),
    };
    let mut contents = vec![0; size + 1];
    reader.read_exact(&mut contents)?;
    contents.pop(); // the trailing newline
    Ok(contents)
}

fn git_output(args: &[&str]) -> Result<Vec<u8>> {
    run(Command::new("git").args(args))
}

fn run(cmd: &mut Command) -> Result<Vec<u8>> {
    let output = cmd
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("could not run {cmd:?}"))?;
    if !output.status.success() {
        bail!(
            "{cmd:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_batch_records_back_to_back() {
        let mut input: &[u8] = b"aaa blob 3\nfoo\nbbb blob 0\n\n";
        assert_eq!(read_object(&mut input, "aaa").unwrap(), b"foo");
        assert_eq!(read_object(&mut input, "bbb").unwrap(), b"");
    }

    #[test]
    fn missing_object_is_an_error() {
        let mut input: &[u8] = b"ccc missing\n";
        assert!(read_object(&mut input, "ccc").is_err());
    }
}
