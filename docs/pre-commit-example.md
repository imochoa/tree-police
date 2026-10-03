# Using tree-police as a pre-commit hook

A worked example for a repo that wants `tree-police` to scan every commit.
This is the consumer's side — see the top-level [`README.md`](../README.md)
for how the image itself gets built and published.

## 1. Minimal setup

In the repo you want scanned, add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/imochoa/tree-police
    rev: v0.1.0 # pin a tag -- see https://github.com/imochoa/tree-police/releases
    hooks:
      - id: tree-police
```

Then, as usual:

```bash
pre-commit install # one-time, per clone
pre-commit run --all-files # first run: scan everything, not just staged changes
```

`pre-commit` pulls `ghcr.io/imochoa/tree-police:latest` (a multi-arch image —
works the same on amd64 and arm64 runners/laptops) and runs it against every
file pre-commit hands it, which by default is every file changed in the
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

app.py
  5:5  error    os_system_usage  os.system() invokes a shell; prefer subprocess

Summary: 1 error(s), 0 warning(s), 0 log(s) across 1 file(s) (scanned 1).
```

The hook's default `entry` (see
[`.pre-commit-hooks.yaml`](../.pre-commit-hooks.yaml)) passes
`--fail-on error`, so only `error`-severity findings block the commit;
`warning`/`log` findings are still printed but don't fail the hook. Override
this per-repo with `args:`:

```yaml
repos:
  - repo: https://github.com/imochoa/tree-police
    rev: v0.1.0
    hooks:
      - id: tree-police
        args: ["--fail-on", "warning", "--format", "json"]
```

(`args` here *replace* the hook's defaults, they don't merge with them —
if you add `args`, also repeat `--fail-on error` if you still want that
threshold.)

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

- **"no such file or directory" mounting the repo** — pre-commit's
  `docker_image` language mounts your repo at `/src` and runs with that as
  the working directory; this is automatic, nothing to configure. If you're
  testing locally with `pre-commit try-repo` against a local checkout of
  `tree-police` instead of the published image, pass a hook override with
  `entry: tree-police:local ...` pointing at an image you've built yourself
  (`just docker-build` in this repo), since `try-repo` doesn't publish or
  pull anything for you.
- **Exit code 1 but no findings printed** — check `--min-severity`; findings
  below it are silently dropped from output but can still factor into
  `--fail-on` if you've also loosened that (unusual combination, but worth
  checking if the two seem inconsistent).
