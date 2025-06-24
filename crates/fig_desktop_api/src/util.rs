use std::borrow::Cow;

use camino::{
    Utf8Path,
    Utf8PathBuf,
};
use fig_os_shim::Env;
use fig_proto::fig::FilePath;

pub fn resolve_filepath<'a>(file_path: &'a FilePath, env: &Env) -> Cow<'a, Utf8Path> {
    let convert = |path: &'a str| -> Cow<'_, str> {
        if file_path.expand_tilde_in_path() {
            shellexpand::tilde_with_context(path, || env.home().and_then(|p| p.to_str().map(|s| s.to_owned())))
        } else {
            path.into()
        }
    };

    match file_path.relative_to {
        Some(ref relative_to) => Utf8Path::new(&convert(relative_to))
            .join(&*convert(&file_path.path))
            .into(),
        None => match convert(&file_path.path) {
            Cow::Borrowed(path) => Utf8Path::new(path).into(),
            Cow::Owned(path) => Utf8PathBuf::from(path).into(),
        },
    }
}

/// Builds a [`FilePath`] with the given path.
///
/// # Example
///
/// ```
/// use fig_desktop_api::util::build_filepath;
/// use fig_proto::fig::FilePath;
///
/// let path = build_filepath("foo/bar");
/// assert_eq!(path.path, "foo/bar");
/// assert_eq!(path.relative_to, None);
/// assert_eq!(path.expand_tilde_in_path, Some(false));
/// ```
pub fn build_filepath(path: impl Into<String>) -> FilePath {
    FilePath {
        path: path.into(),
        relative_to: None,
        expand_tilde_in_path: Some(false),
    }
}
