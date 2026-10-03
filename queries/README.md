# Query directories

This directory stores the Tree-sitter query rules embedded into the
`tree-police` binary at compile time.

## Layout

Flat, one `.scm` file per language today, named `<label>-<code>.scm`:

- `<label>` is a free-form, human-readable tag with no meaning to the loader
  (today just `rules`, since each language's rules still live in one file).
- `<code>` is the routing key — everything after the **last** `-` in the
  filename stem — looked up against `src/registry.rs`'s `LangSpec::code`.
  A file named `foo.scm` with no `-` at all is treated as code `foo`.

```
rules-py.scm   # all active Python rules (registry code "py")
rules-tf.scm   # all active Terraform/HCL rules (registry code "tf")
```

Rules within each file are classified with a `(#set! category "...")` tag
rather than being split across files — see "Rule conventions" below.

`src/registry.rs` registers 93 languages total, far more than have rule files
today — essentially every tree-sitter grammar crate on crates.io compatible
with this project's pinned `tree-sitter` version, baked into the binary so
adding a new rule file is all that's needed to start covering one of them; no
Cargo/registry change required. Run
`tree-police --list-rules` for the up-to-date catalog of languages that
actually have rules (grouped by category) — that listing, not this file, is
the source of truth for "what rule enforces what."

### Repo-local rules

A repo being scanned can add its own `<label>-<code>.scm` files under a
`.tree-police/` directory (configurable via `--rules-dir`); they're merged
with the embedded set at scan time, same convention, same validation. See the
top-level README's "Repo-local rules" section.

## Rule conventions

- Keep rule IDs (the trailing non-`_` capture name) and `category` values
  stable — they're referenced by CI/config elsewhere.
- **One reportable capture per pattern.** The single capture whose name does *not*
  start with `_` is the rule ID (for example `@hardcoded_credential`). Give it a
  meaningful name.
- **Helper captures start with `_`.** Any capture that exists only to drive a
  predicate (`@_block_type`, `@_attr`, `@_bucket_name`, …) must be prefixed with
  `_`. `tree-police` never reports helper captures.
- **Declare severity per pattern** with a `(#set! severity "error|warning|log")`
  directive; omitting it defaults to `warning`.
- **Optionally add `(#set! category "…")`** — a free-form grouping tag
  (`"security"`, `"naming"`, …). This is what organizes `--list-rules`
  output within a language's rule file(s).
- **Optionally add `(#set! description "…")`** for a human-readable
  explanation — shown both per-finding in reports and in the `--list-rules`
  catalog (`tree-police --list-rules --format json` exports the full catalog
  as structured JSON, including categories and descriptions).
- Prefer declarative predicates (`#eq?`, `#match?`, `#not-match?`) over broad matches.
- Only literal string values are checked; interpolated expressions need runtime validation.

Example (Python `print` — a warning):

```scheme
(call
  function: (identifier) @_func (#eq? @_func "print")
  (#set! severity "warning")
  (#set! category "forbidden")
  (#set! description "Use the logger instead of print()")
) @forbidden_print
```

**Cross-category caveat:** every rule in a language's rule file(s) applies to
every fixture scanned, including `tests/query-cases/` fixtures that only
exist to demonstrate one category. A negative example for one rule can
accidentally trip an unrelated rule from another category — e.g. a
`print(...)` call meant to show "not an eval() call" also matches the
`forbidden` category's `forbidden_print`. `tests/query_cases.rs`'s stray-match
check catches this; when it does, either pick example code that doesn't
collide, or add the extra assertion if the overlap is real rule behavior
worth documenting (see `tests/query-cases/python/temporal-patterns.py`'s
`bare_except` case for the latter).

> **Note:** `#not-has-ancestor?` and other non-core predicates are silently
> treated as unsatisfied by this binary's matcher (only
> `#eq?`/`#match?`/`#not-match?`/`#any-of?` are evaluated) — dropping the
> *whole match*, not just leaving the predicate unenforced, confirmed by
> testing. Avoid them.

## Testing rules

```bash
just test        # cargo test: unit + black-box CLI tests + query-case tests
just query-test  # per-rule annotated fixtures (tests/query-cases/) only
```

`query-test` checks the query file against fixtures annotated with
`# <- @capture_name` comments, and fails if anything fires *without* a
matching assertion too — see [`../AGENTS.md`](../AGENTS.md#query-case-tests)
for the format.

Useful references:
- [Tree-sitter query syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/)
- [Tree-sitter Python grammar](https://github.com/tree-sitter/tree-sitter-python)
- [Tree-sitter HCL grammar](https://github.com/tree-sitter-grammars/tree-sitter-hcl)
