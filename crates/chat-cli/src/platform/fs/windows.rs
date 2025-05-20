use std::fs::metadata;
use std::io;
use std::path::{
    Component,
    Path,
    PathBuf,
};

/// Performs `a.join(b)`, except:
/// - if `b` is an absolute path, then the resulting path will equal `/a/b`
/// - if the prefix of `b` contains some `n` copies of a, then the resulting path will equal `/a/b`
pub(super) fn append(a: impl AsRef<Path>, b: impl AsRef<Path>) -> PathBuf {
    let a = a.as_ref();
    let b = b.as_ref();

    // If b is an absolute path, we need to handle it specially
    if b.is_absolute() {
        // Extract drive letter from b if it exists
        let b_without_prefix = b
            .components()
            .skip_while(|c| matches!(c, Component::Prefix(_)))
            .collect::<PathBuf>();

        // Join a with the path components of b without the prefix
        return a.join(b_without_prefix);
    }

    // Check if b starts with a
    let a_str = a.to_string_lossy().to_string();
    let b_str = b.to_string_lossy().to_string();

    if b_str.starts_with(&a_str) {
        // Remove the prefix that matches a
        let remaining = &b_str[a_str.len()..];
        let remaining = remaining.trim_start_matches('\\');
        return a.join(remaining);
    }

    // Standard join for other cases
    a.join(b)
}

/// Creates a new symbolic link on the filesystem.
///
/// The `link` path will be a symbolic link pointing to the `original` path.
/// On Windows, we need to determine if the target is a file or directory.
pub(super) fn symlink_sync(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    // Determine if the original is a file or directory
    let meta = metadata(original.as_ref())?;
    if meta.is_dir() {
        std::os::windows::fs::symlink_dir(original, link)
    } else {
        std::os::windows::fs::symlink_file(original, link)
    }
}

/// Creates a new symbolic link asynchronously.
///
/// This is a helper function for the Windows implementation.
pub(super) async fn symlink_async(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    // Determine if the original is a file or directory
    let meta = metadata(original.as_ref())?;
    if meta.is_dir() {
        tokio::fs::symlink_dir(original, link).await
    } else {
        tokio::fs::symlink_file(original, link).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append() {
        macro_rules! assert_append {
            ($a:expr, $b:expr, $expected:expr) => {
                assert_eq!(append($a, $b), PathBuf::from($expected));
            };
        }

        assert_append!("C:\\temp", "D:\\test", "C:\\temp\\test");
        assert_append!("C:\\temp", "C:\\temp\\subdir", "C:\\temp\\subdir");
        assert_append!("C:\\temp", "C:\\temp\\subdir\\file.txt", "C:\\temp\\subdir\\file.txt");
        assert_append!("C:\\temp", "subdir\\file.txt", "C:\\temp\\subdir\\file.txt");
    }
}
