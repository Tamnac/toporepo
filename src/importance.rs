//! Important-file detection (ported from aider's `special.py` / RepoMapper's
//! `importance.py`). Such files get a rank nudge so key entry points and config
//! surface in the map even with a weak graph signal.

const IMPORTANT_FILENAMES: &[&str] = &[
    "README.md", "README.txt", "readme.md", "README.rst", "README",
    "requirements.txt", "Pipfile", "pyproject.toml", "setup.py", "setup.cfg",
    "package.json", "yarn.lock", "package-lock.json", "npm-shrinkwrap.json",
    "Dockerfile", "docker-compose.yml", "docker-compose.yaml",
    ".gitignore", ".gitattributes", ".dockerignore",
    "Makefile", "makefile", "CMakeLists.txt",
    "LICENSE", "LICENSE.txt", "LICENSE.md", "COPYING",
    "CHANGELOG.md", "CHANGELOG.txt", "HISTORY.md",
    "CONTRIBUTING.md", "CODE_OF_CONDUCT.md",
    ".env", ".env.example", ".env.local",
    "tox.ini", "pytest.ini", ".pytest.ini",
    ".flake8", ".pylintrc", "mypy.ini",
    "go.mod", "go.sum", "Cargo.toml", "Cargo.lock",
    "pom.xml", "build.gradle", "build.gradle.kts",
    "composer.json", "composer.lock",
    "Gemfile", "Gemfile.lock",
    // common entry points worth surfacing
    "main.rs", "lib.rs", "main.py", "__init__.py", "index.js", "index.ts",
];

pub fn is_important(rel: &str) -> bool {
    let name = rel.rsplit(['/', '\\']).next().unwrap_or(rel);
    IMPORTANT_FILENAMES.iter().any(|f| *f == name)
}
