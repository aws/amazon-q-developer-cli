// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// Structure to represent code transformation response.
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct StartTransformationOutput {
    /// Identifier for the Transformation Job
    pub transformation_job_id: ::std::string::String,
    _request_id: Option<String>,
}
impl StartTransformationOutput {
    /// Identifier for the Transformation Job
    pub fn transformation_job_id(&self) -> &str {
        use std::ops::Deref;
        self.transformation_job_id.deref()
    }
}
impl ::aws_types::request_id::RequestId for StartTransformationOutput {
    fn request_id(&self) -> Option<&str> {
        self._request_id.as_deref()
    }
}
impl StartTransformationOutput {
    /// Creates a new builder-style object to manufacture
    /// [`StartTransformationOutput`](crate::operation::start_transformation::StartTransformationOutput).
    pub fn builder() -> crate::operation::start_transformation::builders::StartTransformationOutputBuilder {
        crate::operation::start_transformation::builders::StartTransformationOutputBuilder::default()
    }
}

/// A builder for
/// [`StartTransformationOutput`](crate::operation::start_transformation::StartTransformationOutput).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct StartTransformationOutputBuilder {
    pub(crate) transformation_job_id: ::std::option::Option<::std::string::String>,
    _request_id: Option<String>,
}
impl StartTransformationOutputBuilder {
    /// Identifier for the Transformation Job
    /// This field is required.
    pub fn transformation_job_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.transformation_job_id = ::std::option::Option::Some(input.into());
        self
    }

    /// Identifier for the Transformation Job
    pub fn set_transformation_job_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.transformation_job_id = input;
        self
    }

    /// Identifier for the Transformation Job
    pub fn get_transformation_job_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.transformation_job_id
    }

    pub(crate) fn _request_id(mut self, request_id: impl Into<String>) -> Self {
        self._request_id = Some(request_id.into());
        self
    }

    pub(crate) fn _set_request_id(&mut self, request_id: Option<String>) -> &mut Self {
        self._request_id = request_id;
        self
    }

    /// Consumes the builder and constructs a
    /// [`StartTransformationOutput`](crate::operation::start_transformation::StartTransformationOutput).
    /// This method will fail if any of the following fields are not set:
    /// - [`transformation_job_id`](crate::operation::start_transformation::builders::StartTransformationOutputBuilder::transformation_job_id)
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::start_transformation::StartTransformationOutput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::start_transformation::StartTransformationOutput {
            transformation_job_id: self.transformation_job_id.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "transformation_job_id",
                    "transformation_job_id was not specified but it is required when building StartTransformationOutput",
                )
            })?,
            _request_id: self._request_id,
        })
    }
}