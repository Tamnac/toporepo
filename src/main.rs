use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod cache;
mod embed;
mod graph;
mod index;
mod lang;
mod render;
mod repomap;
mod semantic;
mod tags;
mod walk;

use std::collections::HashSet;

/// codemapper — a token-budgeted code outline/retrieval map.
///
/// Pipeline: tree-sitter tags -> reference graph -> semantic rerank -> token-budget fit.
#[derive(Parser, Debug)]
#[command(name = "codemapper", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate a ranked, token-budgeted map of the repository.
    Map(MapArgs),
    /// Debug: extract and print tree-sitter tags for the given path.
    Tags(TagsArgs),
    /// Debug: build the reference graph and print top-ranked files.
    Graph(GraphArgs),
    /// Debug: embed definitions and print the top semantic matches for a query.
    Query(QueryArgs),
}

#[derive(clap::Args, Debug)]
struct MapArgs {
    /// Repository root to map (walks files, respecting .gitignore).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Natural-language query driving semantic retrieval. Omit for a generic map.
    #[arg(short, long)]
    query: Option<String>,

    /// Token budget for the rendered map.
    #[arg(short = 'n', long, default_value_t = 1024)]
    tokens: usize,

    /// Identifiers the caller cares about (repeatable / comma-separated). Boosts graph edges.
    #[arg(long, value_delimiter = ',')]
    mentioned_idents: Vec<String>,

    /// Files already in focus (repeatable / comma-separated). Seeds the graph walk.
    #[arg(long, value_delimiter = ',')]
    mentioned_files: Vec<String>,

    /// Path to the potion-code-16M model directory.
    #[arg(long, env = "CODEMAPPER_MODEL")]
    model: Option<PathBuf>,

    /// Disable the persistent tag/embedding cache.
    #[arg(long)]
    no_cache: bool,

    /// Verbose diagnostics to stderr.
    #[arg(short, long)]
    verbose: bool,
}

#[derive(clap::Args, Debug)]
struct TagsArgs {
    /// File or directory to extract tags from.
    #[arg(default_value = ".")]
    path: PathBuf,
}

#[derive(clap::Args, Debug)]
struct QueryArgs {
    /// Repository root.
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Natural-language query.
    #[arg(short, long)]
    query: String,
    /// Number of matches to show.
    #[arg(short, long, default_value_t = 20)]
    top: usize,
    #[arg(long, env = "CODEMAPPER_MODEL")]
    model: Option<PathBuf>,
    #[arg(long)]
    no_cache: bool,
}

#[derive(clap::Args, Debug)]
struct GraphArgs {
    /// Repository root.
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Seed files for the walk (comma-separated). Empty = degree prior.
    #[arg(long, value_delimiter = ',')]
    mentioned_files: Vec<String>,
    /// Boosted identifiers (comma-separated).
    #[arg(long, value_delimiter = ',')]
    mentioned_idents: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Map(args) => cmd_map(&args)?,
        Command::Tags(args) => cmd_tags(&args)?,
        Command::Graph(args) => cmd_graph(&args)?,
        Command::Query(args) => cmd_query(&args)?,
    }
    Ok(())
}

fn cmd_map(args: &MapArgs) -> Result<()> {
    let opts = repomap::Options {
        query: args.query.clone(),
        tokens: args.tokens,
        mentioned_idents: args.mentioned_idents.clone(),
        mentioned_files: args.mentioned_files.clone(),
        model: args.model.clone(),
        no_cache: args.no_cache,
        verbose: args.verbose,
    };
    let map = repomap::run(&args.path, &opts)?;
    if map.is_empty() {
        eprintln!("(empty map: no definitions matched)");
    } else {
        println!("{map}");
    }
    Ok(())
}

fn cmd_query(args: &QueryArgs) -> Result<()> {
    let idx = index::Index::build(&args.path);
    let model_dir = embed::resolve_model(args.model.as_deref())?;
    let embedder = embed::Embedder::load(&model_dir)?;
    let sem = semantic::Semantic::build(&idx, &embedder, &args.path, !args.no_cache);
    eprintln!("{} definitions embedded", sem.defs.len());
    let qv = embedder.encode_one(&args.query);
    for (di, score) in sem.rank(&qv).into_iter().take(args.top) {
        let d = sem.defs[di];
        let t = &idx.files[d.file].tags[d.tag];
        println!("{:>7.4}  {}:{}  {}", score, idx.files[d.file].rel, t.line, t.name);
    }
    Ok(())
}

fn cmd_graph(args: &GraphArgs) -> Result<()> {
    let idx = index::Index::build(&args.path);
    let mentioned: HashSet<String> = args.mentioned_idents.iter().cloned().collect();
    let chat: HashSet<usize> = args
        .mentioned_files
        .iter()
        .filter_map(|f| idx.files.iter().position(|fd| &fd.rel == f))
        .collect();
    let g = graph::Graph::build(&idx, &mentioned, &chat);

    let seeds: std::collections::HashMap<usize, f32> = if chat.is_empty() {
        g.degree_prior()
    } else {
        chat.iter().map(|&i| (i, 1.0)).collect()
    };
    let rank = g.walk(&seeds, 3, 0.5);
    let mut ranked: Vec<(usize, f32)> = rank.iter().copied().enumerate().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (i, r) in ranked.into_iter().take(30) {
        if r <= 0.0 {
            break;
        }
        println!("{:>10.4}  {}", r, idx.files[i].rel);
    }
    Ok(())
}

fn cmd_tags(args: &TagsArgs) -> Result<()> {
    let root = &args.path;
    let files = walk::source_files(root);
    let mut n_def = 0usize;
    let mut n_ref = 0usize;
    for file in &files {
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };
        let tags = tags::extract(file, &src);
        if tags.is_empty() {
            continue;
        }
        let rel = walk::rel(file, root);
        println!("{rel}");
        for t in &tags {
            match t.kind {
                tags::Kind::Def => n_def += 1,
                tags::Kind::Ref => n_ref += 1,
            }
            let k = match t.kind {
                tags::Kind::Def => "def",
                tags::Kind::Ref => "ref",
            };
            println!("  {:<4} {:>4}-{:<4} {}", k, t.line, t.end_line, t.name);
        }
    }
    eprintln!(
        "{} files, {} defs, {} refs",
        files.len(),
        n_def,
        n_ref
    );
    Ok(())
}
