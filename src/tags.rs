//! Tag extraction: parse a file with tree-sitter and run its `.scm` query to
//! collect definition and reference tags.

use crate::lang;
use serde::{Deserialize, Serialize};
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Def,
    Ref,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub kind: Kind,
    /// 1-based start line of the name node.
    pub line: usize,
    /// 1-based end line of the enclosing definition node (== `line` for refs).
    pub end_line: usize,
    /// Byte range of the enclosing definition node (used to extract code for embedding).
    pub start_byte: usize,
    pub end_byte: usize,
    pub doc: Option<String>,
    pub doc_line: Option<usize>,
}

/// Parse `source` for `path` and return its tags. Returns empty for unsupported
/// languages or on parse/query failure (never panics on bad input).
pub fn extract(path: &Path, source: &str) -> Vec<Tag> {
    let Some(lang) = lang::detect(path) else {
        return Vec::new();
    };
    extract_with(&lang, source).unwrap_or_default()
}

fn extract_with(lang: &lang::Lang, source: &str) -> Option<Vec<Tag>> {
    let mut parser = Parser::new();
    parser.set_language(&lang.language).ok()?;
    let tree = parser.parse(source, None)?;
    let query = Query::new(&lang.language, lang.query).ok()?;
    let names = query.capture_names();
    let src = source.as_bytes();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), src);
    let mut tags = Vec::new();
    // Standalone @doc captures (not part of a def match): (start_line, end_line, text).
    let mut loose_docs: Vec<(usize, usize, String)> = Vec::new();

    while let Some(m) = matches.next() {
        let mut name_cap = None;
        let mut full_cap = None;
        let mut kind = None;
        let mut doc_nodes: Vec<tree_sitter::Node> = Vec::new();
        for cap in m.captures {
            let cname = names[cap.index as usize];
            if cname == "doc" {
                doc_nodes.push(cap.node);
            } else if cname.starts_with("name.") {
                name_cap = Some(cap.node);
                if cname.contains("definition") {
                    kind = Some(Kind::Def);
                } else if cname.contains("reference") {
                    kind = Some(Kind::Ref);
                }
            } else if cname.starts_with("definition") {
                full_cap = Some(cap.node);
                kind = kind.or(Some(Kind::Def));
            } else if cname.starts_with("reference") {
                full_cap = Some(cap.node);
                kind = kind.or(Some(Kind::Ref));
            }
        }

        // Standalone doc (no def in this match) — collect for adjacency pass.
        if name_cap.is_none() {
            for n in doc_nodes {
                if let Ok(t) = n.utf8_text(src) {
                    loose_docs.push((
                        n.start_position().row + 1,
                        n.end_position().row + 1,
                        t.to_string(),
                    ));
                }
            }
            continue;
        }

        let (Some(name_node), Some(kind)) = (name_cap, kind) else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(src) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let body = full_cap.unwrap_or(name_node);

        // Inline doc from the same match.
        let (doc, doc_line) = if kind == Kind::Def && !doc_nodes.is_empty() {
            doc_nodes.sort_by_key(|n| n.start_byte());
            let first_line = doc_nodes[0].start_position().row + 1;
            let raw: String = doc_nodes
                .iter()
                .filter_map(|n| n.utf8_text(src).ok())
                .collect::<Vec<_>>()
                .join("\n");
            let stripped = strip_comment_markers(&raw);
            if stripped.is_empty() {
                (None, None)
            } else {
                (Some(stripped), Some(first_line))
            }
        } else {
            (None, None)
        };

        tags.push(Tag {
            name: name.to_string(),
            kind,
            line: name_node.start_position().row + 1,
            end_line: body.end_position().row + 1,
            start_byte: body.start_byte(),
            end_byte: body.end_byte(),
            doc,
            doc_line,
        });
    }

    // Attach loose doc comments to the immediately following def.
    if !loose_docs.is_empty() {
        loose_docs.sort_by_key(|(_, end, _)| *end);
        let mut used = vec![false; loose_docs.len()];
        for tag in &mut tags {
            if tag.kind != Kind::Def || tag.doc.is_some() {
                continue;
            }
            let found = loose_docs
                .iter()
                .enumerate()
                .rev()
                .filter(|(i, _)| !used[*i])
                .find(|(_, (_, end, _))| *end < tag.line && tag.line - *end <= 2)
                .map(|(i, (start, _, text))| (i, *start, text.as_str()));
            if let Some((i, start_line, text)) = found {
                let stripped = strip_comment_markers(text);
                if !stripped.is_empty() {
                    tag.doc_line = Some(start_line);
                    tag.doc = Some(stripped);
                    used[i] = true;
                }
            }
        }
    }

    Some(tags)
}

/// Strip comment markers (`//`, `///`, `/*`, `*/`, `*`, `#`) and collapse
/// to a single line.
fn strip_comment_markers(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            let t = line.trim();
            let t = t
                .strip_prefix("///")
                .or_else(|| t.strip_prefix("//!"))
                .or_else(|| t.strip_prefix("//"))
                .or_else(|| t.strip_prefix("/**"))
                .or_else(|| t.strip_prefix("/*"))
                .or_else(|| t.strip_prefix("*/"))
                .or_else(|| t.strip_prefix("* "))
                .or_else(|| t.strip_prefix('*'))
                .or_else(|| t.strip_prefix('#'))
                .unwrap_or(t);
            t.trim_end_matches("*/").trim()
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
