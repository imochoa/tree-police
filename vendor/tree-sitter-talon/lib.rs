//! Tree-sitter grammar for Talon files, vendored from
//! <https://github.com/wenkokke/tree-sitter-talon>. See `Cargo.toml`.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_talon() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for Talon.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_talon) };
