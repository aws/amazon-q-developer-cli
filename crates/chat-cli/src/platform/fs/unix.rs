use std::ffi::OsString;
use std::os::unix::ffi::{
    OsStrExt,
    OsStringExt,
};
use std::path::{
    Path,
    PathBuf,
};

/// Performs `a.join(b)`, except:
/// - if `b` is an absolute path, then the resulting path will equal `/a/b`
/// - if the prefix of `b` contains some `n` copies of a, then the resulting path will equal `/a/b`
pub(super) fn append(a: impl AsRef<Path>, b: impl AsRef<Path>) -> PathBuf {
    // Have to use byte slices since rust seems to always append
    // a forward slash at the end of a path...
    let a = a.as_ref().as_os_str().as_bytes();
    let mut b = b.as_ref().as_os_str().as_bytes();
    while b.starts_with(a) {
        b = b.strip_prefix(a).unwrap();
    }
    while b.starts_with(b"/") {
        b = b.strip_prefix(b"/").unwrap();
    }
    PathBuf::from(OsString::from_vec(a.to_vec())).join(PathBuf::from(OsString::from_vec(b.to_vec())))
}

/// Creates a new symbolic link on the filesystem.
///
/// The `link` path will be a symbolic link pointing to the `original` path.
pub(super) fn symlink_sync(original: impl AsRef<Path>, link: impl AsRef<Path>) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
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
        assert_append!("/abc/test", "/test", "/abc/test/test");
        assert_append!("/tmp/.dir", "/tmp/.dir/home/myuser", "/tmp/.dir/home/myuser");
        assert_append!("/tmp/.dir", "/tmp/hello", "/tmp/.dir/tmp/hello");
        assert_append!("/tmp/.dir", "/tmp/.dir/tmp/.dir/home/user", "/tmp/.dir/home/user");
    }
}
