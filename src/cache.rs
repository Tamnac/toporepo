//! Persistent embedding cache backed by SQLite, keyed by `(rel_path, body_hash)`.
//!
//! Each definition's embedding is cached independently: only defs whose body
//! text changed are re-embedded.  The hash covers the full declaration span
//! (`start_byte..end_byte`), so renaming, restructuring, or editing the body
//! all invalidate correctly.
//!
//! Caches live in a single shared location (the OS cache directory, overridable
//! with `TOPOREPO_CACHE_DIR`), one db file per repo, keyed by the repo's
//! absolute path — never inside the scanned repository.

use rusqlite::{params, Connection};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Shared directory holding every repo's embedding cache.
fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("TOPOREPO_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    dirs::cache_dir().map(|d| d.join("toporepo"))
}

/// Global db path for `root`: `<cache_dir>/<name>-<path-hash>.db`. Keyed by the
/// repo's absolute path so distinct repos never share a cache.
fn cache_path(root: &Path) -> Option<PathBuf> {
    let dir = cache_dir()?;
    let canon = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut h = DefaultHasher::new();
    canon.to_string_lossy().hash(&mut h);
    let hash = h.finish();
    let name: String = canon
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    Some(dir.join(format!("{name}-{hash:016x}.db")))
}

/// Remove the obsolete per-repo cache (now stored globally), keeping repos clean.
fn migrate_legacy(root: &Path) {
    let dir = root.join(".toporepo");
    for f in ["embeds.db", "embeds.db-wal", "embeds.db-shm", "embeds.bin"] {
        let _ = std::fs::remove_file(dir.join(f));
    }
    let _ = std::fs::remove_dir(&dir); // succeeds only if now empty
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
        let path = cache_path(root)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok()?;
        }
        migrate_legacy(root);
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
