use std::collections::HashMap;
use std::fs::Permissions;
use std::io;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    Mutex,
};

use tempfile::TempDir;
use tokio::fs;

#[derive(Debug, Clone, Default)]
pub struct Fs(inner::Inner);

mod inner {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        Mutex,
    };

    use tempfile::TempDir;

    #[derive(Debug, Clone, Default)]
    pub(super) enum Inner {
        #[default]
        Real,
        /// Uses the real filesystem except acts as if the process has
        /// a different root directory by using [TempDir]
        Chroot(Arc<TempDir>),
        Fake(Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>),
    }
}

impl Fs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_chroot() -> Self {
        let tempdir = tempfile::tempdir().expect("failed creating temporary directory");
        Self(inner::Inner::Chroot(tempdir.into()))
    }

    pub fn is_chroot(&self) -> bool {
        matches!(self.0, inner::Inner::Chroot(_))
    }

    pub fn from_slice(vars: &[(&str, &str)]) -> Self {
        use inner::Inner;
        let map: HashMap<_, _> = vars
            .iter()
            .map(|(k, v)| (PathBuf::from(k), v.as_bytes().to_vec()))
            .collect();
        Self(Inner::Fake(Arc::new(Mutex::new(map))))
    }

    pub async fn create_new(&self, path: impl AsRef<Path>) -> io::Result<fs::File> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::File::create_new(path).await,
            Inner::Chroot(root) => fs::File::create_new(append(root.path(), path)).await,
            Inner::Fake(_) => Err(io::Error::new(io::ErrorKind::Other, "unimplemented")),
        }
    }

    pub async fn create_dir(&self, path: impl AsRef<Path>) -> io::Result<()> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::create_dir(path).await,
            Inner::Chroot(root) => fs::create_dir(append(root.path(), path)).await,
            Inner::Fake(_) => Err(io::Error::new(io::ErrorKind::Other, "unimplemented")),
        }
    }

    pub async fn create_dir_all(&self, path: impl AsRef<Path>) -> io::Result<()> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::create_dir_all(path).await,
            Inner::Chroot(root) => fs::create_dir_all(append(root.path(), path)).await,
            Inner::Fake(_) => Err(io::Error::new(io::ErrorKind::Other, "unimplemented")),
        }
    }

    /// Attempts to open a file in read-only mode.
    ///
    /// This is a proxy to [`tokio::fs::File::open`].
    pub async fn open(&self, path: impl AsRef<Path>) -> io::Result<fs::File> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::File::open(path).await,
            Inner::Chroot(root) => fs::File::open(append(root.path(), path)).await,
            Inner::Fake(_) => Err(io::Error::new(io::ErrorKind::Other, "unimplemented")),
        }
    }

    pub async fn read(&self, path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::read(path).await,
            Inner::Chroot(root) => fs::read(append(root.path(), path)).await,
            Inner::Fake(map) => {
                let Ok(lock) = map.lock() else {
                    return Err(io::Error::new(io::ErrorKind::Other, "poisoned lock"));
                };
                let Some(data) = lock.get(path.as_ref()) else {
                    return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
                };
                Ok(data.clone())
            },
        }
    }

    pub async fn read_to_string(&self, path: impl AsRef<Path>) -> io::Result<String> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::read_to_string(path).await,
            Inner::Chroot(root) => fs::read_to_string(append(root.path(), path)).await,
            Inner::Fake(map) => {
                let Ok(lock) = map.lock() else {
                    return Err(io::Error::new(io::ErrorKind::Other, "poisoned lock"));
                };
                let Some(data) = lock.get(path.as_ref()) else {
                    return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
                };
                match String::from_utf8(data.clone()) {
                    Ok(string) => Ok(string),
                    Err(err) => Err(io::Error::new(io::ErrorKind::InvalidData, err)),
                }
            },
        }
    }

    pub fn read_to_string_sync(&self, path: impl AsRef<Path>) -> io::Result<String> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => std::fs::read_to_string(path),
            Inner::Chroot(root) => std::fs::read_to_string(append(root.path(), path)),
            Inner::Fake(map) => {
                let Ok(lock) = map.lock() else {
                    return Err(io::Error::new(io::ErrorKind::Other, "poisoned lock"));
                };
                let Some(data) = lock.get(path.as_ref()) else {
                    return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
                };
                match String::from_utf8(data.clone()) {
                    Ok(string) => Ok(string),
                    Err(err) => Err(io::Error::new(io::ErrorKind::InvalidData, err)),
                }
            },
        }
    }

    /// Creates a future that will open a file for writing and write the entire
    /// contents of `contents` to it.
    ///
    /// This is a proxy to [`tokio::fs::write`].
    pub async fn write(&self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::write(path, contents).await,
            Inner::Chroot(root) => fs::write(append(root.path(), path), contents).await,
            Inner::Fake(map) => {
                let Ok(mut lock) = map.lock() else {
                    return Err(io::Error::new(io::ErrorKind::Other, "poisoned lock"));
                };
                lock.insert(path.as_ref().to_owned(), contents.as_ref().to_owned());
                Ok(())
            },
        }
    }

    /// Removes a file from the filesystem.
    ///
    /// Note that there is no guarantee that the file is immediately deleted (e.g.
    /// depending on platform, other open file descriptors may prevent immediate
    /// removal).
    ///
    /// This is a proxy to [`tokio::fs::remove_file`].
    pub async fn remove_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::remove_file(path).await,
            Inner::Chroot(root) => fs::remove_file(append(root.path(), path)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Removes a directory at this path, after removing all its contents. Use carefully!
    ///
    /// This is a proxy to [`tokio::fs::remove_dir_all`].
    pub async fn remove_dir_all(&self, path: impl AsRef<Path>) -> io::Result<()> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::remove_dir_all(path).await,
            Inner::Chroot(root) => fs::remove_dir_all(append(root.path(), path)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Renames a file or directory to a new name, replacing the original file if
    /// `to` already exists.
    ///
    /// This will not work if the new name is on a different mount point.
    ///
    /// This is a proxy to [`tokio::fs::rename`].
    pub async fn rename(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::rename(from, to).await,
            Inner::Chroot(root) => fs::rename(append(root.path(), from), append(root.path(), to)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Copies the contents of one file to another. This function will also copy the permission bits
    /// of the original file to the destination file.
    /// This function will overwrite the contents of to.
    ///
    /// This is a proxy to [`tokio::fs::copy`].
    pub async fn copy(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<u64> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::copy(from, to).await,
            Inner::Chroot(root) => fs::copy(append(root.path(), from), append(root.path(), to)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Returns `Ok(true)` if the path points at an existing entity.
    ///
    /// This function will traverse symbolic links to query information about the
    /// destination file. In case of broken symbolic links this will return `Ok(false)`.
    ///
    /// This is a proxy to [`tokio::fs::try_exists`].
    pub async fn try_exists(&self, path: impl AsRef<Path>) -> Result<bool, io::Error> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::try_exists(path).await,
            Inner::Chroot(root) => fs::try_exists(append(root.path(), path)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Returns `true` if the path points at an existing entity.
    ///
    /// This is a proxy to [std::path::Path::exists]. See the related doc comment in std
    /// on the pitfalls of using this versus [std::path::Path::try_exists].
    pub fn exists(&self, path: impl AsRef<Path>) -> bool {
        use inner::Inner;
        match &self.0 {
            Inner::Real => path.as_ref().exists(),
            Inner::Chroot(root) => append(root.path(), path).exists(),
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Returns `true` if the path points at an existing entity without following symlinks.
    ///
    /// This does *not* guarantee that the path doesn't point to a symlink. For example, `false`
    /// will be returned if the user doesn't have permission to perform a metadata operation on
    /// `path`.
    pub async fn symlink_exists(&self, path: impl AsRef<Path>) -> bool {
        match self.symlink_metadata(path).await {
            Ok(_) => true,
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => true,
            Err(_) => false,
        }
    }

    pub async fn create_tempdir(&self) -> io::Result<TempDir> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => TempDir::new(),
            Inner::Chroot(root) => TempDir::new_in(root.path()),
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Creates a new symbolic link on the filesystem.
    ///
    /// The `link` path will be a symbolic link pointing to the `original` path.
    ///
    /// This is a proxy to [`tokio::fs::symlink`].
    #[cfg(unix)]
    pub async fn symlink(&self, original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::symlink(original, link).await,
            Inner::Chroot(root) => fs::symlink(append(root.path(), original), append(root.path(), link)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Creates a new symbolic link on the filesystem.
    ///
    /// The `link` path will be a symbolic link pointing to the `original` path.
    ///
    /// This is a proxy to [`std::os::unix::fs::symlink`].
    #[cfg(unix)]
    pub fn symlink_sync(&self, original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => std::os::unix::fs::symlink(original, link),
            Inner::Chroot(root) => std::os::unix::fs::symlink(append(root.path(), original), append(root.path(), link)),
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Query the metadata about a file without following symlinks.
    ///
    /// This is a proxy to [`tokio::fs::symlink_metadata`]
    ///
    /// # Errors
    ///
    /// This function will return an error in the following situations, but is not
    /// limited to just these cases:
    ///
    /// * The user lacks permissions to perform `metadata` call on `path`.
    /// * `path` does not exist.
    pub async fn symlink_metadata(&self, path: impl AsRef<Path>) -> io::Result<std::fs::Metadata> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::symlink_metadata(path).await,
            Inner::Chroot(root) => fs::symlink_metadata(append(root.path(), path)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Reads a symbolic link, returning the file that the link points to.
    ///
    /// This is a proxy to [`tokio::fs::read_link`].
    pub async fn read_link(&self, path: impl AsRef<Path>) -> io::Result<PathBuf> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::read_link(path).await,
            Inner::Chroot(root) => Ok(append(root.path(), fs::read_link(append(root.path(), path)).await?)),
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Returns a stream over the entries within a directory.
    ///
    /// This is a proxy to [`tokio::fs::read_dir`].
    pub async fn read_dir(&self, path: impl AsRef<Path>) -> Result<fs::ReadDir, io::Error> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::read_dir(path).await,
            Inner::Chroot(root) => fs::read_dir(append(root.path(), path)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Returns the canonical, absolute form of a path with all intermediate
    /// components normalized and symbolic links resolved.
    ///
    /// This is a proxy to [`tokio::fs::canonicalize`].
    pub async fn canonicalize(&self, path: impl AsRef<Path>) -> Result<PathBuf, io::Error> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::canonicalize(path).await,
            Inner::Chroot(root) => fs::canonicalize(append(root.path(), path)).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// Changes the permissions found on a file or a directory.
    ///
    /// This is a proxy to [`tokio::fs::set_permissions`]
    pub async fn set_permissions(&self, path: impl AsRef<Path>, perm: Permissions) -> Result<(), io::Error> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => fs::set_permissions(path, perm).await,
            Inner::Chroot(root) => fs::set_permissions(append(root.path(), path), perm).await,
            Inner::Fake(_) => panic!("unimplemented"),
        }
    }

    /// For test [Fs]'s that use a different root, returns an absolute path.
    ///
    /// This must be used for any paths indirectly used by code using a chroot
    /// [Fs].
    pub fn chroot_path(&self, path: impl AsRef<Path>) -> PathBuf {
        use inner::Inner;
        match &self.0 {
            Inner::Chroot(root) => append(root.path(), path),
            _ => path.as_ref().to_path_buf(),
        }
    }

    /// See [Fs::chroot_path].
    pub fn chroot_path_str(&self, path: impl AsRef<Path>) -> String {
        use inner::Inner;
        match &self.0 {
            Inner::Chroot(root) => append(root.path(), path).to_string_lossy().to_string(),
            _ => path.as_ref().to_path_buf().to_string_lossy().to_string(),
        }
    }
}

/// Performs `a.join(b)`, except:
/// - if `b` is an absolute path, then the resulting path will equal `/a/b`
/// - if the prefix of `b` contains some `n` copies of a, then the resulting path will equal `/a/b`
#[cfg(unix)]
fn append(a: impl AsRef<Path>, b: impl AsRef<Path>) -> PathBuf {
    use std::ffi::OsString;
    use std::os::unix::ffi::{
        OsStrExt,
        OsStringExt,
    };

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

#[cfg(windows)]
fn append(a: impl AsRef<Path>, b: impl AsRef<Path>) -> PathBuf {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_impl_is_real() {
        let fs = Fs::default();
        assert!(matches!(fs.0, inner::Inner::Real));
    }

    #[tokio::test]
    async fn test_fake() {
        let dir = PathBuf::from("/dir");
        let fs = Fs::from_slice(&[("/test", "test")]);

        fs.create_dir(dir.join("create_dir")).await.unwrap_err();
        fs.create_dir_all(dir.join("create/dir/all")).await.unwrap_err();
        fs.write(dir.join("write"), b"write").await.unwrap();
        assert_eq!(fs.read(dir.join("write")).await.unwrap(), b"write");
        assert_eq!(fs.read_to_string(dir.join("write")).await.unwrap(), "write");
    }

    #[tokio::test]
    async fn test_real() {
        let dir = tempfile::tempdir().unwrap();
        let fs = Fs::new();

        fs.create_dir(dir.path().join("create_dir")).await.unwrap();
        fs.create_dir_all(dir.path().join("create/dir/all")).await.unwrap();
        fs.write(dir.path().join("write"), b"write").await.unwrap();
        assert_eq!(fs.read(dir.path().join("write")).await.unwrap(), b"write");
        assert_eq!(fs.read_to_string(dir.path().join("write")).await.unwrap(), "write");
    }

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

    #[tokio::test]
    async fn test_read_to_string() {
        let fs = Fs::new();
        fs.write("fake", "contents").await.unwrap();
        fs.write("invalid_utf8", &[255]).await.unwrap();

        // async tests
        assert_eq!(
            fs.read_to_string("fake").await.unwrap(),
            "contents",
            "should read fake file"
        );
        assert!(
            fs.read_to_string("unknown")
                .await
                .is_err_and(|err| err.kind() == io::ErrorKind::NotFound),
            "unknown path should return NotFound"
        );
        assert!(
            fs.read_to_string("invalid_utf8")
                .await
                .is_err_and(|err| err.kind() == io::ErrorKind::InvalidData),
            "invalid utf8 should return InvalidData"
        );

        // sync tests
        assert_eq!(
            fs.read_to_string_sync("fake").unwrap(),
            "contents",
            "should read fake file"
        );
        assert!(
            fs.read_to_string_sync("unknown")
                .is_err_and(|err| err.kind() == io::ErrorKind::NotFound),
            "unknown path should return NotFound"
        );
        assert!(
            fs.read_to_string_sync("invalid_utf8")
                .is_err_and(|err| err.kind() == io::ErrorKind::InvalidData),
            "invalid utf8 should return InvalidData"
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_chroot_file_operations_for_unix() {
        if nix::unistd::Uid::effective().is_root() {
            println!("currently running as root, skipping.");
            return;
        }

        let fs = Fs::new_chroot();
        assert!(fs.is_chroot());

        fs.write("/fake", "contents").await.unwrap();
        assert_eq!(fs.read_to_string("/fake").await.unwrap(), "contents");
        assert_eq!(fs.read_to_string_sync("/fake").unwrap(), "contents");

        assert!(!fs.try_exists("/etc").await.unwrap());

        fs.create_dir_all("/etc/b/c").await.unwrap();
        assert!(fs.try_exists("/etc").await.unwrap());
        let mut read_dir = fs.read_dir("/etc").await.unwrap();
        let e = read_dir.next_entry().await.unwrap();
        assert!(e.unwrap().metadata().await.unwrap().is_dir());
        assert!(read_dir.next_entry().await.unwrap().is_none());

        fs.remove_dir_all("/etc").await.unwrap();
        assert!(!fs.try_exists("/etc").await.unwrap());

        fs.copy("/fake", "/fake_copy").await.unwrap();
        assert_eq!(fs.read_to_string("/fake_copy").await.unwrap(), "contents");
        assert_eq!(fs.read_to_string_sync("/fake_copy").unwrap(), "contents");

        fs.remove_file("/fake_copy").await.unwrap();
        assert!(!fs.try_exists("/fake_copy").await.unwrap());

        fs.symlink("/fake", "/fake_symlink").await.unwrap();
        fs.symlink_sync("/fake", "/fake_symlink_sync").unwrap();
        assert_eq!(fs.read_to_string("/fake_symlink").await.unwrap(), "contents");
        assert_eq!(
            fs.read_to_string(fs.read_link("/fake_symlink").await.unwrap())
                .await
                .unwrap(),
            "contents"
        );
        assert_eq!(fs.read_to_string("/fake_symlink_sync").await.unwrap(), "contents");
        assert_eq!(fs.read_to_string_sync("/fake_symlink").unwrap(), "contents");

        // Checking symlink exist
        assert!(fs.symlink_exists("/fake_symlink").await);
        assert!(fs.exists("/fake_symlink"));
        fs.remove_file("/fake").await.unwrap();
        assert!(fs.symlink_exists("/fake_symlink").await);
        assert!(!fs.exists("/fake_symlink"));

        // Checking rename
        fs.write("/rename_1", "abc").await.unwrap();
        fs.write("/rename_2", "123").await.unwrap();
        fs.rename("/rename_2", "/rename_1").await.unwrap();
        assert_eq!(fs.read_to_string("/rename_1").await.unwrap(), "123");

        // Checking open
        assert!(fs.open("/does_not_exist").await.is_err());
        assert!(fs.open("/rename_1").await.is_ok());
    }

    #[tokio::test]
    async fn test_chroot_tempdir() {
        let fs = Fs::new_chroot();
        let tempdir = fs.create_tempdir().await.unwrap();
        if let Fs(inner::Inner::Chroot(root)) = fs {
            assert_eq!(tempdir.path().parent().unwrap(), root.path());
        } else {
            panic!("tempdir should be created under root");
        }
    }

    #[tokio::test]
    async fn test_create_new() {
        let fs = Fs::new_chroot();
        fs.create_new("my_file.txt").await.unwrap();
        assert!(fs.create_new("my_file.txt").await.is_err());
    }
}
