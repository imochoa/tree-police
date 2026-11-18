//! Arbitrary bytes parsed as Python, then every comment run through
//! `Suppressions::collect` -- the path real source files take, including
//! non-UTF-8 and half-parsed input.
#![no_main]

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use tree_police::config::Catalog;
use tree_police::suppress::Suppressions;

fuzz_target!(|source: &[u8]| {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let Some(tree) = parser.parse(source, None) else {
        return;
    };
    let suppressions =
        Suppressions::collect(Path::new("fuzz.py"), &tree, source, &Catalog::default());
    assert_eq!(suppressions.apply(&mut Vec::new()), 0);
});
