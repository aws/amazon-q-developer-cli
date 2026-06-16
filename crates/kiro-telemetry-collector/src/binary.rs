//! Layered binary resolution for `otelcol-contrib`.
//!
//! Resolution layers (in order; each layer is logged at `debug!` so tests and
//! dev runs can see which one fired):
//!
//! 1. **`KIRO_TELEMETRY_COLLECTOR_BIN`** — env var override. Used verbatim after a
//!    `Path::is_file()` + executable-bit check. Primary mechanism for tests, dev loops, and the
//!    workflow's e2e verification step (which points it at the brew-installed `otelcol-contrib`).
//!
//! 2. **`PATH` lookup for `otelcol-contrib`** — walks `$PATH` directly (no `which` crate
//!    dependency) and returns the first executable file. Covers Homebrew installs and any dev
//!    machine where the user has `brew install opentelemetry-collector-contrib`'d the binary.
//!
//! 3. **Bundled binary** — checks paths next to the kiro-cli executable
//!    (`<exe_dir>/otelcol-contrib`, `<exe_dir>/../share/kiro-cli/otelcol-contrib`). PR G will
//!    populate these; for F-impl we just probe and fall through if absent.
//!
//! 4. **Error: `BinaryNotFound`** with an actionable message listing what was tried.

use std::path::{
    Path,
    PathBuf,
};

use tracing::debug;

use crate::error::CollectorError;

/// Env var name for the manual binary override.
pub const ENV_BINARY_OVERRIDE: &str = "KIRO_TELEMETRY_COLLECTOR_BIN";

/// Name of the binary we resolve.
pub const BINARY_NAME: &str = "otelcol-contrib";

/// Resolve the path to `otelcol-contrib` using the layered logic documented at
/// the module level. Returns the canonical path or [`CollectorError::BinaryNotFound`]
/// listing every candidate that was tried.
pub fn resolve_collector_binary() -> Result<PathBuf, CollectorError> {
    let mut tried: Vec<PathBuf> = Vec::new();

    // Layer 1: env override.
    if let Some(p) = env_override(&mut tried) {
        debug!(path = %p.display(), layer = "env", "resolved otelcol-contrib");
        return Ok(p);
    }

    // Layer 2: PATH walk.
    if let Some(p) = path_walk(BINARY_NAME) {
        tried.push(p.clone());
        debug!(path = %p.display(), layer = "path", "resolved otelcol-contrib");
        return Ok(p);
    }

    // Layer 3: bundled candidates.
    for cand in bundled_candidates() {
        tried.push(cand.clone());
        if is_executable_file(&cand) {
            debug!(path = %cand.display(), layer = "bundled", "resolved otelcol-contrib");
            return Ok(cand);
        }
    }

    Err(CollectorError::BinaryNotFound { searched: tried })
}

fn env_override(tried: &mut Vec<PathBuf>) -> Option<PathBuf> {
    let raw = std::env::var_os(ENV_BINARY_OVERRIDE)?;
    let p = PathBuf::from(raw);
    tried.push(p.clone());
    if is_executable_file(&p) { Some(p) } else { None }
}

/// Walk `$PATH` for `name` and return the first executable file we find.
pub fn path_walk(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Candidate paths next to the kiro-cli binary. PR G populates these.
pub fn bundled_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        out.push(dir.join(BINARY_NAME));
        if let Some(parent) = dir.parent() {
            out.push(parent.join("share").join("kiro-cli").join(BINARY_NAME));
        }
    }
    out
}

/// `true` if the path exists, is a file, and (on unix) has any executable bit
/// set. On non-unix we settle for `is_file()`.
pub fn is_executable_file(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else { return false };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_walk_no_match() {
        assert!(path_walk("this-binary-definitely-does-not-exist-xyz-zzz").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn path_walk_finds_sh() {
        // /bin/sh is on PATH on every unix.
        let p = path_walk("sh").expect("sh on PATH");
        assert!(p.is_file());
        assert!(is_executable_file(&p));
    }

    #[test]
    fn is_executable_file_returns_false_for_missing() {
        assert!(!is_executable_file(Path::new("/this/does/not/exist/zzz")));
    }

    #[cfg(unix)]
    #[test]
    fn bundled_candidates_returns_paths_next_to_exe() {
        let cands = bundled_candidates();
        assert!(!cands.is_empty());
        for c in cands {
            assert!(c.ends_with(BINARY_NAME) || c.ends_with(format!("share/kiro-cli/{BINARY_NAME}")));
        }
    }
}
