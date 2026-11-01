//! Coverage-guided: arbitrary comment text through the directive parser.
//! Catches panics (e.g. slicing off a char boundary) and checks the same
//! invariants as `tests/props.rs`, but on inputs libFuzzer evolves itself.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tree_police::suppress::parse_directive;

fuzz_target!(|text: &str| {
    let parsed = parse_directive(text);
    assert_eq!(parsed.is_some(), text.contains("tree-police:"));
    if let Some(Ok(d)) = parsed {
        for item in d.selectors.unwrap_or_default() {
            assert!(!item.trim().is_empty(), "blank selector from {text:?}");
        }
    }
});
