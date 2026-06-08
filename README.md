# toporepo

A token-budgeted code outline/retrieval map for LLM agents. Given a repository
(and optionally a natural-language query), it emits a compact, ranked outline of
the most relevant definitions.

Concept derived from aider's `repomap.py`, reworked around a static-embedding semantic ranking
layer.

## Usage

```sh
# Generic structural map of current directory, ~1024 tokens
toporepo

# Query-driven map of a specific folder
toporepo path/to/repo -q "where are tokens counted for the budget" -n 2048

# Hint identifiers the agent already cares about (boosts graph edges + exact defs)
toporepo map . -q "reference graph" --idents get_ranked_tags,walk

# Files already in focus seed the walk
toporepo map . --files src/graph.rs,src/index.rs
```

### Options

| flag | meaning |
|------|---------|
| `-q, --query <TEXT>` | natural-language query driving semantic retrieval |
| `-n, --tokens <N>` | token budget (default 1024) |
| `--idents <A,B>` | identifiers to boost |
| `--files <A,B>` | files to seed the walk |
| `--model <DIR>` | path to the potion-code-16M model directory |
| `--no-cache` | disable the embedding cache |
| `-v, --verbose` | diagnostics to stderr |

### Debug subcommands

- `toporepo tags <path>` — dump extracted def/ref tags.
- `toporepo graph <path>` — print top files by graph rank.
- `toporepo query <path> -q <text>` — print top semantic matches.

## Model

The semantic layer needs the `potion-code-16M` model directory. The
model is downloaded from HuggingFace (`minishlab/potion-code-16M`) on first use
and cached in the standard HuggingFace cache.

## Build

```sh
cargo build --release  
```

The `.scm` tag queries are vendored under `queries/` and bundled into the binary.
Embedding caches are stored globally (the OS cache dir, e.g. `%LOCALAPPDATA%\toporepo`
or `~/.cache/toporepo`, overridable with `TOPOREPO_CACHE_DIR`), one db per repo,
never inside the scanned repository.

Tag queries adapted from aider (MIT).
