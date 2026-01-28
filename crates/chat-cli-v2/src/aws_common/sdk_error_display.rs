use std::error::Error;
use std::fmt::{
    self,
    Debug,
    Display,
};

use aws_smithy_runtime_api::client::result::SdkError;

#[derive(Debug)]
pub struct SdkErrorDisplay<'a, E, R>(pub &'a SdkError<E, R>);

impl<E, R> Display for SdkErrorDisplay<'_, E, R>
where
    E: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            SdkError::ConstructionFailure(_) => {
                write!(f, "failed to construct request")
            },
            SdkError::TimeoutError(_) => write!(f, "request has timed out"),
            SdkError::DispatchFailure(e) => {
                write!(f, "dispatch failure")?;
                if let Some(connector_error) = e.as_connector_error() {
                    if let Some(source) = connector_error.source() {
                        write!(f, " ({connector_error}): {source}")?;
                    } else {
                        write!(f, ": {connector_error}")?;
                    }
                }
                Ok(())
            },
            SdkError::ResponseError(_) => write!(f, "response error"),
            SdkError::ServiceError(e) => {
                write!(f, "{}", e.err())
            },
            other => write!(f, "{other}"),
        }
    }
}

impl<E, R> Error for SdkErrorDisplay<'_, E, R>
where
    E: Error + 'static,
    R: Debug,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

#[cfg(test)]
mod tests {
    use aws_smithy_runtime_api::client::result::{
        ConnectorError,
        ConstructionFailure,
        DispatchFailure,
        ResponseError,
        SdkError,
        ServiceError,
        TimeoutError,
    };

    use super::SdkErrorDisplay;

    #[test]
    fn test_displays_sdk_error() {
        let construction_failure = ConstructionFailure::builder().source("<source>").build();
        let sdk_error: SdkError<String, String> = SdkError::ConstructionFailure(construction_failure);
        let sdk_error_display = SdkErrorDisplay(&sdk_error);
        assert_eq!("failed to construct request", sdk_error_display.to_string());

        let timeout_error = TimeoutError::builder().source("<source>").build();
        let sdk_error: SdkError<String, String> = SdkError::TimeoutError(timeout_error);
        let sdk_error_display = SdkErrorDisplay(&sdk_error);
        assert_eq!("request has timed out", sdk_error_display.to_string());

        let dispatch_failure = DispatchFailure::builder()
            .source(ConnectorError::io("<source>".into()))
            .build();
        let sdk_error: SdkError<String, String> = SdkError::DispatchFailure(dispatch_failure);
        let sdk_error_display = SdkErrorDisplay(&sdk_error);
        assert_eq!("dispatch failure (io error): <source>", sdk_error_display.to_string());

        let response_error = ResponseError::builder().source("<source>").raw("<raw>".into()).build();
        let sdk_error: SdkError<String, String> = SdkError::ResponseError(response_error);
        let sdk_error_display = SdkErrorDisplay(&sdk_error);
        assert_eq!("response error", sdk_error_display.to_string());

        let service_error = ServiceError::builder().source("<source>").raw("<raw>".into()).build();
        let sdk_error: SdkError<String, String> = SdkError::ServiceError(service_error);
        let sdk_error_display = SdkErrorDisplay(&sdk_error);
        assert_eq!("<source>", sdk_error_display.to_string());
    }
}
