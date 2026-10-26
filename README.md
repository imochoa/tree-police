<img src="docs/logo.jpg" alt="tree-police logo" width="160" align="right">

# tree-police

A ripgrep-style linter that patrols source code for forbidden AST shapes,
using [tree-sitter](https://tree-sitter.github.io/) queries baked into a
single static binary. Built to run as a [pre-commit](https://pre-commit.com/)
hook (a prebuilt PyPI wheel, or a Docker image) — no Rust/Nix/tree-sitter
toolchain required in the repos it scans.

Adapted from an internal hackathon proof of concept.

## Credits

Most of this crate's Rust architecture is built on top of, and adapted from,
[**tree-grepper**](https://github.com/BrianHicks/tree-grepper) (archived) by
Brian Hicks — a general-purpose structural code search tool that this project
turns into a rule-bundling linter. tree-grepper is the source and foundation
for the parallel single-file-per-thread scan (`ignore` + `rayon`), git-ignore
awareness via the `ignore` crate, the embedded-grammar-per-language approach,
and the `_`-prefix-for-internal-captures capture convention used throughout
`src/`.

**License note:** tree-grepper is licensed under the
[Hippocratic License 2.1](https://github.com/BrianHicks/tree-grepper/blob/main/LICENSE),
not a standard OSS license — it carries its own attribution and
notice-of-changes requirements. No source from tree-grepper is copied into
this repo (the architecture is reimplemented independently, following its
design), so this project's own `MIT OR Apache-2.0` license is unaffected, but
the design debt to tree-grepper is real and this note exists so it stays
visible.

## Pre-commit quickstart

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/imochoa/tree-police
    rev: v0.3.0  # pin a tag; see releases
    hooks:
      - id: tree-police
```

This installs the prebuilt `tree-police` wheel from PyPI into the hook's
environment, so it only needs Python: no Docker and no Rust toolchain. It
works with both pre-commit and [prek](https://github.com/j178/prek). The hook
then runs against every changed file in the commit; files whose extension
isn't registered (see `src/registry.rs`) are silently skipped. For the Docker
image instead, use `id: tree-police-docker`.

Only `error` findings block the commit, but the hook always prints every
finding, one line each (`--format compact`, `verbose: true`). `args` are
appended to the hook's own flags and a later flag wins, so
`args: ["--fail-on", "warning"]` or `args: ["--format", "pretty"]` override the
defaults.

See [`docs/pre-commit-example.md`](docs/pre-commit-example.md) for a full
worked example: expected output on a failing commit, scoping to specific
rules/categories, and troubleshooting.

## Standalone CLI

Install it from PyPI (the `tree_police` wheel built from
`python/pyproject.toml`). The wheel only wraps the native binary, so there's no
Python code involved and no Rust toolchain is needed:

```bash
uv tool install tree-police    # or: pipx install tree-police / pip install tree-police
tree-police --list-rules
```

Wheels are built for Linux (x86_64/aarch64, glibc and musl) and macOS
(Intel/Apple Silicon). On any other platform pip falls back to the sdist,
which needs Rust and a C/C++ compiler to build.

Or download a binary directly. Every
[release](https://github.com/imochoa/tree-police/releases) attaches prebuilt
binaries for Linux (amd64/arm64, static musl) and macOS (Intel/Apple
Silicon), plus a checksums file and a CycloneDX SBOM. Every grammar and query
file is statically linked into these same binaries — there's no separate
grammars folder to download or manage, on any platform.

```bash
curl -LO https://github.com/imochoa/tree-police/releases/latest/download/tree-police-aarch64-apple-darwin.tar.gz
tar xzf tree-police-aarch64-apple-darwin.tar.gz
./tree-police --list-rules
```

(swap the filename for your platform: `x86_64-unknown-linux-musl`,
`aarch64-unknown-linux-musl`, `x86_64-apple-darwin`, or
`aarch64-apple-darwin`.)

## Repo-local rules

Drop extra `<label>-<code>.scm` files (same convention as `queries/`, see
below) in a `.tree-police/` directory at your repo root and they're merged
with the built-in ruleset automatically — no fork, no image rebuild:

```scheme
; .tree-police/no-foo-py.scm
((identifier) @no_foo
  (#eq? @no_foo "foo")
  (#set! severity "warning")
  (#set! description "Don't use the identifier `foo`"))
```

Override the lookup path with `--rules-dir <dir>` if you'd rather keep it
somewhere else.

A repo-local rule with the same ID as a built-in one replaces it: the built-in
version stops running, so you can tighten or loosen a built-in rule without
getting every finding twice. [`examples/tree-police/`](examples/tree-police/)
has a starter set (an org bucket prefix and a role-name suffix, which used to
be built in) and an example config.

## Config file

By default every rule runs on every file of its language. A
`.tree-police/config.toml` (next to any repo-local rules) restricts where
rules run and overrides their severity per path:

```toml
# Never scan these files at all (skipped before parsing).
exclude = ["vendor/", "archive/**"]

# Turn rules off everywhere.
disable = ["nested_ternary", "category:web"]

# Only run the temporal rules under src/workflows/.
[[scope]]
rules = ["category:temporal"]
paths = ["src/workflows/"]

# Run forbidden_print everywhere except tests and scripts.
[[scope]]
rules   = ["forbidden_print"]
exclude = ["tests/**", "scripts/"]

# Downgrade security findings to `log` in legacy code (still reported).
[[override]]
rules    = ["category:security"]
paths    = ["legacy/**"]
severity = "log"
```

- Top-level keys (`exclude`, `disable`) must come before any `[[scope]]` or
  `[[override]]` table, as TOML requires.
- `rules` and `disable` take rule IDs, `category:<name>`, or `file:<query-file-name>`
  (e.g. `file:no-foo-py.scm`). Unknown names are an error (exit 2), so a
  typo can't silently match nothing; see `--list-rules`.
- **`[[scope]]`**: `paths` is an allowlist and `exclude` a denylist (at
  least one of them is required). If several scopes select the same rule,
  a finding has to pass all of them.
- **`[[override]]`**: `severity` is required. `paths`/`exclude` pick which
  files the override covers, but they never stop a rule from running. When
  several overrides match, the last one wins.
- Languages are handled for you: a rule only ever runs on files of its own
  language, so globs don't need extensions (`src/**` is enough). A glob
  without `**` also matches everything under it, like `.gitignore` (`tests`
  = `tests/` = `tests/**`).
- Globs are relative to the directory that *contains* `.tree-police/`
  (normally the repo root), whatever directory you run from. If you pass
  `--config FILE` instead, globs are relative to `FILE`'s directory.
- The config only filters findings. Every rule still runs, so `--rule`,
  `--category` and `--min-severity` still apply on top of it.

## Suppressing findings

For one-off exceptions, put a directive in a comment. It works in any
language's comment syntax:

```python
print(x)  # tree-police: ignore[forbidden_print] -- CLI entry point

# tree-police: ignore-next-line[category:security]
eval(trusted_expr)

# tree-police: ignore-file[todo]
```

| Directive | Suppresses findings… |
| --- | --- |
| `tree-police: ignore` | that start on the comment's own line |
| `tree-police: ignore-next-line` | that start on the line after the comment |
| `tree-police: ignore-file` | anywhere in the file, wherever the comment is |

The `[...]` list takes the same selectors as the config file. Leave it out
to suppress every rule. Anything after the directive, such as a reason, is
ignored. Only real comments count, so the same text inside a string literal
does nothing. An unknown selector prints a warning on stderr and suppresses
nothing; it never falls back to "every rule". The pretty report's summary
says how many findings were suppressed.

## Baseline

To adopt a rule on a codebase that already has hundreds of findings, record
them once and only report new ones:

```bash
tree-police --update-baseline .     # writes .tree-police/baseline.json; commit it
tree-police .                       # known findings hidden ("N baselined")
tree-police --no-baseline .         # everything, e.g. for a cleanup pass
```

A finding is matched by file, rule and the text of its source line (trimmed),
not by line number, so editing code above it doesn't resurface it; a second
copy of the same line does show up. `--update-baseline` on a subset of files
(say, the ones pre-commit passes) replaces only those files' entries. Use
`--baseline FILE` for another location; its paths are then relative to that
file's directory.

## Testing repo-local rules

Put annotated fixtures under `.tree-police/tests/` and run
`tree-police --test` (or `--test DIR`). A `<- @rule_id` comment, in any line
comment syntax, says that rule must fire on the line directly above:

```python
# .tree-police/tests/no-foo.py
foo()
# <- @no_foo
bar()            # no annotation: no_foo must not fire here
```

It fails on a missing match and on an unannotated match of any repo-local rule
(or any rule the fixture asserts), so one file covers both "should match" and
"should not match". Built-in rules you don't assert are ignored. Normal scans
skip `.tree-police/tests/` even when files there are passed explicitly, since
fixtures break rules on purpose. Exit code 1 means a failure.

## Output formats

`--format pretty` (default, grouped by file with source excerpts), `compact`
(one `path:line:col: severity rule  description` line per finding),
`summary` (finding and file counts per rule, most frequent first; handy for
sizing up a rollout), `json` and `jsonl`.

## What's covered

Rules exist today for **Python** (`forbidden`, `security`, `temporal`, `web`,
`occt` categories) and **Terraform/HCL** (`naming`, `conventions`, `security`). Both
rule sets are a small illustrative starting point, not an exhaustive policy —
extend them or add your own via a repo-local `.tree-police/` directory (see
above).

The binary also bakes in grammars for **93 languages total** with no rules
yet beyond those two — every tree-sitter grammar crate on crates.io whose
`tree-sitter` core dependency is compatible with this crate's pinned version,
plus a vendored grammar for [Talon](https://talonvoice.com) `.talon` files
(from [wenkokke/tree-sitter-talon](https://github.com/wenkokke/tree-sitter-talon))
(a handful of popular languages are excluded because their published crate's
`tree-sitter` pin can't coexist with ours in one binary, or because the
published crate itself is broken — see the comment above
`registry::LANGUAGES` for specifics and how to recheck). Covering a new
language is just a new query file, no Cargo/registry change; unrecognized
extensions are silently skipped, so a pre-commit hook can pass every changed
file with no file-type allowlist of its own.

`ada`, `adl`, `agda`, `bash`, `bicep`, `c`, `clojure`, `cmake`, `commonlisp`,
`cpp`, `crystal`, `csharp`, `css`, `cuda`, `d`, `dart`, `dbscheme`,
`devicetree`, `diff`, `elisp`, `elixir`, `elm`, `erb`, `erlang`, `fortran`,
`fsharp`, `gdscript`, `gleam`, `glsl`, `go`, `godotresource`, `graphql`,
`groovy`, `haskell`, `heex`, `html`, `ini`, `java`, `javascript`, `jinja2`,
`json`, `jsonnet`, `julia`, `kdl`, `llvm`, `lua`, `luau`, `make`, `matlab`,
`netlinx`, `nim`, `nix`, `objc`, `ocaml`, `ocamllex`, `odin`, `pascal`, `pgn`,
`php`, `powershell`, `prolog`, `properties`, `proto`, `python`, `ql`,
`qmljs`, `r`, `razor`, `rst`, `ruby`, `rust`, `scala`, `scheme`, `slang`,
`slint`, `solidity`, `sparql`, `swift`, `t32`, `templ`, `tera`, `terraform`,
`tlaplus`, `tsx`, `typescript`, `vcl`, `verilog`, `vhdl`, `wesl`, `xml`,
`yaml`, `zig`.

This list (name, registry code, file extensions, and the grammar crate
behind each) lives in one place: [`src/registry.rs`](src/registry.rs) — treat
it, not this README, as the source of truth, since it's regenerated from
code rather than hand-maintained prose. Run `tree-police --list-rules` (or
`--list-rules --format json`) for the live *rules* catalog, grouped by
category (a much shorter list — most languages above have no rules yet).

## Query file naming

`queries/` is flat: `<label>-<code>.scm`, where `<code>` is the routing key
(`py`, `tf`, …) defined once in `src/registry.rs` and `<label>` is a free-form
readable tag. See [`queries/README.md`](queries/README.md) for the full
layout and rule-authoring conventions, and
[`src/registry.rs`](src/registry.rs) for the language list.

New to writing tree-sitter queries? See
[`docs/writing-queries.md`](docs/writing-queries.md) for a from-scratch
syntax reference (fields, captures, quantifiers, anchors, predicates) and
tips for inspecting a file's AST before writing one.

An Agent Skills-compatible workflow for structural searches and rule authoring
is also available at
[`skills/tree-sitter-queries/SKILL.md`](skills/tree-sitter-queries/SKILL.md).
Copy that directory into your agent's skill location, or point your harness at
it directly. The binary also bundles it, so `tree-police --skill` prints the
copy matching your installed version:

```bash
mkdir -p ~/.claude/skills/tree-sitter-queries
tree-police --skill > ~/.claude/skills/tree-sitter-queries/SKILL.md
```

It defaults to tree-police's built-in tree viewer and ad-hoc query
mode, then covers promoting a tested query into a reusable rule.

## Local development

```bash
direnv allow   # or: nix develop
just build                        # cargo build --release
just scan-bin .                   # scan the current tree (pretty report)
just scan-bin --format json src/  # machine-readable output
just scan-bin --list-rules        # embedded rule catalog, grouped by category
just test                         # unit + black-box CLI + query-case tests
just docker-build                 # nix build .#tree-police-static, then podman build
just docker-run .                 # run the built image against a mounted dir
```

Key flags: `--format pretty|json|jsonl`, `--min-severity log|warning|error`,
`--rule <id>` / `--category <name>` (repeatable, scope to specific
rules/categories — see `--list-rules` for valid values), `--rules-dir <dir>`
(default `.tree-police`), `--fail-on none|log|warning|error` (exit-code
threshold, default `warning`), `--show-tree <file>` (print an indented AST
instead of scanning — see [`docs/writing-queries.md`](docs/writing-queries.md)),
`--no-ignore`, `--hidden`, `-j/--threads`, `-v/-vv`. Exit codes: `0` = no
findings at/above `--fail-on`, `1` = findings at/above the threshold, `2` =
error.

### Ad-hoc queries

`--query '<scm>'` (or `--query-file <path>`) runs a one-off tree-sitter
query instead of the embedded ruleset — no `.scm` file to write first.
Language is inferred from `paths` when that's unambiguous (a single file,
or a directory that's all one language); pass `--lang <name>` yourself for
a mixed-language directory:

```console
$ tree-police --query '(call_expression function: (identifier) @fn)' src/main.rs
src/main.rs
  117:23  @fn
   117 |     if let Err(err) = init_tracing(args.verbose) {
                               ^^^^^^^^^^^^

  121:11  @fn
   121 |     match run(args) {
               ^^^
[... more matches ...]

Summary: 16 match(es) in 1 file(s) (scanned 1).
```

Unlike ruleset scanning, this reports **every** non-`_` capture per match
(tree-grepper's convention — see [`docs/writing-queries.md`](docs/writing-queries.md)),
not one rule-id capture per pattern, and it's a search, not a lint gate: exit
code is always `0`, and `--fail-on`/`--min-severity`/`--rule`/`--category`
don't apply (none have meaning without a severity/rule-id/category).
`--format json`/`jsonl` both work, for scripting.

See [`AGENTS.md`](AGENTS.md) for the full agent-facing reference (adding a
rule, adding a language, query-authoring gotchas).

### This repo's own pre-commit checks

Separate from `.pre-commit-hooks.yaml` (which makes *this repo* usable as a
hook source for others), [`.pre-commit-config.yaml`](.pre-commit-config.yaml)
lints tree-police's own Rust, Nix, and Markdown, running every tool straight
off the Nix devShell's PATH (`language: system` — no separate per-hook
install):

```bash
direnv allow   # or: nix develop -- provides cargo fmt/clippy, alejandra, statix, deadnix, markdownlint-cli2, pre-commit itself
pre-commit install          # one-time
pre-commit run --all-files  # cargo fmt --check, cargo clippy --deny warnings, alejandra --check, statix, deadnix, markdownlint-cli2
```

`.markdownlint-cli2.jsonc` disables line-length (`MD013`, docs have long
inline links/code) and excludes the generated `THIRD-PARTY-LICENSES.md`.

## Publishing

`.github/workflows/release.yml` cross-compiles musl-static binaries for both
`x86_64` and `aarch64` via Nix (`nix build .#tree-police-static-x86_64` /
`.#tree-police-static-aarch64` — no QEMU, these are genuine cross builds, not
emulated), then publishes a single multi-arch `ghcr.io/imochoa/tree-police`
manifest (`linux/amd64` + `linux/arm64`) via `docker buildx` on every push to
`main` (tag `latest`) and version tag (tag `vX.Y.Z`). `docker pull`/pre-commit
picks the right architecture automatically — nothing arch-specific to
configure on the consuming side.

`.github/workflows/cli-release.yml` runs only on a version tag (a GitHub
Release is a deliberate act, unlike `:latest`'s continuous republishing) and
attaches the standalone binaries described above: the same Nix musl builds
for Linux, plus two native `cargo build --release` jobs on GitHub's Intel and
Apple Silicon macOS runners (no cross-compiling or universal binaries — each
runner just builds for itself), a `checksums.txt`, and the CycloneDX SBOM.

[`renovate.json`](renovate.json) keeps the Rust (`Cargo.lock`), Nix
(`flake.lock` — beta Renovate manager, needs a `nix` binary on the Renovate
runner to actually rewrite the lock file, not just open a stale-looking PR),
and Docker/GitHub Actions build tooling versions current.

## SBOM

`nix build .#sbom` (or `just sbom`) reproducibly generates a CycloneDX SBOM
of the exact dependency set in `Cargo.lock`, using
[`cargo-cyclonedx`](https://github.com/CycloneDX/cyclonedx-rust-cargo). The
release workflow builds it the same way and attaches it to the published
image as an OCI referrer via `cosign attach sbom` — pull it with
`cosign download sbom ghcr.io/imochoa/tree-police@<digest>` (or any
OCI-referrers-aware scanner) without needing a separate download URL.

## License

Dual-licensed [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE), at your
option — the Rust ecosystem convention, and compatible with every dependency
in `Cargo.lock` (all MIT, Apache-2.0, Unicode-3.0, or Unlicense; none copyleft
— see `about.toml`'s `accepted` list). [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md)
bundles every dependency's license text, generated with
[`cargo-about`](https://github.com/EmbarkStudios/cargo-about) via
`just third-party-licenses` — regenerate it after any dependency change
(`cargo about` fails the build if a new dependency's license isn't in
`about.toml`'s accepted list, so this can't silently drift).

See "Credits" above for the separate, non-dependency question of
tree-grepper's Hippocratic License.
