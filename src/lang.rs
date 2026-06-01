//! Language detection and bundled tree-sitter tag queries.

use include_dir::{include_dir, Dir};
use std::path::Path;
use tree_sitter::Language;

static QUERIES: Dir = include_dir!("$CARGO_MANIFEST_DIR/queries");

/// A supported language: its tree-sitter grammar and the bundled `.scm` tag query.
pub struct Lang {
    pub name: &'static str,
    pub language: Language,
    pub query: &'static str,
}

fn query_text(file: &str) -> &'static str {
    QUERIES
        .get_file(file)
        .and_then(|f| f.contents_utf8())
        .unwrap_or_else(|| panic!("bundled query missing: {file}"))
}

/// Resolve a file path to a supported language, or `None` if unsupported.
pub fn detect(path: &Path) -> Option<Lang> {
    let ext = path.extension().and_then(|e| e.to_str())?.to_ascii_lowercase();
    let (name, language, scm): (&str, Language, &str) = match ext.as_str() {
        "rs" => ("rust", tree_sitter_rust::LANGUAGE.into(), "rust-tags.scm"),
        "py" | "pyi" => ("python", tree_sitter_python::LANGUAGE.into(), "python-tags.scm"),
        "js" | "jsx" | "mjs" | "cjs" => (
            "javascript",
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript-tags.scm",
        ),
        "ts" | "mts" | "cts" => (
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript-tags.scm",
        ),
        "tsx" => (
            "tsx",
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "typescript-tags.scm",
        ),
        "go" => ("go", tree_sitter_go::LANGUAGE.into(), "go-tags.scm"),
        "java" => ("java", tree_sitter_java::LANGUAGE.into(), "java-tags.scm"),
        "c" | "h" => ("c", tree_sitter_c::LANGUAGE.into(), "c-tags.scm"),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => {
            ("cpp", tree_sitter_cpp::LANGUAGE.into(), "cpp-tags.scm")
        }
        "cs" => ("csharp", tree_sitter_c_sharp::LANGUAGE.into(), "csharp-tags.scm"),
        "rb" => ("ruby", tree_sitter_ruby::LANGUAGE.into(), "ruby-tags.scm"),
        _ => return None,
    };
    Some(Lang {
        name,
        language,
        query: query_text(scm),
    })
}
