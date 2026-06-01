//! Map assembly: blend the reference-graph walk with the semantic ranking,
//! fit the result to a token budget, and render the outline.

use crate::embed::{self, Embedder};
use crate::graph::Graph;
use crate::index::Index;
use crate::render;
use crate::semantic::Semantic;
use anyhow::Result;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// How strongly a definition's own semantic similarity reorders it on top of
/// its file's graph rank.
const SEM_ALPHA: f32 = 5.0;
/// Number of top semantic matches used to seed the graph walk.
const SEED_K: usize = 15;
const WALK_HOPS: usize = 3;
const WALK_DECAY: f32 = 0.5;

pub struct Options {
    pub query: Option<String>,
    pub tokens: usize,
    pub mentioned_idents: Vec<String>,
    pub mentioned_files: Vec<String>,
    pub model: Option<PathBuf>,
    pub no_cache: bool,
    pub verbose: bool,
}

/// A definition selected for the map: (file index, index into file's tags).
struct RankedDef {
    file: usize,
    tag: usize,
    score: f32,
}

pub fn run(root: &Path, opts: &Options) -> Result<String> {
    let idx = Index::build(root);
    if idx.files.is_empty() {
        return Ok(String::new());
    }

    let mentioned: HashSet<String> = opts.mentioned_idents.iter().cloned().collect();
    let chat: HashSet<usize> = opts
        .mentioned_files
        .iter()
        .filter_map(|f| file_index(&idx, f))
        .collect();

    // Semantic ranking (only when a query is given; otherwise no model needed).
    let mut def_sim: HashMap<(usize, usize), f32> = HashMap::new();
    let mut sem_seeds: HashMap<usize, f32> = HashMap::new();
    if let Some(q) = &opts.query {
        let model_dir = embed::resolve_model(opts.model.as_deref())?;
        let embedder = Embedder::load(&model_dir)?;
        let sem = Semantic::build(&idx, &embedder, root, !opts.no_cache);
        if opts.verbose {
            eprintln!("embedded {} definitions", sem.defs.len());
        }
        let qv = embedder.encode_one(q);
        let ranked = sem.rank(&qv);
        for &(di, score) in &ranked {
            let d = sem.defs[di];
            def_sim.insert((d.file, d.tag), score);
        }
        sem_seeds = sem.seed_files(&ranked, SEED_K);
    }

    // Reference graph + seeds.
    let graph = Graph::build(&idx, &mentioned, &chat);
    let mut seeds = sem_seeds;
    let seed_floor = seeds.values().cloned().fold(0.0f32, f32::max).max(1.0);
    for &fi in &chat {
        seeds.insert(fi, seed_floor);
    }
    if seeds.is_empty() {
        seeds = graph.degree_prior();
    }
    let file_rank = graph.walk(&seeds, WALK_HOPS, WALK_DECAY);

    // Final per-definition score = file rank, refined by the def's own similarity.
    let querying = opts.query.is_some();
    let mut defs: Vec<RankedDef> = Vec::new();
    for fi in 0..idx.files.len() {
        let fr = file_rank[fi];
        if fr <= 0.0 {
            continue;
        }
        for (tag_idx, _) in idx.defs(fi) {
            let sim = def_sim.get(&(fi, tag_idx)).copied().unwrap_or(0.0).max(0.0);
            let score = if querying { fr * (1.0 + SEM_ALPHA * sim) } else { fr };
            defs.push(RankedDef { file: fi, tag: tag_idx, score });
        }
    }
    defs.sort_by(|a, b| b.score.total_cmp(&a.score));
    if defs.is_empty() {
        return Ok(String::new());
    }

    // Precompute the display lines for every candidate definition (parse once/file).
    let mut plan: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    let mut per_file: HashMap<usize, Vec<(usize, &crate::tags::Tag)>> = HashMap::new();
    for d in &defs {
        per_file
            .entry(d.file)
            .or_default()
            .push((d.tag, &idx.files[d.file].tags[d.tag]));
    }
    for (&fi, dts) in &per_file {
        if let Some(lang) = crate::lang::detect(&idx.files[fi].path) {
            for (tag_idx, lines) in render::plan_lines(&lang, &idx.files[fi].source, dts) {
                plan.insert((fi, tag_idx), lines);
            }
        }
    }

    // Binary-search the number of definitions that fit the token budget.
    let assemble = |n: usize| -> String { assemble_map(&idx, &defs[..n], &plan) };
    let mut lo = 0usize;
    let mut hi = defs.len();
    let mut best = String::new();
    while lo <= hi {
        let mid = (lo + hi) / 2;
        if mid == 0 {
            lo = 1;
            continue;
        }
        let out = assemble(mid);
        if render::approx_tokens(&out) <= opts.tokens {
            best = out;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    if best.is_empty() && !defs.is_empty() {
        // Budget too small for even one definition; emit the single best anyway.
        best = assemble(1);
    }
    Ok(best)
}

fn assemble_map(
    idx: &Index,
    defs: &[RankedDef],
    plan: &HashMap<(usize, usize), Vec<usize>>,
) -> String {
    // Group selected defs by file, tracking each file's best score for ordering.
    let mut by_file: HashMap<usize, (f32, BTreeSet<usize>)> = HashMap::new();
    for d in defs {
        let entry = by_file.entry(d.file).or_insert((d.score, BTreeSet::new()));
        if d.score > entry.0 {
            entry.0 = d.score;
        }
        if let Some(lines) = plan.get(&(d.file, d.tag)) {
            entry.1.extend(lines.iter().copied());
        }
    }
    let mut files: Vec<(usize, f32, BTreeSet<usize>)> = by_file
        .into_iter()
        .map(|(fi, (s, lines))| (fi, s, lines))
        .collect();
    files.sort_by(|a, b| b.1.total_cmp(&a.1).then(idx.files[a.0].rel.cmp(&idx.files[b.0].rel)));

    let mut parts = Vec::new();
    for (fi, _, lines) in files {
        let rendered = render::render_file(&idx.files[fi].rel, &idx.files[fi].source, &lines);
        if !rendered.is_empty() {
            parts.push(rendered);
        }
    }
    parts.join("\n")
}

fn file_index(idx: &Index, rel: &str) -> Option<usize> {
    let want = rel.replace('\\', "/");
    idx.files.iter().position(|f| f.rel == want)
}
