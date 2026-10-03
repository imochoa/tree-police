//! Language registry: maps a language name and file extensions to the
//! tree-sitter grammar compiled into the binary.
//!
//! Adding a language is three lines here plus a `queries/<label>-<code>.scm`
//! file and the grammar crate in `Cargo.toml`.

use tree_sitter::Language;

/// A supported language: its display name, query-filename routing code, the
/// file extensions it owns, and a constructor for its tree-sitter [`Language`].
pub struct LangSpec {
    /// Human-readable label shown in output (e.g. `--list-rules`, `Finding.language`).
    pub name: &'static str,
    /// Short code used both as the `queries/<label>-<code>.scm` filename
    /// suffix and as the `by_code` lookup key.
    pub code: &'static str,
    /// File extensions (without the dot) dispatched to this language.
    pub extensions: &'static [&'static str],
    language_fn: fn() -> Language,
}

impl LangSpec {
    /// Build the tree-sitter [`Language`] for this grammar.
    pub fn language(&self) -> Language {
        (self.language_fn)()
    }
}

fn python_language() -> Language {
    tree_sitter_python::LANGUAGE.into()
}

fn terraform_language() -> Language {
    tree_sitter_hcl::LANGUAGE.into()
}

fn javascript_language() -> Language {
    tree_sitter_javascript::LANGUAGE.into()
}

fn typescript_language() -> Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

fn tsx_language() -> Language {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

fn yaml_language() -> Language {
    tree_sitter_yaml::LANGUAGE.into()
}

fn json_language() -> Language {
    tree_sitter_json::LANGUAGE.into()
}

fn bash_language() -> Language {
    tree_sitter_bash::LANGUAGE.into()
}

fn ada_language() -> Language {
    tree_sitter_ada::LANGUAGE.into()
}

fn adl_language() -> Language {
    tree_sitter_adl::LANGUAGE.into()
}

fn agda_language() -> Language {
    tree_sitter_agda::LANGUAGE.into()
}

fn bicep_language() -> Language {
    tree_sitter_bicep::LANGUAGE.into()
}

fn c_language() -> Language {
    tree_sitter_c::LANGUAGE.into()
}

fn csharp_language() -> Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}

fn clojure_language() -> Language {
    tree_sitter_clojure::LANGUAGE.into()
}

fn cmake_language() -> Language {
    tree_sitter_cmake::LANGUAGE.into()
}

fn commonlisp_language() -> Language {
    tree_sitter_commonlisp::LANGUAGE_COMMONLISP.into()
}

fn cpp_language() -> Language {
    tree_sitter_cpp::LANGUAGE.into()
}

fn crystal_language() -> Language {
    tree_sitter_crystal::LANGUAGE.into()
}

fn cuda_language() -> Language {
    tree_sitter_cuda::LANGUAGE.into()
}

fn d_language() -> Language {
    tree_sitter_d::LANGUAGE.into()
}

fn dart_language() -> Language {
    tree_sitter_dart::LANGUAGE.into()
}

fn devicetree_language() -> Language {
    tree_sitter_devicetree::LANGUAGE.into()
}

fn diff_language() -> Language {
    tree_sitter_diff::LANGUAGE.into()
}

fn elisp_language() -> Language {
    tree_sitter_elisp::LANGUAGE.into()
}

fn elixir_language() -> Language {
    tree_sitter_elixir::LANGUAGE.into()
}

fn elm_language() -> Language {
    tree_sitter_elm::LANGUAGE.into()
}

fn erb_language() -> Language {
    tree_sitter_embedded_template::LANGUAGE.into()
}

fn erlang_language() -> Language {
    tree_sitter_erlang::LANGUAGE.into()
}

fn fortran_language() -> Language {
    tree_sitter_fortran::LANGUAGE.into()
}

fn fsharp_language() -> Language {
    tree_sitter_fsharp::LANGUAGE_FSHARP.into()
}

fn gdscript_language() -> Language {
    tree_sitter_gdscript::LANGUAGE.into()
}

fn gleam_language() -> Language {
    tree_sitter_gleam::LANGUAGE.into()
}

fn glsl_language() -> Language {
    tree_sitter_glsl::LANGUAGE_GLSL.into()
}

fn go_language() -> Language {
    tree_sitter_go::LANGUAGE.into()
}

fn godotresource_language() -> Language {
    tree_sitter_godot_resource::LANGUAGE.into()
}

fn graphql_language() -> Language {
    tree_sitter_graphql::LANGUAGE.into()
}

fn groovy_language() -> Language {
    tree_sitter_groovy::LANGUAGE.into()
}

fn haskell_language() -> Language {
    tree_sitter_haskell::LANGUAGE.into()
}

fn heex_language() -> Language {
    tree_sitter_heex::LANGUAGE.into()
}

fn html_language() -> Language {
    tree_sitter_html::LANGUAGE.into()
}

fn ini_language() -> Language {
    tree_sitter_ini::LANGUAGE.into()
}

fn java_language() -> Language {
    tree_sitter_java::LANGUAGE.into()
}

fn jinja2_language() -> Language {
    tree_sitter_jinja2::LANGUAGE.into()
}

fn jsonnet_language() -> Language {
    tree_sitter_jsonnet::LANGUAGE.into()
}

fn julia_language() -> Language {
    tree_sitter_julia::LANGUAGE.into()
}

fn kdl_language() -> Language {
    tree_sitter_kdl::LANGUAGE.into()
}

fn llvm_language() -> Language {
    tree_sitter_llvm::LANGUAGE.into()
}

fn lua_language() -> Language {
    tree_sitter_lua::LANGUAGE.into()
}

fn luau_language() -> Language {
    tree_sitter_luau::LANGUAGE.into()
}

fn make_language() -> Language {
    tree_sitter_make::LANGUAGE.into()
}

fn matlab_language() -> Language {
    tree_sitter_matlab::LANGUAGE.into()
}

fn netlinx_language() -> Language {
    tree_sitter_netlinx::LANGUAGE.into()
}

fn nim_language() -> Language {
    tree_sitter_nim::LANGUAGE.into()
}

fn nix_language() -> Language {
    tree_sitter_nix::LANGUAGE.into()
}

fn objc_language() -> Language {
    tree_sitter_objc::LANGUAGE.into()
}

fn ocaml_language() -> Language {
    tree_sitter_ocaml::LANGUAGE_OCAML.into()
}

fn ocamllex_language() -> Language {
    tree_sitter_ocamllex::LANGUAGE.into()
}

fn odin_language() -> Language {
    tree_sitter_odin::LANGUAGE.into()
}

fn pascal_language() -> Language {
    tree_sitter_pascal::LANGUAGE.into()
}

fn pgn_language() -> Language {
    tree_sitter_pgn::LANGUAGE.into()
}

fn php_language() -> Language {
    tree_sitter_php::LANGUAGE_PHP.into()
}

fn powershell_language() -> Language {
    tree_sitter_powershell::LANGUAGE.into()
}

fn prolog_language() -> Language {
    tree_sitter_prolog::LANGUAGE.into()
}

fn properties_language() -> Language {
    tree_sitter_properties::LANGUAGE.into()
}

fn proto_language() -> Language {
    tree_sitter_proto::LANGUAGE.into()
}

fn ql_language() -> Language {
    tree_sitter_ql::LANGUAGE.into()
}

fn dbscheme_language() -> Language {
    tree_sitter_ql_dbscheme::LANGUAGE.into()
}

fn qmljs_language() -> Language {
    tree_sitter_qmljs::LANGUAGE.into()
}

fn r_language() -> Language {
    tree_sitter_r::LANGUAGE.into()
}

fn razor_language() -> Language {
    tree_sitter_razor::LANGUAGE.into()
}

fn rst_language() -> Language {
    tree_sitter_rst::LANGUAGE.into()
}

fn ruby_language() -> Language {
    tree_sitter_ruby::LANGUAGE.into()
}

fn rust_language() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

fn scala_language() -> Language {
    tree_sitter_scala::LANGUAGE.into()
}

fn scheme_language() -> Language {
    tree_sitter_scheme::LANGUAGE.into()
}

fn slang_language() -> Language {
    tree_sitter_slang::LANGUAGE_SLANG.into()
}

fn slint_language() -> Language {
    tree_sitter_slint::LANGUAGE.into()
}

fn solidity_language() -> Language {
    tree_sitter_solidity::LANGUAGE.into()
}

fn sparql_language() -> Language {
    tree_sitter_sparql::LANGUAGE.into()
}

fn swift_language() -> Language {
    tree_sitter_swift::LANGUAGE.into()
}

fn t32_language() -> Language {
    tree_sitter_t32::LANGUAGE.into()
}

fn templ_language() -> Language {
    tree_sitter_templ::LANGUAGE.into()
}

fn tera_language() -> Language {
    tree_sitter_tera::LANGUAGE.into()
}

fn tlaplus_language() -> Language {
    tree_sitter_tlaplus::LANGUAGE.into()
}

fn vcl_language() -> Language {
    tree_sitter_vcl::LANGUAGE.into()
}

fn verilog_language() -> Language {
    tree_sitter_verilog::LANGUAGE.into()
}

fn vhdl_language() -> Language {
    tree_sitter_vhdl::LANGUAGE.into()
}

fn wesl_language() -> Language {
    tree_sitter_wesl::LANGUAGE.into()
}

fn xml_language() -> Language {
    tree_sitter_xml::LANGUAGE_XML.into()
}

fn zig_language() -> Language {
    tree_sitter_zig::LANGUAGE.into()
}
fn css_language() -> Language {
    tree_sitter_css::LANGUAGE.into()
}

/// Every language the binary knows about, in a stable order. Covers every
/// tree-sitter grammar crate on crates.io whose runtime dependency on
/// `tree-sitter`/`tree-sitter-language` resolves against this crate's pinned
/// `tree-sitter = "0.25"` (see `Cargo.toml`) -- almost all of them have no
/// rule file yet beyond the two with actual rules today (`python`,
/// `terraform`). That's deliberate: a new rule file is all that's needed to
/// start covering one of them, no Cargo/registry change, and a pre-commit
/// hook can pass every changed file without its own file-type allowlist --
/// files whose extension isn't recognized here are simply skipped by
/// `for_extension`.
///
/// Excluded, and why: crates whose declared `tree-sitter`/`tree-sitter-language`
/// version can't coexist with `0.25` in one binary (Cargo's `links` key allows
/// only one linked version of the native `tree-sitter` library project-wide),
/// and two (`tree-sitter-ink`, `tree-sitter-latex`, both v0.1.0) whose published
/// crate omits/disables the external scanner source their own parser needs,
/// failing at link time regardless of version. Re-check both categories
/// periodically -- newer releases may fix either problem.
pub static LANGUAGES: &[LangSpec] = &[
    LangSpec {
        name: "python",
        code: "py",
        extensions: &["py"],
        language_fn: python_language,
    },
    LangSpec {
        name: "terraform",
        code: "tf",
        extensions: &["tf", "tfvars", "hcl"],
        language_fn: terraform_language,
    },
    LangSpec {
        name: "javascript",
        code: "js",
        extensions: &["js", "mjs", "cjs", "jsx"],
        language_fn: javascript_language,
    },
    LangSpec {
        name: "typescript",
        code: "ts",
        extensions: &["ts", "mts", "cts"],
        language_fn: typescript_language,
    },
    LangSpec {
        name: "tsx",
        code: "tsx",
        extensions: &["tsx"],
        language_fn: tsx_language,
    },
    LangSpec {
        name: "yaml",
        code: "yaml",
        extensions: &["yaml", "yml"],
        language_fn: yaml_language,
    },
    LangSpec {
        name: "json",
        code: "json",
        extensions: &["json"],
        language_fn: json_language,
    },
    LangSpec {
        name: "bash",
        code: "sh",
        extensions: &["sh", "bash"],
        language_fn: bash_language,
    },
    LangSpec {
        name: "ada",
        code: "ada",
        extensions: &["adb", "ads"],
        language_fn: ada_language,
    },
    LangSpec {
        name: "adl",
        code: "adl",
        extensions: &["adl"],
        language_fn: adl_language,
    },
    LangSpec {
        name: "agda",
        code: "agda",
        extensions: &["agda"],
        language_fn: agda_language,
    },
    LangSpec {
        name: "bicep",
        code: "bicep",
        extensions: &["bicep"],
        language_fn: bicep_language,
    },
    LangSpec {
        name: "c",
        code: "c",
        extensions: &["c", "h"],
        language_fn: c_language,
    },
    LangSpec {
        name: "csharp",
        code: "csharp",
        extensions: &["cs"],
        language_fn: csharp_language,
    },
    LangSpec {
        name: "clojure",
        code: "clojure",
        extensions: &["clj", "cljs", "cljc", "edn"],
        language_fn: clojure_language,
    },
    LangSpec {
        name: "cmake",
        code: "cmake",
        extensions: &["cmake"],
        language_fn: cmake_language,
    },
    LangSpec {
        name: "commonlisp",
        code: "commonlisp",
        extensions: &["lisp", "lsp", "cl"],
        language_fn: commonlisp_language,
    },
    LangSpec {
        name: "cpp",
        code: "cpp",
        extensions: &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        language_fn: cpp_language,
    },
    LangSpec {
        name: "crystal",
        code: "crystal",
        extensions: &["cr"],
        language_fn: crystal_language,
    },
    LangSpec {
        name: "cuda",
        code: "cuda",
        extensions: &["cu", "cuh"],
        language_fn: cuda_language,
    },
    LangSpec {
        name: "d",
        code: "d",
        extensions: &["d"],
        language_fn: d_language,
    },
    LangSpec {
        name: "dart",
        code: "dart",
        extensions: &["dart"],
        language_fn: dart_language,
    },
    LangSpec {
        name: "devicetree",
        code: "devicetree",
        extensions: &["dts", "dtsi"],
        language_fn: devicetree_language,
    },
    LangSpec {
        name: "diff",
        code: "diff",
        extensions: &["diff", "patch"],
        language_fn: diff_language,
    },
    LangSpec {
        name: "elisp",
        code: "elisp",
        extensions: &["el"],
        language_fn: elisp_language,
    },
    LangSpec {
        name: "elixir",
        code: "elixir",
        extensions: &["ex", "exs"],
        language_fn: elixir_language,
    },
    LangSpec {
        name: "elm",
        code: "elm",
        extensions: &["elm"],
        language_fn: elm_language,
    },
    LangSpec {
        name: "erb",
        code: "erb",
        extensions: &["erb", "ejs"],
        language_fn: erb_language,
    },
    LangSpec {
        name: "erlang",
        code: "erlang",
        extensions: &["erl", "hrl"],
        language_fn: erlang_language,
    },
    LangSpec {
        name: "fortran",
        code: "fortran",
        extensions: &["f90", "f95", "f03", "f08"],
        language_fn: fortran_language,
    },
    LangSpec {
        name: "fsharp",
        code: "fsharp",
        extensions: &["fs", "fsx", "fsi"],
        language_fn: fsharp_language,
    },
    LangSpec {
        name: "gdscript",
        code: "gdscript",
        extensions: &["gd"],
        language_fn: gdscript_language,
    },
    LangSpec {
        name: "gleam",
        code: "gleam",
        extensions: &["gleam"],
        language_fn: gleam_language,
    },
    LangSpec {
        name: "glsl",
        code: "glsl",
        extensions: &["glsl", "vert", "frag"],
        language_fn: glsl_language,
    },
    LangSpec {
        name: "go",
        code: "go",
        extensions: &["go"],
        language_fn: go_language,
    },
    LangSpec {
        name: "godotresource",
        code: "godotresource",
        extensions: &["tscn", "tres"],
        language_fn: godotresource_language,
    },
    LangSpec {
        name: "graphql",
        code: "graphql",
        extensions: &["graphql", "gql"],
        language_fn: graphql_language,
    },
    LangSpec {
        name: "groovy",
        code: "groovy",
        extensions: &["groovy", "gvy"],
        language_fn: groovy_language,
    },
    LangSpec {
        name: "haskell",
        code: "haskell",
        extensions: &["hs", "lhs"],
        language_fn: haskell_language,
    },
    LangSpec {
        name: "heex",
        code: "heex",
        extensions: &["heex"],
        language_fn: heex_language,
    },
    LangSpec {
        name: "html",
        code: "html",
        extensions: &["html", "htm"],
        language_fn: html_language,
    },
    LangSpec {
        name: "ini",
        code: "ini",
        extensions: &["ini", "cfg"],
        language_fn: ini_language,
    },
    LangSpec {
        name: "java",
        code: "java",
        extensions: &["java"],
        language_fn: java_language,
    },
    LangSpec {
        name: "jinja2",
        code: "jinja2",
        extensions: &["jinja", "jinja2", "j2"],
        language_fn: jinja2_language,
    },
    LangSpec {
        name: "jsonnet",
        code: "jsonnet",
        extensions: &["jsonnet", "libsonnet"],
        language_fn: jsonnet_language,
    },
    LangSpec {
        name: "julia",
        code: "julia",
        extensions: &["jl"],
        language_fn: julia_language,
    },
    LangSpec {
        name: "kdl",
        code: "kdl",
        extensions: &["kdl"],
        language_fn: kdl_language,
    },
    LangSpec {
        name: "llvm",
        code: "llvm",
        extensions: &["ll"],
        language_fn: llvm_language,
    },
    LangSpec {
        name: "lua",
        code: "lua",
        extensions: &["lua"],
        language_fn: lua_language,
    },
    LangSpec {
        name: "luau",
        code: "luau",
        extensions: &["luau"],
        language_fn: luau_language,
    },
    LangSpec {
        name: "make",
        code: "make",
        extensions: &["mk"],
        language_fn: make_language,
    },
    // matlab and objc both canonically use ".m"; objc claims it below
    // (more common in general-purpose repos). No extension for matlab
    // here as a result -- add one back (or arbitrate differently) if
    // you specifically need matlab detection.
    LangSpec {
        name: "matlab",
        code: "matlab",
        extensions: &[],
        language_fn: matlab_language,
    },
    LangSpec {
        name: "netlinx",
        code: "netlinx",
        extensions: &["axs", "axi"],
        language_fn: netlinx_language,
    },
    LangSpec {
        name: "nim",
        code: "nim",
        extensions: &["nim"],
        language_fn: nim_language,
    },
    LangSpec {
        name: "nix",
        code: "nix",
        extensions: &["nix"],
        language_fn: nix_language,
    },
    LangSpec {
        name: "objc",
        code: "objc",
        extensions: &["m", "mm"],
        language_fn: objc_language,
    },
    LangSpec {
        name: "ocaml",
        code: "ocaml",
        extensions: &["ml", "mli"],
        language_fn: ocaml_language,
    },
    LangSpec {
        name: "ocamllex",
        code: "ocamllex",
        extensions: &["mll"],
        language_fn: ocamllex_language,
    },
    LangSpec {
        name: "odin",
        code: "odin",
        extensions: &["odin"],
        language_fn: odin_language,
    },
    LangSpec {
        name: "pascal",
        code: "pascal",
        extensions: &["pas", "pp"],
        language_fn: pascal_language,
    },
    LangSpec {
        name: "pgn",
        code: "pgn",
        extensions: &["pgn"],
        language_fn: pgn_language,
    },
    LangSpec {
        name: "php",
        code: "php",
        extensions: &["php"],
        language_fn: php_language,
    },
    LangSpec {
        name: "powershell",
        code: "powershell",
        extensions: &["ps1", "psm1", "psd1"],
        language_fn: powershell_language,
    },
    LangSpec {
        name: "prolog",
        code: "prolog",
        extensions: &["pl", "pro"],
        language_fn: prolog_language,
    },
    LangSpec {
        name: "properties",
        code: "properties",
        extensions: &["properties"],
        language_fn: properties_language,
    },
    LangSpec {
        name: "proto",
        code: "proto",
        extensions: &["proto"],
        language_fn: proto_language,
    },
    LangSpec {
        name: "ql",
        code: "ql",
        extensions: &["ql"],
        language_fn: ql_language,
    },
    LangSpec {
        name: "dbscheme",
        code: "dbscheme",
        extensions: &["dbscheme"],
        language_fn: dbscheme_language,
    },
    LangSpec {
        name: "qmljs",
        code: "qmljs",
        extensions: &["qml"],
        language_fn: qmljs_language,
    },
    LangSpec {
        name: "r",
        code: "r",
        extensions: &["r"],
        language_fn: r_language,
    },
    LangSpec {
        name: "razor",
        code: "razor",
        extensions: &["razor", "cshtml"],
        language_fn: razor_language,
    },
    LangSpec {
        name: "rst",
        code: "rst",
        extensions: &["rst"],
        language_fn: rst_language,
    },
    LangSpec {
        name: "ruby",
        code: "ruby",
        extensions: &["rb"],
        language_fn: ruby_language,
    },
    LangSpec {
        name: "rust",
        code: "rust",
        extensions: &["rs"],
        language_fn: rust_language,
    },
    LangSpec {
        name: "scala",
        code: "scala",
        extensions: &["scala", "sc"],
        language_fn: scala_language,
    },
    // Deliberately not ".scm": that's this project's own query-file
    // extension (see queries/README.md) -- claiming it here would make
    // a scan of this repo (or any `.tree-police/` rules dir) parse rule
    // files as Scheme source the moment a scheme-*.scm rule exists.
    LangSpec {
        name: "scheme",
        code: "scheme",
        extensions: &["ss"],
        language_fn: scheme_language,
    },
    LangSpec {
        name: "slang",
        code: "slang",
        extensions: &["slang"],
        language_fn: slang_language,
    },
    LangSpec {
        name: "slint",
        code: "slint",
        extensions: &["slint"],
        language_fn: slint_language,
    },
    LangSpec {
        name: "solidity",
        code: "solidity",
        extensions: &["sol"],
        language_fn: solidity_language,
    },
    LangSpec {
        name: "sparql",
        code: "sparql",
        extensions: &["rq", "sparql"],
        language_fn: sparql_language,
    },
    LangSpec {
        name: "swift",
        code: "swift",
        extensions: &["swift"],
        language_fn: swift_language,
    },
    LangSpec {
        name: "t32",
        code: "t32",
        extensions: &["cmm"],
        language_fn: t32_language,
    },
    LangSpec {
        name: "templ",
        code: "templ",
        extensions: &["templ"],
        language_fn: templ_language,
    },
    LangSpec {
        name: "tera",
        code: "tera",
        extensions: &["tera"],
        language_fn: tera_language,
    },
    LangSpec {
        name: "tlaplus",
        code: "tlaplus",
        extensions: &["tla"],
        language_fn: tlaplus_language,
    },
    LangSpec {
        name: "vcl",
        code: "vcl",
        extensions: &["vcl"],
        language_fn: vcl_language,
    },
    LangSpec {
        name: "verilog",
        code: "verilog",
        extensions: &["v", "vh", "sv"],
        language_fn: verilog_language,
    },
    LangSpec {
        name: "vhdl",
        code: "vhdl",
        extensions: &["vhd", "vhdl"],
        language_fn: vhdl_language,
    },
    LangSpec {
        name: "wesl",
        code: "wesl",
        extensions: &["wesl"],
        language_fn: wesl_language,
    },
    LangSpec {
        name: "xml",
        code: "xml",
        extensions: &["xml"],
        language_fn: xml_language,
    },
    LangSpec {
        name: "zig",
        code: "zig",
        extensions: &["zig"],
        language_fn: zig_language,
    },
    LangSpec {
        name: "css",
        code: "css",
        extensions: &["css"],
        language_fn: css_language,
    },
];

/// Look up a language by its human-readable name.
pub fn by_name(name: &str) -> Option<&'static LangSpec> {
    LANGUAGES.iter().find(|spec| spec.name == name)
}

/// Look up a language by its `queries/*-<code>.scm` routing code.
pub fn by_code(code: &str) -> Option<&'static LangSpec> {
    LANGUAGES.iter().find(|spec| spec.code == code)
}

/// Look up the language that owns a file extension (without the dot).
pub fn for_extension(ext: &str) -> Option<&'static LangSpec> {
    LANGUAGES.iter().find(|spec| spec.extensions.contains(&ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_grammar_constructs() {
        // Regression guard: an ABI mismatch (e.g. a grammar crate pinned to
        // an incompatible tree-sitter core version slipping into Cargo.lock)
        // would panic here per-language rather than surfacing as a vague
        // link-time or runtime failure.
        for spec in LANGUAGES {
            let _ = spec.language();
        }
        assert_eq!(LANGUAGES.len(), 92);
    }
}
