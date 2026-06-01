//! Outline rendering, in the spirit of grep_ast's `TreeContext`: show selected
//! definition lines together with their enclosing scope headers, collapsing the
//! gaps with an ellipsis marker.

use crate::lang::Lang;
use crate::tags::Tag;
use std::collections::BTreeSet;
use tree_sitter::Parser;

/// For each definition (keyed by its index into the file's `tags`), compute the
/// 1-based line numbers to display: the signature line plus the start line of
/// every enclosing multi-line scope (so methods show their class/impl header).
pub fn plan_lines(lang: &Lang, source: &str, defs: &[(usize, &Tag)]) -> Vec<(usize, Vec<usize>)> {
    let mut parser = Parser::new();
    if parser.set_language(&lang.language).is_err() {
        return defs.iter().map(|(i, t)| (*i, vec![t.line])).collect();
    }
    let Some(tree) = parser.parse(source, None) else {
        return defs.iter().map(|(i, t)| (*i, vec![t.line])).collect();
    };
    let root = tree.root_node();

    defs.iter()
        .map(|(i, t)| {
            let mut lines = BTreeSet::new();
            let sig = t.line; // 1-based name/signature line
            lines.insert(sig);
            // Walk ancestors, adding the header line of each multi-line scope above.
            if let Some(node) = root.descendant_for_byte_range(t.start_byte, t.start_byte) {
                let mut cur = node.parent();
                while let Some(p) = cur {
                    let s = p.start_position().row + 1;
                    let e = p.end_position().row + 1;
                    if e > s && s < sig {
                        lines.insert(s);
                    }
                    cur = p.parent();
                }
            }
            (*i, lines.into_iter().collect())
        })
        .collect()
}

/// Render one file: `rel` header followed by the chosen `lines` (1-based),
/// with `...` markers where lines are skipped.
pub fn render_file(rel: &str, source: &str, lines: &BTreeSet<usize>) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let src_lines: Vec<&str> = source.lines().collect();
    let width = src_lines.len().to_string().len().max(3);
    let mut out = String::new();
    out.push_str(rel);
    out.push('\n');

    let mut prev: Option<usize> = None;
    for &ln in lines {
        if ln == 0 || ln > src_lines.len() {
            continue;
        }
        match prev {
            Some(p) if ln == p + 1 => {}
            Some(_) => out.push_str(&format!("{:>width$}  ...\n", "", width = width)),
            None => {}
        }
        out.push_str(&format!("{:>width$}| {}\n", ln, src_lines[ln - 1], width = width));
        prev = Some(ln);
    }
    out
}

/// Approximate token count (~4 chars/token) for budget fitting.
pub fn approx_tokens(s: &str) -> usize {
    (s.chars().count() + 3) / 4
}
