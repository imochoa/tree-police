//! Baseline files: record today's findings so only new ones are reported.
//!
//! A baseline is JSON listing `(file, rule_id, snippet, count)` entries. A
//! finding is "baselined" (dropped) if an entry with the same file, rule and
//! trimmed source line still has count left; each finding consumes one. Line
//! numbers are deliberately not part of the key, so editing code above a
//! known finding doesn't resurface it, while a second copy of the same bad
//! line does show up.
//!
//! Paths are stored relative to the baseline's root (the same root the config
//! uses: the directory containing `.tree-police/` for the default
//! `<rules-dir>/baseline.json`, or an explicit file's own directory), so the
//! file is stable whatever directory the scan runs from.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::scan::Finding;

const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct File {
    version: u32,
    findings: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
struct Entry {
    file: String,
    rule_id: String,
    snippet: String,
    count: usize,
}

type Key = (String, String, String);

/// A loaded baseline, keyed for matching.
#[derive(Debug)]
pub struct Baseline {
    path: PathBuf,
    root: PathBuf,
    counts: HashMap<Key, usize>,
}

impl Baseline {
    /// An empty baseline that will be written to `path`, with paths relative
    /// to `root`.
    pub fn empty(path: PathBuf, root: PathBuf) -> Self {
        Baseline {
            path,
            root,
            counts: HashMap::new(),
        }
    }

    /// Read `path`; missing is an error (callers decide whether it's optional).
    pub fn load(path: PathBuf, root: PathBuf) -> Result<Self> {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("could not read baseline {}", path.display()))?;
        let file: File = serde_json::from_str(&text)
            .with_context(|| format!("invalid baseline {}", path.display()))?;
        if file.version != VERSION {
            bail!(
                "baseline {} has version {}, expected {VERSION}",
                path.display(),
                file.version
            );
        }
        let mut counts = HashMap::new();
        for e in file.findings {
            *counts.entry((e.file, e.rule_id, e.snippet)).or_insert(0) += e.count;
        }
        Ok(Baseline { path, root, counts })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Drop baselined findings in place; returns how many were dropped.
    pub fn apply(&self, findings: &mut Vec<Finding>) -> usize {
        let mut left = self.counts.clone();
        let before = findings.len();
        findings.retain(|f| match left.get_mut(&self.key(f)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        });
        before - findings.len()
    }

    /// Replace the entries for every file in `scanned` with `findings`,
    /// keeping entries for files this run didn't scan, and write the result.
    /// That makes a partial run (e.g. pre-commit's changed files) safe.
    /// Returns the total number of baselined findings written.
    pub fn update(mut self, scanned: &[PathBuf], findings: &[Finding]) -> Result<usize> {
        let scanned: HashSet<String> = scanned.iter().map(|p| self.rel(p)).collect();
        self.counts
            .retain(|(file, _, _), _| !scanned.contains(file));
        for f in findings {
            *self.counts.entry(self.key(f)).or_insert(0) += 1;
        }

        let sorted: BTreeMap<&Key, usize> = self.counts.iter().map(|(k, v)| (k, *v)).collect();
        let entries: Vec<Entry> = sorted
            .into_iter()
            .map(|((file, rule_id, snippet), count)| Entry {
                file: file.clone(),
                rule_id: rule_id.clone(),
                snippet: snippet.clone(),
                count,
            })
            .collect();
        let total = entries.iter().map(|e| e.count).sum();
        let file = File {
            version: VERSION,
            findings: entries,
        };
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut text = serde_json::to_string_pretty(&file)?;
        text.push('\n');
        fs::write(&self.path, text)
            .with_context(|| format!("could not write baseline {}", self.path.display()))?;
        Ok(total)
    }

    fn key(&self, f: &Finding) -> Key {
        (
            self.rel(&f.file),
            f.rule_id.clone(),
            f.snippet.trim().to_string(),
        )
    }

    /// Root-relative path with `/` separators; files outside the root keep
    /// their absolute path.
    fn rel(&self, path: &Path) -> String {
        let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let rel = abs.strip_prefix(&self.root).unwrap_or(&abs);
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::severity::Severity;

    fn finding(file: &str, line: usize, snippet: &str) -> Finding {
        Finding {
            file: PathBuf::from(file),
            language: "python",
            rule_id: "forbidden_print".to_string(),
            severity: Severity::Warning,
            description: None,
            category: None,
            line,
            column: 1,
            end_line: line,
            end_column: 2,
            query_file: "rules-py.scm".to_string(),
            snippet: snippet.to_string(),
        }
    }

    #[test]
    fn matches_by_content_not_line_and_counts_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let path = root.join("baseline.json");
        let a = root.join("a.py");
        let b = root.join("b.py");
        let a = a.to_str().unwrap();
        let b = b.to_str().unwrap();

        let known = vec![finding(a, 1, "print(1)"), finding(b, 3, "  print(2)")];
        let total = Baseline::empty(path.clone(), root.clone())
            .update(&[PathBuf::from(a), PathBuf::from(b)], &known)
            .unwrap();
        assert_eq!(total, 2);

        let baseline = Baseline::load(path.clone(), root.clone()).unwrap();
        // Moved down two lines, plus a second copy of the same line, plus a
        // new one.
        let mut now = vec![
            finding(a, 3, "print(1)"),
            finding(a, 4, "print(1)"),
            finding(b, 3, "print(2)"),
            finding(b, 9, "print(3)"),
        ];
        assert_eq!(baseline.apply(&mut now), 2);
        let left: Vec<_> = now.iter().map(|f| (f.line, f.snippet.as_str())).collect();
        assert_eq!(left, vec![(4, "print(1)"), (9, "print(3)")]);

        // A partial update (only b.py scanned) keeps a.py's entry.
        let total = Baseline::load(path.clone(), root.clone())
            .unwrap()
            .update(&[PathBuf::from(b)], &[])
            .unwrap();
        assert_eq!(total, 1);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"file\": \"a.py\""), "{text}");
        assert!(!text.contains("b.py"), "{text}");
    }
}
