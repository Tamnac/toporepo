use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod lang;
mod tags;
mod walk;

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Map(args) => {
            eprintln!("map: not yet implemented (path={:?}, query={:?}, tokens={})", args.path, args.query, args.tokens);
        }
        Command::Tags(args) => cmd_tags(&args)?,
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
