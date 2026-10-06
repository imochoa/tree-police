<img src="docs/logo.jpg" alt="tree-police logo" width="160" align="right">

# tree-police

A ripgrep-style linter that patrols source code for forbidden AST shapes,
using [tree-sitter](https://tree-sitter.github.io/) queries baked into a
single static binary. Built to run as a [pre-commit](https://pre-commit.com/)
hook via a published Docker image — no Rust/Nix/tree-sitter toolchain
required in the repos it scans.

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
    rev: v0.1.0  # pin a tag; see releases
    hooks:
      - id: tree-police
```

This pulls `ghcr.io/imochoa/tree-police` and runs it against every changed
file in the commit; files whose extension isn't registered (see
`src/registry.rs`) are silently skipped. Override severity/format via `args`,
e.g. `args: ["--min-severity", "warning"]`.

See [`docs/pre-commit-example.md`](docs/pre-commit-example.md) for a full
worked example: expected output on a failing commit, scoping to specific
rules/categories, and troubleshooting.

## Standalone CLI

Prefer running it directly over Docker/pre-commit? Every
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

## What's covered

Rules exist today for **Python** (`forbidden`, `security`, `temporal`, `web`
categories) and **Terraform/HCL** (`naming`, `conventions`, `security`). Both
rule sets are a small illustrative starting point, not an exhaustive policy —
extend them or add your own via a repo-local `.tree-police/` directory (see
above).

The binary also bakes in grammars for **92 languages total** with no rules
yet beyond those two — every tree-sitter grammar crate on crates.io whose
`tree-sitter` core dependency is compatible with this crate's pinned version
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
