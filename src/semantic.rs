//! Semantic index: embed every definition's code (cached by mtime/size), then
//! rank definitions against a natural-language query by cosine similarity.

use crate::cache::{self, Cache};
use crate::embed::{self, Embedder};
use crate::index::Index;
use crate::tags::Tag;
use std::collections::HashMap;

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

/// Extract the code text of a definition tag.
pub fn def_text(source: &str, tag: &Tag) -> String {
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
    slice.trim().to_string()
}

impl Semantic {
    /// Build the definition embedding index, reusing cached vectors where valid.
    pub fn build(index: &Index, embedder: &Embedder, root: &std::path::Path, use_cache: bool) -> Self {
        let mut cache = if use_cache { Cache::load(root) } else { Cache::default() };
        let mut defs = Vec::new();
        let mut vecs = Vec::new();

        for (fi, file) in index.files.iter().enumerate() {
            let def_tags: Vec<(usize, &Tag)> = index.defs(fi).collect();
            if def_tags.is_empty() {
                continue;
            }
            let key = cache::meta_key(&file.path);
            let cached = match (use_cache, key) {
                (true, Some((mtime, size))) => cache
                    .get(&file.rel, mtime, size)
                    .filter(|v| v.len() == def_tags.len())
                    .map(|v| v.to_vec()),
                _ => None,
            };

            let file_vecs = match cached {
                Some(v) => v,
                None => {
                    let texts: Vec<String> = def_tags
                        .iter()
                        .map(|(_, t)| def_text(&file.source, t))
                        .collect();
                    let v = embedder.encode(&texts);
                    if use_cache {
                        if let Some((mtime, size)) = key {
                            cache.put(file.rel.clone(), mtime, size, v.clone());
                        }
                    }
                    v
                }
            };

            for ((tag_idx, _), vec) in def_tags.iter().zip(file_vecs) {
                defs.push(DefRef { file: fi, tag: *tag_idx });
                vecs.push(vec);
            }
        }

        if use_cache {
            let _ = cache.save(root);
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
