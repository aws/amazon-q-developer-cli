//! Render the embedded YAML template + compute the config fingerprint.
//!
//! The template lives in `collector.yaml.tmpl`; we deliberately use plain
//! `{{VAR}}` substitution rather than pulling a templating engine — the
//! variables are few and the output needs to remain easy to inspect.
//!
//! The fingerprint is a SHA-256 over the rendered YAML, the binary path, and
//! (best-effort) the binary's `--version` output. A change in any of those
//! forces `lifecycle::ensure_running` to respawn rather than reuse a stale
//! collector that was started against a different config or binary.

use std::path::{
    Path,
    PathBuf,
};
use std::process::{
    Command,
    Stdio,
};
use std::time::Duration;

use sha2::{
    Digest,
    Sha256,
};
use tracing::debug;

/// Embedded YAML template. Path-style variables render with `Path::display()`.
pub const COLLECTOR_YAML_TEMPLATE: &str = include_str!("collector.yaml.tmpl");

/// Inputs for [`render`]. Borrowed so callers can construct it on the stack.
///
/// `binary_path` is not consumed by the current template but is retained for
/// the eventual schema-driven renderer (PR G's bundled-binary path needs to
/// know where the binary lives so it can compute relative paths).
pub struct ConfigInputs<'a> {
    pub upstream_endpoint: &'a str,
    pub queue_dir: &'a Path,
    pub crashes_dir: &'a Path,
    #[allow(dead_code)]
    pub binary_path: &'a Path,
}

/// Substitute `{{UPSTREAM_ENDPOINT}}`, `{{QUEUE_DIR}}`, `{{CRASHES_DIR}}` into
/// the embedded template.
///
/// Path values are rendered with `Path::display()`. The template is bundled at
/// compile time so this is infallible.
pub fn render(inputs: &ConfigInputs<'_>) -> String {
    let queue = inputs.queue_dir.display().to_string();
    let crashes = inputs.crashes_dir.display().to_string();
    COLLECTOR_YAML_TEMPLATE
        .replace("{{UPSTREAM_ENDPOINT}}", inputs.upstream_endpoint)
        .replace("{{QUEUE_DIR}}", &queue)
        .replace("{{CRASHES_DIR}}", &crashes)
}

/// SHA-256 fingerprint over `(rendered_yaml, canonical(binary_path), binary --version)`.
///
/// Components are NUL-separated so a path containing `\n` can't collide with a
/// pipe of YAML+path. The version output is best-effort: if `--version` fails
/// or times out, an empty string is used.
pub fn fingerprint(rendered_yaml: &str, binary_path: &Path) -> String {
    let canonical = std::fs::canonicalize(binary_path).unwrap_or_else(|_| binary_path.to_path_buf());
    let version = best_effort_version(&canonical);

    let mut hasher = Sha256::new();
    hasher.update(rendered_yaml.as_bytes());
    hasher.update(b"\0");
    hasher.update(canonical.as_os_str().to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(version.as_bytes());
    hex::encode(hasher.finalize())
}

/// Run `<bin> --version` with a 2-second wall-clock budget, returning the
/// stdout trimmed. Failures and timeouts collapse to `""` so the fingerprint
/// remains deterministic.
fn best_effort_version(bin: &Path) -> String {
    let bin = bin.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let res = Command::new(&bin)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        let _ = tx.send(res);
    });
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Ok(Ok(out)) => {
            debug!(status = ?out.status, "binary --version returned non-zero; using empty version");
            String::new()
        },
        Ok(Err(e)) => {
            debug!(error = %e, "binary --version spawn failed");
            String::new()
        },
        Err(_) => {
            debug!("binary --version timed out");
            String::new()
        },
    }
}

/// Convenience: store a rendered config + fingerprint to disk. Used by
/// `lifecycle::ensure_running` so the spawned collector can be restarted with
/// the same effective config and so tests can inspect what was last written.
pub fn write_artifacts(
    config_path: &Path,
    fingerprint_path: &Path,
    rendered: &str,
    fp: &str,
) -> Result<(), crate::error::CollectorError> {
    use crate::error::CollectorError;
    std::fs::write(config_path, rendered).map_err(|source| CollectorError::ConfigWrite {
        path: PathBuf::from(config_path),
        source,
    })?;
    std::fs::write(fingerprint_path, fp).map_err(|source| CollectorError::ConfigWrite {
        path: PathBuf::from(fingerprint_path),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_inputs<'a>(upstream: &'a str, queue: &'a Path, crashes: &'a Path, bin: &'a Path) -> ConfigInputs<'a> {
        ConfigInputs {
            upstream_endpoint: upstream,
            queue_dir: queue,
            crashes_dir: crashes,
            binary_path: bin,
        }
    }

    #[test]
    fn render_substitutes_all_vars() {
        let queue = PathBuf::from("/tmp/q");
        let crashes = PathBuf::from("/tmp/c");
        let bin = PathBuf::from("/usr/local/bin/otelcol-contrib");
        let s = render(&dummy_inputs("https://up.example/v1/metrics", &queue, &crashes, &bin));
        assert!(s.contains("https://up.example/v1/metrics"));
        assert!(s.contains("/tmp/q"));
        assert!(s.contains("/tmp/c"));
        assert!(!s.contains("{{UPSTREAM_ENDPOINT}}"));
        assert!(!s.contains("{{QUEUE_DIR}}"));
        assert!(!s.contains("{{CRASHES_DIR}}"));
    }

    #[test]
    fn render_is_deterministic() {
        let queue = PathBuf::from("/tmp/q");
        let crashes = PathBuf::from("/tmp/c");
        let bin = PathBuf::from("/usr/local/bin/otelcol-contrib");
        let inputs = dummy_inputs("https://up.example/v1/metrics", &queue, &crashes, &bin);
        let a = render(&inputs);
        let b = render(&inputs);
        assert_eq!(a, b);
    }

    #[test]
    fn render_parses_as_yaml() {
        let queue = PathBuf::from("/tmp/q");
        let crashes = PathBuf::from("/tmp/c");
        let bin = PathBuf::from("/usr/local/bin/otelcol-contrib");
        let s = render(&dummy_inputs("https://up.example/v1/metrics", &queue, &crashes, &bin));
        let _: serde_yaml::Value = serde_yaml::from_str(&s).expect("yaml parses");
    }

    #[test]
    fn fingerprint_is_64_lowercase_hex() {
        let bin = PathBuf::from("/usr/bin/true");
        let fp = fingerprint("hello", &bin);
        assert_eq!(fp.len(), 64);
        assert!(
            fp.chars()
                .all(|c| c.is_ascii_hexdigit() && (!c.is_ascii_alphabetic() || c.is_ascii_lowercase()))
        );
    }

    #[test]
    fn fingerprint_changes_with_endpoint() {
        let queue = PathBuf::from("/tmp/q");
        let crashes = PathBuf::from("/tmp/c");
        let bin = PathBuf::from("/usr/bin/true");
        let a = render(&dummy_inputs("https://a.example/v1/metrics", &queue, &crashes, &bin));
        let b = render(&dummy_inputs("https://b.example/v1/metrics", &queue, &crashes, &bin));
        assert_ne!(fingerprint(&a, &bin), fingerprint(&b, &bin));
    }
}
