//! Workspace path -> session-bucket hash.
//!
//! Mirrors `@kiro/agent`'s workspace-hash algorithm. KAS uses this to
//! decide which subdirectory of `~/.kiro/sessions/` a session lives in,
//! so the algorithm has to match upstream byte-for-byte. The parity
//! tests in this file pin known-input hash values; if they ever fail,
//! the upstream algorithm drifted and this implementation needs an
//! update.

use std::path::PathBuf;

use sha2::{
    Digest,
    Sha256,
};

const GLOBAL_WORKSPACE_HASH: &str = "_global";

/// Normalize a path the same way KAS does before hashing.
///
/// Converts Windows backslashes to forward slashes and lowercases on
/// Windows (matching KAS's case-insensitive treatment of the
/// platform). Absolute paths are passed through unchanged.
///
/// Relative paths are joined to the current working directory but
/// **not** lexically simplified, so segments like `.` and `..` are
/// preserved verbatim. KAS's Node-side `path.resolve` does collapse
/// these; the relative-input case is therefore best-effort and not
/// guaranteed to round-trip with KAS. Callers in this crate always
/// pass absolute paths (the import/export entry points derive them
/// from `std::env::current_dir`), so this divergence is latent.
pub(crate) fn normalize_path(file_path: &str) -> String {
    let absolute: String = if PathBuf::from(file_path).is_absolute() {
        file_path.to_string()
    } else {
        // Best-effort fallback for relative inputs. See doc comment;
        // this does NOT match KAS exactly for paths containing `.`
        // or `..` segments.
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(file_path).to_string_lossy().into_owned(),
            Err(_) => file_path.to_string(),
        }
    };
    let forward_slash = absolute.replace('\\', "/");
    if cfg!(windows) {
        forward_slash.to_lowercase()
    } else {
        forward_slash
    }
}

/// Hash a set of workspace paths into a stable bucket id.
///
/// Empty input -> the special `_global` bucket. Any non-empty input is
/// normalized per-path, sorted (so order does not matter), joined with
/// NUL bytes, SHA-256'd, and truncated to the first 16 hex chars.
pub fn compute_workspace_hash(workspace_paths: &[String]) -> String {
    if workspace_paths.is_empty() {
        return GLOBAL_WORKSPACE_HASH.to_string();
    }
    let mut normalized: Vec<String> = workspace_paths.iter().map(|p| normalize_path(p)).collect();
    normalized.sort();
    let joined = normalized.join("\0");
    let mut hasher = Sha256::new();
    hasher.update(joined.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    // The hardcoded values below pin known-input hash values from the
    // upstream KAS algorithm. If any of these tests fail, either KAS's
    // hash algorithm drifted or this implementation deviated; in either
    // case both sides need to be reconciled before merging.

    #[test]
    fn empty_input_returns_global_marker() {
        assert_eq!(compute_workspace_hash(&[]), "_global");
    }

    #[test]
    fn single_absolute_posix_path() {
        assert_eq!(compute_workspace_hash(&["/foo".to_string()]), "6f64c6e6261f492a");
    }

    #[test]
    fn longer_absolute_posix_path() {
        assert_eq!(
            compute_workspace_hash(&["/Users/me/work".to_string()]),
            "b6542e257ca225de"
        );
    }

    #[test]
    fn order_insensitive_for_multiple_paths() {
        let forward = compute_workspace_hash(&["/a".to_string(), "/b".to_string()]);
        let reverse = compute_workspace_hash(&["/b".to_string(), "/a".to_string()]);
        assert_eq!(forward, "148c32fbefbb40c1");
        assert_eq!(reverse, "148c32fbefbb40c1");
    }

    #[test]
    fn trailing_slash_is_distinct_from_no_trailing_slash() {
        // KAS does not normalize trailing slashes, so `/foo` and
        // `/foo/` hash to different buckets. Not ideal, but we mirror
        // KAS exactly.
        assert_ne!(
            compute_workspace_hash(&["/foo".to_string()]),
            compute_workspace_hash(&["/foo/".to_string()])
        );
        assert_eq!(compute_workspace_hash(&["/foo/".to_string()]), "2f98d99fc40133ec");
    }

    #[test]
    fn normalize_path_converts_backslashes_to_forward_slashes() {
        // Absolute Windows path - normalize_path should not touch the
        // drive letter on non-Windows but should still flip slashes.
        let normalized = normalize_path("C:\\Users\\me\\work");
        assert!(normalized.contains('/'), "expected forward slashes in {normalized:?}",);
        assert!(!normalized.contains('\\'), "expected no backslashes in {normalized:?}",);
    }

    #[test]
    fn normalize_path_passes_through_absolute_posix_path() {
        assert_eq!(normalize_path("/foo/bar"), "/foo/bar");
    }
}
