//! Semantic index: embed every definition's code (cached per-def by body hash),
//! then rank definitions against a natural-language query by cosine similarity.

use crate::cache;
use crate::embed::{self, Embedder};
use crate::index::Index;
use crate::tags::Tag;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::Parser;

/// Max characters of a definition's code fed to the embedder (the model also
/// truncates by tokens; this just bounds tokenisation work on huge bodies).
const MAX_DEF_CHARS: usize = 2000;

/// A definition located by (file index, index into that file's `tags`).
#[derive(Clone, Copy)]
pub struct DefRef {
    pub file: usize,
    pub tag: usize,
}

pub struct Semantic {
    pub defs: Vec<DefRef>,
    /// Embedding per def (aligned with `defs`), L2-normalised.
    pub vecs: Vec<Vec<f32>>,
}

/// Extract the text embedded for a definition tag: repository context first,
/// then docs and the definition code. The code slice includes the definition
/// header/signature because it is the full tree-sitter definition node.
pub fn def_text(source: &str, rel_path: &str, scopes: &[String], tag: &Tag) -> String {
    let end = tag.end_byte.min(source.len());
    let start = tag.start_byte.min(end);
    let slice = &source[start..end];
    let slice = if slice.len() > MAX_DEF_CHARS {
        // Cap on a char boundary.
        let mut e = MAX_DEF_CHARS;
        while !slice.is_char_boundary(e) {
            e -= 1;
        }
        &slice[..e]
    } else {
        slice
    };
    let body = slice.trim();
    let mut parts = Vec::with_capacity(2 + scopes.len() + usize::from(tag.doc.is_some()));
    parts.push(format!("path: {rel_path}"));
    parts.extend(scopes.iter().map(|scope| format!("scope: {scope}")));
    if let Some(doc) = &tag.doc {
        parts.push(doc.clone());
    }
    parts.push(body.to_string());
    parts.join("\n")
}

/// Extract short signatures for enclosing type/module scopes of each def.
fn scope_signatures_for_defs(
    path: &Path,
    source: &str,
    defs: &[(usize, &Tag)],
) -> HashMap<usize, Vec<String>> {
    let Some(lang) = crate::lang::detect(path) else {
        return HashMap::new();
    };
    let mut parser = Parser::new();
    if parser.set_language(&lang.language).is_err() {
        return HashMap::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return HashMap::new();
    };
    let root = tree.root_node();

    defs.iter()
        .filter_map(|(tag_idx, tag)| {
            let node = root.descendant_for_byte_range(tag.start_byte, tag.start_byte)?;
            let mut scopes = Vec::new();
            let mut cur = node.parent();
            while let Some(parent) = cur {
                let s = parent.start_position().row + 1;
                let e = parent.end_position().row + 1;
                if e > s
                    && s < tag.line
                    && parent.parent().is_some()
                    && is_container_scope_kind(parent.kind())
                {
                    if let Some(sig) = line_at(source, s) {
                        scopes.push(sig.trim().to_string());
                    }
                }
                cur = parent.parent();
            }
            scopes.reverse();
            if scopes.is_empty() {
                None
            } else {
                Some((*tag_idx, scopes))
            }
        })
        .collect()
}

fn is_container_scope_kind(kind: &str) -> bool {
    const KW: &[&str] = &[
        "class",
        "struct",
        "enum",
        "impl",
        "interface",
        "module",
        "namespace",
        "trait",
    ];
    KW.iter().any(|kw| kind.contains(kw))
}

fn line_at(source: &str, line: usize) -> Option<&str> {
    source.lines().nth(line.checked_sub(1)?)
}

impl Semantic {
    /// Build the definition embedding index, reusing cached vectors where valid.
    pub fn build(
        index: &Index,
        embedder: &Embedder,
        root: &std::path::Path,
        use_cache: bool,
    ) -> Self {
        let cache = if use_cache {
            cache::Cache::open(root)
        } else {
            None
        };
        let mut defs = Vec::new();
        let mut vecs = Vec::new();

        for (fi, file) in index.files.iter().enumerate() {
            let def_tags: Vec<(usize, &Tag)> = index.defs(fi).collect();
            if def_tags.is_empty() {
                continue;
            }
            let scopes = scope_signatures_for_defs(&file.path, &file.source, &def_tags);

            // Per-def: compute body text + hash, check cache.
            let mut entries: Vec<(usize, i64, Option<Vec<f32>>)> = Vec::new();
            let mut to_embed: Vec<String> = Vec::new();
            for &(tag_idx, t) in &def_tags {
                let text = def_text(
                    &file.source,
                    &file.rel,
                    scopes.get(&tag_idx).map(Vec::as_slice).unwrap_or(&[]),
                    t,
                );
                let hash = cache::body_hash(&text);
                let cached = cache.as_ref().and_then(|c| c.get(&file.rel, hash));
                if cached.is_none() {
                    to_embed.push(text);
                }
                entries.push((tag_idx, hash, cached));
            }

            let new_vecs = if to_embed.is_empty() {
                Vec::new()
            } else {
                embedder.encode(&to_embed)
            };
            let mut new_iter = new_vecs.into_iter();

            for (tag_idx, hash, cached) in entries {
                let vec = match cached {
                    Some(v) => v,
                    None => {
                        let v = new_iter.next().unwrap_or_default();
                        if let Some(c) = &cache {
                            c.put(&file.rel, hash, &v);
                        }
                        v
                    }
                };
                defs.push(DefRef {
                    file: fi,
                    tag: tag_idx,
                });
                vecs.push(vec);
            }
        }

        Semantic { defs, vecs }
    }

    /// Rank definitions against `query_vec`; returns (def index, cosine) sorted desc.
    pub fn rank(&self, query_vec: &[f32]) -> Vec<(usize, f32)> {
        let mut scored: Vec<(usize, f32)> = self
            .vecs
            .iter()
            .enumerate()
            .map(|(i, v)| (i, embed::cosine(query_vec, v)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored
    }

    /// Aggregate the top-`k` definition matches into per-file seed scores, with a
    /// file-coherence boost (multiple relevant defs in one file rank it higher).
    pub fn seed_files(&self, ranked: &[(usize, f32)], k: usize) -> HashMap<usize, f32> {
        let mut per_file: HashMap<usize, (f32, f32)> = HashMap::new(); // file -> (max, sum_rest)
        for &(di, score) in ranked.iter().take(k) {
            if score <= 0.0 {
                continue;
            }
            let file = self.defs[di].file;
            let e = per_file.entry(file).or_insert((0.0, 0.0));
            if score > e.0 {
                e.1 += e.0; // demote previous max into the rest-sum
                e.0 = score;
            } else {
                e.1 += score;
            }
        }
        per_file
            .into_iter()
            .map(|(f, (max, rest))| (f, max + 0.25 * rest))
            .collect()
    }
}
