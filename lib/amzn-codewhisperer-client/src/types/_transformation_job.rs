// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// Represent a Transformation Job
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct TransformationJob {
    /// Identifier for the Transformation Job
    pub job_id: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub transformation_spec: ::std::option::Option<crate::types::TransformationSpec>,
    #[allow(missing_docs)] // documentation missing in model
    pub status: ::std::option::Option<crate::types::TransformationStatus>,
    #[allow(missing_docs)] // documentation missing in model
    pub reason: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub creation_time: ::std::option::Option<::aws_smithy_types::DateTime>,
    #[allow(missing_docs)] // documentation missing in model
    pub start_execution_time: ::std::option::Option<::aws_smithy_types::DateTime>,
    #[allow(missing_docs)] // documentation missing in model
    pub end_execution_time: ::std::option::Option<::aws_smithy_types::DateTime>,
}
impl TransformationJob {
    /// Identifier for the Transformation Job
    pub fn job_id(&self) -> ::std::option::Option<&str> {
        self.job_id.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn transformation_spec(&self) -> ::std::option::Option<&crate::types::TransformationSpec> {
        self.transformation_spec.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn status(&self) -> ::std::option::Option<&crate::types::TransformationStatus> {
        self.status.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn reason(&self) -> ::std::option::Option<&str> {
        self.reason.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn creation_time(&self) -> ::std::option::Option<&::aws_smithy_types::DateTime> {
        self.creation_time.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn start_execution_time(&self) -> ::std::option::Option<&::aws_smithy_types::DateTime> {
        self.start_execution_time.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn end_execution_time(&self) -> ::std::option::Option<&::aws_smithy_types::DateTime> {
        self.end_execution_time.as_ref()
    }
}
impl TransformationJob {
    /// Creates a new builder-style object to manufacture
    /// [`TransformationJob`](crate::types::TransformationJob).
    pub fn builder() -> crate::types::builders::TransformationJobBuilder {
        crate::types::builders::TransformationJobBuilder::default()
    }
}

/// A builder for [`TransformationJob`](crate::types::TransformationJob).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct TransformationJobBuilder {
    pub(crate) job_id: ::std::option::Option<::std::string::String>,
    pub(crate) transformation_spec: ::std::option::Option<crate::types::TransformationSpec>,
    pub(crate) status: ::std::option::Option<crate::types::TransformationStatus>,
    pub(crate) reason: ::std::option::Option<::std::string::String>,
    pub(crate) creation_time: ::std::option::Option<::aws_smithy_types::DateTime>,
    pub(crate) start_execution_time: ::std::option::Option<::aws_smithy_types::DateTime>,
    pub(crate) end_execution_time: ::std::option::Option<::aws_smithy_types::DateTime>,
}
impl TransformationJobBuilder {
    /// Identifier for the Transformation Job
    pub fn job_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.job_id = ::std::option::Option::Some(input.into());
        self
    }

    /// Identifier for the Transformation Job
    pub fn set_job_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.job_id = input;
        self
    }

    /// Identifier for the Transformation Job
    pub fn get_job_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.job_id
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn transformation_spec(mut self, input: crate::types::TransformationSpec) -> Self {
        self.transformation_spec = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_transformation_spec(mut self, input: ::std::option::Option<crate::types::TransformationSpec>) -> Self {
        self.transformation_spec = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_transformation_spec(&self) -> &::std::option::Option<crate::types::TransformationSpec> {
        &self.transformation_spec
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn status(mut self, input: crate::types::TransformationStatus) -> Self {
        self.status = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_status(mut self, input: ::std::option::Option<crate::types::TransformationStatus>) -> Self {
        self.status = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_status(&self) -> &::std::option::Option<crate::types::TransformationStatus> {
        &self.status
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn reason(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.reason = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_reason(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.reason = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_reason(&self) -> &::std::option::Option<::std::string::String> {
        &self.reason
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn creation_time(mut self, input: ::aws_smithy_types::DateTime) -> Self {
        self.creation_time = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_creation_time(mut self, input: ::std::option::Option<::aws_smithy_types::DateTime>) -> Self {
        self.creation_time = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_creation_time(&self) -> &::std::option::Option<::aws_smithy_types::DateTime> {
        &self.creation_time
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn start_execution_time(mut self, input: ::aws_smithy_types::DateTime) -> Self {
        self.start_execution_time = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_start_execution_time(mut self, input: ::std::option::Option<::aws_smithy_types::DateTime>) -> Self {
        self.start_execution_time = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_start_execution_time(&self) -> &::std::option::Option<::aws_smithy_types::DateTime> {
        &self.start_execution_time
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn end_execution_time(mut self, input: ::aws_smithy_types::DateTime) -> Self {
        self.end_execution_time = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_end_execution_time(mut self, input: ::std::option::Option<::aws_smithy_types::DateTime>) -> Self {
        self.end_execution_time = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_end_execution_time(&self) -> &::std::option::Option<::aws_smithy_types::DateTime> {
        &self.end_execution_time
    }

    /// Consumes the builder and constructs a
    /// [`TransformationJob`](crate::types::TransformationJob).
    pub fn build(self) -> crate::types::TransformationJob {
        crate::types::TransformationJob {
            job_id: self.job_id,
            transformation_spec: self.transformation_spec,
            status: self.status,
            reason: self.reason,
            creation_time: self.creation_time,
            start_execution_time: self.start_execution_time,
            end_execution_time: self.end_execution_time,
        }
    }
}