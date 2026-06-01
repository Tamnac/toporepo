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

/// Resolve the model directory: explicit `--model`, else `CODEMAPPER_MODEL`
/// (handled by clap), else a list of sensible fallbacks.
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
    anyhow::bail!(
        "could not find the potion-code-16M model. Pass --model <dir> or set CODEMAPPER_MODEL. \
         Looked in: {}",
        candidates
            .iter()
            .map(|c| c.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
}
