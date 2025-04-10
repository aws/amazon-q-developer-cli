use url::ParseError;

#[cfg(feature = "midway")]
use crate::midway;

#[derive(Debug)]
pub enum Error {
    Reqwest(reqwest::Error),
    Serde(serde_json::Error),
    Io(std::io::Error),
    Dir(fig_util::directories::DirectoryError),
    Settings(fig_settings::Error),
    NoClient,
    NoToken,
    UrlParseError(ParseError),
    #[cfg(feature = "midway")]
    Midway(midway::MidwayError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Reqwest(err) => write!(f, "Reqwest error: {err}"),
            // Error::Status(err) => write!(f, "Status error: {err}"),
            Error::Serde(err) => write!(f, "Serde error: {err}"),
            Error::Io(err) => write!(f, "Io error: {err}"),
            Error::Dir(err) => write!(f, "Dir error: {err}"),
            Error::Settings(err) => write!(f, "Settings error: {err}"),
            Error::NoClient => write!(f, "No client"),
            Error::NoToken => write!(f, "No token"),
            Error::UrlParseError(err) => write!(f, "Url parse error: {err}"),
            #[cfg(feature = "midway")]
            Error::Midway(err) => write!(f, "Midway error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Reqwest(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serde(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<fig_util::directories::DirectoryError> for Error {
    fn from(e: fig_util::directories::DirectoryError) -> Self {
        Error::Dir(e)
    }
}

impl From<fig_settings::Error> for Error {
    fn from(e: fig_settings::Error) -> Self {
        Error::Settings(e)
    }
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Error::UrlParseError(e)
    }
}

#[cfg(feature = "midway")]
impl From<midway::MidwayError> for Error {
    fn from(e: midway::MidwayError) -> Self {
        Error::Midway(e)
    }
}
