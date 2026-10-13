---
name: tree-sitter-queries
description: >
  Search code by AST structure and author tree-police lint rules. Use whenever a
  request involves tree-sitter queries, tree-sitter `.scm` rule files,
  `tree-police`, AST or
  structural code search, or patterns such as "this call without that argument"
  and "this definition missing that decorator" that grep cannot express
  reliably. Covers inspecting real syntax trees, iterating with tree-police's
  ad-hoc query mode, query syntax and predicates, reusable rule metadata, and
  rule tests. Prefer ripgrep for simple literal or identifier searches.
license: MIT OR Apache-2.0
---

# Structural code search and tree-police rules

## Choose the smallest suitable tool

Use structural search when the match is defined by syntax rather than text:

- calls to `requests.get` without a `timeout=` argument;
- classes that subclass `Model` but lack an inner `Meta` class;
- functions missing a decorator;
- imports of one syntactic form but not another;
- reusable policy checks that should ignore comments and strings.

Use `rg` for a literal string, a simple identifier, or a log/config search. An
AST query adds no value there. Needing multiline regex, many `\s*` fragments,
or special handling for comments and strings is a strong signal to switch to
structural search.

Use these tools in this order:

1. **tree-police** for tree inspection, one-off S-expression queries, and
   reusable lint rules. It ships grammars for many languages in one binary and
   needs no grammar checkout.
2. **ast-grep (`sg`)** when a code-shaped `$META` pattern is easier to express,
   or when the task is a structural rewrite rather than a search/lint check.
3. **Raw `tree-sitter` CLI or the web playground** for grammar development,
   highlighting work, unsupported grammars, or engine-level debugging.
4. **tree-grepper** only when already required by an existing workflow. It is
   archived; tree-police now provides its tree-view and ad-hoc-query workflow.

If tree-police is unavailable, install the published binary with
`uv tool install tree-police`, use a release binary, or run the repository's
built binary. Do not clone a grammar merely to inspect a language that
`tree-police` already registers.

## Core workflow

### 1. Inspect the real tree

Never guess node or field names from source syntax. Parse one representative
file first:

```bash
tree-police --show-tree path/to/example.py
```

Copy node names and field names from that output. If the extension is unknown,
consult the tree-police language registry or use the relevant grammar's
`node-types.json` and playground.

### 2. Start with a broad ad-hoc query

Begin with one node and one capture:

```bash
tree-police --query '(call function: (identifier) @fn)' path/to/example.py
```

Then add fields, children, predicates, and anchors one constraint at a time.
Use `--query-file query.scm` when shell quoting becomes awkward:

```bash
tree-police --query-file /tmp/query.scm path/to/src/
```

Language is inferred for a single file or single-language directory. Pass the
registered language name for mixed-language input:

```bash
tree-police --query-file /tmp/query.scm --lang python path/to/src/
```

Ad-hoc mode reports every non-`_` capture. It is a search, not a lint gate, so
matches still exit `0`. Use `--format json` or `--format jsonl` for scripting.
It respects ignore files by default; use `--no-ignore` or `--hidden` only when
that is intentional.

### 3. Tighten against positive and negative examples

Keep a small fixture containing:

- code that must match;
- the nearest lookalikes that must not match;
- multiline and nested variants where relevant;
- comments or strings containing the same text.

Run the query after every meaningful constraint. A query that finds the
positive example is only half-tested; false positives are usually the harder
failure mode.

### 4. Decide whether it remains a search or becomes a rule

For a one-off investigation, keep using `--query` or `--query-file`. For policy
that should run repeatedly, turn the pattern into a tree-police rule and add
fixtures before relying on it in CI.

## Query syntax

A pattern names a node and optionally constrains fields or children:

```scheme
(call
  function: (identifier) @fn
  arguments: (argument_list (string)? @first_arg))
```

- `@name` captures a node. A query needs a capture to produce output.
- `field_name:` constrains which child occupies a grammar field.
- `!field_name` requires that a node lacks that field.
- `_` matches any node; `(_)` matches any named node.
- Anonymous keywords and punctuation are quoted, such as `"return"` or `"!="`.
- `*`, `+`, and `?` mean zero-or-more, one-or-more, and optional.
- `( ... )` groups a sibling sequence; groups can be quantified.
- `[ (type_a) (type_b) "keyword" ]` is alternation.
- `.` anchors named-child adjacency: leading means first, trailing means last,
  and between patterns means immediate siblings.

Predicates filter structural matches by captured text:

```scheme
((identifier) @constant
  (#match? @constant "^[A-Z][A-Z0-9_]+$"))

((identifier) @builtin
  (#any-of? @builtin "eval" "exec"))
```

Common predicates are `#eq?`, `#not-eq?`, `#match?`, `#not-match?`,
`#any-of?`, and `#not-any-of?`. Predicate support varies by query engine; do
not assume an editor-specific predicate works in tree-police.

### Capture and predicate ordering

A predicate cannot refer to a capture declared only after the predicate inside
that same node. Wrap the captured node and predicate so they are siblings:

```scheme
; Correct
((comment) @todo
  (#match? @todo "TODO|FIXME"))
```

If query compilation says `Invalid capture name`, inspect textual declaration
order before changing the grammar pattern.

## Turn a query into a tree-police rule

Place consumer-repository rules in `.tree-police/<label>-<code>.scm`. In the
tree-police source repository, embedded rules live in
`queries/<label>-<code>.scm`. The code after the final `-` routes the file to a
registered language, for example `py` or `tf`.

Each rule pattern should have exactly one reportable capture. That capture is
the stable rule ID. Prefix predicate-only captures with `_` so they are not
reported:

```scheme
(call
  function: (attribute
    object: (identifier) @_module
    attribute: (identifier) @_method)
  (#eq? @_module "os")
  (#eq? @_method "system")
  (#set! severity "error")
  (#set! category "security")
  (#set! description "os.system() invokes a shell; prefer subprocess")
) @os_system_usage
```

Rule metadata:

- `severity`: `error`, `warning`, or `log`; defaults to `warning` if omitted;
- `category`: optional stable grouping used by filters and configuration;
- `description`: optional explanation shown with findings and in the catalog.

Keep rule IDs and categories stable because configuration and suppression
comments can refer to them. Use `tree-police --list-rules` (or
`--format json`) to inspect the effective catalog after adding local rules.

Prefer path scoping and explicit suppressions over encoding repository layout
or exceptional files into an AST query. `.tree-police/config.toml` can select
rule IDs, `category:<name>`, or `file:<query-file>` for paths, exclusions, and
severity overrides. Source comments can use `tree-police: ignore`,
`ignore-next-line`, or `ignore-file` for reviewed exceptions.

## Test reusable rules

In the tree-police repository:

1. Add positive and negative examples next to each other under
   `tests/query-cases/<language>/`.
2. Put `# <- @rule_id` on the line immediately below a positive Python
   example. Unannotated lines implicitly assert that no rule should fire.
3. Run `just query-test` while iterating.
4. Run `just test` before considering the rule complete.

The query-case harness checks both directions: every annotation must match and
every actual capture must have an annotation. Because all rules for a language
run against every fixture, examples can accidentally trigger a different rule;
treat that as useful cross-rule feedback rather than weakening the harness.

For a consumer repository without that harness, keep a dedicated fixture and
run both the ad-hoc query and the normal ruleset scan over it. Verify the normal
scan emits the intended rule ID, severity, category, and description.

## Tree-police engine gotchas

- Use the predicates supported by tree-police's matcher. Non-core predicates
  such as `#not-has-ancestor?` can make the entire match disappear rather than
  being ignored.
- Rust regex syntax has no lookahead or lookbehind. Combine `#match?` and
  `#not-match?` predicates instead.
- Text predicates see literal node text. They cannot semantically evaluate
  interpolated strings or computed expressions.
- Helper captures starting with `_` and `#set!` metadata are conventions with
  runtime meaning in tree-police, not decorative naming.
- A reusable rule gets one non-helper capture per pattern; ad-hoc search may
  intentionally expose several captures.
- Query filenames must end in a registered language code. A typo is a hard
  load error rather than a silently skipped rule.

## References

When working inside the tree-police repository, prefer its current docs over a
copied summary:

- `docs/writing-queries.md` for syntax and interactive query examples;
- `queries/README.md` for rule-file conventions;
- `AGENTS.md` for matcher gotchas and the query-case test workflow;
- `src/registry.rs` for registered names, codes, and extensions.

External references:

- [Tree-sitter query syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/)
- [Tree-sitter playground](https://tree-sitter.github.io/tree-sitter/7-playground.html)
- [ast-grep pattern syntax](https://ast-grep.github.io/guide/pattern-syntax.html)
