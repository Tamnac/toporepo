//! Reference graph: weighted `(referencer -> definer)` edges built from shared
//! identifier names, plus a local spreading-activation walk from seed files.
//!
//! Edge weighting follows aider's `get_ranked_tags` (with the `len >= 8` case gate
//! dropped, per PLAN.md): per-identifier multiplier x chat bonus x sqrt(num_refs).

use crate::index::Index;
use std::collections::{HashMap, HashSet};

pub struct Graph {
    /// Number of file nodes (indices align with `Index::files`).
    n: usize,
    /// Undirected adjacency: node -> {neighbor -> combined weight}.
    adj: Vec<HashMap<usize, f32>>,
}

/// Identifier multiplier (aider-style, no length gate).
fn ident_mul(name: &str, n_def_files: usize, mentioned: &HashSet<String>) -> f32 {
    let mut mul = 1.0f32;
    if mentioned.contains(name) {
        mul *= 10.0;
    }
    if is_structured_name(name) {
        mul *= 10.0;
    }
    if name.starts_with('_') {
        mul *= 0.1;
    }
    if n_def_files > 5 {
        mul *= 0.1;
    }
    mul
}

/// snake_case / kebab-case / camelCase detection.
fn is_structured_name(name: &str) -> bool {
    let has_sep = name.trim_matches('_').contains('_') || name.contains('-');
    let mut chars = name.chars();
    let camel = match chars.next() {
        Some(_) => name
            .chars()
            .zip(name.chars().skip(1))
            .any(|(a, b)| a.is_lowercase() && b.is_uppercase()),
        None => false,
    };
    has_sep || camel
}

impl Graph {
    pub fn build(
        index: &Index,
        mentioned_idents: &HashSet<String>,
        chat_files: &HashSet<usize>,
    ) -> Graph {
        let n = index.files.len();

        // Directed edge accumulation: (referencer, definer) -> weight.
        let mut directed: HashMap<(usize, usize), f32> = HashMap::new();
        for (name, refs) in &index.references {
            let Some(def_files) = index.defines.get(name) else {
                continue;
            };
            let mul = ident_mul(name, def_files.len(), mentioned_idents);
            for (&ref_file, &count) in refs {
                let chat_bonus = if chat_files.contains(&ref_file) { 50.0 } else { 1.0 };
                let w = mul * chat_bonus * (count as f32).sqrt();
                for &def_file in def_files {
                    if def_file == ref_file {
                        continue;
                    }
                    *directed.entry((ref_file, def_file)).or_insert(0.0) += w;
                }
            }
        }

        // Fold into undirected adjacency for the walk.
        let mut adj: Vec<HashMap<usize, f32>> = vec![HashMap::new(); n];
        for ((a, b), w) in directed {
            *adj[a].entry(b).or_insert(0.0) += w;
            *adj[b].entry(a).or_insert(0.0) += w;
        }

        Graph { n, adj }
    }

    /// Weighted degree of every node (sum of incident edge weights). Used as a
    /// generic prior when there are no semantic/explicit seeds.
    pub fn degree_prior(&self) -> HashMap<usize, f32> {
        self.adj
            .iter()
            .enumerate()
            .map(|(i, nbrs)| (i, nbrs.values().sum()))
            .filter(|(_, w): &(usize, f32)| *w > 0.0)
            .collect()
    }

    /// Spreading-activation walk: starting from weighted `seeds`, propagate over
    /// `hops` with per-hop `decay`, normalising each node's outgoing edges so
    /// high-degree hubs don't dominate. Returns accumulated rank per node.
    pub fn walk(&self, seeds: &HashMap<usize, f32>, hops: usize, decay: f32) -> Vec<f32> {
        let mut rank = vec![0.0f32; self.n];
        let mut active = seeds.clone();
        for (&i, &w) in seeds {
            rank[i] += w;
        }
        for _ in 0..hops {
            let mut next: HashMap<usize, f32> = HashMap::new();
            for (&node, &a) in &active {
                let total: f32 = self.adj[node].values().sum();
                if total <= 0.0 {
                    continue;
                }
                for (&nbr, &w) in &self.adj[node] {
                    let push = a * (w / total) * decay;
                    if push > 1e-6 {
                        *next.entry(nbr).or_insert(0.0) += push;
                    }
                }
            }
            for (&i, &w) in &next {
                rank[i] += w;
            }
            active = next;
            if active.is_empty() {
                break;
            }
        }
        rank
    }
}
