//! tree-police — a ripgrep-style scanner that runs baked-in tree-sitter queries
//! against source files and reports findings tagged with a severity.
//!
//! The queries live flat in `queries/*.scm`, named `<label>-<code>.scm` where
//! `<code>` routes to a [`registry::LangSpec`], and are embedded into the
//! binary at compile time (see [`rules`]). Each query pattern may declare its
//! severity with a `(#set! severity "error|warning|log")` directive; helper
//! captures (used only to drive predicates) are named with a leading `_` and
//! are not reported.

pub mod registry;
pub mod report;
pub mod rules;
pub mod scan;
pub mod severity;
pub mod tree_view;

pub use report::Format;
pub use scan::{Finding, ScanOptions, ScanStats};
pub use severity::Severity;
