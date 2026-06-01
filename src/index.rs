//! Repository index: per-file tags plus the global define/reference maps that
//! the reference graph is built from.

use crate::tags::{Kind, Tag};
use crate::{tags, walk};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct FileData {
    pub path: PathBuf,
    pub rel: String,
    pub source: String,
    pub tags: Vec<Tag>,
}

pub struct Index {
    pub files: Vec<FileData>,
    /// identifier -> file indices that define it (deduped).
    pub defines: HashMap<String, Vec<usize>>,
    /// identifier -> (file index -> reference count in that file).
    pub references: HashMap<String, HashMap<usize, usize>>,
}

impl Index {
    /// Build an index by walking `root` and extracting tags from every supported file.
    pub fn build(root: &Path) -> Self {
        let files_paths = walk::source_files(root);
        let mut files = Vec::new();
        for path in files_paths {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let tags = tags::extract(&path, &source);
            if tags.is_empty() {
                continue;
            }
            let rel = walk::rel(&path, root);
            files.push(FileData { path, rel, source, tags });
        }
        Self::from_files(files)
    }

    fn from_files(files: Vec<FileData>) -> Self {
        let mut defines: HashMap<String, Vec<usize>> = HashMap::new();
        let mut references: HashMap<String, HashMap<usize, usize>> = HashMap::new();
        for (fi, f) in files.iter().enumerate() {
            for t in &f.tags {
                match t.kind {
                    Kind::Def => {
                        let v = defines.entry(t.name.clone()).or_default();
                        if v.last() != Some(&fi) {
                            v.push(fi);
                        }
                    }
                    Kind::Ref => {
                        *references
                            .entry(t.name.clone())
                            .or_default()
                            .entry(fi)
                            .or_default() += 1;
                    }
                }
            }
        }
        Index { files, defines, references }
    }

    /// Definition tags of a file, paired with the index into `file.tags`.
    pub fn defs(&self, fi: usize) -> impl Iterator<Item = (usize, &Tag)> {
        self.files[fi]
            .tags
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == Kind::Def)
    }
}
