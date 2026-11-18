//! Property tests: invariants checked on thousands of generated inputs
//! instead of a handful of hand-picked ones. `PROPTEST_CASES=100000 cargo
//! test --test props` for a longer run; failures shrink to a minimal input
//! and are saved under `tests/props.proptest-regressions`.

use proptest::prelude::*;
use tree_police::config::Selector;
use tree_police::suppress::{parse_directive, DirectiveError, Kind};
use tree_police::Severity;

/// Comment text biased toward the directive grammar, so most cases get past
/// the marker instead of being rejected on the first byte.
fn comment_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("tree-police:".to_string()),
        Just("ignore".to_string()),
        Just("-next-line".to_string()),
        Just("-file".to_string()),
        Just("category:".to_string()),
        Just("file:".to_string()),
        "[\\[\\],\\-# ]".prop_map(String::from),
        "\\s".prop_map(String::from),
        "[a-z_]{1,6}".prop_map(String::from),
        any::<char>().prop_map(String::from),
    ];
    prop::collection::vec(piece, 0..12).prop_map(|v| v.concat())
}

proptest! {
    #[test]
    fn severity_display_parse_roundtrip(s in prop_oneof![
        Just(Severity::Log), Just(Severity::Warning), Just(Severity::Error)
    ]) {
        prop_assert_eq!(s.to_string().parse::<Severity>().unwrap(), s);
    }

    #[test]
    fn severity_parse_ignores_case_and_padding(
        s in prop_oneof![Just("log"), Just("warn"), Just("error"), Just("note")],
        upper in any::<bool>(),
        pad in "[ \t]{0,3}",
    ) {
        let raw = if upper { s.to_uppercase() } else { s.to_string() };
        prop_assert_eq!(
            format!("{pad}{raw}{pad}").parse::<Severity>().unwrap(),
            s.parse::<Severity>().unwrap()
        );
    }

    /// `Display` is a faithful inverse of `parse` on everything `parse`
    /// accepts, so config files and `--list-rules` output can round-trip.
    #[test]
    fn selector_display_parse_roundtrip(s in comment_text()) {
        if let Ok(sel) = Selector::parse(&s) {
            prop_assert_eq!(Selector::parse(&sel.to_string()).unwrap(), sel);
        }
    }

    #[test]
    fn directive_needs_marker(s in comment_text()) {
        prop_assert_eq!(parse_directive(&s).is_some(), s.contains("tree-police:"));
    }

    /// Every selector handed on to `Selector::parse` is non-blank, so its
    /// "empty rule selector" error can't be reached from a comment. (Also
    /// proved for the Lean model: `parseSelectors_nonblank`.)
    #[test]
    fn directive_selectors_are_non_blank(s in comment_text()) {
        if let Some(Ok(d)) = parse_directive(&s) {
            for item in d.selectors.unwrap_or_default() {
                prop_assert!(!item.trim().is_empty());
            }
        }
    }

    /// Trailing text after a complete directive never changes it.
    #[test]
    fn directive_ignores_trailing_reason(
        word in prop_oneof![Just("ignore"), Just("ignore-next-line"), Just("ignore-file")],
        sels in prop::option::of("[a-z_:]{1,8}(,[a-z_:]{1,8}){0,2}"),
        reason in "( --[ -~]*)?",
    ) {
        let list = sels.as_ref().map(|s| format!("[{s}]")).unwrap_or_default();
        let base = format!("# tree-police: {word}{list}");
        let with_reason = format!("{base}{reason}");
        let a = parse_directive(&base).unwrap().unwrap();
        let b = parse_directive(&with_reason).unwrap().unwrap();
        prop_assert_eq!(a, b);
    }

    #[test]
    fn unknown_word_is_reported_verbatim(word in "[a-z0-9-]{1,12}") {
        prop_assume!(!["ignore", "ignore-next-line", "ignore-file"].contains(&word.as_str()));
        let text = format!("tree-police: {word}");
        prop_assert_eq!(
            parse_directive(&text),
            Some(Err(DirectiveError::Unrecognized(word.as_str())))
        );
    }
}

/// Pinned behaviour found by exploring the model: a space between the
/// directive and its `[…]` drops the list, so the comment suppresses every
/// rule. Flip this test if that gets changed to a warning or an error.
#[test]
fn space_before_selector_list_widens_to_every_rule() {
    let d = parse_directive("# tree-police: ignore [forbidden_print]")
        .unwrap()
        .unwrap();
    assert_eq!(d.kind, Kind::Ignore);
    assert_eq!(d.selectors, None);
}
