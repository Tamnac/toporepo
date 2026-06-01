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

    while let Some(m) = matches.next() {
        // Within a single pattern match, locate the name node and the enclosing
        // definition/reference node (the captures without the `name.` prefix).
        let mut name_cap = None;
        let mut full_cap = None;
        let mut kind = None;
        for cap in m.captures {
            let cname = names[cap.index as usize];
            if cname.starts_with("name.") {
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
        tags.push(Tag {
            name: name.to_string(),
            kind,
            line: name_node.start_position().row + 1,
            end_line: body.end_position().row + 1,
            start_byte: body.start_byte(),
            end_byte: body.end_byte(),
        });
    }
    Some(tags)
}
