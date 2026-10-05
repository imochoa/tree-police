//! Indented syntax-tree pretty-printer, used by `--show-tree`.

use std::collections::HashSet;
use std::io::Write;

use anyhow::{Context, Result};
use tree_sitter::Tree;

/// Print an indented tree view of `tree` to `out`. Each line is
/// `{node kind} {row}:{column}`, with leaf nodes also showing their source
/// text; row/column are 1-indexed, matching `Finding`'s convention
/// elsewhere in this crate.
pub fn print_tree(tree: &Tree, source: &[u8], mut out: impl Write) -> Result<()> {
    let mut cursor = tree.walk();
    let mut visited: HashSet<usize> = HashSet::new();
    let mut indent = 0;

    loop {
        let node = cursor.node();
        let node_visited = visited.contains(&node.id());
        let is_leaf = node.child_count() == 0;

        visited.insert(node.id());

        if !node_visited {
            write!(
                out,
                "{}{} {}:{}",
                "  ".repeat(indent),
                node.kind(),
                node.start_position().row + 1,
                node.start_position().column + 1,
            )?;
            if is_leaf {
                writeln!(
                    out,
                    ": {}",
                    node.utf8_text(source)
                        .context("could not get node source")?
                )?;
            } else {
                writeln!(out)?;
            }
        }

        if !is_leaf && !node_visited && cursor.goto_first_child() {
            indent += 1;
            continue;
        }

        if cursor.goto_next_sibling() {
            continue;
        }

        if cursor.goto_parent() {
            indent -= 1;
            continue;
        }

        break;
    }

    Ok(())
}
