# Using tree-police as a pre-commit hook

A worked example for a repo that wants `tree-police` to scan every commit.
This is the consumer's side — see the top-level [`README.md`](../README.md)
for how the wheel and image get built and published.

## 1. Minimal setup

In the repo you want scanned, add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/imochoa/tree-police
    rev: v0.2.0 # pin a tag -- see https://github.com/imochoa/tree-police/releases
    hooks:
      - id: tree-police
```

Then, as usual:

```bash
pre-commit install # one-time, per clone
pre-commit run --all-files # first run: scan everything, not just staged changes
```

`pre-commit` (or prek) installs the prebuilt `tree-police` wheel from PyPI
into the hook's Python environment (Linux and macOS, amd64 and arm64; no
Docker or Rust toolchain needed) and runs it against every file pre-commit
hands it, which by default is every file changed in the
commit. Files whose extension isn't registered in
[`src/registry.rs`](../src/registry.rs) are silently skipped by the binary
itself, so you don't need `files:`/`types_or:` filtering in your own config.

## 2. A commit that trips a rule

Given a staged file like:

```python
# app.py
import os

def run_migration():
    os.system("psql -f migration.sql")
```

```console
$ git commit -m "add migration runner"
tree-police (AST pattern scan)...........................................Failed
- hook id: tree-police
- duration: 0.42s
- exit code: 1

app.py:5:5: error os_system_usage  os.system() invokes a shell; prefer subprocess

Summary: 1 error(s), 0 warning(s), 0 log(s) across 1 file(s) (scanned 1).
```

The hook's default `entry` (see
[`.pre-commit-hooks.yaml`](../.pre-commit-hooks.yaml)) passes
`--fail-on error --format compact` with `verbose: true`: only
`error`-severity findings block the commit, and `warning`/`log` findings are
printed one line each on every run, even when the hook passes. Your `args:`
are appended after those flags and a later flag wins, so override only what
you want to change:

```yaml
repos:
  - repo: https://github.com/imochoa/tree-police
    rev: v0.2.0
    hooks:
      - id: tree-police
        args: ["--fail-on", "warning", "--format", "pretty"]
```

Set `verbose: false` on the hook to only see output when it fails.

To adopt the hook on a codebase with many existing findings, commit a
baseline (`tree-police --update-baseline .`, see the README's "Baseline"
section) so only new findings show up.

## 3. Adding your own rules, no fork required

Drop a `<label>-<code>.scm` file (same naming convention as
[`queries/`](../queries/README.md)) under a `.tree-police/` directory at your
repo's root:

```scheme
; .tree-police/no-print-py.scm
(call
  function: (identifier) @no_print
  (#eq? @no_print "print")
  (#set! severity "warning")
  (#set! category "local")
  (#set! description "Use the project logger, not print()"))
```

No `.pre-commit-config.yaml` changes needed: the default `--rules-dir`
(`.tree-police`, relative to the repo root pre-commit mounts into the
container) picks this up automatically and merges it with the embedded
ruleset at scan time. Commit the `.tree-police/` directory like any other
repo config.

## 4. Scoping to specific rules or categories

`--rule <id>` and `--category <name>` are repeatable and filter which
findings get reported (run `tree-police --list-rules` to see valid values
for the embedded ruleset, or check your own `.tree-police/*.scm` files'
`(#set! category "…")` tags):

```yaml
        args: ["--category", "security", "--fail-on", "error"]
```

## Troubleshooting

- **Testing an unreleased checkout with `pre-commit try-repo`** — the
  `tree-police` hook installs the root `pyproject.toml` stub, which pins
  `tree-police==<version>` from PyPI, so a version that isn't published yet
  won't resolve. Build a wheel (`just wheel` in this repo) and point pip at
  it: `PIP_FIND_LINKS=/path/to/tree-police/target/wheels pre-commit try-repo
  /path/to/tree-police tree-police --all-files` (prek uses uv, so set
  `UV_FIND_LINKS` instead).
- **A brand-new tag fails to install** — the hook can only install a version
  once `pypi-release.yml` has published its wheels, which takes a few
  minutes after the tag is pushed. Re-run once the release shows up on
  PyPI.
- **Using the Docker hook (`id: tree-police-docker`)** — pre-commit's
  `docker_image` language mounts your repo at `/src` and runs with that as
  the working directory; nothing to configure. To try a local image instead
  of the published one, override `entry:` with an image you've built
  yourself (`just docker-build` in this repo).
- **Exit code 1 but no findings printed** — check `--min-severity`; findings
  below it are silently dropped from output but can still factor into
  `--fail-on` if you've also loosened that (unusual combination, but worth
  checking if the two seem inconsistent).
