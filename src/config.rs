//! Path-scoped rule configuration (`.tree-police/config.toml`).
//!
//! Every rule runs on every file of its language unless a config says
//! otherwise. Two kinds of entries, both keyed by rule [`Selector`]s:
//!
//! - `[[scope]]` restricts *where* the selected rules run: `paths` (if set)
//!   is an allowlist, `exclude` a denylist. A finding survives only if every
//!   scope that selects its rule admits its file.
//! - `[[override]]` changes the selected rules' severity for the files it
//!   matches (same `paths`/`exclude` semantics, but no restriction). The last
//!   matching override wins.
//!
//! Language filtering is implicit: a rule only ever runs on files of its own
//! language, so globs never need an extension (`src/**` is enough).
//!
//! Globs are matched against the file's path relative to the config's root:
//! the directory *containing* the rules dir for the default
//! `<rules-dir>/config.toml` (i.e. the repo root for `.tree-police/`), or the
//! config file's own directory for an explicit `--config FILE`.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

use crate::rules::Ruleset;
use crate::scan::Finding;
use crate::severity::Severity;

/// What a config entry or suppression comment applies to: a rule ID
/// (`forbidden_print`), a category (`category:security`), or a query file
/// by name (`file:no-foo-py.scm`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Rule(String),
    Category(String),
    File(String),
}

impl Selector {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        let sel = if let Some(c) = s.strip_prefix("category:") {
            Selector::Category(c.trim().to_string())
        } else if let Some(f) = s.strip_prefix("file:") {
            Selector::File(f.trim().to_string())
        } else {
            Selector::Rule(s.to_string())
        };
        match &sel {
            Selector::Rule(v) | Selector::Category(v) | Selector::File(v) if v.is_empty() => {
                bail!("empty rule selector {s:?}")
            }
            _ => Ok(sel),
        }
    }

    pub fn matches(&self, finding: &Finding) -> bool {
        match self {
            Selector::Rule(id) => finding.rule_id == *id,
            Selector::Category(c) => finding.category.as_deref() == Some(c.as_str()),
            Selector::File(name) => file_name(&finding.query_file) == name,
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Selector::Rule(id) => f.write_str(id),
            Selector::Category(c) => write!(f, "category:{c}"),
            Selector::File(name) => write!(f, "file:{name}"),
        }
    }
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

/// Every rule ID, category, and query file name a [`Ruleset`] defines, for
/// validating selectors so a typo fails loudly instead of matching nothing.
#[derive(Debug, Default)]
pub struct Catalog {
    rules: BTreeSet<String>,
    categories: BTreeSet<String>,
    files: BTreeSet<String>,
}

impl Catalog {
    pub fn from_ruleset(ruleset: &Ruleset) -> Self {
        let mut catalog = Catalog::default();
        for lang in ruleset.languages() {
            for cq in &lang.queries {
                catalog.files.insert(file_name(&cq.source_path).to_string());
                catalog.rules.extend(
                    cq.query
                        .capture_names()
                        .iter()
                        .filter(|n| !n.starts_with('_'))
                        .map(|n| n.to_string()),
                );
                catalog
                    .categories
                    .extend(cq.patterns().iter().filter_map(|p| p.category.clone()));
            }
        }
        catalog
    }

    /// `Err` with a human-readable reason if `sel` names nothing known.
    pub fn check(&self, sel: &Selector) -> Result<()> {
        let (known, kind) = match sel {
            Selector::Rule(v) => (self.rules.contains(v), "rule ID"),
            Selector::Category(v) => (self.categories.contains(v), "category"),
            Selector::File(v) => (self.files.contains(v), "query file"),
        };
        if known {
            Ok(())
        } else {
            Err(anyhow!(
                "unknown {kind} {:?} (see --list-rules)",
                sel.to_string()
            ))
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    scope: Vec<RawEntry>,
    #[serde(default, rename = "override")]
    overrides: Vec<RawEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    rules: Vec<String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    severity: Option<String>,
}

/// A `paths`/`exclude` pair plus the rules it applies to.
#[derive(Debug)]
struct Entry {
    selectors: Vec<Selector>,
    paths: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl Entry {
    fn selects(&self, finding: &Finding) -> bool {
        self.selectors.iter().any(|s| s.matches(finding))
    }

    /// Whether `rel` (a root-relative path, or `None` for a file outside the
    /// root) is inside `paths` (if any) and not in `exclude`.
    fn admits(&self, rel: Option<&Path>) -> bool {
        let Some(rel) = rel else {
            // Outside the root no glob can match: only a `paths` allowlist
            // can reject it.
            return self.paths.is_none();
        };
        self.paths.as_ref().is_none_or(|set| set.is_match(rel))
            && !self.exclude.as_ref().is_some_and(|set| set.is_match(rel))
    }
}

/// A parsed, validated config. [`Config::default`] is the empty config
/// (every rule runs everywhere at its declared severity).
#[derive(Debug, Default)]
pub struct Config {
    root: PathBuf,
    scopes: Vec<Entry>,
    overrides: Vec<(Entry, Severity)>,
}

impl Config {
    /// Load the default `<rules_dir>/config.toml` (root = `rules_dir`'s
    /// parent), or an empty config if it doesn't exist.
    pub fn load_default(rules_dir: &Path, catalog: &Catalog) -> Result<Self> {
        let path = rules_dir.join("config.toml");
        if !path.is_file() {
            return Ok(Config::default());
        }
        let rules_dir = std::path::absolute(rules_dir)?;
        let root = rules_dir.parent().unwrap_or(&rules_dir).to_path_buf();
        Self::load(&path, root, catalog)
    }

    /// Load an explicit config file (root = its directory). Missing is an
    /// error: it was asked for by name.
    pub fn load_explicit(path: &Path, catalog: &Catalog) -> Result<Self> {
        let abs = std::path::absolute(path)?;
        let root = abs.parent().unwrap_or(&abs).to_path_buf();
        Self::load(path, root, catalog)
    }

    fn load(path: &Path, root: PathBuf, catalog: &Catalog) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("could not read config {}", path.display()))?;
        Self::parse(&text, root, catalog)
            .with_context(|| format!("invalid config {}", path.display()))
    }

    pub fn parse(text: &str, root: PathBuf, catalog: &Catalog) -> Result<Self> {
        let raw: RawConfig = toml::from_str(text)?;
        let mut config = Config {
            root,
            ..Config::default()
        };
        for (i, raw) in raw.scope.into_iter().enumerate() {
            let ctx = || format!("[[scope]] #{}", i + 1);
            if raw.severity.is_some() {
                return Err(anyhow!("`severity` belongs in an [[override]] entry"))
                    .with_context(ctx);
            }
            let entry = build_entry(raw, catalog).with_context(ctx)?;
            if entry.paths.is_none() && entry.exclude.is_none() {
                return Err(anyhow!("needs `paths` and/or `exclude`")).with_context(ctx);
            }
            config.scopes.push(entry);
        }
        for (i, mut raw) in raw.overrides.into_iter().enumerate() {
            let ctx = || format!("[[override]] #{}", i + 1);
            let severity = raw
                .severity
                .take()
                .ok_or_else(|| anyhow!("missing `severity`"))
                .with_context(ctx)?
                .parse::<Severity>()
                .map_err(|e| anyhow!(e))
                .with_context(ctx)?;
            let entry = build_entry(raw, catalog).with_context(ctx)?;
            config.overrides.push((entry, severity));
        }
        Ok(config)
    }

    /// Apply scopes and overrides to one file's findings in place.
    pub fn apply(&self, path: &Path, findings: &mut Vec<Finding>) {
        if self.scopes.is_empty() && self.overrides.is_empty() {
            return;
        }
        let rel = self.relative(path);
        let rel = rel.as_deref();
        findings.retain(|f| {
            self.scopes
                .iter()
                .all(|scope| !scope.selects(f) || scope.admits(rel))
        });
        for f in findings.iter_mut() {
            if let Some((_, severity)) = self
                .overrides
                .iter()
                .rev()
                .find(|(entry, _)| entry.selects(f) && entry.admits(rel))
            {
                f.severity = *severity;
            }
        }
    }

    fn relative(&self, path: &Path) -> Option<PathBuf> {
        let abs = std::path::absolute(path).ok()?;
        abs.strip_prefix(&self.root).ok().map(Path::to_path_buf)
    }
}

fn build_entry(raw: RawEntry, catalog: &Catalog) -> Result<Entry> {
    if raw.rules.is_empty() {
        bail!("`rules` must not be empty");
    }
    let selectors = raw
        .rules
        .iter()
        .map(|s| {
            let sel = Selector::parse(s)?;
            catalog.check(&sel)?;
            Ok(sel)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Entry {
        selectors,
        paths: glob_set(&raw.paths)?,
        exclude: glob_set(&raw.exclude)?,
    })
}

fn glob_set(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let p = p.trim_start_matches("./");
        builder.add(Glob::new(p).with_context(|| format!("invalid glob {p:?}"))?);
        // `dir/` and `dir` both mean "everything under dir" too, like
        // .gitignore; a bare file glob simply never matches as a dir prefix.
        let dir = p.trim_end_matches('/');
        if !dir.contains("**") {
            builder.add(Glob::new(&format!("{dir}/**"))?);
        }
    }
    Ok(Some(builder.build()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(file: &str, rule: &str, category: Option<&str>) -> Finding {
        Finding {
            file: PathBuf::from(file),
            language: "python",
            rule_id: rule.to_string(),
            severity: Severity::Warning,
            description: None,
            category: category.map(str::to_string),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 2,
            query_file: "rules-py.scm".to_string(),
            snippet: String::new(),
        }
    }

    fn catalog() -> Catalog {
        Catalog::from_ruleset(&crate::rules::load().unwrap())
    }

    fn config(text: &str) -> Config {
        Config::parse(text, PathBuf::from("/repo"), &catalog()).unwrap()
    }

    fn surviving(config: &Config, file: &str, f: Finding) -> Option<Severity> {
        let mut v = vec![f];
        config.apply(Path::new(file), &mut v);
        v.pop().map(|f| f.severity)
    }

    #[test]
    fn scope_paths_is_an_allowlist() {
        let c = config(
            r#"
            [[scope]]
            rules = ["category:temporal"]
            paths = ["src/workflows"]
            "#,
        );
        let f = || finding("x", "unhandled_await_in_workflow", Some("temporal"));
        assert!(surviving(&c, "/repo/src/workflows/a.py", f()).is_some());
        assert!(surviving(&c, "/repo/src/other/a.py", f()).is_none());
        assert!(surviving(&c, "/elsewhere/a.py", f()).is_none());
        // Unselected rules are untouched.
        let other = finding("x", "forbidden_print", Some("forbidden"));
        assert!(surviving(&c, "/repo/src/other/a.py", other).is_some());
    }

    #[test]
    fn scope_exclude_is_a_denylist() {
        let c = config(
            r#"
            [[scope]]
            rules = ["forbidden_print"]
            exclude = ["tests/**", "scripts/"]
            "#,
        );
        let f = || finding("x", "forbidden_print", None);
        assert!(surviving(&c, "/repo/tests/unit/test_a.py", f()).is_none());
        assert!(surviving(&c, "/repo/scripts/run.py", f()).is_none());
        assert!(surviving(&c, "/repo/src/a.py", f()).is_some());
        assert!(surviving(&c, "/elsewhere/a.py", f()).is_some());
    }

    #[test]
    fn last_matching_override_wins() {
        let c = config(
            r#"
            [[override]]
            rules = ["forbidden_print"]
            severity = "error"

            [[override]]
            rules = ["forbidden_print"]
            paths = ["legacy/**"]
            severity = "log"
            "#,
        );
        let f = || finding("x", "forbidden_print", None);
        assert_eq!(surviving(&c, "/repo/legacy/a.py", f()), Some(Severity::Log));
        assert_eq!(surviving(&c, "/repo/src/a.py", f()), Some(Severity::Error));
    }

    #[test]
    fn rejects_unknown_selectors_and_misplaced_keys() {
        let cat = catalog();
        let root = || PathBuf::from("/repo");
        for bad in [
            "[[scope]]\nrules = [\"no_such_rule\"]\npaths = [\"a\"]",
            "[[scope]]\nrules = [\"category:nope\"]\npaths = [\"a\"]",
            "[[scope]]\nrules = [\"forbidden_print\"]",
            "[[scope]]\nrules = [\"forbidden_print\"]\npaths = [\"a\"]\nseverity = \"log\"",
            "[[override]]\nrules = [\"forbidden_print\"]",
            "[[override]]\nrules = [\"forbidden_print\"]\nseverity = \"loud\"",
            "[[scop]]\nrules = [\"forbidden_print\"]",
        ] {
            assert!(Config::parse(bad, root(), &cat).is_err(), "accepted: {bad}");
        }
    }
}
