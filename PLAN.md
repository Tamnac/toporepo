# Repo-map tool — implementation plan

A standalone code outline/retrieval tool. Concept derived from aider's `repomap.py`
(tree-sitter tags -> reference graph -> token-budgeted outline), reworked around a
semantic ranking layer and shipped as a CLI an LLM agent can call.

This is a fresh implementation, not a fork of RepoMapper (../RepoMapper), but use it for reference if needed. It's not a great implementation, I added commits on the base project, you can check the last two to see some changes. If you need to run it, there's a venv and uv.

## Language & stack

- **Rust.** Single static binary, fast startup, no runtime deps — ideal for a CLI an
  agent shells out to. Decisive factor: the embedding model is static (see below), so
  there's no ONNX/tensor-runtime burden that would otherwise push toward Python.
- `clap` for argument parsing.
- [`tree-sitter`](https://crates.io/crates/tree-sitter) Rust bindings for parsing.
- Vendored `.scm` tag-query files (one per language), initiated from [aider's set](https://github.com/Aider-AI/aider/tree/main/aider/queries) (MIT).
- [`model2vec-rs`](https://github.com/MinishLab/model2vec-rs) + [`potion-code-16M`](https://huggingface.co/minishlab/potion-code-16M) for embeddings.
- Cache: your choice: sqlite/serialized blob keyed by file mtime, etc.
  embeddings.
- Cosine similarity/distance as you see fit

## Embedding model — potion-code-16M

- Static embedding model (no neural-net inference): tokenize -> look up each token's
  precomputed vector -> weighted mean-pool -> L2-normalize. Orders of magnitude
  faster than transformer models; embedding cost is effectively free.
- Trained on (natural-language query, code document) pairs. So:
  - **Query side**: the user's/LLM's natural-language query.
  - **Document side**: the actual code of each definition (body/signature), not a
    synthesized name+doc string. Feed it code — that's what it was trained on.
- The `model.safetensors` ships `embeddings`, `mapping`, and `weights` (confirmed).
  All three are mandatory:
  - `mapping` (I64): indirection on row lookup — `row = embeddings[mapping[tok]]`.
  - `weights` (F64): per-token scalar applied before pooling (SIF re-regularization).
  - Do **not** use the naive "row[tok], scale 1.0" path; vectors would be wrong.
  - `model2vec-rs` already handles all three correctly.

## Core pipeline

```
tree-sitter tags -> reference graph -> [semantic rerank] -> token-budget fit -> output
```

1. **Tag extraction.** tree-sitter + `.scm` queries to extract definitions and
   references. The `.scm` captures (`@name.definition.*` / `@name.reference.*`) are the
   bridge between the parsed tree and the tags. Load `.scm` files as bundled resources
   loaded as bundled resources (cf. [aider's `get_scm_fname`](https://github.com/Aider-AI/aider/blob/main/aider/repomap.py#L528))
   rather than a hardcoded path map like RepoMapper's `scm.py`.

2. **Reference graph.** Build `(referencer_file, definer_file)` edges from shared
   identifier names (string matching — tree-sitter does NOT resolve which def a ref
   points to). Significantly simplified vs aider:
   - Keep weighted edges (see below), but drop aider's full PageRank +
     rank-distribution-across-out-edges machinery.
   - Use the graph for a **local walk** from semantically-matched seed files, not a
     global ranking pass.

3. **Semantic layer (new).** Embed every definition's code at index time; cache by
   `(file_path, mtime)`. At query time:
   - Embed the query.
   - Cosine similarity vs all cached definition embeddings.
   - Top-N matches seed the graph walk (expand to files that reference / are
     referenced by the matches).
     Semantic relevance is the init, graph walk expands it to all relevant items

4. **Output assembly.** Binary-search over ranked tags to fit a token budget (as in
   aider's `get_ranked_tags_map_uncached`). Render with `grep_ast`-style `TreeContext`
   — call it correctly: `add_lines_of_interest()` -> `add_context()` -> `format()`
   with no args (RepoMapper's `format(lois)` call is wrong).

## Edge weighting (adopt from aider, with one change)

Reference: aider `repomap.py`, `get_ranked_tags`.

Per-identifier multiplier:
- `mentioned_idents` -> x10
- snake/kebab/camel case -> x10 **but drop/lower aider's `len >= 8` gate** (it nukes
  useful single-word names like `main`, `app`, `parse`, `config`).
- `_`-prefixed -> x0.1 (private/internal)
- defined in >5 files -> x0.1 (not discriminative)

Per-edge score: `ident_mul * chat_bonus(x50 if referencer in chat files) * sqrt(num_refs)`.
The sqrt gives diminishing returns so high-frequency idents (`self`, `os`) don't
dominate.

Note: with embeddings doing the heavy ranking, much of this can likely shrink further.
Start with the above, simplify once the semantic signal proves good.

## What to adopt from [aider](https://github.com/Aider-AI/aider) (reference, don't copy RepoMapper)

- Edge weighting + sqrt scaling ([`get_ranked_tags`](https://github.com/Aider-AI/aider/blob/main/aider/repomap.py#L270)).
- Tag caching with sqlite + error fallback ([`get_tags`](https://github.com/Aider-AI/aider/blob/main/aider/repomap.py#L170) / [`tags_cache_error`](https://github.com/Aider-AI/aider/blob/main/aider/repomap.py#L185)).
- `.scm` resource loading ([`get_scm_fname`](https://github.com/Aider-AI/aider/blob/main/aider/repomap.py#L528)).
- Pygments backfill for defs-only languages ([`get_tags_raw`](https://github.com/Aider-AI/aider/blob/main/aider/repomap.py#L240)).
- `filter_important_files` ([aider's `special.py`](https://github.com/Aider-AI/aider/blob/main/aider/special.py#L100)), already ported in RepoMapper's `importance.py`.
- Correct `TreeContext` usage ([`render_tree`](https://github.com/Aider-AI/aider/blob/main/aider/repomap.py#L430)).

## What to drop from RepoMapper

- Unweighted graph edges — use weighted edges.
- Post-hoc boost multipliers applied to the tag list after ranking — bake into the
  graph/score instead.
- `TreeContext.format(lois)` — wrong API.
- Hardcoded `scm.py` path map — use resource loading.
- `FileReport` and the `search_identifiers` tool — the embedding index subsumes most
  of its use; reconsider later if needed.
- MCP server — not needed; an LLM can call the CLI directly.

## What's new vs both

- Embedding index + cache (potion-code-16M, static).
- Semantic `query` parameter driving retrieval.
- Local graph walk from semantic seeds instead of global PageRank.
- Score blending (graph x semantic).

## Learnings from [Semble](https://github.com/MinishLab/semble) (MinishLab/semble)

Semble is the lab's own retrieval system built on the same model. It's a *retrieval*
tool (return top-k chunks, replace grep+read), not a *map*, so most of its machinery
solves problems we don't have. What we keep:

- **Model usage pattern (confirmed).** `model.encode([query])`, `normalize=True`,
  cosine similarity. Query side is raw NL, document side is the raw code span. No
  instruction prefix or special formatting. This validates our approach exactly.
- **File-coherence boost.** Semble's `boost_multi_chunk_files`: a file with several
  query-relevant chunks gets its top chunk promoted. Maps onto our tag model as "a
  file with multiple query-relevant definitions ranks above one with a single hit."
  Worth adopting in spirit.
- **RRF fusion** (`1/(k+rank)`, k=60) as the score-combination method *if* we end up
  doing hybrid — it makes the blend weight meaningful across rankers with different
  raw-score scales. Cleaner than the `score * (1 + alpha*sim)` blend noted above;
  revisit when/if BM25 lands.

Deliberately NOT taken from Semble:

- **BM25 / lexical index** — deferred ("up in the air"). Its main job in Semble is
  exact-identifier recall, i.e. the grep case we're not serving. Our `mentioned_idents`
  channel already covers "exact symbol the agent cares about" and rides the reference
  graph, which is strictly more useful than substring matching. Add BM25 only if recall
  on rare/project-specific identifiers proves bad in practice.
- **Symbol-vs-NL query detection / auto-alpha** (`is_symbol_query`, `resolve_alpha`)
  — exists to compete with grep on exact-symbol lookups. Out of scope; if an agent
  wants a specific symbol it should grep/LSP directly. Our query is always NL.
- **Symbol-definition regex boost** (`_boost_symbol_definitions`, the `class X`/`def X`
  patterns) — Semble uses regex to reconstruct the def-vs-use distinction we get for
  free from tree-sitter tags. Every unit in our map is already a definition, so this
  collapses to a no-op for us.
- **AST-merge chunker** (`chunking/core.py`) — Semble needs it because it has no
  notion of "definition" and embeds arbitrary spans. Our embedding unit is the
  definition body. Keep their recursive-node-merge chunker only as a fallback for
  languages without usable tag queries.

Note on `mentioned_idents`: it's the exact-symbol input channel (caller passes known
identifier names). Bake it into graph edge weights aider-style (x10 on edges carrying
that ident) so the signal propagates to both definers and referencers — not as a
post-hoc tag multiplier like RepoMapper does.
