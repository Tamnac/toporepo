//! Persistent embedding cache backed by SQLite, keyed by `(rel_path, body_hash)`.
//!
//! Each definition's embedding is cached independently: only defs whose body
//! text changed are re-embedded.  The hash covers the full declaration span
//! (`start_byte..end_byte`), so renaming, restructuring, or editing the body
//! all invalidate correctly.

use rusqlite::{params, Connection};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

fn cache_path(root: &Path) -> PathBuf {
    root.join(".codemapper").join("embeds.db")
}

/// 64-bit content hash of a definition's body text.
pub fn body_hash(text: &str) -> i64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish() as i64
}

pub struct Cache {
    conn: Connection,
}

impl Cache {
    /// Open (or create) the cache database.  Returns `None` on any I/O or
    /// schema error — callers fall back to uncached embedding.
    pub fn open(root: &Path) -> Option<Self> {
        let path = cache_path(root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok()?;
        }
        // Remove legacy bincode cache if present.
        let old = root.join(".codemapper").join("embeds.bin");
        if old.exists() {
            let _ = std::fs::remove_file(&old);
        }
        let conn = Connection::open(&path).ok()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embeds (
                rel_path  TEXT NOT NULL,
                body_hash INTEGER NOT NULL,
                embed     BLOB NOT NULL,
                PRIMARY KEY (rel_path, body_hash)
            )",
        )
        .ok()?;
        conn.execute_batch("PRAGMA journal_mode=WAL").ok();
        Some(Cache { conn })
    }

    pub fn get(&self, rel: &str, hash: i64) -> Option<Vec<f32>> {
        self.conn
            .prepare_cached("SELECT embed FROM embeds WHERE rel_path=?1 AND body_hash=?2")
            .ok()?
            .query_row(params![rel, hash], |row| {
                let blob: Vec<u8> = row.get(0)?;
                Ok(blob_to_vec(&blob))
            })
            .ok()
    }

    pub fn put(&self, rel: &str, hash: i64, vec: &[f32]) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO embeds (rel_path,body_hash,embed) VALUES (?1,?2,?3)",
            params![rel, hash, vec_to_blob(vec)],
        );
    }
}

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
