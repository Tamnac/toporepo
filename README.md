# toporepo

A token-budgeted code outline/retrieval map for LLM agents. Given a repository
(and optionally a natural-language query), it emits a compact, ranked outline of
the most relevant definitions — the file/function structure an agent needs to
orient itself, fit to a token budget.

Concept derived from aider's `repomap.py` (tree-sitter tags → reference graph →
token-budgeted outline), reworked around a static-embedding semantic ranking
layer. See [PLAN.md](PLAN.md) for the design rationale.

## Pipeline

```
tree-sitter tags → reference graph → semantic rerank → token-budget fit → outline
```

1. **Tags** — tree-sitter parses each file; bundled `.scm` queries extract
   definitions and references (Rust, Python, JS, TS/TSX, Go, Java, C, C++, C#, Ruby).
2. **Reference graph** — weighted `(referencer → definer)` edges from shared
   identifier names. Edge weight = identifier multiplier × chat bonus ×
   `sqrt(num_refs)` (aider-style, without the length gate).
3. **Semantic layer** — every definition's code is embedded with
   [`potion-code-16M`](https://huggingface.co/minishlab/potion-code-16M) (a static
   model, via [`model2vec-rs`](https://github.com/MinishLab/model2vec-rs)) and
   cached by file mtime/size. A query is embedded and cosine-ranked; the top
   matches seed a local walk over the graph.
4. **Assembly** — definitions are ranked (generic: graph rank; query: scale-free
   RRF fusion of graph and semantic rankings), then a binary search fits the
   rendered outline to the token budget. Rendering shows definition lines with
   their enclosing scope headers and `...` gap markers.

## Usage

```sh
# Generic structural map of a repo, ~1024 tokens
toporepo map path/to/repo

# Query-driven map
toporepo map path/to/repo -q "where are tokens counted for the budget" -n 2048

# Hint identifiers the agent already cares about (boosts graph edges + exact defs)
toporepo map . -q "reference graph" --mentioned-idents get_ranked_tags,walk

# Files already in focus seed the walk
toporepo map . --mentioned-files src/graph.rs,src/index.rs
```

### Options (`map`)

| flag | meaning |
|------|---------|
| `-q, --query <TEXT>` | natural-language query driving semantic retrieval |
| `-n, --tokens <N>` | token budget (default 1024) |
| `--mentioned-idents <A,B>` | identifiers to boost |
| `--mentioned-files <A,B>` | files to seed the walk |
| `--model <DIR>` | path to the potion-code-16M model directory |
| `--no-cache` | disable the embedding cache |
| `-v, --verbose` | diagnostics to stderr |

### Debug subcommands

- `toporepo tags <path>` — dump extracted def/ref tags.
- `toporepo graph <path>` — print top files by graph rank.
- `toporepo query <path> -q <text>` — print top semantic matches.

## Model

The semantic layer needs the `potion-code-16M` model directory (the one
containing `model.safetensors`, `tokenizer.json`, `config.json`). It is resolved
in order from: `--model`, the `TOPOREPO_MODEL` env var, `./potion-code-16M`,
`../potion-code-16M`, then next to the executable. If none of those exist, the
model is downloaded from HuggingFace (`minishlab/potion-code-16M`) on first use
and cached in the standard HuggingFace cache. Only required when a query is
given — generic maps run without it.

## Build

```sh
cargo build --release   # target/release/toporepo
```

The `.scm` tag queries are vendored under `queries/` and bundled into the binary.
Embedding caches are stored globally (the OS cache dir, e.g. `%LOCALAPPDATA%\toporepo`
or `~/.cache/toporepo`, overridable with `TOPOREPO_CACHE_DIR`), one db per repo,
never inside the scanned repository.

## License

MIT. Tag queries adapted from aider (MIT).
