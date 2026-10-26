//! `--git-history`: run a scan over every file version in a git
//! repository's history rather than the working tree.
//!
//! Each distinct blob is read and parsed exactly once, however many commits
//! contain it, and is attributed to the oldest commit that introduced it. A
//! file that never changed across 10k commits costs one parse, not 10k. The
//! human-facing report then collapses the same hit across file versions into
//! one [`Occurrence`] (see `report::render_history`).
//! Everything goes through the `git` binary (`git log --raw` to list the
//! blobs, one `git cat-file --batch` to stream them), so there's no libgit2
//! dependency and anything `git` can read works, shallow clones included.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
use crate::severity::Severity;

/// A commit that introduced at least one blob.
#[derive(Debug, Clone)]
pub struct Commit {
    pub id: String,
    /// Author date, `YYYY-MM-DD`.
    pub date: String,
    pub author: String,
    pub subject: String,
}

impl Commit {
    /// The abbreviated id shown to humans.
    pub fn short(&self) -> &str {
        &self.id[..self.id.len().min(7)]
    }
}

/// One distinct file version, at the oldest commit that introduced it.
#[derive(Debug, Clone)]
pub struct Blob {
    /// Object id of the file contents.
    pub id: String,
    /// Index into [`History::commits`] of the oldest commit that added this
    /// content.
    pub commit: usize,
    /// Absolute path the file had in that commit (what config scoping and
    /// the language lookup see).
    pub path: PathBuf,
    /// The same path relative to the cwd: the reported `file`.
    pub file: PathBuf,
    /// Whether `HEAD`'s tree still contains this exact content.
    pub at_head: bool,
}

impl Blob {
    fn language(&self) -> Option<&'static LangSpec> {
        let ext = self.path.extension()?.to_str()?;
        registry::for_extension(ext)
    }
}

/// Every distinct blob in the scanned history, oldest commit first, and the
/// commits that introduced them.
#[derive(Debug, Default)]
pub struct History {
    pub commits: Vec<Commit>,
    pub blobs: Vec<Blob>,
}

/// A finding or match, tagged with the blob (index into
/// [`History::blobs`]) it was found in.
#[derive(Debug, Clone)]
pub struct Hit<T> {
    pub blob: usize,
    pub item: T,
}

/// What the history report needs from a [`Finding`] or [`Match`].
pub trait Reportable: serde::Serialize {
    fn file(&self) -> &Path;
    fn line(&self) -> usize;
    fn column(&self) -> usize;
    fn snippet(&self) -> &str;
    /// What matched: the rule id, or `@capture` for an ad-hoc query.
    fn label(&self) -> String;
    fn severity(&self) -> Option<Severity>;
}

impl Reportable for Finding {
    fn file(&self) -> &Path {
        &self.file
    }
    fn line(&self) -> usize {
        self.line
    }
    fn column(&self) -> usize {
        self.column
    }
    fn snippet(&self) -> &str {
        &self.snippet
    }
    fn label(&self) -> String {
        self.rule_id.clone()
    }
    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }
}

impl Reportable for Match {
    fn file(&self) -> &Path {
        &self.file
    }
    fn line(&self) -> usize {
        self.line
    }
    fn column(&self) -> usize {
        self.column
    }
    fn snippet(&self) -> &str {
        &self.snippet
    }
    fn label(&self) -> String {
        format!("@{}", self.capture)
    }
    fn severity(&self) -> Option<Severity> {
        None
    }
}

/// The same hit (same file, rule/capture and source line) across every file
/// version that contains it, collapsed into one row for humans. Line numbers
/// drift as a file is edited, so the line's text is the identity, not its
/// number.
#[derive(Debug)]
pub struct Occurrence<'a, T> {
    /// The hit in the oldest version.
    pub first: &'a Hit<T>,
    /// The hit in the newest version that still has it.
    pub last: &'a Hit<T>,
    /// How many distinct file versions contain it.
    pub versions: usize,
    /// Whether `HEAD` still contains it.
    pub at_head: bool,
}

/// Collapse `hits` (oldest blob first) into [`Occurrence`]s, sorted by file,
/// then by when they first appeared.
pub fn occurrences<'a, T: Reportable>(
    history: &History,
    hits: &'a [Hit<T>],
) -> Vec<Occurrence<'a, T>> {
    let mut index: HashMap<(&Path, String, &str, usize), usize> = HashMap::new();
    let mut rows: Vec<Occurrence<'a, T>> = Vec::new();
    for hit in hits {
        let item = &hit.item;
        let text = item.snippet().trim_start();
        let indent = item.snippet().len() - text.len();
        let key = (
            item.file(),
            item.label(),
            text,
            item.column().saturating_sub(indent),
        );
        let at_head = history.blobs[hit.blob].at_head;
        match index.get(&key) {
            Some(&row) => {
                let row = &mut rows[row];
                if row.last.blob != hit.blob {
                    row.versions += 1;
                }
                row.last = hit;
                row.at_head |= at_head;
            }
            None => {
                index.insert(key, rows.len());
                rows.push(Occurrence {
                    first: hit,
                    last: hit,
                    versions: 1,
                    at_head,
                });
            }
        }
    }
    rows.sort_by(|a, b| {
        a.first
            .item
            .file()
            .cmp(b.first.item.file())
            .then(a.first.blob.cmp(&b.first.blob))
            .then(a.first.item.line().cmp(&b.first.item.line()))
    });
    rows
}

impl History {
    /// List every distinct blob reachable from `revs` (all refs when
    /// empty), limited to `pathspecs` (interpreted by git, relative to the
    /// cwd), oldest commit first.
    pub fn load(revs: &[String], pathspecs: &[PathBuf]) -> Result<Self> {
        let toplevel = git_output(&["rev-parse", "--show-toplevel"])
            .context("--git-history must run inside a git repository")?;
        let toplevel = PathBuf::from(String::from_utf8_lossy(&toplevel).trim());
        let cwd = std::env::current_dir()?;
        let head = head_blobs()?;

        let mut cmd = Command::new("git");
        // --topo-order with --reverse visits parents before children, so a
        // blob's first sighting is its oldest commit. Merges diff against
        // their first parent only: content coming in from the other side was
        // already seen on that side's own commits, so only conflict
        // resolutions are new.
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
            "--format=%x01%H%x02%as%x02%an%x02%s",
        ]);
        if revs.is_empty() {
            cmd.arg("--all");
        } else {
            cmd.args(revs);
        }
        cmd.arg("--").args(pathspecs);
        let raw = run(&mut cmd)?;

        let mut history = History::default();
        // The current commit, pushed to `history.commits` on its first blob.
        let mut pending: Option<Commit> = None;
        let mut seen = HashSet::new();
        let mut fields = raw.split(|&b| b == 0).map(|f| f.trim_ascii_start());
        while let Some(field) = fields.next() {
            if let Some(header) = field.strip_prefix(b"\x01") {
                let header = String::from_utf8_lossy(header);
                let mut parts = header.splitn(4, '\x02').map(String::from);
                let mut next = || parts.next().unwrap_or_default();
                pending = Some(Commit {
                    id: next(),
                    date: next(),
                    author: next(),
                    subject: next(),
                });
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
            // Regular files only: deletions (mode 000000), symlinks (120000)
            // and submodule commits (160000) have no source to scan.
            if !matches!(new_mode, "100644" | "100755") || !seen.insert(new_id.to_string()) {
                continue;
            }
            if let Some(commit) = pending.take() {
                history.commits.push(commit);
            }
            let path = toplevel.join(String::from_utf8_lossy(path).as_ref());
            history.blobs.push(Blob {
                id: new_id.to_string(),
                commit: history.commits.len() - 1,
                file: path.strip_prefix(&cwd).unwrap_or(&path).to_path_buf(),
                at_head: head.contains(new_id),
                path,
            });
        }
        Ok(history)
    }

    /// The commit that introduced `hit`'s blob.
    pub fn commit_of<T>(&self, hit: &Hit<T>) -> &Commit {
        &self.commits[self.blobs[hit.blob].commit]
    }

    /// History counterpart of [`scan::scan`]: the ruleset over every blob of
    /// a language it has rules for.
    pub fn scan(
        &self,
        ruleset: &Ruleset,
        config: &Config,
    ) -> Result<(Vec<Hit<Finding>>, ScanStats)> {
        let catalog = Catalog::from_ruleset(ruleset);
        let selected: Vec<(usize, &'static LangSpec)> = self
            .blobs
            .iter()
            .enumerate()
            .filter(|(_, b)| !config.excludes(&b.path))
            .filter_map(|(i, b)| Some((i, b.language()?)))
            .filter(|(_, spec)| ruleset.language(spec.name).is_some())
            .collect();

        let per_blob = self.for_each_blob(&selected, |parser, blob, &(_, spec), source| {
            let (findings, suppressed) =
                scan::scan_source(parser, &blob.path, spec, source, ruleset, config, &catalog);
            (relabel(findings, blob, |f| &mut f.file), suppressed)
        })?;

        let mut stats = self.stats_for(&selected);
        let mut hits = Vec::new();
        for (&(blob, _), (findings, suppressed)) in selected.iter().zip(per_blob) {
            stats.suppressed += suppressed;
            stats.files_with_findings += usize::from(!findings.is_empty());
            hits.extend(findings.into_iter().map(|item| Hit { blob, item }));
        }
        Ok((hits, stats))
    }

    /// History counterpart of [`scan::run_query`].
    pub fn run_query(
        &self,
        spec: &'static LangSpec,
        query: &Query,
    ) -> Result<(Vec<Hit<Match>>, ScanStats)> {
        let selected: Vec<(usize, ())> = self
            .blobs
            .iter()
            .enumerate()
            .filter(|(_, b)| b.language().is_some_and(|s| s.name == spec.name))
            .map(|(i, _)| (i, ()))
            .collect();
        let per_blob = self.for_each_blob(&selected, |parser, blob, _, source| {
            let matches = scan::query_source(parser, &blob.path, spec, source, query);
            relabel(matches, blob, |m| &mut m.file)
        })?;
        Ok(self.collect_matches(&selected, per_blob))
    }

    /// History counterpart of [`scan::grep`]: every blob, any extension.
    pub fn grep(&self, regex: &Regex) -> Result<(Vec<Hit<Match>>, ScanStats)> {
        let selected: Vec<(usize, ())> = (0..self.blobs.len()).map(|i| (i, ())).collect();
        let per_blob = self.for_each_blob(&selected, |_, blob, _, source| {
            relabel(scan::grep_source(&blob.path, source, regex), blob, |m| {
                &mut m.file
            })
        })?;
        Ok(self.collect_matches(&selected, per_blob))
    }

    /// [`scan::infer_language`] for history: the single registered language
    /// among the blobs, or an error asking for `--lang`.
    pub fn infer_language(&self) -> Result<&'static LangSpec> {
        let mut found: Vec<&'static LangSpec> = Vec::new();
        for spec in self.blobs.iter().filter_map(Blob::language) {
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

    fn collect_matches<X>(
        &self,
        selected: &[(usize, X)],
        per_blob: Vec<Vec<Match>>,
    ) -> (Vec<Hit<Match>>, ScanStats) {
        let mut stats = self.stats_for(selected);
        let mut hits = Vec::new();
        for (&(blob, _), matches) in selected.iter().zip(per_blob) {
            stats.files_with_findings += usize::from(!matches.is_empty());
            hits.extend(matches.into_iter().map(|item| Hit { blob, item }));
        }
        (hits, stats)
    }

    fn stats_for<X>(&self, selected: &[(usize, X)]) -> ScanStats {
        ScanStats {
            files_scanned: selected.len(),
            files: selected
                .iter()
                .map(|(i, _)| self.blobs[*i].file.clone())
                .collect(),
            ..ScanStats::default()
        }
    }

    /// Stream the contents of the `selected` blobs (indexes into
    /// [`Self::blobs`], each with some per-blob data) through one
    /// `git cat-file --batch` and run `f` on each in parallel (one
    /// tree-sitter parser per thread). Results come back in `selected` order.
    fn for_each_blob<X, R, F>(&self, selected: &[(usize, X)], f: F) -> Result<Vec<R>>
    where
        X: Sync,
        R: Send,
        F: Fn(&mut Parser, &Blob, &(usize, X), &[u8]) -> R + Sync,
    {
        let blobs: Vec<&Blob> = selected.iter().map(|(i, _)| &self.blobs[*i]).collect();
        let results = for_each_blob(&blobs, |parser, index, source| {
            f(parser, blobs[index], &selected[index], source)
        })?;
        Ok(results)
    }
}

/// Point each item's `file` at the blob's cwd-relative path, and sort a
/// blob's items by position.
fn relabel<T: Reportable>(
    mut items: Vec<T>,
    blob: &Blob,
    file: impl Fn(&mut T) -> &mut PathBuf,
) -> Vec<T> {
    for item in &mut items {
        *file(item) = blob.file.clone();
    }
    items.sort_by_key(|i| (i.line(), i.column()));
    items
}

/// Ids of every blob in `HEAD`'s tree (empty in a repo with no commits).
fn head_blobs() -> Result<HashSet<String>> {
    let Ok(raw) = git_output(&["ls-tree", "-r", "-z", "--format=%(objectname)", "HEAD"]) else {
        return Ok(HashSet::new());
    };
    Ok(raw
        .split(|&b| b == 0)
        .filter(|id| !id.is_empty())
        .map(|id| String::from_utf8_lossy(id).into_owned())
        .collect())
}

/// Stream `blobs`' contents through one `git cat-file --batch` and run `f`
/// on each (with its index in `blobs`) in parallel, one tree-sitter parser
/// per thread. Results come back in `blobs` order.
fn for_each_blob<R, F>(blobs: &[&Blob], f: F) -> Result<Vec<R>>
where
    R: Send,
    F: Fn(&mut Parser, usize, &[u8]) -> R + Sync,
{
    if blobs.is_empty() {
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
    let ids: Vec<String> = blobs.iter().map(|b| b.id.clone()).collect();
    let writer = thread::spawn(move || -> std::io::Result<()> {
        for id in ids {
            writeln!(stdin, "{id}")?;
        }
        Ok(())
    });

    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout"));
    let contents = blobs
        .iter()
        .enumerate()
        .map(move |(index, blob)| (index, read_object(&mut reader, &blob.id)));
    let results: Result<Vec<(usize, R)>> = contents
        .par_bridge()
        .map_init(Parser::new, |parser, (index, source)| {
            source.map(|source| (index, f(parser, index, &source)))
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
