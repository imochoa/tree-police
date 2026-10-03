//! Loading and compiling `.scm` query files, embedded and repo-local.
//!
//! The entire `queries/` tree is baked into the binary with [`include_dir`].
//! It's flat: every `queries/<label>-<code>.scm` file is routed to a language
//! by its filename's trailing `-<code>` segment (looked up via
//! [`registry::by_code`]), not by directory nesting -- `<label>` is a free-form
//! human-readable tag with no semantic meaning to the loader. A repo using
//! this scanner can add its own files under the same convention in a
//! `--rules-dir` (default `.tree-police/`), merged with the embedded set at
//! load time with no rebuild required.
//!
//! Each query pattern carries per-pattern metadata read from `#set!` directives:
//! `severity` (defaults to [`Severity::DEFAULT`]), an optional `description`
//! (shown both per-finding and in the `--list-rules` catalog), and an
//! optional `category` (a free-form tag grouping related rules within a
//! language's query files -- see queries/README.md).
//!
//! Capture names that begin with `_` are *helper* captures — they exist only to
//! drive predicates (`#eq?`, `#match?`, …) and are never reported. The single
//! non-helper capture in a pattern is its rule ID.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use include_dir::{include_dir, Dir};
use tree_sitter::Query;

use crate::registry::{self, LangSpec};
use crate::severity::Severity;

static QUERIES_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/queries");

/// Per-pattern metadata extracted at load time.
#[derive(Debug, Clone)]
pub struct PatternMeta {
    /// The reportable rule ID (the pattern's single non-helper capture name).
    pub rule_id: Option<String>,
    pub severity: Severity,
    /// Human-readable explanation, shown per-finding and in `--list-rules`.
    pub description: Option<String>,
    /// Free-form grouping tag (e.g. "security", "naming"), shown in `--list-rules`.
    pub category: Option<String>,
}

/// A compiled query plus the metadata needed to report its matches.
pub struct CompiledQuery {
    /// Source path relative to `queries/`, e.g. `rules-py.scm`.
    pub source_path: String,
    pub query: Query,
    /// Indexed by `pattern_index`.
    patterns: Vec<PatternMeta>,
    /// Indexed by capture index: `true` if the capture name starts with `_`.
    helper_captures: Vec<bool>,
    capture_names: Vec<String>,
}

impl CompiledQuery {
    /// Whether the capture at `index` is a helper (predicate-only) capture.
    pub fn is_helper(&self, index: u32) -> bool {
        self.helper_captures
            .get(index as usize)
            .copied()
            .unwrap_or(false)
    }

    /// The capture name for a capture index.
    pub fn capture_name(&self, index: u32) -> &str {
        &self.capture_names[index as usize]
    }

    /// Metadata for a given `pattern_index`.
    pub fn pattern_meta(&self, pattern_index: usize) -> &PatternMeta {
        &self.patterns[pattern_index]
    }

    /// All pattern metadata, for `--list-rules`.
    pub fn patterns(&self) -> &[PatternMeta] {
        &self.patterns
    }
}

/// All compiled queries for one language.
pub struct LanguageRules {
    pub spec: &'static LangSpec,
    pub queries: Vec<CompiledQuery>,
}

/// The full set of embedded rules, grouped by language.
pub struct Ruleset {
    languages: Vec<LanguageRules>,
}

impl Ruleset {
    /// Rules for a language by name.
    pub fn language(&self, name: &str) -> Option<&LanguageRules> {
        self.languages.iter().find(|l| l.spec.name == name)
    }

    /// All languages that have at least one embedded query.
    pub fn languages(&self) -> &[LanguageRules] {
        &self.languages
    }
}

/// Compile every embedded `.scm` file, with no repo-local additions.
pub fn load() -> Result<Ruleset> {
    load_with_extra(None)
}

/// Compile every embedded `.scm` file plus, if `extra_dir` exists, every
/// `.scm` file directly under it. A missing `extra_dir` is not an error --
/// most repos won't have repo-local rules, and that's the common case.
pub fn load_with_extra(extra_dir: Option<&Path>) -> Result<Ruleset> {
    let mut by_lang: HashMap<&'static str, Vec<CompiledQuery>> = HashMap::new();

    let mut embedded: Vec<_> = QUERIES_DIR
        .files()
        .filter(|f| f.path().extension().is_some_and(|e| e == "scm"))
        .collect();
    embedded.sort_by_key(|f| f.path().to_path_buf());
    for file in embedded {
        let label = file.path().to_string_lossy().into_owned();
        let source = file
            .contents_utf8()
            .ok_or_else(|| anyhow!("query file {label} is not valid UTF-8"))?;
        let (spec, query) = compile_for_label(&label, source)?;
        by_lang.entry(spec.name).or_default().push(query);
    }

    if let Some(dir) = extra_dir {
        if dir.is_dir() {
            let mut extra: Vec<_> = fs::read_dir(dir)
                .with_context(|| format!("could not read rules dir {}", dir.display()))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "scm"))
                .collect();
            extra.sort();
            for path in extra {
                let label = path.to_string_lossy().into_owned();
                let source = fs::read_to_string(&path)
                    .with_context(|| format!("could not read query file {label}"))?;
                let (spec, query) = compile_for_label(&label, &source)?;
                by_lang.entry(spec.name).or_default().push(query);
            }
        }
    }

    if by_lang.is_empty() {
        return Err(anyhow!("no queries were embedded into the binary"));
    }

    // Keep `registry::LANGUAGES`'s stable order rather than the HashMap's.
    let languages = registry::LANGUAGES
        .iter()
        .filter_map(|spec| {
            by_lang
                .remove(spec.name)
                .map(|queries| LanguageRules { spec, queries })
        })
        .collect();
    Ok(Ruleset { languages })
}

/// Resolve a query file's language from its `<label>-<code>.scm` name (the
/// code is everything after the *last* `-`, or the whole stem if there is
/// none) and compile it against that language's grammar.
fn compile_for_label(label: &str, source: &str) -> Result<(&'static LangSpec, CompiledQuery)> {
    let stem = Path::new(label)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("query file {label} has no file name"))?;
    let code = stem.rsplit_once('-').map_or(stem, |(_, code)| code);
    let spec = registry::by_code(code).ok_or_else(|| {
        let known: Vec<_> = registry::LANGUAGES.iter().map(|s| s.code).collect();
        anyhow!(
            "query file {label} has unrecognized language code {code:?} \
             (expected the name to end in one of: {known:?})"
        )
    })?;
    let query = Query::new(&spec.language(), source)
        .with_context(|| format!("failed to compile query {label}"))?;
    Ok((spec, compile(label.to_string(), source, query)))
}

fn compile(source_path: String, source: &str, query: Query) -> CompiledQuery {
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let helper_captures: Vec<bool> = capture_names.iter().map(|n| n.starts_with('_')).collect();

    let count = query.pattern_count();
    let mut patterns = Vec::with_capacity(count);
    for i in 0..count {
        let mut severity = Severity::DEFAULT;
        let mut description = None;
        let mut category = None;
        for prop in query.property_settings(i) {
            match &*prop.key {
                "severity" => {
                    if let Some(v) = prop.value.as_deref() {
                        if let Ok(parsed) = v.parse::<Severity>() {
                            severity = parsed;
                        }
                    }
                }
                "description" => description = prop.value.as_deref().map(str::to_string),
                "category" => category = prop.value.as_deref().map(str::to_string),
                _ => {}
            }
        }

        // Slice this pattern's source text to recover its rule ID (the single
        // non-helper `@capture`). Used only for `--list-rules`; the scanner
        // derives the rule ID from the matched capture directly.
        let start = query.start_byte_for_pattern(i);
        let end = if i + 1 < count {
            query.start_byte_for_pattern(i + 1)
        } else {
            source.len()
        };
        let rule_id = primary_capture_in(&source[start..end]);

        patterns.push(PatternMeta {
            rule_id,
            severity,
            description,
            category,
        });
    }

    CompiledQuery {
        source_path,
        query,
        patterns,
        helper_captures,
        capture_names,
    }
}

/// Find the last non-helper `@capture` name in a pattern's source slice.
///
/// The slice runs from this pattern's start byte to the *next* pattern's
/// start byte (or EOF), so it includes any `;` comment trailing the
/// pattern's closing paren -- comments must be skipped while scanning, or a
/// `@word`-shaped mention in prose (e.g. "isn't @activity/@workflow
/// decorated") gets mistaken for the pattern's reportable capture. Confirmed
/// by testing: this previously picked up `@workflow` from exactly such a
/// comment and reported it as the rule ID instead of the real capture.
///
/// String literals (predicate regex arguments, `#set!` values) get the same
/// treatment, for the same reason: a regex like `"@validate\\("` contains a
/// `@word`-shaped substring that isn't a capture reference. Also confirmed
/// by testing -- missing_route_validation's own `#not-match?` argument
/// shadowed its real capture name this way before strings were skipped too.
fn primary_capture_in(slice: &str) -> Option<String> {
    let bytes = slice.as_bytes();
    let mut result = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b';' {
            i = bytes[i..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |p| i + p + 1);
        } else if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(bytes.len());
        } else if bytes[i] == b'@' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b'_' | b'.' | b'-'))
            {
                j += 1;
            }
            if j > start {
                let name = &slice[start..j];
                if !name.starts_with('_') {
                    result = Some(name.to_string());
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_queries_compile() {
        // If any `.scm` fails to compile against its grammar, this errors —
        // the guard against node-name drift between the CLI and the crates.
        let ruleset = load().expect("embedded queries should compile");
        assert!(ruleset.language("python").is_some());
        assert!(ruleset.language("terraform").is_some());
    }

    #[test]
    fn primary_capture_ignores_helpers() {
        let slice = "(call function: (identifier) @_func (#eq? @_func \"print\")) @forbidden_print";
        assert_eq!(
            primary_capture_in(slice).as_deref(),
            Some("forbidden_print")
        );
    }

    #[test]
    fn primary_capture_ignores_at_mentions_in_trailing_comments() {
        // The slice for a pattern runs up to the *next* pattern's start byte,
        // so it includes a comment trailing this pattern's close paren --
        // that comment must not be mistaken for the pattern's own capture.
        let slice = "(call function: (identifier) @_func) @os_popen_usage\n\
                      ; isn't @activity/@workflow decorated\n";
        assert_eq!(primary_capture_in(slice).as_deref(), Some("os_popen_usage"));
    }

    #[test]
    fn primary_capture_ignores_at_mentions_in_string_literals() {
        // A regex predicate argument like "@validate\(" contains a
        // @word-shaped substring that is not a capture reference either.
        let slice = "((decorated_definition) @missing_route_validation\n\
                      (#not-match? @missing_route_validation \"@validate\\\\(\"))";
        assert_eq!(
            primary_capture_in(slice).as_deref(),
            Some("missing_route_validation")
        );
    }
}
