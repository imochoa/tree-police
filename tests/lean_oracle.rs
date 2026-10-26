//! Differential test: `suppress::parse_directive` against the executable
//! Lean model in `formal/` (`TreePolice.Suppress.parseDirective`), on the
//! same generated inputs. The Lean side carries proofs; this test is what
//! ties those proofs back to the Rust that actually ships.
//!
//! Skipped unless `TREE_POLICE_LEAN_ORACLE` points at the oracle binary:
//!
//! ```text
//! (cd formal && lake build)
//! TREE_POLICE_LEAN_ORACLE=formal/.lake/build/bin/oracle cargo test --test lean_oracle
//! ```

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, OnceLock};

use proptest::prelude::*;
use serde_json::{json, Value};
use tree_police::suppress::{parse_directive, DirectiveError};

struct Oracle {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Oracle {
    fn ask(&mut self, text: &str) -> Value {
        writeln!(self.stdin, "{}", serde_json::to_string(text).unwrap()).unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("oracle sent {line:?}: {e}"))
    }
}

fn oracle() -> Option<&'static Mutex<Oracle>> {
    static ORACLE: OnceLock<Option<Mutex<Oracle>>> = OnceLock::new();
    ORACLE
        .get_or_init(|| {
            let path = std::env::var_os("TREE_POLICE_LEAN_ORACLE")?;
            let mut child = Command::new(&path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap_or_else(|e| panic!("spawning {path:?}: {e}"));
            Some(Mutex::new(Oracle {
                stdin: child.stdin.take().unwrap(),
                stdout: BufReader::new(child.stdout.take().unwrap()),
                _child: child,
            }))
        })
        .as_ref()
}

/// The Rust result in the oracle's JSON shape (see `formal/Oracle.lean`).
fn rust_side(text: &str) -> Value {
    match parse_directive(text) {
        None => Value::Null,
        Some(Err(DirectiveError::Unrecognized(word))) => {
            json!({"error": "unrecognized", "word": word})
        }
        Some(Err(DirectiveError::UnclosedBracket)) => json!({"error": "unclosed"}),
        Some(Ok(d)) => json!({"kind": d.kind.as_str(), "selectors": d.selectors}),
    }
}

fn comment_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("tree-police:".to_string()),
        Just("ignore".to_string()),
        Just("-next-line".to_string()),
        Just("-file".to_string()),
        "[\\[\\],\\-# ]".prop_map(String::from),
        // Unicode whitespace: where a naive model (`Char.isWhitespace`) and
        // Rust's `char::is_whitespace` disagree.
        prop_oneof![
            Just('\u{a0}'),
            Just('\u{2003}'),
            Just('\u{3000}'),
            Just('\u{85}')
        ]
        .prop_map(String::from),
        "\\s".prop_map(String::from),
        "[a-zA-Z0-9_]{1,6}".prop_map(String::from),
        any::<char>().prop_map(String::from),
    ];
    prop::collection::vec(piece, 0..12).prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(
        std::env::var("PROPTEST_CASES").ok().and_then(|n| n.parse().ok()).unwrap_or(20_000)
    ))]

    #[test]
    fn rust_matches_lean_model(text in comment_text()) {
        let Some(oracle) = oracle() else {
            return Ok(());
        };
        let lean = oracle.lock().unwrap().ask(&text);
        prop_assert_eq!(rust_side(&text), lean, "input: {:?}", text);
    }
}
