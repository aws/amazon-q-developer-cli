use std::borrow::Cow;
use std::io::{
    self,
    ErrorKind,
};
use std::path::{
    Path,
    PathBuf,
};

use fig_util::CLI_BINARY_NAME;
use owo_colors::OwoColorize as _;
use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Legacy integration: {0}")]
    LegacyInstallation(Cow<'static, str>),
    #[error("Improper integration installation: {0}")]
    ImproperInstallation(Cow<'static, str>),
    #[error("Integration not installed: {0}")]
    NotInstalled(Cow<'static, str>),
    #[error("File does not exist: {}", .0.to_string_lossy())]
    FileDoesNotExist(Cow<'static, Path>),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Dir(#[from] fig_util::directories::DirectoryError),
    #[error("Regex Error: {0}")]
    Regex(#[from] regex::Error),
    #[error(transparent)]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error("{0}")]
    Custom(Cow<'static, str>),
    #[cfg(target_os = "macos")]
    #[error(transparent)]
    InputMethod(#[from] crate::input_method::InputMethodError),
    #[cfg(target_os = "macos")]
    #[error("Application not installed: {0}")]
    ApplicationNotInstalled(Cow<'static, str>),
    #[error(transparent)]
    SerdeJSON(#[from] serde_json::Error),
    #[cfg(target_os = "macos")]
    #[error(transparent)]
    PList(#[from] plist::Error),
    #[error("Permission denied: {}", .path.display())]
    PermissionDenied { path: PathBuf, inner: io::Error },
    #[error("nix: {}", .0)]
    Nix(#[from] nix::Error),
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    ExtensionsError(#[from] dbus::gnome_shell::ExtensionsError),

    #[error("{context}: {error}")]
    Context {
        #[source]
        error: Box<Self>,
        context: Cow<'static, str>,
    },
}

#[derive(Debug, Clone, serde::Serialize)]

pub struct VerboseMessage {
    pub title: String,
    pub message: Option<String>,
}

impl std::fmt::Display for VerboseMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.title)?;
        if let Some(message) = &self.message {
            writeln!(f, "\n{}\n", message)?;
        }
        Ok(())
    }
}

impl Error {
    /// Returns a verbose message with ansii colors
    pub fn verbose_message(&self) -> VerboseMessage {
        match self {
            Self::PermissionDenied { path, inner } => VerboseMessage {
                title: format!("Permissions denied for {}", path.display().bold()),
                message: Some(
                    [
                        format!(
                            "To automatically fix the permissions run: {}",
                            format!("sudo {CLI_BINARY_NAME} debug fix-permissions").magenta()
                        ),
                        "".into(),
                        format!("  Error: {}", inner.red()),
                    ]
                    .join("\n"),
                ),
            },
            err => VerboseMessage {
                title: err.to_string(),
                message: None,
            },
        }
    }
}

pub(crate) trait ErrorExt<T, E> {
    #[allow(dead_code)]
    fn context(self, context: impl Into<Cow<'static, str>>) -> Result<T, Error>;

    #[allow(dead_code)]
    fn with_context(self, context_fn: impl FnOnce(&E) -> String) -> Result<T, Error>;

    /// If this is an [`io::Error`] and is [`io::ErrorKind::PermissionDenied`] map to
    /// [`Error::PermissionDenied`]
    fn with_path(self, path: impl AsRef<Path>) -> Result<T, Error>;
}

impl<T, E: Into<Error>> ErrorExt<T, E> for Result<T, E> {
    fn context(self, context: impl Into<Cow<'static, str>>) -> Result<T, Error> {
        self.map_err(|err| {
            let context = context.into();
            let error = err.into();
            Error::Context {
                error: Box::new(error),
                context,
            }
        })
    }

    fn with_context(self, context_fn: impl FnOnce(&E) -> String) -> Result<T, Error> {
        self.map_err(|err| {
            let context = context_fn(&err);
            let error = err.into();
            Error::Context {
                error: Box::new(error),
                context: context.into(),
            }
        })
    }

    /// Add a path to the error if this is an [`io::Error`] and is
    /// [`io::ErrorKind::PermissionDenied`] map to [`Error::PermissionDenied`]
    fn with_path(self, path: impl AsRef<Path>) -> Result<T, Error> {
        self.map_err(|err| {
            let error = err.into();
            match error {
                Error::Io(err) if err.kind() == ErrorKind::PermissionDenied => Error::PermissionDenied {
                    path: path.as_ref().to_path_buf(),
                    inner: err,
                },
                _ => error,
            }
        })
    }
}
