//! Zip-format I/O helpers for the session archive.
//!
//! Defensive: KAS itself does not read zips - this module only exists
//! to support the import side of the archive utility, and to keep the
//! zip-handling churn out of `import.rs`.

use std::collections::BTreeMap;
use std::io::{
    Cursor,
    Read,
};
use std::path::{
    Component,
    Path,
    PathBuf,
};

use super::shared::SessionArchiveError;

/// Cap for per-entry preallocation when extracting a zip. The zip
/// header reports an entry's uncompressed size, but the entry can lie
/// (or just be huge); preallocating that many bytes upfront would
/// trivially OOM the process. The actual read is still
/// `read_to_end` - this only bounds the *initial* `Vec` capacity.
pub(super) const ZIP_ENTRY_PREALLOC_CAP: usize = 16 * 1024 * 1024;

/// Resolve a zip entry name against a destination directory and assert
/// that the result stays inside that directory. Errors if the entry
/// attempts to escape via absolute paths, `..` traversal, or contains
/// null bytes.
///
/// Standard zip-slip mitigation: the `zip` crate hands us entry names
/// verbatim and does not validate them, so the safety check is the
/// caller's responsibility. Backslash separators (sometimes used by
/// older Windows zip writers) are normalized to forward slashes so the
/// same checks apply consistently across platforms.
pub(super) fn safe_resolve(base_dir: &Path, entry_name: &str) -> Result<PathBuf, SessionArchiveError> {
    if entry_name.is_empty() {
        return Err(SessionArchiveError::msg("archive entry has empty name"));
    }
    if entry_name.contains('\0') {
        return Err(SessionArchiveError::msg(format!(
            "archive entry name contains null byte: {entry_name:?}"
        )));
    }
    let normalized = entry_name.replace('\\', "/");

    // Reject Windows drive-letter prefixes (`C:foo`, `D:\Windows`).
    // These are drive-relative on Windows and `Path::is_absolute` does
    // not flag them, but a join can still escape `base_dir` by
    // re-anchoring on the drive's current directory.
    if is_windows_drive_letter_prefixed(&normalized) {
        return Err(SessionArchiveError::msg(format!(
            "archive entry uses Windows drive letter: {entry_name}"
        )));
    }
    // POSIX-style absolute path (starts with `/`). The `zip` crate does
    // not strip leading slashes for us; an entry like `/etc/passwd`
    // would otherwise resolve outside `base_dir`.
    if normalized.starts_with('/') {
        return Err(SessionArchiveError::msg(format!(
            "archive entry uses absolute path: {entry_name}"
        )));
    }

    let resolved_base = absolute_lexical(base_dir);
    let resolved_target = resolve_relative_against(&resolved_base, &normalized);

    if resolved_target == resolved_base || resolved_target.starts_with(&resolved_base) {
        Ok(resolved_target)
    } else {
        Err(SessionArchiveError::msg(format!(
            "archive entry escapes destination: {entry_name}"
        )))
    }
}

/// True if `p` starts with an ASCII letter followed by `:` (drive-letter
/// prefix). Operates on the already-forward-slashed string.
fn is_windows_drive_letter_prefixed(p: &str) -> bool {
    let bytes = p.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Best-effort absolute, lexically-normalized form of `p`. Does not
/// touch the filesystem; safe to call on paths that do not exist yet
/// (which is the case during zip extraction staging).
fn absolute_lexical(p: &Path) -> PathBuf {
    let absolute = if p.is_absolute() {
        p.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(p),
            Err(_) => p.to_path_buf(),
        }
    };
    lexical_clean(&absolute)
}

/// Collapse `.` and `..` components in `p` without filesystem access.
fn lexical_clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {},
            Component::ParentDir => {
                // Only pop a previously-pushed `Normal` component. Going
                // above an absolute root is a no-op (matches POSIX
                // `path.resolve` semantics).
                let last_was_normal = out
                    .components()
                    .next_back()
                    .is_some_and(|c| matches!(c, Component::Normal(_)));
                if last_was_normal {
                    out.pop();
                } else if !out.has_root() {
                    // For a relative result, preserve the `..` so the
                    // caller can still tell we walked above the input.
                    out.push("..");
                }
            },
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                out.push(comp.as_os_str());
            },
        }
    }
    out
}

/// Apply a forward-slash relative path `relative` to absolute `base`,
/// collapsing `.` and `..` along the way. Mirrors Node's
/// `path.resolve(base, relative)` for the inputs this module sees:
/// always-absolute base, always-relative entry name.
fn resolve_relative_against(base: &Path, relative: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for part in relative.split('/') {
        match part {
            "" | "." => {},
            ".." => {
                out.pop();
            },
            other => out.push(other),
        }
    }
    lexical_clean(&out)
}

/// Decode a zip blob into a name -> bytes map. Errors on non-zip input
/// or malformed archives. Uses `BTreeMap` for stable iteration order so
/// extraction is deterministic regardless of how the archive was
/// produced. Duplicate entry names within the source archive collapse
/// to the last-read copy (defense-in-depth against adversarial inputs;
/// KAS-produced archives never duplicate).
pub(super) fn decode_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, SessionArchiveError> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|_e| SessionArchiveError::msg("archive is not a valid zip file"))?;
    let mut entries = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|_e| SessionArchiveError::msg("archive is not a valid zip file"))?;
        let name = entry.name().to_string();
        let mut buf = Vec::with_capacity((entry.size() as usize).min(ZIP_ENTRY_PREALLOC_CAP));
        entry
            .read_to_end(&mut buf)
            .map_err(|e| SessionArchiveError::io(format!("read zip entry {name}"), e))?;
        entries.insert(name, buf);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::temp_base;
    use super::*;

    #[test]
    fn safe_resolve_rejects_dangerous_inputs() {
        let cases: &[(&str, &str, &str)] = &[
            ("empty", "", "empty name"),
            ("null_byte", "foo\0bar", "null byte"),
            ("posix_absolute", "/etc/passwd", "absolute path"),
            ("windows_drive_letter", "C:Windows\\System32", "Windows drive letter"),
            ("dotdot_traversal", "../escape", "escapes destination"),
            ("backslash_dotdot_traversal", "..\\escape", "escapes destination"),
            ("deep_traversal", "a/../../escape", "escapes destination"),
        ];
        let base = temp_base();
        let mut failures = Vec::new();
        for (id, entry, expected) in cases {
            match safe_resolve(base.path(), entry) {
                Err(err) => {
                    let msg = err.to_string();
                    if !msg.contains(expected) {
                        failures.push(format!(
                            "[{id}] entry={entry:?}: expected substring {expected:?}, got {msg:?}"
                        ));
                    }
                },
                Ok(p) => {
                    failures.push(format!("[{id}] entry={entry:?}: expected error, got Ok({p:?})"));
                },
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn safe_resolve_returns_paths_for_safe_inputs() {
        let cases: &[(&str, &str, &[&str])] = &[
            ("simple_relative", "session.json", &["session.json"]),
            ("nested", "sub-executions/abc/messages.jsonl", &[
                "sub-executions",
                "abc",
                "messages.jsonl",
            ]),
            ("dot_segment", "./session.json", &["session.json"]),
            ("backslash_separator_normalized", "sub-executions\\abc", &[
                "sub-executions",
                "abc",
            ]),
        ];
        let base = temp_base();
        let mut failures = Vec::new();
        for (id, entry, expected_components) in cases {
            let mut expected = base.path().to_path_buf();
            for c in *expected_components {
                expected.push(c);
            }
            match safe_resolve(base.path(), entry) {
                Ok(actual) if actual == expected => {},
                Ok(actual) => {
                    failures.push(format!("[{id}] entry={entry:?}: expected {expected:?}, got {actual:?}"));
                },
                Err(err) => {
                    failures.push(format!("[{id}] entry={entry:?}: expected Ok, got Err({err})"));
                },
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn safe_resolve_handles_relative_base_dir() {
        let cwd = std::env::current_dir().unwrap();
        let resolved = safe_resolve(Path::new("./some-base"), "session.json").unwrap();
        assert_eq!(resolved, cwd.join("some-base").join("session.json"));
    }
}
