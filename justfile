set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()

list:
  @just --list


# Build the release binary
build:
    cargo build --release

# Scan with the compiled binary. Extra args pass through, e.g.
#   just scan-bin --format json src/
#   just scan-bin --list-rules
# Exit code: 0 = clean, 1 = findings at/above --fail-on (default: warning).
[no-exit-message]
scan-bin *args=".":
    cargo run --release --quiet -- {{args}}

# Per-rule annotated fixtures (tests/query-cases/<lang>/*), checked against
# the compiled matcher by tests/query_cases.rs. See AGENTS.md
# "Query-case tests" for the format.
query-test:
    cargo test --test query_cases

# All tests: unit tests, black-box CLI tests (tests/cli.rs), and query-case
# tests (tests/query_cases.rs).
test:
    cargo test

# Build a static (musl) tree-police for the host architecture and package it
# into a `FROM scratch` image, e.g. for local testing. The published image
# (see .github/workflows/release.yml) is multi-arch (amd64 + arm64) via
# `docker buildx`; this recipe only builds the one you're running on.
# Dereferences the `result` symlink before `podman build`: the build-context
# tar preserves symlinks as-is and won't follow one pointing out to
# /nix/store, so COPYing `result/bin/tree-police` directly would fail to resolve.
docker-build:
    #!/usr/bin/env bash
    set -euo pipefail
    case "$(uname -m)" in
        x86_64) nix_pkg=tree-police-static-x86_64; arch=amd64 ;;
        arm64|aarch64) nix_pkg=tree-police-static-aarch64; arch=arm64 ;;
        *) echo "unsupported host architecture: $(uname -m)" >&2; exit 1 ;;
    esac
    nix build "{{root}}#$nix_pkg"
    mkdir -p "{{root}}/dist/$arch"
    install -m755 "{{root}}/result/bin/tree-police" "{{root}}/dist/$arch/tree-police"
    podman build --build-arg TARGETARCH="$arch" -t tree-police:local -f "{{root}}/Dockerfile" "{{root}}"

# Run the built image against a mounted directory, e.g.
#   just docker-run --format json /work/queries
# `-t` allocates a pty so tree-police's TTY auto-detection (anstream) sees a
# terminal and emits color, same as `cargo run`; without it, podman attaches
# stdout as a plain pipe and output comes back uncolored even though your
# shell is interactive.
[no-exit-message]
docker-run *args=".":
    podman run --rm -t -v {{root}}:/work:ro -w /work tree-police:local {{args}}

# Regenerate THIRD-PARTY-LICENSES.md from Cargo.lock (see about.toml/about.hbs).
# Run after any dependency change; `cargo about` fails loudly on a license
# outside about.toml's `accepted` list, so a new disallowed license is a
# build-time signal, not a silent gap.
third-party-licenses:
    cargo about generate about.hbs -o THIRD-PARTY-LICENSES.md

# Generate a CycloneDX SBOM for the exact dependency set in Cargo.lock.
sbom:
    nix build {{root}}#sbom -o sbom-result
    cp sbom-result/bom.json sbom.cdx.json
