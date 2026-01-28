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
    let a_path = a.as_ref();
    let b_path = b.as_ref();

    // Extract the non-prefix, non-root components of paths a and b for comparison
    let a_normal_components: Vec<_> = a_path
        .components()
        .filter(|c| !matches!(c, Component::Prefix(_) | Component::RootDir))
        .collect();

    // Create a version of b_path without prefix/root components
    let mut b_normal_path = PathBuf::new();
    for comp in b_path.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => (),
            _ => b_normal_path.push(comp.as_os_str()),
        }
    }

    // Iteratively strip a from the beginning of b
    let mut cleaned_b = b_normal_path.clone();
    let mut done = false;

    while !done {
        let b_normal_components: Vec<_> = cleaned_b.components().collect();

        if b_normal_components.len() >= a_normal_components.len() {
            // Check if the beginning of b matches a (case-insensitive on Windows)
            let matches = a_normal_components
                .iter()
                .zip(b_normal_components.iter())
                .all(|(a_comp, b_comp)| {
                    // Case-insensitive comparison for Windows
                    a_comp.as_os_str().to_string_lossy().to_lowercase()
                        == b_comp.as_os_str().to_string_lossy().to_lowercase()
                });

            if matches {
                // Create a new path with a's components removed from the beginning of b
                let mut new_b = PathBuf::new();
                for comp in b_normal_components.iter().skip(a_normal_components.len()) {
                    new_b.push(comp.as_os_str());
                }
                cleaned_b = new_b;
            } else {
                done = true;
            }
        } else {
            done = true;
        }
    }

    // Join the paths
    a_path.join(cleaned_b)
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

        // Test different drive letters (should strip prefix)
        assert_append!("C:\\temp", "D:\\test", "C:\\temp\\test");

        // Test same path prefixes (should use strip_prefix)
        assert_append!("C:\\temp", "C:\\temp\\subdir", "C:\\temp\\subdir");
        assert_append!("C:\\temp", "C:\\temp\\subdir\\file.txt", "C:\\temp\\subdir\\file.txt");

        // Test relative path (standard join)
        assert_append!("C:\\temp", "subdir\\file.txt", "C:\\temp\\subdir\\file.txt");

        // Test different absolute paths with same drive (strip drive and root)
        assert_append!("C:\\temprootdir", "C:\\test_file.txt", "C:\\temprootdir\\test_file.txt");

        // Test different absolute paths with different drives
        assert_append!("C:\\temprootdir", "D:\\test_file.txt", "C:\\temprootdir\\test_file.txt");

        // Test paths with mixed case (should be case-insensitive on Windows)
        assert_append!("C:\\Temp", "c:\\temp\\file.txt", "C:\\Temp\\file.txt");
    }
}

#[cfg(test)]
mod integration_tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_append_with_real_paths() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Test appending an absolute path
        let drive_letter = temp_path.to_string_lossy().chars().next().unwrap_or('C');
        let absolute_path = format!("{}:\\test.txt", drive_letter);

        let result = append(temp_path, absolute_path);
        assert!(result.to_string_lossy().contains("test.txt"));
        assert!(!result.to_string_lossy().contains(":\\test.txt"));
    }
}
