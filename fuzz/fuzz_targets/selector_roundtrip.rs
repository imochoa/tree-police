//! `Selector::parse(sel.to_string()) == sel` for every selector `parse`
//! accepts.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tree_police::config::Selector;

fuzz_target!(|text: &str| {
    if let Ok(sel) = Selector::parse(text) {
        let shown = sel.to_string();
        assert_eq!(Selector::parse(&shown).ok(), Some(sel), "via {shown:?}");
    }
});
