{
  description = "tree-police -- compiled Rust AST pattern scanner, embeddable in pre-commit";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {inherit system;};

      # Statically-linked (musl) Linux builds of `tree-police`, regardless
      # of the host system this flake is evaluated on. This is what a
      # `FROM scratch` Docker image needs: zero dynamic deps (no ld-linux,
      # no libc.so). The tree-sitter-* grammar crates compile their C/C++
      # grammars via build.rs (the `cc` crate); each pkgsCross.*-musl* set
      # wires TARGET_CC so those land in the same static binary, cross or
      # native, for that target.
      mkStatic = crossPkgs:
        crossPkgs.rustPlatform.buildRustPackage {
          pname = "tree-police";
          version = "0.1.2";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          doCheck = false; # cross-compiled test binaries can't run on the build host
          RUSTFLAGS = "-C target-feature=+crt-static -C relocation-model=static";
        };

      tree-police-static-x86_64 = mkStatic pkgs.pkgsCross.musl64;
      tree-police-static-aarch64 = mkStatic pkgs.pkgsCross.aarch64-multiplatform-musl;

      # A CycloneDX SBOM of the exact dependency set in Cargo.lock, built
      # reproducibly via Nix rather than requiring network access to
      # crates.io at SBOM-generation time (the vendored cargo registry
      # buildRustPackage already sets up covers it). Doesn't need the
      # musl cross build -- `cargo cyclonedx` only reads Cargo
      # metadata, it doesn't compile the binary.
      sbom = pkgs.rustPlatform.buildRustPackage {
        pname = "tree-police-sbom";
        version = "0.1.2";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
        nativeBuildInputs = [pkgs.cargo-cyclonedx];
        doCheck = false;

        buildPhase = ''
          runHook preBuild
          cargo cyclonedx --spec-version 1.5 --format json --all
          runHook postBuild
        '';

        installPhase = ''
          runHook preInstall
          mkdir -p $out
          cp *.cdx.json "$out/bom.json"
          runHook postInstall
        '';
      };
    in {
      packages = {
        tree-police-static = tree-police-static-x86_64;
        default = tree-police-static-x86_64;
        inherit tree-police-static-x86_64 tree-police-static-aarch64 sbom;
      };

      devShells.default = pkgs.mkShell {
        packages = [
          pkgs.just
          pkgs.rustc
          pkgs.cargo
          pkgs.clippy
          pkgs.rustfmt

          # The tree-sitter-* crates compile their C/C++ grammars via their
          # own build.rs, so a C/C++ compiler must be on PATH.
          pkgs.stdenv.cc

          # `just third-party-licenses` -- regenerates THIRD-PARTY-LICENSES.md
          pkgs.cargo-about

          # Pre-commit hooks (.pre-commit-config.yaml) run these via
          # `language: system`, i.e. straight off this devShell's PATH --
          # no separate per-hook environment to install.
          pkgs.pre-commit
          pkgs.alejandra # nix formatter
          pkgs.statix # nix linter
          pkgs.deadnix # nix dead-code finder
          pkgs.markdownlint-cli2
          pkgs.actionlint
        ];

        # Let the grammar crates' build scripts find libclang/headers if needed.
        RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      };
    });
}
