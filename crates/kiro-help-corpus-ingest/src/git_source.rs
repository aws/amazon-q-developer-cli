//! Walk a kiro-cli checkout (or any directory) and emit `RawChunk`s for the
//! `docs` partition. The git clone itself happens in `main.rs`; this module is
//! pure filesystem walking so it stays unit-testable.

use std::path::{
    Path,
    PathBuf,
};

use chrono::{
    DateTime,
    TimeZone,
    Utc,
};

use crate::{
    Partition,
    RawChunk,
    Source,
};

/// Files larger than this (in bytes) are skipped — they're more likely to be
/// generated artefacts (lockfiles, vendored bundles) than docs.
const MAX_FILE_BYTES: u64 = 1_000_000;

/// Default extensions that are kept when walking the docs subtree.
pub const DEFAULT_DOC_EXTENSIONS: &[&str] = &["md", "mdx", "txt"];

/// Default subdirectories that are walked under the repo root.
pub const DEFAULT_DOC_DIRS: &[&str] = &["docs", "autodocs"];

/// `Source` impl that reads the docs partition from a local checkout.
pub struct GitSource {
    repo_root: PathBuf,
    doc_dirs: Vec<String>,
    doc_extensions: Vec<String>,
}

impl GitSource {
    pub fn new<P: Into<PathBuf>>(repo_root: P) -> Self {
        Self {
            repo_root: repo_root.into(),
            doc_dirs: DEFAULT_DOC_DIRS.iter().map(|s| s.to_string()).collect(),
            doc_extensions: DEFAULT_DOC_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn with_doc_dirs<I, S>(mut self, dirs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.doc_dirs = dirs.into_iter().map(Into::into).collect();
        self
    }
}

#[async_trait::async_trait]
impl Source for GitSource {
    fn partition(&self) -> Partition {
        Partition::Docs
    }

    async fn fetch(&self) -> anyhow::Result<Vec<RawChunk>> {
        let chunks = collect_docs(&self.repo_root, &self.doc_dirs, &self.doc_extensions)?;
        Ok(chunks)
    }
}

fn collect_docs(repo_root: &Path, dirs: &[String], exts: &[String]) -> anyhow::Result<Vec<RawChunk>> {
    let mut out = Vec::new();
    for dir in dirs {
        let walk_root = repo_root.join(dir);
        if !walk_root.exists() {
            continue;
        }
        walk(&walk_root, repo_root, exts, &mut out)?;
    }
    // Top-level *.md files (README, CONTRIBUTING, etc.).
    for entry in std::fs::read_dir(repo_root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && has_ext(&path, exts) {
            push_file(&path, repo_root, &mut out)?;
        }
    }
    Ok(out)
}

fn walk(dir: &Path, repo_root: &Path, exts: &[String], out: &mut Vec<RawChunk>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, repo_root, exts, out)?;
        } else if has_ext(&path, exts) {
            push_file(&path, repo_root, out)?;
        }
    }
    Ok(())
}

fn has_ext(path: &Path, exts: &[String]) -> bool {
    match path.extension().and_then(|s| s.to_str()) {
        Some(e) => exts.iter().any(|x| x == e),
        None => false,
    }
}

fn push_file(path: &Path, repo_root: &Path, out: &mut Vec<RawChunk>) -> anyhow::Result<()> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Ok(());
    }
    let content = std::fs::read_to_string(path)?;
    let rel = path
        .strip_prefix(repo_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    out.push(RawChunk {
        source_path: rel,
        content,
        last_modified: file_mtime(&metadata),
        partition: Partition::Docs,
    });
    Ok(())
}

fn file_mtime(metadata: &std::fs::Metadata) -> DateTime<Utc> {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos()).single())
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join("docs/sub")).unwrap();
        std::fs::write(root.join("docs/auth.md"), "# auth\n").unwrap();
        std::fs::write(root.join("docs/sub/nested.md"), "# nested\n").unwrap();

        std::fs::create_dir_all(root.join("autodocs")).unwrap();
        std::fs::write(root.join("autodocs/index.md"), "# auto\n").unwrap();

        std::fs::write(root.join("README.md"), "# readme\n").unwrap();
        std::fs::write(root.join("CHANGELOG.md"), "# changelog\n").unwrap();

        // junk that should be skipped
        std::fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(root.join("docs/giant.md"), vec![b'x'; 1_500_000]).unwrap();

        tmp
    }

    #[tokio::test]
    async fn collects_md_files_from_docs_autodocs_and_top_level() {
        let tmp = setup_repo();
        let src = GitSource::new(tmp.path());
        let chunks = src.fetch().await.unwrap();
        let paths: std::collections::HashSet<_> = chunks.iter().map(|c| c.source_path.as_str()).collect();

        assert!(paths.contains("docs/auth.md"), "got {:?}", paths);
        assert!(paths.contains("docs/sub/nested.md"));
        assert!(paths.contains("autodocs/index.md"));
        assert!(paths.contains("README.md"));
        assert!(paths.contains("CHANGELOG.md"));
        assert!(!paths.contains("Cargo.toml"), "non-md files must be skipped");
        assert!(!paths.contains("docs/giant.md"), "files >1MB must be skipped");

        assert!(
            chunks.iter().all(|c| matches!(c.partition, Partition::Docs)),
            "all chunks must be partitioned as Docs"
        );
    }

    #[tokio::test]
    async fn missing_repo_root_returns_error() {
        let src = GitSource::new("/this/does/not/exist");
        let err = src.fetch().await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("No such file") || msg.contains("not found") || msg.contains("os error"),
            "unexpected error: {msg}"
        );
    }
}
