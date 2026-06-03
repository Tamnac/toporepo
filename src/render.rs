//! Outline rendering: show selected definition lines together with their
//! enclosing scope headers, with blank-line separators at gaps.

use crate::lang::Lang;
use crate::tags::Tag;
use std::collections::BTreeSet;
use tree_sitter::Parser;

/// Only these ancestor node kinds produce useful scope-header lines.
fn is_scope_kind(kind: &str) -> bool {
    const KW: &[&str] = &[
        "function", "method", "class", "struct", "enum", "impl",
        "interface", "module", "namespace", "trait",
    ];
    KW.iter().any(|kw| kind.contains(kw))
}

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
            lines.insert(t.end_line);
            if let Some(dl) = t.doc_line {
                for l in dl..t.line {
                    lines.insert(l);
                }
            }
            // Walk ancestors, adding the header line of each multi-line scope above.
            if let Some(node) = root.descendant_for_byte_range(t.start_byte, t.start_byte) {
                let mut cur = node.parent();
                while let Some(p) = cur {
                    let s = p.start_position().row + 1;
                    let e = p.end_position().row + 1;
                    if e > s && s < sig && p.parent().is_some() && is_scope_kind(p.kind()) {
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
/// with blank lines where lines are skipped.
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
        if let Some(p) = prev {
            if ln > p + 1 { out.push('\n'); }
        }
        out.push_str(&format!("{:>width$}| {}\n", ln, src_lines[ln - 1], width = width));
        prev = Some(ln);
    }
    out
}

/// Approximate token count (~4 chars/token) for budget fitting.
pub fn approx_tokens(s: &str) -> usize {
    (s.chars().count() as f64 / 3.8) as usize
}
