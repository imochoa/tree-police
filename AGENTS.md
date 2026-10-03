# AGENTS.md (tree-police)

Guide for AI coding agents working on `tree-police`. Human docs live in
[`README.md`](README.md) and [`queries/README.md`](queries/README.md); this
file is the fast path.

## What this is

A declarative linter, embeddable as a pre-commit `docker_image` hook: `.scm`
query files in `queries/` are compiled and embedded into the `tree-police`
binary at build time (`include_dir!` in `src/rules.rs`). Parses each file
once, in parallel (`ignore` + `rayon`), with severity levels and
pretty/JSON/JSONL output.

Rules currently cover Python and Terraform/HCL. `src/registry.rs` also bakes
in 91 other grammars (93 languages total) with no rule files yet — every
tree-sitter grammar crate on crates.io compatible with this crate's pinned
`tree-sitter` core version (see the comment above `registry::LANGUAGES` for
what's excluded and why). A new rule file is all that's needed to start
covering one of them, no Cargo/registry change required, and a pre-commit
hook can pass every changed file without its own file-type allowlist
(unrecognized extensions are just skipped).

## Getting started

```bash
direnv allow   # or: nix develop
```

The dev shell provides the Rust toolchain (`rustc`, `cargo`, `clippy`,
`rustfmt`), a C/C++ compiler (the grammar crates compile their grammars via
their own `build.rs`), and `just`.

## Commands

All commands are recipes in the [`justfile`](justfile). Exit codes: `0` =
no findings, `1` = findings detected (suitable for CI gates), `2` = internal error.

| Command | Purpose |
|---|---|
| `just build` | `cargo build --release`. |
| `just scan-bin [args]` | Run `tree-police`. Args pass through, e.g. `just scan-bin --format json src/`. |
| `just query-test` | `cargo test --test query_cases` — per-rule annotated fixtures; see below. |
| `just test` | `cargo test` — unit tests + black-box CLI tests (`tests/cli.rs`) + query-case tests. |
| `just docker-build` | `nix build .#tree-police-static`, then `podman build` a `FROM scratch` image. |
| `just docker-run [args]` | Run the built image (`-t` for color; see the recipe comment). |

## Layout

```
.
├── flake.nix                 # devShell: Rust toolchain; tree-police-static (musl) package
├── Cargo.toml / Cargo.lock    # crate manifest (lockfile committed)
├── Dockerfile / .dockerignore
├── .pre-commit-hooks.yaml     # makes this repo usable as a pre-commit `repo:` source
├── .github/workflows/         # builds + publishes the image to ghcr.io on push/tag
├── src/                       # main, registry, rules, scan, report, severity
├── tests/cli.rs               # black-box CLI tests
├── tests/query_cases.rs       # per-rule annotated fixture tests (see below)
├── .envrc                     # direnv: `use flake`
├── justfile                   # build / scan-bin / test / docker-* commands
├── queries/                   # flat: <label>-<code>.scm, see queries/README.md
│   ├── rules-py.scm           # categories: forbidden, security, temporal, web
│   └── rules-tf.scm           # categories: naming, conventions, security
├── tests/fixtures/            # whole-file good/bad samples for tests/cli.rs
│   ├── python/
│   └── terraform/
├── tests/query-cases/         # per-rule annotated fixtures for `just query-test`
│   └── python/
├── README.md
└── queries/README.md          # rule conventions
```

## Adding a rule

1. Add a pattern to `queries/<label>-<code>.scm` (`<code>` from
   `src/registry.rs`, e.g. `py`, `tf`); embedded on the next `cargo build`.
2. Give each pattern exactly **one** non-`_` `@capture` — it is the rule ID in output (e.g. `@s3_bucket_missing_sf_prefix`). Prefix every predicate-only helper capture with `_` (e.g. `@_block_type`) so `tree-police` doesn't report it. Declare severity with `(#set! severity "error|warning|log")` (defaults to `warning`); optionally add `(#set! category "…")` (groups `--list-rules` output) and `(#set! description "…")` (shown per-finding and in `--list-rules`).
3. Add fixtures under `tests/fixtures/<language>/`:
   - `*_violations.<ext>` — code that must trigger captures.
   - `clean.<ext>` — code that must produce zero captures.
4. Add an assertion to `tests/cli.rs` for the new rule.
5. Add a `tests/query-cases/<language>/<topic>.<ext>` fixture (see below) and run `just query-test`.

## Query-case tests

`tests/query-cases/<lang>/<topic>.<ext>` are annotated fixtures, grouped by
topic/category rather than by `.scm` file (there's one `.scm` file per
language), checked by `tests/query_cases.rs` (`just query-test`, wired into
`just test`). A comment names the capture expected on the line directly
above it:

```python
print("debug")
# <- @forbidden_print
```

Unlike `tree-sitter query --test` (same comment syntax, but it mis-pairs
assertions once a query file defines more than one distinct capture name —
confirmed by testing, not a guess), this harness checks each assertion
against the actual capture spans directly, and — the other direction —
fails if any capture fires on a line with no matching assertion. That gives
both "should match" and "should not match" coverage from one file, which
whole-file `tests/fixtures/*` checks don't: a rule firing on the wrong line
of a big fixture still passes a file-level `assert_clean`/`assert_match`.

An unannotated line is an *implicit* "nothing should fire here" assertion —
there's no separate negative-assertion syntax, and no need for one. Put each
rule's positive example directly above its negative counterexample (not in
a separate "clean" section at the bottom); see any file under
`tests/query-cases/python/` for the pattern.

It runs the **compiled `tree-police` matcher** directly (via
`tree_police::rules::load()`), not a subprocess. It caught a real bug when
this engine's rule files were consolidated to one per language: because
every rule now applies to every fixture, a rule's negative example in one
topic's fixture started tripping an unrelated rule from another category —
see the cross-category caveat in `queries/README.md`.

## Adding a language

1. Add the `tree-sitter-<lang>` crate to `Cargo.toml` and one `LangSpec`
   entry (`name`, `code`, `extensions`, `LANGUAGE.into()`) to
   [`src/registry.rs`](src/registry.rs).
2. Add `queries/<label>-<code>.scm` rule file(s), matching the new `code`.
3. Add fixtures and `tests/cli.rs` assertions as above.

Adding just the grammar + registry entry with no rule file yet is fine and
already done for several languages (see "What this is") — it costs nothing
at scan time (files with no rules for their language are skipped) and means
a future rule file needs no Cargo/registry change.

## Gotchas

- **No regex lookaheads.** Rust's `regex` crate rejects `(?!...)` / `(?=...)`. Split into separate `#not-match?` / `#match?` predicates on the same capture instead.
- **A predicate can't reference the capture on its own enclosing node.** A node's capture is only declared at its closing paren, so a predicate that's one of that node's *own children* comes before the capture exists, textually — `(node (child) (#pred? @x) ) @x` fails to compile ("Invalid capture name"). Wrap the captured node and the predicate together in one more pair of parens instead — `((node (child)) @x (#pred? @x ...))` — so they become siblings at the outer level, where ordering works. See `missing_route_validation` in `queries/rules-py.scm` (and `bare_except`/`todo`, which use the same wrapping).
- **Only literal strings are visible to queries.** Interpolated expressions (`"${var.env}-thing"`) aren't matched against string predicates — those need runtime validation elsewhere.
- **`#set!` severity + `_`-helper captures are load-bearing.** The one non-`_` capture per pattern is the reported rule ID; severity comes from `(#set! severity …)` (default `warning`).
- **Non-core predicates don't just go unenforced — they zero out the whole match.** `#not-has-ancestor?` and similar are silently treated as unsatisfied by the embedded `tree_sitter::QueryCursor` matcher, dropping the match entirely rather than ignoring the predicate. Confirmed by testing (`tests/query_cases.rs`), not documented upstream. Avoid them.
- **Rule IDs and `category` values are stable identifiers** — referenced by CI/config elsewhere, don't rename without a coordinated change.
- **A query filename's code must match a registered `LangSpec::code`, or the binary fails to build the ruleset** — both for embedded queries and for `--rules-dir` repo-local ones. This is deliberate (fail fast on a typo) rather than silently dropping the file.
- **`--list-rules`'s catalog-building text scan skips `;` comments and `"..."` string literals, nothing else.** `rules.rs::primary_capture_in` recovers a pattern's rule ID by scanning the raw source slice from one pattern's start byte to the *next* pattern's start byte for the last `@word`-shaped mention. Two confirmed bugs from this, both fixed with regression tests in `rules.rs`: a trailing comment reading "isn't @activity/@workflow decorated" got its `@workflow` mistaken for the *preceding* pattern's rule ID (misreported `os_popen_usage` as `workflow`); a `(#not-match? ... "@validate\\(")` regex argument got its `@validate` mistaken the same way (misreported `missing_route_validation` as `validate`). Both are skipped now — but nothing else is, so a future `@word`-shaped mention anywhere else non-obvious could reproduce this. If `--list-rules` ever shows a rule ID that doesn't match the capture you wrote, check here first.

## Conventions when editing queries

- Prefer declarative predicates (`#eq?`, `#match?`, `#not-match?`) over broad structural matches.
- Capture names are the rule ID surfaced to users — make them descriptive (`hardcoded_provider_credentials`, not `bad1`).
- Keep one logical rule per capture name; combine predicates rather than duplicating patterns.
- Terraform rules are illustrative examples of naming/convention/security checks (e.g. a placeholder `acme-` bucket prefix, `-role`/`-sg` suffixes) — swap them for your own org's conventions rather than assuming they apply as-is.

## Credits

This crate's Rust architecture (parallel `ignore`+`rayon` scanning,
git-ignore awareness, embedded-grammar-per-language design, the `_`-prefix
capture convention) is built on and adapted from
[tree-grepper](https://github.com/BrianHicks/tree-grepper) (archived) by
Brian Hicks — it is the source and foundation for most of `src/`'s shape.
Note tree-grepper is Hippocratic-licensed, not standard OSS; see the
"Credits"/"License" sections in [`README.md`](README.md).

## References

- [`README.md`](README.md) — setup, pre-commit quickstart, repo-local rules, and credits/license.
- [`queries/README.md`](queries/README.md) — rule conventions.
- [Tree-sitter query syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/)
- [tree-sitter-python grammar](https://github.com/tree-sitter/tree-sitter-python)
- [tree-sitter-hcl grammar](https://github.com/tree-sitter-grammars/tree-sitter-hcl)
