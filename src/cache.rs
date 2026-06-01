//! Persistent embedding cache, keyed by (relative path, mtime, size).
//!
//! Stored as a single bincode blob at `<root>/.codemapper/embeds.bin`. Tag
//! parsing is cheap and done fresh each run; embeddings (the only model-bound
//! cost) are what we cache. A file's cached embeddings are valid only when its
//! mtime and size both match, in which case the definition order is identical.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    pub mtime_ns: u128,
    pub size: u64,
    /// Embeddings in definition-encounter order for the file.
    pub embeds: Vec<Vec<f32>>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Cache {
    entries: HashMap<String, Entry>,
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(".codemapper").join("embeds.bin")
}

impl Cache {
    pub fn load(root: &Path) -> Cache {
        let path = cache_path(root);
        std::fs::read(&path)
            .ok()
            .and_then(|b| bincode::deserialize(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        let path = cache_path(root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let bytes = bincode::serialize(self).unwrap_or_default();
        std::fs::write(path, bytes)
    }

    /// Return cached embeddings if the entry matches the file's current mtime/size.
    pub fn get(&self, rel: &str, mtime_ns: u128, size: u64) -> Option<&[Vec<f32>]> {
        let e = self.entries.get(rel)?;
        (e.mtime_ns == mtime_ns && e.size == size).then(|| e.embeds.as_slice())
    }

    pub fn put(&mut self, rel: String, mtime_ns: u128, size: u64, embeds: Vec<Vec<f32>>) {
        self.entries.insert(rel, Entry { mtime_ns, size, embeds });
    }
}

/// `(mtime_ns, size)` metadata key for a file, or `None` if unavailable.
pub fn meta_key(path: &Path) -> Option<(u128, u64)> {
    let m = std::fs::metadata(path).ok()?;
    let mtime = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((mtime, m.len()))
}
