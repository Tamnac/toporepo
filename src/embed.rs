//! Static-embedding layer (potion-code-16M via model2vec-rs).
//!
//! The model is loaded from a local directory; embeddings are L2-normalised
//! (per the model config), so cosine similarity reduces to a dot product.

use anyhow::{Context, Result};
use model2vec_rs::model::StaticModel;
use std::path::{Path, PathBuf};

pub struct Embedder {
    model: StaticModel,
}

impl Embedder {
    pub fn load(path: &Path) -> Result<Self> {
        let model = StaticModel::from_pretrained(path, None, None, None)
            .with_context(|| format!("loading model from {}", path.display()))?;
        Ok(Self { model })
    }

    /// Embed a batch of texts. Output vectors are L2-normalised.
    pub fn encode(&self, texts: &[String]) -> Vec<Vec<f32>> {
        if texts.is_empty() {
            return Vec::new();
        }
        self.model.encode(texts)
    }

    pub fn encode_one(&self, text: &str) -> Vec<f32> {
        self.encode(&[text.to_string()]).into_iter().next().unwrap_or_default()
    }
}

/// Cosine similarity for already-normalised vectors (dot product).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// HuggingFace repo downloaded on demand when no local model is found.
const MODEL_REPO: &str = "minishlab/potion-code-16M";

/// Resolve the model: explicit `--model`, else `TOPOREPO_MODEL` (handled by
/// clap), else a local `potion-code-16M` directory (next to the CWD or the
/// binary). Failing all of those, return the HuggingFace repo id, which
/// `model2vec-rs` downloads and caches in the shared HuggingFace cache on first
/// use.
pub fn resolve_model(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from("potion-code-16M"),
        PathBuf::from("../potion-code-16M"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("potion-code-16M"));
        }
    }
    for c in &candidates {
        if c.join("model.safetensors").is_file() {
            return Ok(c.clone());
        }
    }
    eprintln!("potion-code-16M not found locally; using HuggingFace {MODEL_REPO} (downloads to cache on first run)");
    Ok(PathBuf::from(MODEL_REPO))
}
