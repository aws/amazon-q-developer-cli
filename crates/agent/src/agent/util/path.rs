use std::borrow::Cow;
use std::env::VarError;
use std::path::{
    Path,
    PathBuf,
};

use super::error::{
    ErrorContext as _,
    UtilError,
};
use super::providers::{
    EnvProvider,
    HomeProvider,
    RealProvider,
    SystemProvider,
};

/// Performs tilde and environment variable expansion on the provided input.
pub fn expand_path<'a>(input: &'a str, provider: &'_ impl SystemProvider) -> Result<Cow<'a, str>, UtilError> {
    Ok(shellexpand::full_with_context(
        input,
        shellexpand_home(provider),
        shellexpand_context(provider),
    )?)
}

/// Converts the given path to a normalized absolute path.
///
/// See [`canonicalize_path_sys`] for the canonicalization semantics.
pub fn canonicalize_path(path: impl AsRef<str>) -> Result<String, UtilError> {
    let sys = RealProvider;
    canonicalize_path_sys(path, &sys)
}

/// Convenience wrapper around [`resolve_path_fuzzy`] using the real system provider.
pub fn resolve_path_fuzzy_real(path: impl AsRef<str>) -> Result<String, UtilError> {
    let sys = RealProvider;
    resolve_path_fuzzy(path, &sys)
}

/// Converts the given path to a normalized absolute path, resolving symlinks where possible.
///
/// This function:
/// - Performs tilde expansion
/// - Performs env var expansion
/// - Converts relative paths to absolute using [`SystemProvider::cwd`]
/// - Resolves `.` and `..` path components (physically for the existing prefix, lexically for the
///   non-existent tail)
/// - Resolves symlinks on any ancestor directory that exists
///
/// # Non-existent paths
///
/// [`std::path::Path::canonicalize`] fails for paths that don't exist, which is common
/// for write targets (e.g. `> /tmp/new_file.log`). Instead of falling back to pure
/// lexical normalization (which would silently skip symlink resolution AND incorrectly
/// collapse `..` across symlinks), this function walks up the path until it finds an
/// existing ancestor, canonicalizes that ancestor (kernel-level physical resolution,
/// including any `..` components), then rejoins the remaining non-existent tail with
/// lexical `.`/`..` resolution. This matches `realpath -m` / Python's
/// `os.path.realpath(strict=False)` semantics.
///
/// This matters on systems where common directories are symlinks (e.g. macOS
/// `/tmp -> /private/tmp`): if `/tmp` is in an allowed-paths list (canonicalized
/// to `/private/tmp`), a non-existent target `/tmp/new.log` will correctly resolve
/// to `/private/tmp/new.log` rather than staying as `/tmp/new.log` (which would
/// cause a glob match against `/private/tmp/**` to spuriously fail).
///
/// It also matters for paths that mix symlinks and `..`: `/a/symlink/../foo` where
/// `symlink -> /b` resolves to `<physical_parent_of_/b>/foo`, matching what the
/// kernel would do when opening the file. Lexical collapse would incorrectly yield
/// `/a/foo` and could be a security hole if `/a` is allow-listed but `/b`'s parent
/// is not.
///
/// If no ancestor exists (extremely rare - root always exists on Unix), falls back
/// to pure lexical normalization.
pub fn canonicalize_path_sys<P: SystemProvider>(path: impl AsRef<str>, provider: &P) -> Result<String, UtilError> {
    let expanded =
        shellexpand::full_with_context(path.as_ref(), shellexpand_home(provider), shellexpand_context(provider))?;
    let expanded_path = Path::new(expanded.as_ref());

    let path_buf = if expanded_path.is_absolute() {
        // Already absolute path
        expanded_path.to_path_buf()
    } else {
        // Convert relative paths to absolute paths
        let current_dir = provider
            .cwd()
            .with_context(|| "could not get current directory".to_string())?;
        current_dir.join(expanded_path)
    };

    // Fast path: full canonicalization succeeds when the whole path exists.
    // dunce::canonicalize is used instead of std::fs::canonicalize because the latter
    // returns \\?\ verbatim paths on Windows which break downstream path comparisons.
    // On non-Windows platforms dunce::canonicalize is identical to std::fs::canonicalize.
    if let Ok(normalized) = dunce::canonicalize(&path_buf) {
        return Ok(normalized.to_string_lossy().to_string());
    }

    // Slow path: the path doesn't exist. Walk up to the nearest existing ancestor,
    // canonicalize it (resolving symlinks), then rejoin the non-existent tail.
    // This is the same inode-equivalent path that the kernel would produce when
    // the file is eventually created.
    if let Some(resolved) = canonicalize_with_ancestor_fallback(&path_buf) {
        return Ok(resolved.to_string_lossy().to_string());
    }

    // No ancestor canonicalized (extremely rare). Fall back to pure lexical
    // normalization. Symlinks are not resolved in this branch.
    Ok(normalize_path(&path_buf).to_string_lossy().to_string())
}

/// Canonicalize a path by walking up to the nearest existing ancestor, canonicalizing
/// it (resolving symlinks and `..` components physically, as the kernel does), then
/// rejoining the non-existent tail components with lexical `.`/`..` resolution.
///
/// This matches the semantics of `realpath -m` / Python's `os.path.realpath(strict=False)`:
/// for the existing prefix, `..` is resolved AFTER any preceding symlinks are dereferenced
/// (physical); for the non-existent tail, `..` is resolved lexically (safe, because no
/// symlinks can live in non-existent paths).
///
/// Returns `None` only if no ancestor canonicalizes (practically, empty paths - root
/// always canonicalizes on Unix).
fn canonicalize_with_ancestor_fallback(path: &Path) -> Option<PathBuf> {
    // Work at the component level rather than string-stripping via `Path::ancestors()`.
    // `Path::file_name()` returns `None` for `..` and `.`, which would cause those
    // components to be silently dropped if we tried to collect the tail via `file_name()`.
    let components: Vec<std::path::Component<'_>> = path.components().collect();

    // Find the longest prefix of components whose concatenated path canonicalizes.
    // Walk from full path down to just the root prefix. `.canonicalize()` on any such
    // prefix correctly handles symlinks and `..` in the existing part via the kernel.
    let mut end = components.len();
    while end > 0 {
        let prefix: PathBuf = components[..end].iter().collect();
        if let Ok(canonical) = dunce::canonicalize(&prefix) {
            // Apply the remaining (non-existent) components lexically. This is safe
            // because non-existent paths cannot contain symlinks, so `..` resolution
            // in the tail has no physical/lexical ambiguity.
            let mut result = canonical;
            for comp in &components[end..] {
                match comp {
                    std::path::Component::CurDir => {},
                    std::path::Component::ParentDir => {
                        result.pop();
                    },
                    std::path::Component::Normal(name) => {
                        result.push(name);
                    },
                    // `RootDir` / `Prefix` appearing mid-path would reset the result.
                    // Shouldn't happen after the first component, but handle defensively.
                    std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                        result.push(comp.as_os_str());
                    },
                }
            }
            return Some(result);
        }
        end -= 1;
    }
    None
}

/// Normalize all Unicode whitespace characters to ASCII space for comparison.
/// Safer than stripping — preserves space positions so "my file.txt" won't
/// accidentally match "myfile.txt". Only matches when whitespace differs in
/// type (e.g., U+202F vs U+0020), not in position.
fn normalize_whitespace(s: &str) -> String {
    s.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect()
}

/// Try to find a file in the parent directory that matches after normalizing
/// Unicode whitespace. Handles cases where filenames contain special whitespace
/// characters but the LLM outputs regular ASCII spaces.
fn try_fuzzy_whitespace_match(path: &Path) -> Option<PathBuf> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty())?;
    let filename = path.file_name()?.to_str()?;
    let normalized = normalize_whitespace(filename);

    let mut matched: Option<PathBuf> = None;
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
        let entry_name = entry.file_name();
        let Some(entry_str) = entry_name.to_str() else {
            continue;
        };
        if normalize_whitespace(entry_str) == normalized {
            if matched.is_some() {
                // Multiple fuzzy matches — ambiguous, bail out
                return None;
            }
            let actual = entry.path();
            matched = Some(actual.canonicalize().ok().unwrap_or(actual));
        }
    }
    matched
}

/// Resolve a path with fuzzy Unicode whitespace matching.
///
/// First attempts exact canonicalization via [`canonicalize_path_sys`]. If the
/// path doesn't exist, falls back to scanning the parent directory for a file
/// whose name matches after normalizing Unicode whitespace to ASCII spaces.
///
/// Use this instead of `canonicalize_path_sys` when the path originates from
/// an LLM tool call, where Unicode whitespace variants (U+202F, U+00A0) in
/// real filenames may have been replaced with regular ASCII spaces.
pub fn resolve_path_fuzzy<P: SystemProvider>(path: impl AsRef<str>, provider: &P) -> Result<String, UtilError> {
    let canonical = canonicalize_path_sys(path, provider)?;
    let canonical_path = Path::new(&canonical);
    if canonical_path.exists() {
        return Ok(canonical);
    }
    // Exact path doesn't exist — try fuzzy whitespace matching
    if let Some(matched) = try_fuzzy_whitespace_match(canonical_path) {
        return Ok(matched.to_string_lossy().to_string());
    }
    Ok(canonical)
}

/// Manually normalize a path by resolving . and .. components
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {
                // Skip current directory components
            },
            std::path::Component::ParentDir => {
                // Pop the last component for parent directory
                let prev_component = components.pop().expect("only traverse to parent for absolute paths");
                if prev_component == std::path::Component::RootDir {
                    // interpret self-referencing parent link for root on most systems
                    components.push(prev_component);
                }
            },
            _ => {
                components.push(component);
            },
        }
    }
    components.iter().collect()
}

/// Helper for [shellexpand::full_with_context]
fn shellexpand_home<H: HomeProvider>(provider: &H) -> impl Fn() -> Option<String> {
    || HomeProvider::home(provider).map(|h| h.to_string_lossy().to_string())
}

/// Helper for [shellexpand::full_with_context]
fn shellexpand_context<E: EnvProvider>(provider: &E) -> impl Fn(&str) -> Result<Option<String>, VarError> {
    |input: &str| Ok(EnvProvider::var(provider, input).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::util::test::TestProvider;

    #[test]
    fn test_canonicalize_path() {
        #[cfg(unix)]
        let sys = TestProvider::new()
            .with_var("TEST_VAR", "test_var")
            .with_cwd("/kirocli_test_home/testuser/testdir");

        #[cfg(windows)]
        let sys = TestProvider::new()
            .with_var("TEST_VAR", "test_var")
            .with_cwd("C:\\kirocli_test_home\\testuser\\testdir");

        #[cfg(unix)]
        let tests = [
            ("path", "/kirocli_test_home/testuser/testdir/path"),
            ("../**/.rs", "/kirocli_test_home/testuser/**/.rs"),
            ("~", "/kirocli_test_home/testuser"),
            ("~/file/**.md", "/kirocli_test_home/testuser/file/**.md"),
            (
                "~/.././../kirocli_test_home//testuser/path/..",
                "/kirocli_test_home/testuser",
            ),
            ("../../../../../../abc", "/abc"), // traversing through root multiple times
        ];

        #[cfg(windows)]
        let tests = [
            ("path", "C:\\kirocli_test_home\\testuser\\testdir\\path"),
            ("../**/.rs", "C:\\kirocli_test_home\\testuser\\**\\.rs"),
            ("~", "C:\\kirocli_test_home\\testuser"),
            ("~/file/**.md", "C:\\kirocli_test_home\\testuser\\file\\**.md"),
            (
                "~/.././..\\kirocli_test_home\\testuser\\path\\..",
                "C:\\kirocli_test_home\\testuser",
            ),
        ];

        for (path, expected) in tests {
            let actual = canonicalize_path_sys(path, &sys).unwrap();
            assert_eq!(
                actual, expected,
                "Expected '{}' to expand to '{}', instead got '{}'",
                path, expected, actual
            );
        }
    }

    #[test]
    fn test_normalize_path() {
        let tests = [("/./abc/.", "/abc"), ("/abc/../def/", "/def"), ("/../abc/", "/abc")];

        for (path, expected) in tests {
            assert_eq!(Path::new(&expected), normalize_path(Path::new(&path)));
        }
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("hello world"), "hello world");
        assert_eq!(normalize_whitespace("no\u{202F}break"), "no break");
        assert_eq!(normalize_whitespace("no\u{00A0}break"), "no break");
        assert_eq!(normalize_whitespace("a\u{202F}b\u{00A0}c"), "a b c");
    }

    #[test]
    fn test_fuzzy_whitespace_match_finds_unicode_spaces() {
        let dir = tempfile::tempdir().unwrap();
        // Create a file with narrow no-break space (U+202F) — like macOS screenshots
        let actual_name = "Screenshot\u{202F}2026-03-22.png";
        std::fs::write(dir.path().join(actual_name), b"test").unwrap();

        // Query with regular ASCII space
        let query_path = dir.path().join("Screenshot 2026-03-22.png");
        let result = try_fuzzy_whitespace_match(&query_path);
        assert!(result.is_some(), "should find file with Unicode whitespace");
        assert!(result.unwrap().exists(), "matched path should exist");
    }

    #[test]
    fn test_fuzzy_whitespace_match_returns_none_for_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("other_file.txt"), b"test").unwrap();

        let query_path = dir.path().join("nonexistent file.txt");
        assert!(try_fuzzy_whitespace_match(&query_path).is_none());
    }

    #[test]
    fn test_fuzzy_whitespace_match_exact_match_not_needed() {
        let dir = tempfile::tempdir().unwrap();
        // File exists with exact name — canonicalize would succeed, so fuzzy
        // match shouldn't be needed, but it should still work if called
        let name = "file with spaces.txt";
        std::fs::write(dir.path().join(name), b"test").unwrap();

        let query_path = dir.path().join(name);
        let result = try_fuzzy_whitespace_match(&query_path);
        assert!(result.is_some());
    }

    #[test]
    fn test_fuzzy_whitespace_match_ambiguous_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        // Two files that differ only in whitespace type — ambiguous
        std::fs::write(dir.path().join("file name.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("file\u{00A0}name.txt"), b"b").unwrap();

        // Both normalize to "file name.txt" — should return None (ambiguous)
        let query_path = dir.path().join("file\u{202F}name.txt");
        assert!(try_fuzzy_whitespace_match(&query_path).is_none());
    }

    #[test]
    fn test_fuzzy_whitespace_match_no_false_positive_on_missing_spaces() {
        let dir = tempfile::tempdir().unwrap();
        // "myfile.txt" exists but user asked for "my file.txt"
        // Normalize preserves space positions, so these should NOT match
        std::fs::write(dir.path().join("myfile.txt"), b"test").unwrap();

        let query_path = dir.path().join("my file.txt");
        assert!(try_fuzzy_whitespace_match(&query_path).is_none());
    }

    /// Regression: non-existent paths under a symlinked ancestor must have the symlink
    /// resolved (not just lexically normalized), so that string comparison against an
    /// independently-canonicalized allow-list path matches.
    #[cfg(unix)]
    #[test]
    fn test_canonicalize_path_sys_resolves_symlinked_ancestor_for_missing_target() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        // Create real/<existing> and symlink link -> real
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        symlink(&real, &link).unwrap();

        let sys = TestProvider::new_with_base(dir.path());
        let real_canonical = real.canonicalize().unwrap();

        // Access a non-existent file through the symlink path.
        let via_link = link.join("new_file.txt");
        let expected = real_canonical.join("new_file.txt").to_string_lossy().to_string();

        let got = canonicalize_path_sys(via_link.to_string_lossy(), &sys).unwrap();
        assert_eq!(
            got, expected,
            "non-existent file under symlinked ancestor should resolve the symlink"
        );

        // Same via a deeply nested non-existent path - walks up to the symlink and rejoins.
        let deep_via_link = link.join("new_dir/sub/file.txt");
        let expected_deep = real_canonical
            .join("new_dir/sub/file.txt")
            .to_string_lossy()
            .to_string();
        let got_deep = canonicalize_path_sys(deep_via_link.to_string_lossy(), &sys).unwrap();
        assert_eq!(
            got_deep, expected_deep,
            "deeply nested non-existent path should still resolve the symlinked ancestor"
        );
    }

    /// Regression: `..` after a symlink must resolve physically, not lexically. The kernel
    /// resolves `/link/..` as `<physical_parent_of_link_target>`, so a lexical collapse
    /// of `/allowed/link/../foo` to `/allowed/foo` would let an agent escape the
    /// allow-list via a symlink that points outside.
    #[cfg(unix)]
    #[test]
    fn test_canonicalize_path_sys_handles_dotdot_after_symlink_physically() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        // Layout:
        //   <root>/allowed/           (dir - would be an allow-list entry)
        //   <root>/outside/           (dir - NOT allow-listed)
        //   <root>/allowed/escape -> <root>/outside
        let allowed = root.path().join("allowed");
        let outside = root.path().join("outside");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let escape = allowed.join("escape");
        symlink(&outside, &escape).unwrap();

        let sys = TestProvider::new_with_base(root.path());

        // `/allowed/escape/../target.txt` should physically resolve to
        // `<physical_parent_of_outside>/target.txt` (i.e. <root>/target.txt), NOT
        // `<root>/allowed/target.txt` (which lexical `..` collapse would produce).
        let via_escape = escape.join("../target.txt");
        let got = canonicalize_path_sys(via_escape.to_string_lossy(), &sys).unwrap();
        let expected = outside
            .canonicalize()
            .unwrap()
            .parent()
            .unwrap()
            .join("target.txt")
            .to_string_lossy()
            .to_string();
        assert_eq!(
            got, expected,
            "`..` after a symlink must resolve physically (parent of symlink target), not lexically"
        );

        // Sanity: without the `..`, the escape symlink itself resolves into `outside`.
        let via_escape_file = escape.join("nope.txt");
        let got_direct = canonicalize_path_sys(via_escape_file.to_string_lossy(), &sys).unwrap();
        let expected_direct = outside
            .canonicalize()
            .unwrap()
            .join("nope.txt")
            .to_string_lossy()
            .to_string();
        assert_eq!(got_direct, expected_direct);
    }
}
