//! Repository walking: enumerate candidate source files, respecting .gitignore.

use crate::lang;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Walk `root`, returning supported source files (relative paths, sorted).
/// If `root` is a single file, returns just that file.
pub fn source_files(root: &Path) -> Vec<PathBuf> {
    if root.is_file() {
        return if lang::detect(root).is_some() {
            vec![root.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    let mut files: Vec<PathBuf> = WalkBuilder::new(root)
        .hidden(false)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.into_path())
        .filter(|p| lang::detect(p).is_some())
        .collect();
    files.sort();
    files
}

/// Relative path string (forward slashes) of `path` against `root`, for display.
pub fn rel(path: &Path, root: &Path) -> String {
    let p = path.strip_prefix(root).unwrap_or(path);
    let s = p.to_string_lossy().replace('\\', "/");
    if s.is_empty() {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    } else {
        s
    }
}
