// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
/// All possible error types for this service.
#[non_exhaustive]
#[derive(::std::fmt::Debug)]
pub enum Error {
    /// This exception is thrown when the user does not have sufficient access to perform this
    /// action.
    AccessDeniedError(crate::types::error::AccessDeniedError),
    /// This exception is thrown when the action to perform could not be completed because the
    /// resource is in a conflicting state.
    ConflictError(crate::types::error::ConflictError),
    /// This exception is thrown when an unexpected error occurred during the processing of a
    /// request.
    InternalServerError(crate::types::error::InternalServerError),
    /// This exception is thrown when describing a resource that does not exist.
    ResourceNotFoundError(crate::types::error::ResourceNotFoundError),
    /// This exception is thrown when request was denied due to request throttling.
    ThrottlingError(crate::types::error::ThrottlingError),
    /// This exception is thrown when the input fails to satisfy the constraints specified by the
    /// service.
    ValidationError(crate::types::error::ValidationError),
    /// An unexpected error occurred (e.g., invalid JSON returned by the service or an unknown error
    /// code).
    #[deprecated(
        note = "Matching `Unhandled` directly is not forwards compatible. Instead, match using a \
    variable wildcard pattern and check `.code()`:
     \
    &nbsp;&nbsp;&nbsp;`err if err.code() == Some(\"SpecificExceptionCode\") => { /* handle the error */ }`
     \
    See [`ProvideErrorMetadata`](#impl-ProvideErrorMetadata-for-Error) for what information is available for the error."
    )]
    Unhandled(crate::error::sealed_unhandled::Unhandled),
}
impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::AccessDeniedError(inner) => inner.fmt(f),
            Error::ConflictError(inner) => inner.fmt(f),
            Error::InternalServerError(inner) => inner.fmt(f),
            Error::ResourceNotFoundError(inner) => inner.fmt(f),
            Error::ThrottlingError(inner) => inner.fmt(f),
            Error::ValidationError(inner) => inner.fmt(f),
            Error::Unhandled(_) => {
                if let ::std::option::Option::Some(code) =
                    ::aws_smithy_types::error::metadata::ProvideErrorMetadata::code(self)
                {
                    write!(f, "unhandled error ({code})")
                } else {
                    f.write_str("unhandled error")
                }
            },
        }
    }
}
impl From<::aws_smithy_types::error::operation::BuildError> for Error {
    fn from(value: ::aws_smithy_types::error::operation::BuildError) -> Self {
        Error::Unhandled(crate::error::sealed_unhandled::Unhandled {
            source: value.into(),
            meta: ::std::default::Default::default(),
        })
    }
}
impl ::aws_smithy_types::error::metadata::ProvideErrorMetadata for Error {
    fn meta(&self) -> &::aws_smithy_types::error::metadata::ErrorMetadata {
        match self {
            Self::AccessDeniedError(inner) => inner.meta(),
            Self::ConflictError(inner) => inner.meta(),
            Self::InternalServerError(inner) => inner.meta(),
            Self::ResourceNotFoundError(inner) => inner.meta(),
            Self::ThrottlingError(inner) => inner.meta(),
            Self::ValidationError(inner) => inner.meta(),
            Self::Unhandled(inner) => &inner.meta,
        }
    }
}
impl<R>
    From<
        ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::export_result_archive::ExportResultArchiveError,
            R,
        >,
    > for Error
where
    R: Send + Sync + std::fmt::Debug + 'static,
{
    fn from(
        err: ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::export_result_archive::ExportResultArchiveError,
            R,
        >,
    ) -> Self {
        match err {
            ::aws_smithy_runtime_api::client::result::SdkError::ServiceError(context) => Self::from(context.into_err()),
            _ => Error::Unhandled(crate::error::sealed_unhandled::Unhandled {
                meta: ::aws_smithy_types::error::metadata::ProvideErrorMetadata::meta(&err).clone(),
                source: err.into(),
            }),
        }
    }
}
impl From<crate::operation::export_result_archive::ExportResultArchiveError> for Error {
    fn from(err: crate::operation::export_result_archive::ExportResultArchiveError) -> Self {
        match err {
            crate::operation::export_result_archive::ExportResultArchiveError::ThrottlingError(inner) => {
                Error::ThrottlingError(inner)
            },
            crate::operation::export_result_archive::ExportResultArchiveError::ConflictError(inner) => {
                Error::ConflictError(inner)
            },
            crate::operation::export_result_archive::ExportResultArchiveError::ValidationError(inner) => {
                Error::ValidationError(inner)
            },
            crate::operation::export_result_archive::ExportResultArchiveError::InternalServerError(inner) => {
                Error::InternalServerError(inner)
            },
            crate::operation::export_result_archive::ExportResultArchiveError::ResourceNotFoundError(inner) => {
                Error::ResourceNotFoundError(inner)
            },
            crate::operation::export_result_archive::ExportResultArchiveError::AccessDeniedError(inner) => {
                Error::AccessDeniedError(inner)
            },
            crate::operation::export_result_archive::ExportResultArchiveError::Unhandled(inner) => {
                Error::Unhandled(inner)
            },
        }
    }
}
impl<R>
    From<
        ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::generate_assistant_response::GenerateAssistantResponseError,
            R,
        >,
    > for Error
where
    R: Send + Sync + std::fmt::Debug + 'static,
{
    fn from(
        err: ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::generate_assistant_response::GenerateAssistantResponseError,
            R,
        >,
    ) -> Self {
        match err {
            ::aws_smithy_runtime_api::client::result::SdkError::ServiceError(context) => Self::from(context.into_err()),
            _ => Error::Unhandled(crate::error::sealed_unhandled::Unhandled {
                meta: ::aws_smithy_types::error::metadata::ProvideErrorMetadata::meta(&err).clone(),
                source: err.into(),
            }),
        }
    }
}
impl From<crate::operation::generate_assistant_response::GenerateAssistantResponseError> for Error {
    fn from(err: crate::operation::generate_assistant_response::GenerateAssistantResponseError) -> Self {
        match err {
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::ThrottlingError(inner) => {
                Error::ThrottlingError(inner)
            },
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::ValidationError(inner) => {
                Error::ValidationError(inner)
            },
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::InternalServerError(
                inner,
            ) => Error::InternalServerError(inner),
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::AccessDeniedError(inner) => {
                Error::AccessDeniedError(inner)
            },
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::Unhandled(inner) => {
                Error::Unhandled(inner)
            },
        }
    }
}
impl<R>
    From<
        ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError,
            R,
        >,
    > for Error
where
    R: Send + Sync + std::fmt::Debug + 'static,
{
    fn from(
        err: ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError,
            R,
        >,
    ) -> Self {
        match err {
            ::aws_smithy_runtime_api::client::result::SdkError::ServiceError(context) => Self::from(context.into_err()),
            _ => Error::Unhandled(crate::error::sealed_unhandled::Unhandled {
                meta: ::aws_smithy_types::error::metadata::ProvideErrorMetadata::meta(&err).clone(),
                source: err.into(),
            }),
        }
    }
}
impl From<crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError> for Error {
    fn from(err: crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError) -> Self {
        match err {
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError::ThrottlingError(inner) => {
                Error::ThrottlingError(inner)
            },
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError::ConflictError(inner) => {
                Error::ConflictError(inner)
            },
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError::ValidationError(inner) => {
                Error::ValidationError(inner)
            },
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError::InternalServerError(inner) => {
                Error::InternalServerError(inner)
            },
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError::ResourceNotFoundError(inner) => {
                Error::ResourceNotFoundError(inner)
            },
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError::AccessDeniedError(inner) => {
                Error::AccessDeniedError(inner)
            },
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError::Unhandled(inner) => {
                Error::Unhandled(inner)
            },
        }
    }
}
impl<R> From<::aws_smithy_runtime_api::client::result::SdkError<crate::types::error::ResultArchiveStreamError, R>>
    for Error
where
    R: Send + Sync + std::fmt::Debug + 'static,
{
    fn from(
        err: ::aws_smithy_runtime_api::client::result::SdkError<crate::types::error::ResultArchiveStreamError, R>,
    ) -> Self {
        match err {
            ::aws_smithy_runtime_api::client::result::SdkError::ServiceError(context) => Self::from(context.into_err()),
            _ => Error::Unhandled(crate::error::sealed_unhandled::Unhandled {
                meta: ::aws_smithy_types::error::metadata::ProvideErrorMetadata::meta(&err).clone(),
                source: err.into(),
            }),
        }
    }
}
impl From<crate::types::error::ResultArchiveStreamError> for Error {
    fn from(err: crate::types::error::ResultArchiveStreamError) -> Self {
        match err {
            crate::types::error::ResultArchiveStreamError::InternalServerError(inner) => {
                Error::InternalServerError(inner)
            },
            crate::types::error::ResultArchiveStreamError::Unhandled(inner) => Error::Unhandled(inner),
        }
    }
}
impl<R> From<::aws_smithy_runtime_api::client::result::SdkError<crate::types::error::ChatResponseStreamError, R>>
    for Error
where
    R: Send + Sync + std::fmt::Debug + 'static,
{
    fn from(
        err: ::aws_smithy_runtime_api::client::result::SdkError<crate::types::error::ChatResponseStreamError, R>,
    ) -> Self {
        match err {
            ::aws_smithy_runtime_api::client::result::SdkError::ServiceError(context) => Self::from(context.into_err()),
            _ => Error::Unhandled(crate::error::sealed_unhandled::Unhandled {
                meta: ::aws_smithy_types::error::metadata::ProvideErrorMetadata::meta(&err).clone(),
                source: err.into(),
            }),
        }
    }
}
impl From<crate::types::error::ChatResponseStreamError> for Error {
    fn from(err: crate::types::error::ChatResponseStreamError) -> Self {
        match err {
            crate::types::error::ChatResponseStreamError::InternalServerError(inner) => {
                Error::InternalServerError(inner)
            },
            crate::types::error::ChatResponseStreamError::Unhandled(inner) => Error::Unhandled(inner),
        }
    }
}
impl ::std::error::Error for Error {
    fn source(&self) -> std::option::Option<&(dyn ::std::error::Error + 'static)> {
        match self {
            Error::AccessDeniedError(inner) => inner.source(),
            Error::ConflictError(inner) => inner.source(),
            Error::InternalServerError(inner) => inner.source(),
            Error::ResourceNotFoundError(inner) => inner.source(),
            Error::ThrottlingError(inner) => inner.source(),
            Error::ValidationError(inner) => inner.source(),
            Error::Unhandled(inner) => ::std::option::Option::Some(&*inner.source),
        }
    }
}
impl ::aws_types::request_id::RequestId for Error {
    fn request_id(&self) -> Option<&str> {
        match self {
            Self::AccessDeniedError(e) => e.request_id(),
            Self::ConflictError(e) => e.request_id(),
            Self::InternalServerError(e) => e.request_id(),
            Self::ResourceNotFoundError(e) => e.request_id(),
            Self::ThrottlingError(e) => e.request_id(),
            Self::ValidationError(e) => e.request_id(),
            Self::Unhandled(e) => e.meta.request_id(),
        }
    }
}