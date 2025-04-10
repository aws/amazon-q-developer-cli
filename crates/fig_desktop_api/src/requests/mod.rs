use std::borrow::Cow;

use fig_proto::ReflectMessage;

pub mod auth;
pub mod codewhisperer;
pub mod fs;
pub mod history;
pub mod install;
pub mod other;
pub mod platform;
pub mod settings;
pub mod state;
pub mod telemetry;
pub mod update;

pub type ServerOriginatedSubMessage = fig_proto::fig::server_originated_message::Submessage;
pub type RequestResult = Result<Box<ServerOriginatedSubMessage>, Error>;

pub trait RequestResultImpl {
    fn success() -> Self;
    fn error(msg: impl Into<String>) -> Self;
    fn deprecated(message: impl ReflectMessage) -> Self;
    fn unimplemented(message: impl ReflectMessage) -> Self;
}

impl RequestResultImpl for RequestResult {
    fn success() -> Self {
        RequestResult::Ok(Box::new(ServerOriginatedSubMessage::Success(true)))
    }

    fn error(msg: impl Into<String>) -> Self {
        RequestResult::Ok(Box::new(ServerOriginatedSubMessage::Error(msg.into())))
    }

    fn deprecated(message: impl ReflectMessage) -> Self {
        RequestResult::error(format!("{} is deprecated", message.descriptor().name()))
    }

    fn unimplemented(message: impl ReflectMessage) -> Self {
        RequestResult::error(format!("{} is unimplemented", message.descriptor().name()))
    }
}

#[derive(Debug)]
pub enum Error {
    Custom(Cow<'static, str>),
    Wrapped {
        context: Option<Cow<'static, str>>,
        source: Box<Error>,
    },
    Std(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Custom(v) => f.write_str(v),
            Error::Wrapped {
                context: Some(context), ..
            } => f.write_str(context),
            Error::Wrapped { source, .. } => source.fmt(f),
            Error::Std(v) => v.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn from_std(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Error::Wrapped {
            context: None,
            source: Box::new(Error::Std(Box::new(error))),
        }
    }

    pub fn wrap_err(self, context: impl Into<Cow<'static, str>>) -> Self {
        Error::Wrapped {
            context: Some(context.into()),
            source: Box::new(self),
        }
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Custom(s.into())
    }
}

impl From<&'static str> for Error {
    fn from(s: &'static str) -> Self {
        Error::Custom(s.into())
    }
}
